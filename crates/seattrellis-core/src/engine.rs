// ---------------------------------------------------------------------------
// engine.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Hard search engine: cost context, local search, greedy, MRV/backtracking, validation.
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::cost::{avoid_recent_neighbors_cost, individual_cost, normalize_edge};
use crate::models::{effective_neighbor_rule, Layout, Seat, Student};
use crate::objectives::{
    compile_soft_objectives_with_adjacency_controlled, evaluate_soft_objectives,
};
use crate::rng::SplitMix64;
#[cfg(test)]
use crate::solver::SolveControl;

use crate::evaluation::{
    assigned_students_are_adjacent, assigned_students_meet_distance, build_index_adjacency,
    graph_distance, seat_distance, CoreDistanceMetric,
};
use crate::solver::{
    resolve_group_rules_controlled, CoreSolveRequest, CostContext, ResolvedHardRules,
    SolveRunControl, StopReason,
};
use crate::NATIVE_API_VERSION;

pub(crate) fn assignment_by_key(
    probe: &[Option<usize>],
    ctx: &CostContext,
) -> std::collections::HashMap<String, String> {
    let mut by_key = std::collections::HashMap::new();
    for (student, seat) in probe.iter().enumerate() {
        if let Some(seat) = seat {
            by_key.insert(
                ctx.students[student].key.clone(),
                ctx.layout.seats[*seat].seat_id.clone(),
            );
        }
    }
    by_key
}

/// Validate a solve request without spending the solver's attempt budget.
///
/// This is the native counterpart to the Python CLI's input-only `validate`
/// command. It checks the versioned DTO, capacity, coordinates, student
/// records, and hard-rule references, but it does not claim that a feasible
/// assignment exists; use [`solve_problem_json`] for that check.
pub fn validate_solve_request_json(request_json: &str) -> Result<(), String> {
    let request = crate::solver::parse_core_solve_request(request_json)?;
    validate_solve_request(&request)
}

/// Build the cost context for a solve request, degrading gracefully when the
/// optional cost data is absent.
pub(crate) fn build_cost_context(request: &CoreSolveRequest) -> CostContext {
    build_cost_context_controlled(request, None).expect("uncontrolled cost compilation cannot stop")
}

pub(crate) fn build_cost_context_controlled(
    request: &CoreSolveRequest,
    run: Option<&SolveRunControl<'_>>,
) -> Result<CostContext, StopReason> {
    let students = effective_students(request);
    let layout = effective_layout(request);
    let rules = request.rules.clone().unwrap_or_default();
    // Rule inputs remain immutable during a solve. Compile their merged form
    // once rather than cloning relation strings for every student/seat probe.
    let neighbor_rule = effective_neighbor_rule(&rules);
    let soft = &rules.soft;
    let has_score_objectives = (soft.score_position.enabled && soft.score_position.weight != 0)
        || (soft.score_distribution.enabled && soft.score_distribution.weight != 0)
        || (soft.mentor_pairing.enabled && soft.mentor_pairing.weight != 0);
    let adjacency_edges = adjacency_edges_by_seat_id(&layout, &request.edges);
    let objective_context = compile_soft_objectives_with_adjacency_controlled(
        &students,
        &layout,
        &rules,
        request.pair_history.as_ref(),
        Some(&adjacency_edges),
        &mut || run.and_then(SolveRunControl::stop_reason).is_some(),
    )
    .ok_or_else(|| {
        run.and_then(SolveRunControl::stop_reason)
            .unwrap_or(StopReason::Deadline)
    })?;
    let enabled_seats = layout.enabled_seats();
    let min_row = enabled_seats.iter().map(|seat| seat.row).min().unwrap_or(1);
    let max_row = enabled_seats.iter().map(|seat| seat.row).max().unwrap_or(1);
    Ok(CostContext {
        students,
        layout,
        rules,
        neighbor_rule,
        has_score_objectives,
        history: request.history.clone(),
        pair_history: request.pair_history.clone(),
        adjacency_edges,
        objective_context,
        min_row,
        max_row,
    })
}

/// Placeholder students when the request carries no records: index-derived keys
/// with scores taken from `student_scores` when provided.
pub(crate) fn effective_students(request: &CoreSolveRequest) -> Vec<Student> {
    if !request.students.is_empty() {
        return request.students.clone();
    }
    (0..request.student_count)
        .map(|index| Student {
            key: format!("STU{:03}", index + 1),
            score: request.student_scores.get(index).copied().flatten(),
            ..Student::default()
        })
        .collect()
}

/// The seat records behind `seat_positions`: the request layout when given,
/// otherwise a grid derived from the coordinates (row = round(y), col = round(x)).
pub(crate) fn effective_layout(request: &CoreSolveRequest) -> Layout {
    if let Some(layout) = &request.layout {
        let mut layout = layout.clone();
        layout.seats.retain(|seat| seat.enabled);
        for (seat, position) in layout.seats.iter_mut().zip(&request.seat_positions) {
            seat.x = Some(position[0]);
            seat.y = Some(position[1]);
        }
        return layout;
    }
    let seats: Vec<Seat> = request
        .seat_positions
        .iter()
        .enumerate()
        .map(|(index, position)| Seat {
            seat_id: format!("seat-{}", index + 1),
            row: position[1].round() as i32,
            col: position[0].round() as i32,
            x: Some(position[0]),
            y: Some(position[1]),
            enabled: true,
            zone: None,
            group_id: None,
            near_window: false,
            near_door: false,
            near_platform: false,
            near_ac: false,
        })
        .collect();
    Layout::new(seats)
}

/// Convert the index-pair request edges into the normalized seat-id edge set the
/// cost functions expect (mirrors passing `adjacency_edges=problem.edges`).
pub(crate) fn adjacency_edges_by_seat_id(
    layout: &Layout,
    edges: &[[usize; 2]],
) -> HashSet<(String, String)> {
    let mut result = HashSet::new();
    for [first, second] in edges {
        if let (Some(first_seat), Some(second_seat)) =
            (layout.seats.get(*first), layout.seats.get(*second))
        {
            result.insert(normalize_edge(&first_seat.seat_id, &second_seat.seat_id));
        }
    }
    result
}

/// Per-(student, seat) cost used to rank candidate seats, mirroring
/// `_fallback_candidate_cost`. `assignment` is the partial assignment so far.
fn candidate_ranking_cost(
    student_index: usize,
    seat_index: usize,
    assignment: &[Option<usize>],
    ctx: &CostContext,
) -> f64 {
    let student = &ctx.students[student_index];
    let seat = &ctx.layout.seats[seat_index];
    // `_fallback_individual_cost` uses a fresh `random.Random(0)` per call; we
    // mirror that with a fresh fixed-seed SplitMix64 so the randomize term is a
    // deterministic constant that never skews the ranking.
    let mut cost = individual_cost(
        student,
        seat,
        &ctx.layout,
        &ctx.rules,
        ctx.history.as_ref(),
        &mut SplitMix64::new(0),
        ctx.min_row,
        ctx.max_row,
    ) as f64;

    let neighbor_rule = &ctx.neighbor_rule;
    if neighbor_rule.enabled && neighbor_rule.weight != 0 {
        for (assigned_index, assigned_seat_index) in assignment.iter().enumerate() {
            if let Some(assigned_seat_index) = assigned_seat_index {
                cost += avoid_recent_neighbors_cost(
                    &student.key,
                    &ctx.students[assigned_index].key,
                    seat,
                    &ctx.layout.seats[*assigned_seat_index],
                    &ctx.layout,
                    neighbor_rule,
                    ctx.pair_history.as_ref(),
                    Some(&ctx.adjacency_edges),
                ) as f64;
            }
        }
    }

    if ctx.has_score_objectives {
        let mut prospective: HashMap<String, String> = HashMap::new();
        for (index, assigned_seat_index) in assignment.iter().enumerate() {
            if let Some(seat_index) = assigned_seat_index {
                prospective.insert(
                    ctx.students[index].key.clone(),
                    ctx.layout.seats[*seat_index].seat_id.clone(),
                );
            }
        }
        prospective.insert(student.key.clone(), seat.seat_id.clone());
        cost +=
            evaluate_soft_objectives(&prospective, &ctx.objective_context, &ctx.rules).total_cost();
    }
    cost
}

/// Total cost of a complete assignment, mirroring `_fallback_total_cost`:
/// individual costs (with one seeded RNG), the score-balance adjacent reward,
/// the recent-neighbor penalty over every pair, and the soft-objective cost.
pub(crate) fn full_solution_total_cost(
    assignment: &[usize],
    adjacency: &[Vec<usize>],
    ctx: &CostContext,
) -> f64 {
    let mut rng = SplitMix64::new(ctx.rules.seed);
    let mut cost = 0.0f64;
    for (student_index, seat_index) in assignment.iter().enumerate() {
        cost += individual_cost(
            &ctx.students[student_index],
            &ctx.layout.seats[*seat_index],
            &ctx.layout,
            &ctx.rules,
            ctx.history.as_ref(),
            &mut rng,
            ctx.min_row,
            ctx.max_row,
        ) as f64;
    }

    if ctx.rules.soft.score_balance.enabled && ctx.rules.soft.score_balance.weight != 0 {
        for first_index in 0..assignment.len() {
            let Some(first_score) = ctx.students[first_index].score else {
                continue;
            };
            for second_index in (first_index + 1)..assignment.len() {
                let Some(second_score) = ctx.students[second_index].score else {
                    continue;
                };
                if adjacency[assignment[first_index]].contains(&assignment[second_index]) {
                    cost -= (ctx.rules.soft.score_balance.weight as f64)
                        * (first_score - second_score).abs();
                }
            }
        }
    }

    let neighbor_rule = &ctx.neighbor_rule;
    if neighbor_rule.enabled && neighbor_rule.weight != 0 {
        for first_index in 0..assignment.len() {
            for second_index in (first_index + 1)..assignment.len() {
                cost += avoid_recent_neighbors_cost(
                    &ctx.students[first_index].key,
                    &ctx.students[second_index].key,
                    &ctx.layout.seats[assignment[first_index]],
                    &ctx.layout.seats[assignment[second_index]],
                    &ctx.layout,
                    neighbor_rule,
                    ctx.pair_history.as_ref(),
                    Some(&ctx.adjacency_edges),
                ) as f64;
            }
        }
    }

    if ctx.has_score_objectives {
        let mut assignment_by_key: HashMap<String, String> = HashMap::new();
        for (student_index, seat_index) in assignment.iter().enumerate() {
            assignment_by_key.insert(
                ctx.students[student_index].key.clone(),
                ctx.layout.seats[*seat_index].seat_id.clone(),
            );
        }
        cost += evaluate_soft_objectives(&assignment_by_key, &ctx.objective_context, &ctx.rules)
            .total_cost();
    }
    cost
}

/// Local search budget: candidate moves per optimization run (plan §6.2).
const LOCAL_SEARCH_ITERATIONS: usize = 2_000;

/// Stop after this many consecutive non-improving moves (stagnation
/// detection, plan §6.2).
const LOCAL_SEARCH_STAGNATION_LIMIT: usize = 250;

/// Stop the greedy attempt loop after this many consecutive attempts that
/// did not improve the best plan (plan §6.6 interactive response). The loop
/// stays deterministic: the attempt order is seed-driven, so the same
/// version/input/seed reproduces the same result, while easy problems stop
/// after a short plateau instead of spending the full `n*12` attempts.
pub(crate) const GREEDY_STAGNATION_LIMIT: usize = 48;

/// Soft optimization (plan §6.2): hill-climbing local search on top of a
/// legal assignment. Swaps two students' seats or moves a student to an
/// empty seat; every candidate move is re-validated against the hard rules
/// before acceptance, so hard correctness is never broken. Moves are sampled
/// with the shared deterministic RNG — same seed, same result.
///
/// Only strictly-improving moves are accepted; after `STAGNATION_LIMIT`
/// consecutive failures the search stops. Returns the best assignment found
/// (may be the input itself).
#[cfg(test)]
pub(crate) fn local_search(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    assignment: &[usize],
    ctx: &CostContext,
    rng: &mut SplitMix64,
) -> Vec<usize> {
    let cancellation = SolveControl::new();
    let run = SolveRunControl {
        deadline: None,
        cancellation: &cancellation,
    };
    local_search_controlled(
        request,
        resolved,
        adjacency,
        graph_distances,
        assignment,
        ctx,
        rng,
        &run,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn local_search_controlled(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    assignment: &[usize],
    ctx: &CostContext,
    rng: &mut SplitMix64,
    run: &SolveRunControl<'_>,
    excluded_assignments: &[Vec<usize>],
) -> Vec<usize> {
    // With fewer than two students and no empty seat there is no legal
    // neighbor to explore. Returning immediately also avoids constructing a
    // swap index from `len - 1` for the single-student case.
    if assignment.len() < 2 && ctx.layout.seats.len() <= assignment.len() {
        return assignment.to_vec();
    }
    let mut current = assignment.to_vec();
    let mut current_cost = full_solution_total_cost(&current, adjacency, ctx);
    let mut stagnation = 0;

    for _ in 0..LOCAL_SEARCH_ITERATIONS {
        if run.stop_reason().is_some() {
            break;
        }
        let candidate = random_neighbor(&current, ctx, rng);
        let Ok(probe) =
            validate_candidate_move(request, resolved, adjacency, graph_distances, candidate)
        else {
            stagnation += 1;
            if stagnation >= LOCAL_SEARCH_STAGNATION_LIMIT {
                break;
            }
            continue;
        };
        if assignment_is_excluded(&probe, excluded_assignments) {
            stagnation += 1;
            if stagnation >= LOCAL_SEARCH_STAGNATION_LIMIT {
                break;
            }
            continue;
        }
        let candidate_cost = full_solution_total_cost(&probe, adjacency, ctx);
        if candidate_cost < current_cost {
            current = probe;
            current_cost = candidate_cost;
            stagnation = 0;
        } else {
            stagnation += 1;
            if stagnation >= LOCAL_SEARCH_STAGNATION_LIMIT {
                break;
            }
        }
    }
    current
}

fn assignment_is_excluded(assignment: &[usize], excluded_assignments: &[Vec<usize>]) -> bool {
    excluded_assignments
        .iter()
        .any(|excluded| excluded.as_slice() == assignment)
}

/// A candidate neighbor assignment: either a swap of two students' seats or
/// a move of one student into an empty seat (sampled deterministically).
fn random_neighbor(assignment: &[usize], ctx: &CostContext, rng: &mut SplitMix64) -> Vec<usize> {
    let mut neighbor = assignment.to_vec();
    let seat_count = ctx.layout.seats.len();
    if neighbor.is_empty() {
        return neighbor;
    }
    if neighbor.len() == 1 {
        // A lone student can only move into an empty seat; if none exists the
        // original assignment is the only possible neighbor.
        if let Some(empty_seat) = (0..seat_count).find(|seat| !neighbor.contains(seat)) {
            neighbor[0] = empty_seat;
        }
        return neighbor;
    }
    let swap = rng.next_usize(2) == 0;
    if swap {
        let first = rng.next_usize(neighbor.len());
        let second = rng.next_usize(neighbor.len() - 1);
        let second = if second >= first { second + 1 } else { second };
        neighbor.swap(first, second);
    } else {
        // Move one student into a seat that is currently empty.
        let student = rng.next_usize(neighbor.len());
        let mut empty_seats: Vec<usize> = (0..seat_count)
            .filter(|seat| !neighbor.contains(seat))
            .collect();
        if let Some(seat) = empty_seats.pop() {
            neighbor[student] = seat;
        } else {
            let first = rng.next_usize(neighbor.len());
            let second = rng.next_usize(neighbor.len() - 1);
            let second = if second >= first { second + 1 } else { second };
            neighbor.swap(first, second);
        }
    }
    neighbor
}

/// Validate a candidate neighbor: every hard rule must still hold. Returns
/// the probe assignment on success, the violating rule description on
/// failure.
fn validate_candidate_move(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    candidate: Vec<usize>,
) -> Result<Vec<usize>, String> {
    let probe: Vec<Option<usize>> = candidate.iter().map(|&seat| Some(seat)).collect();
    if !solve_partial_assignment_valid(request, resolved, &probe, adjacency, graph_distances) {
        return Err("candidate move violates a hard rule".to_string());
    }
    // Uniqueness: the neighbor generator only swaps seats or moves into an
    // empty seat, so seats stay unique; double-check for safety.
    if candidate
        .iter()
        .any(|seat| candidate.iter().filter(|other| other == &seat).count() > 1)
    {
        return Err("candidate move duplicates a seat".to_string());
    }
    Ok(candidate)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GreedyOutcome {
    Found(Vec<usize>),
    DeadEnd,
    Stopped(StopReason),
}

#[cfg(test)]
pub(crate) fn greedy_attempt(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    rng: &mut SplitMix64,
    ctx: &CostContext,
    attempt: usize,
) -> Option<Vec<usize>> {
    let cancellation = SolveControl::new();
    let run = SolveRunControl {
        deadline: None,
        cancellation: &cancellation,
    };
    match greedy_attempt_controlled(
        request,
        resolved,
        adjacency,
        graph_distances,
        rng,
        ctx,
        attempt,
        &run,
        &[],
    ) {
        GreedyOutcome::Found(assignment) => Some(assignment),
        GreedyOutcome::DeadEnd | GreedyOutcome::Stopped(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn greedy_attempt_controlled(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    rng: &mut SplitMix64,
    ctx: &CostContext,
    attempt: usize,
    run: &SolveRunControl<'_>,
    excluded_assignments: &[Vec<usize>],
) -> GreedyOutcome {
    let student_count = request.student_count;
    let seat_count = request.seat_positions.len();
    let mut assignment: Vec<Option<usize>> = vec![None; student_count];
    let mut used: Vec<bool> = vec![false; seat_count];
    let mut order: Vec<usize> = (0..student_count).collect();
    shuffle(&mut order, rng);

    loop {
        if let Some(reason) = run.stop_reason() {
            return GreedyOutcome::Stopped(reason);
        }
        // Pick the unassigned student with the fewest valid candidate seats.
        let mut best: Option<(usize, Vec<usize>)> = None;
        for &student in &order {
            if let Some(reason) = run.stop_reason() {
                return GreedyOutcome::Stopped(reason);
            }
            if assignment[student].is_some() {
                continue;
            }
            let candidates = match valid_candidate_seats_controlled(
                request,
                resolved,
                &mut assignment,
                &used,
                adjacency,
                graph_distances,
                student,
                run,
            ) {
                Ok(candidates) => candidates,
                Err(reason) => return GreedyOutcome::Stopped(reason),
            };
            if candidates.is_empty() {
                return GreedyOutcome::DeadEnd;
            }
            if best
                .as_ref()
                .is_none_or(|(_, existing)| candidates.len() < existing.len())
            {
                best = Some((student, candidates));
            }
        }
        let Some((student, candidates)) = best else {
            return GreedyOutcome::DeadEnd;
        };

        // Rank candidates by cost; attempt 0 takes the cheapest, later attempts
        // sample uniformly from the top-3 (mirrors Python `rng.choice`).
        let mut ranked: Vec<(f64, usize)> = Vec::with_capacity(candidates.len());
        for seat in candidates {
            if let Some(reason) = run.stop_reason() {
                return GreedyOutcome::Stopped(reason);
            }
            ranked.push((
                candidate_ranking_cost(student, seat, &assignment, ctx),
                seat,
            ));
        }
        ranked.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        let seat = if attempt == 0 {
            ranked[0].1
        } else {
            let top = ranked.len().min(3);
            ranked[rng.next_usize(top)].1
        };

        assignment[student] = Some(seat);
        used[seat] = true;
        if assignment.iter().all(Option::is_some) {
            let complete: Vec<usize> = assignment
                .into_iter()
                .map(|seat| seat.expect("all students were checked as assigned"))
                .collect();
            if assignment_is_excluded(&complete, excluded_assignments) {
                return GreedyOutcome::DeadEnd;
            }
            return GreedyOutcome::Found(complete);
        }
    }
}

/// Independent hard-rule validator for a complete assignment (M3-05).
///
/// Every `Solved` response must pass this before leaving the core: a solver
/// bug that produced a violating assignment becomes `InternalError` instead
/// of a silently "feasible" result. The check is deliberately separate from
/// the solver's own bookkeeping — it re-derives the probe from the raw
/// assignment and re-checks every hard rule from the request.
pub fn validate_assignment(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    assignment: &[usize],
) -> Result<(), String> {
    if assignment.len() != request.student_count {
        return Err(format!(
            "solver produced an assignment for {} of {} students",
            assignment.len(),
            request.student_count
        ));
    }
    let mut seen = vec![false; request.seat_positions.len()];
    for (student, &seat) in assignment.iter().enumerate() {
        if seat >= request.seat_positions.len() {
            return Err(format!(
                "solver produced an out-of-range seat {seat} for student {student}"
            ));
        }
        if seen[seat] {
            return Err(format!("solver produced a duplicate seat {seat}"));
        }
        seen[seat] = true;
    }
    let probe: Vec<Option<usize>> = assignment.iter().map(|&seat| Some(seat)).collect();
    if !solve_partial_assignment_valid(request, resolved, &probe, adjacency, graph_distances) {
        return Err("solver produced an assignment that violates a hard rule".to_string());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // solver-internal candidate filter mirroring Python
fn valid_candidate_seats_controlled(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    assignment: &mut [Option<usize>],
    used: &[bool],
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    student: usize,
    run: &SolveRunControl<'_>,
) -> Result<Vec<usize>, StopReason> {
    let fixed = request
        .fixed_seats
        .iter()
        .find(|pair| pair[0] == student)
        .map(|pair| pair[1]);
    let mut candidates = Vec::new();
    for (seat, &is_used) in used.iter().enumerate() {
        if let Some(reason) = run.stop_reason() {
            return Err(reason);
        }
        if is_used {
            continue;
        }
        if let Some(fixed_seat) = fixed {
            if seat != fixed_seat {
                continue;
            }
        }
        assignment[student] = Some(seat);
        let ok = solve_partial_assignment_valid(
            request,
            resolved,
            assignment,
            adjacency,
            graph_distances,
        );
        assignment[student] = None;
        if ok {
            candidates.push(seat);
        }
    }
    Ok(candidates)
}

pub(crate) fn solve_partial_assignment_valid(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    assignment: &[Option<usize>],
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
) -> bool {
    for [student_index, seat_index] in &request.fixed_seats {
        if let Some(assigned_seat) = assignment[*student_index] {
            if assigned_seat != *seat_index {
                return false;
            }
        }
    }
    for [first_student, second_student] in &resolved.must_be_adjacent {
        if assignment[*first_student].is_some()
            && assignment[*second_student].is_some()
            && !assigned_students_are_adjacent(
                assignment,
                adjacency,
                *first_student,
                *second_student,
            )
        {
            return false;
        }
    }
    for [first_student, second_student] in &resolved.cannot_be_adjacent {
        if assignment[*first_student].is_some()
            && assignment[*second_student].is_some()
            && assigned_students_are_adjacent(
                assignment,
                adjacency,
                *first_student,
                *second_student,
            )
        {
            return false;
        }
    }
    for rule in &request.min_distance {
        if assignment[rule.students[0]].is_some()
            && assignment[rule.students[1]].is_some()
            && !assigned_students_meet_distance(
                &request.seat_positions,
                assignment,
                graph_distances,
                rule,
            )
        {
            return false;
        }
    }
    true
}

/// Candidate seat domain for one student under the current hard rules,
/// ignoring other students' occupancy (M3-02, plan §6.1 second layer).
///
/// `seats` is the list of seats that do not violate any hard rule when this
/// student sits there and every fixed student keeps their seat. `excluded`
/// records, for every seat outside the domain, the first hard rule that
/// rejects it — used by feasibility reports (M3-06).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateDomain {
    pub student: usize,
    pub seats: Vec<usize>,
    pub excluded: Vec<(usize, String)>,
}

/// Build the per-student candidate seat domains (plan §6.1 second layer).
///
/// A student with an empty domain is a *sound* infeasibility proof: no
/// complete assignment can seat them, regardless of how other students are
/// placed (occupancy only removes candidates, never adds them).
pub fn build_candidate_domains(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
) -> Vec<CandidateDomain> {
    build_candidate_domains_controlled(request, resolved, adjacency, graph_distances, None)
        .expect("uncontrolled domain construction cannot stop")
}

pub(crate) fn build_candidate_domains_controlled(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    run: Option<&SolveRunControl<'_>>,
) -> Result<Vec<CandidateDomain>, StopReason> {
    // Pre-place every fixed student so pair rules involving them constrain the
    // probed student exactly like in a real assignment (mirrors the strict
    // compile-time fixed/pair interaction checks in Python).
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    for [student, seat] in &request.fixed_seats {
        probe[*student] = Some(*seat);
    }

    let mut domains = Vec::with_capacity(request.student_count);
    for student in 0..request.student_count {
        let fixed = request
            .fixed_seats
            .iter()
            .find(|pair| pair[0] == student)
            .map(|pair| pair[1]);
        let mut seats = Vec::new();
        let mut excluded = Vec::new();
        for seat in 0..request.seat_positions.len() {
            if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
                return Err(reason);
            }
            if let Some(fixed_seat) = fixed {
                if seat != fixed_seat {
                    excluded.push((seat, format!("fixed to seat {fixed_seat}")));
                    continue;
                }
            }
            probe[student] = Some(seat);
            let violation = first_hard_rule_violation(
                request,
                resolved,
                &probe,
                adjacency,
                graph_distances,
                student,
            );
            // Restore the pre-probe state: fixed students keep their seat,
            // everyone else goes back to unassigned.
            probe[student] = fixed;
            match violation {
                None => seats.push(seat),
                Some(reason) => excluded.push((seat, reason)),
            }
        }
        domains.push(CandidateDomain {
            student,
            seats,
            excluded,
        });
    }
    Ok(domains)
}

/// Global matching precheck (plan §6.1 third layer): maximum bipartite
/// matching between students and their candidate seats.
///
/// When the maximum matching cannot seat every student, no complete
/// assignment exists (each student must take one of their own candidates,
/// seats are unique) — a sound `ProvenInfeasible` proof. The converse is not
/// asserted: a full matching does not prove feasibility, because pair rules
/// may still conflict across students (that is the hard search's job).
pub fn maximum_candidate_matching(domains: &[CandidateDomain]) -> usize {
    maximum_candidate_matching_controlled(domains, None).expect("uncontrolled matching cannot stop")
}

pub(crate) fn maximum_candidate_matching_controlled(
    domains: &[CandidateDomain],
    run: Option<&SolveRunControl<'_>>,
) -> Result<usize, StopReason> {
    let seat_count = domains
        .iter()
        .flat_map(|domain| domain.seats.iter().copied())
        .max()
        .map_or(0, |max| max + 1);
    let mut seat_owner: Vec<Option<usize>> = vec![None; seat_count];
    let mut student_seat: Vec<Option<usize>> = vec![None; domains.len()];
    let mut matched = 0;
    for source in 0..domains.len() {
        // Iterative augmenting paths avoid a recursion depth proportional to
        // the input class size and check cancellation throughout matching.
        let mut predecessor: Vec<Option<usize>> = vec![None; seat_count];
        let mut visited_students = vec![false; domains.len()];
        let mut queue = std::collections::VecDeque::from([source]);
        visited_students[source] = true;
        let mut free_seat = None;
        'search: while let Some(student) = queue.pop_front() {
            for &seat in &domains[student].seats {
                if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
                    return Err(reason);
                }
                if predecessor[seat].is_some() {
                    continue;
                }
                predecessor[seat] = Some(student);
                match seat_owner[seat] {
                    None => {
                        free_seat = Some(seat);
                        break 'search;
                    }
                    Some(owner) if !visited_students[owner] => {
                        visited_students[owner] = true;
                        queue.push_back(owner);
                    }
                    Some(_) => {}
                }
            }
        }
        if let Some(mut seat) = free_seat {
            loop {
                let student = predecessor[seat].expect("an augmenting path has a predecessor");
                let previous_seat = student_seat[student];
                seat_owner[seat] = Some(student);
                student_seat[student] = Some(seat);
                match previous_seat {
                    Some(previous_seat) => seat = previous_seat,
                    None => break,
                }
            }
            matched += 1;
        }
    }
    Ok(matched)
}

/// Exhaustive hard search outcome (plan §6.1 fourth layer).
///
/// [`SearchOutcome::ProvenInfeasible`] is only returned when the entire state
/// space was swept without a legal seating; hitting the node budget returns
/// [`SearchOutcome::BudgetExceeded`] so callers can keep the honest `Unknown`
/// status (M1-03: heuristic exhaustion is never ProvenInfeasible).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SearchOutcome {
    Found(Vec<usize>),
    ProvenInfeasible,
    BudgetExceeded,
    DeadlineExceeded,
    Cancelled,
}

/// Node budget for one hard search. Classes are small (<= 60 students) and
/// MRV + forward checking prune hard, so 200k nodes is generous for the
/// full sweep while still bounding worst-case time.
pub(crate) const HARD_SEARCH_NODE_BUDGET: usize = 200_000;

/// Full hard search: MRV student selection with degree tie-break, forward
/// checking over the candidate domains, deterministic (fixed order) branch
/// exploration. Fixed students are pre-placed; their domains are singletons
/// so MRV picks them first. `budget` bounds nodes; `deadline` (optional
/// wall-clock, M3-04) bounds time — whichever hits first stops the sweep.
#[cfg(test)]
pub(crate) fn hard_search_with_budget(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    budget: usize,
    deadline: Option<Instant>,
) -> SearchOutcome {
    let cancellation = SolveControl::new();
    let run = SolveRunControl {
        deadline,
        cancellation: &cancellation,
    };
    hard_search_controlled(
        request,
        resolved,
        adjacency,
        graph_distances,
        budget,
        &run,
        &[],
    )
}

pub(crate) fn hard_search_controlled(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    budget: usize,
    run: &SolveRunControl<'_>,
    excluded_assignments: &[Vec<usize>],
) -> SearchOutcome {
    let mut assignment: Vec<Option<usize>> = vec![None; request.student_count];
    for [student, seat] in &request.fixed_seats {
        assignment[*student] = Some(*seat);
    }
    let domains = match build_candidate_domains_controlled(
        request,
        resolved,
        adjacency,
        graph_distances,
        Some(run),
    ) {
        Ok(domains) => domains,
        Err(StopReason::Deadline) => return SearchOutcome::DeadlineExceeded,
        Err(StopReason::Cancelled) => return SearchOutcome::Cancelled,
    };
    let mut domains: Vec<Vec<usize>> = domains.into_iter().map(|domain| domain.seats).collect();
    let mut budget = budget;
    backtrack(
        request,
        resolved,
        adjacency,
        graph_distances,
        &mut assignment,
        &mut domains,
        &mut budget,
        run,
        excluded_assignments,
    )
}

/// One backtracking step. On success returns the complete assignment; on
/// exhaustive failure returns [`SearchOutcome::ProvenInfeasible`]; on budget
/// or deadline exhaustion [`SearchOutcome::BudgetExceeded`].
#[allow(clippy::too_many_arguments)]
fn backtrack(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    assignment: &mut [Option<usize>],
    domains: &mut [Vec<usize>],
    budget: &mut usize,
    run: &SolveRunControl<'_>,
    excluded_assignments: &[Vec<usize>],
) -> SearchOutcome {
    // A complete legal assignment is an incumbent even when cancellation or
    // the deadline becomes visible at this exact checkpoint.
    if assignment.iter().all(Option::is_some) {
        let complete: Vec<usize> = assignment
            .iter()
            .map(|seat| seat.expect("all students were checked as assigned"))
            .collect();
        if !assignment_is_excluded(&complete, excluded_assignments) {
            return SearchOutcome::Found(complete);
        }
        if let Some(reason) = run.stop_reason() {
            return match reason {
                StopReason::Deadline => SearchOutcome::DeadlineExceeded,
                StopReason::Cancelled => SearchOutcome::Cancelled,
            };
        }
        return SearchOutcome::ProvenInfeasible;
    }
    if let Some(reason) = run.stop_reason() {
        return match reason {
            StopReason::Deadline => SearchOutcome::DeadlineExceeded,
            StopReason::Cancelled => SearchOutcome::Cancelled,
        };
    }
    if *budget == 0 {
        return SearchOutcome::BudgetExceeded;
    }
    *budget -= 1;

    // MRV: the unassigned student with the smallest domain, tie-broken by
    // constraint degree (more pair rules first) then student index.
    let student = (0..request.student_count)
        .filter(|student| assignment[*student].is_none())
        .min_by_key(|student| {
            let degree = constraint_degree(resolved, request, *student);
            (domains[*student].len(), std::cmp::Reverse(degree), *student)
        })
        .expect("at least one unassigned student after the all-assigned check");

    if domains[student].is_empty() {
        return SearchOutcome::ProvenInfeasible;
    }

    for seat in domains[student].clone() {
        if let Some(reason) = run.stop_reason() {
            return match reason {
                StopReason::Deadline => SearchOutcome::DeadlineExceeded,
                StopReason::Cancelled => SearchOutcome::Cancelled,
            };
        }
        // Deterministic seat order; skip seats already taken.
        if assignment.contains(&Some(seat)) {
            continue;
        }

        // Forward checking: assign student -> seat and filter every other
        // student's domain. Any empty domain prunes this branch.
        let mut next_domains = domains.to_vec();
        let mut pruned = false;
        for other in 0..request.student_count {
            if let Some(reason) = run.stop_reason() {
                return match reason {
                    StopReason::Deadline => SearchOutcome::DeadlineExceeded,
                    StopReason::Cancelled => SearchOutcome::Cancelled,
                };
            }
            if assignment[other].is_some() || other == student {
                continue;
            }
            let mut filtered = Vec::with_capacity(next_domains[other].len());
            for candidate in next_domains[other].iter().copied() {
                if let Some(reason) = run.stop_reason() {
                    return match reason {
                        StopReason::Deadline => SearchOutcome::DeadlineExceeded,
                        StopReason::Cancelled => SearchOutcome::Cancelled,
                    };
                }
                if candidate != seat
                    && partial_pair_valid(
                        request,
                        resolved,
                        adjacency,
                        graph_distances,
                        assignment,
                        student,
                        seat,
                        other,
                        candidate,
                    )
                {
                    filtered.push(candidate);
                }
            }
            next_domains[other] = filtered;
            if next_domains[other].is_empty() {
                pruned = true;
                break;
            }
        }
        if pruned {
            continue;
        }

        assignment[student] = Some(seat);
        let result = backtrack(
            request,
            resolved,
            adjacency,
            graph_distances,
            assignment,
            &mut next_domains,
            budget,
            run,
            excluded_assignments,
        );
        assignment[student] = None;
        match result {
            SearchOutcome::Found(_) => return result,
            SearchOutcome::BudgetExceeded
            | SearchOutcome::DeadlineExceeded
            | SearchOutcome::Cancelled => return result,
            SearchOutcome::ProvenInfeasible => continue,
        }
    }

    SearchOutcome::ProvenInfeasible
}

/// Number of hard-rule pairs this student participates in (MRV tie-break).
fn constraint_degree(
    resolved: &ResolvedHardRules,
    request: &CoreSolveRequest,
    student: usize,
) -> usize {
    let pairs = resolved
        .must_be_adjacent
        .iter()
        .chain(resolved.cannot_be_adjacent.iter())
        .filter(|pair| pair[0] == student || pair[1] == student)
        .count();
    let distance = request
        .min_distance
        .iter()
        .filter(|rule| rule.students[0] == student || rule.students[1] == student)
        .count();
    pairs + distance
}

/// Is `(student -> seat, other -> candidate)` jointly legal given the current
/// partial assignment? Checks only the pairs that become fully assigned, so
/// this is the incremental forward-checking test.
#[allow(clippy::too_many_arguments)]
fn partial_pair_valid(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    assignment: &[Option<usize>],
    student: usize,
    seat: usize,
    other: usize,
    candidate: usize,
) -> bool {
    // Reuse the partial-assignment validator with a probe that contains the
    // current assignment plus the two new placements. Unassigned students
    // stay None, so only fully-assigned pairs are ever checked.
    let mut probe: Vec<Option<usize>> = assignment.to_vec();
    probe[student] = Some(seat);
    probe[other] = Some(candidate);
    solve_partial_assignment_valid(request, resolved, &probe, adjacency, graph_distances)
}

/// Find the first hard rule violated by `probe[student] = Some(seat)`.
///
/// Returns a human-readable reason naming the rule, mirroring the order the
/// partial-assignment validator checks rules in (fixed, must_be_adjacent,
/// cannot_be_adjacent, min_distance).
fn first_hard_rule_violation(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    probe: &[Option<usize>],
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    student: usize,
) -> Option<String> {
    for [student_index, seat_index] in &request.fixed_seats {
        if *student_index == student && probe[*student_index] != Some(*seat_index) {
            return Some(format!("fixed seat {seat_index} not honored"));
        }
    }
    for [first_student, second_student] in &resolved.must_be_adjacent {
        if (*first_student == student || *second_student == student)
            && probe[*first_student].is_some()
            && probe[*second_student].is_some()
            && !assigned_students_are_adjacent(probe, adjacency, *first_student, *second_student)
        {
            return Some(format!(
                "not adjacent to required partner {other}",
                other = if *first_student == student {
                    second_student
                } else {
                    first_student
                }
            ));
        }
    }
    for [first_student, second_student] in &resolved.cannot_be_adjacent {
        if (*first_student == student || *second_student == student)
            && probe[*first_student].is_some()
            && probe[*second_student].is_some()
            && assigned_students_are_adjacent(probe, adjacency, *first_student, *second_student)
        {
            return Some(format!(
                "adjacent to forbidden partner {other}",
                other = if *first_student == student {
                    second_student
                } else {
                    first_student
                }
            ));
        }
    }
    for rule in &request.min_distance {
        if (rule.students[0] == student || rule.students[1] == student)
            && probe[rule.students[0]].is_some()
            && probe[rule.students[1]].is_some()
            && !assigned_students_meet_distance(
                &request.seat_positions,
                probe,
                graph_distances,
                rule,
            )
        {
            return Some(format!(
                "too close to partner {other}",
                other = if rule.students[0] == student {
                    rule.students[1]
                } else {
                    rule.students[0]
                }
            ));
        }
    }
    None
}

pub(crate) fn validate_solve_request(request: &CoreSolveRequest) -> Result<(), String> {
    validate_solve_request_controlled(request, None).map(|_| ())
}

pub(crate) fn validate_solve_request_controlled(
    request: &CoreSolveRequest,
    run: Option<&SolveRunControl<'_>>,
) -> Result<Option<StopReason>, String> {
    if request.api_version != NATIVE_API_VERSION {
        return Err(format!(
            "unsupported native solve api_version {}; expected {}",
            request.api_version, NATIVE_API_VERSION
        ));
    }
    if request.student_count == 0 {
        return Err("native solve requires at least one student".to_string());
    }
    if request.student_count > 1_000 {
        return Err(
            "invalid student_count: native solve supports at most 1000 students".to_string(),
        );
    }
    if request
        .fixed_seats
        .len()
        .saturating_add(request.must_be_adjacent.len())
        .saturating_add(request.cannot_be_adjacent.len())
        .saturating_add(request.min_distance.len())
        > 100_000
    {
        return Err("invalid hard rules: supports at most 100000 input constraints".to_string());
    }
    if request.edges.len() > 100_000 {
        return Err("invalid adjacency graph: supports at most 100000 input edges".to_string());
    }
    if request.seat_positions.is_empty() {
        return Err("native solve requires at least one seat".to_string());
    }
    if request
        .seat_positions
        .iter()
        .flatten()
        .any(|coordinate| !coordinate.is_finite())
    {
        return Err("seat positions must contain finite numbers".to_string());
    }
    let seat_count = request.seat_positions.len();
    if request.student_count > seat_count {
        return Err("native solve cannot seat more students than available seats".to_string());
    }
    // Bound the seat domain independently of the optional graph matrix.
    // Classroom grids are far below this; graph-distance inputs have their
    // own stricter memory bound below.
    const MAX_SOLVE_SEATS: usize = 10_000;
    if seat_count > MAX_SOLVE_SEATS {
        return Err(format!(
            "native solve supports at most {MAX_SOLVE_SEATS} seats, got {seat_count}"
        ));
    }
    // Bound the quadratic structure that is required only by graph-distance
    // rules (~32 MiB at this limit). Ordinary seating may use 10,000 seats.
    if seat_count > 2_000
        && request
            .min_distance
            .iter()
            .any(|rule| matches!(rule.metric, CoreDistanceMetric::Graph))
    {
        return Err("invalid graph-distance problem: supports at most 2000 seats".to_string());
    }
    if !request.students.is_empty() && request.students.len() != request.student_count {
        return Err("students must be empty or match student_count".to_string());
    }
    if !request.students.is_empty() {
        let mut student_keys: HashSet<&str> = HashSet::new();
        for student in &request.students {
            if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
                return Ok(Some(reason));
            }
            if student.key.trim().is_empty() {
                return Err("students require non-empty keys".to_string());
            }
            if !student_keys.insert(student.key.as_str()) {
                return Err(format!("duplicate student key: {:?}", student.key));
            }
            // `student_scores` above is already finiteness-checked; the
            // richer student records must be held to the same contract, or a
            // NaN/inf score silently propagates NaN costs and percentiles
            // into the response (serializing as JSON null).
            if student
                .score
                .is_some_and(|score| !score.is_finite() || score.abs() > 1_000_000_000.0)
            {
                return Err(format!(
                    "invalid student {:?} score: must be finite and between -1000000000 and 1000000000",
                    student.key
                ));
            }
            if student
                .height_cm
                .is_some_and(|height| !height.is_finite() || !(0.0..=300.0).contains(&height))
            {
                return Err(format!(
                    "invalid student {:?} height_cm: must be finite and between 0 and 300 centimetres",
                    student.key
                ));
            }
        }
    }
    if !request.student_scores.is_empty() && request.student_scores.len() != request.student_count {
        return Err("student_scores must be empty or match student_count".to_string());
    }
    if request
        .student_scores
        .iter()
        .flatten()
        .any(|score| !score.is_finite() || score.abs() > 1_000_000_000.0)
    {
        return Err(
            "student scores must be finite and between -1000000000 and 1000000000".to_string(),
        );
    }
    if request
        .time_limit_seconds
        .is_some_and(|seconds| !seconds.is_finite() || seconds <= 0.0)
    {
        return Err(
            "invalid time_limit_seconds: expected a finite value greater than zero".to_string(),
        );
    }
    if let Some(seconds) = request.time_limit_seconds {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| "invalid time_limit_seconds: duration is out of range".to_string())?;
        Instant::now()
            .checked_add(duration)
            .ok_or_else(|| "invalid time_limit_seconds: deadline is out of range".to_string())?;
    }
    if let Some(layout) = &request.layout {
        if layout.seats.iter().filter(|seat| seat.enabled).count() != seat_count {
            return Err("enabled layout seats must match seat_positions one-to-one; every seat in the solve index domain must be enabled".to_string());
        }
        let mut seat_ids = HashSet::new();
        for seat in &layout.seats {
            if seat.seat_id.trim().is_empty() || !seat_ids.insert(seat.seat_id.as_str()) {
                return Err("layout seats must have unique non-empty seat_id values".to_string());
            }
        }
        for (index, seat) in layout.seats.iter().filter(|seat| seat.enabled).enumerate() {
            let position = request.seat_positions[index];
            if seat.x.is_some_and(|x| x != position[0]) || seat.y.is_some_and(|y| y != position[1])
            {
                return Err(format!(
                    "invalid layout seat {:?}: coordinates must match seat_positions[{index}]",
                    seat.seat_id
                ));
            }
        }
    }
    for [first_seat, second_seat] in &request.edges {
        if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
            return Ok(Some(reason));
        }
        if first_seat == second_seat || *first_seat >= seat_count || *second_seat >= seat_count {
            return Err("edges must reference two different known seats".to_string());
        }
    }
    for pair in request
        .fixed_seats
        .iter()
        .chain(request.must_be_adjacent.iter())
        .chain(request.cannot_be_adjacent.iter())
    {
        // Only the student slot (`pair[0]`) is shared across all three lists;
        // fixed_seats pairs are `[student, seat]`, so their second element is
        // validated separately below against the seat count.
        if pair[0] >= request.student_count {
            return Err("hard rules reference an unknown student".to_string());
        }
    }
    for [student_index, seat_index] in &request.fixed_seats {
        if *student_index >= request.student_count || *seat_index >= seat_count {
            return Err("fixed_seats reference an unknown student or seat".to_string());
        }
    }
    for pair in request
        .must_be_adjacent
        .iter()
        .chain(request.cannot_be_adjacent.iter())
    {
        if pair[0] >= request.student_count || pair[1] >= request.student_count {
            return Err("pair rules reference an unknown student".to_string());
        }
        if pair[0] == pair[1] {
            return Err("invalid pair rule: must reference two different students".to_string());
        }
    }
    for rule in &request.min_distance {
        if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
            return Ok(Some(reason));
        }
        if rule.students[0] >= request.student_count || rule.students[1] >= request.student_count {
            return Err("min_distance references an unknown student".to_string());
        }
        if rule.students[0] == rule.students[1] {
            return Err(
                "invalid min_distance rule: must reference two different students".to_string(),
            );
        }
        if !rule.distance.is_finite() || rule.distance <= 0.0 {
            return Err("min_distance values must be positive and finite".to_string());
        }
    }
    // Static conflict layer (plan §6.1 first layer), mirroring Python's strict
    // `compile_hard_rules` + `_validate_compiled_rule_conflicts`: duplicate
    // fixed seats and fixed seats that contradict a pair rule are caught
    // before any search runs.
    let mut fixed_by_student: HashMap<usize, usize> = HashMap::new();
    let mut fixed_by_seat: HashMap<usize, usize> = HashMap::new();
    for [student_index, seat_index] in &request.fixed_seats {
        if fixed_by_student
            .insert(*student_index, *seat_index)
            .is_some()
        {
            return Err(format!(
                "conflicting hard rules: student {student_index} is fixed to more than one seat"
            ));
        }
        if fixed_by_seat.insert(*seat_index, *student_index).is_some() {
            return Err(format!(
                "conflicting hard rules: seat {seat_index} is fixed to more than one student"
            ));
        }
    }
    let adjacency = build_index_adjacency(seat_count, &request.edges);
    let must_pairs: HashSet<[usize; 2]> = request.must_be_adjacent.iter().copied().collect();
    let cannot_pairs: HashSet<[usize; 2]> = request.cannot_be_adjacent.iter().copied().collect();
    if let Some(pair) = must_pairs.intersection(&cannot_pairs).next() {
        return Err(format!(
            "conflicting hard rules: the same student pair appears in both \
             must_be_adjacent and cannot_be_adjacent ({pair:?})"
        ));
    }
    for [first_student, second_student] in &request.must_be_adjacent {
        if let (Some(first_seat), Some(second_seat)) = (
            fixed_by_student.get(first_student),
            fixed_by_student.get(second_student),
        ) {
            if !adjacency[*first_seat].contains(second_seat) {
                return Err(
                    "conflicting hard rules: fixed seats do not satisfy a must_be_adjacent rule"
                        .to_string(),
                );
            }
        }
    }
    for [first_student, second_student] in &request.cannot_be_adjacent {
        if let (Some(first_seat), Some(second_seat)) = (
            fixed_by_student.get(first_student),
            fixed_by_student.get(second_student),
        ) {
            if adjacency[*first_seat].contains(second_seat) {
                return Err(
                    "conflicting hard rules: fixed seats violate a cannot_be_adjacent rule"
                        .to_string(),
                );
            }
        }
    }
    for rule in &request.min_distance {
        if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
            return Ok(Some(reason));
        }
        if let (Some(first_seat), Some(second_seat)) = (
            fixed_by_student.get(&rule.students[0]),
            fixed_by_student.get(&rule.students[1]),
        ) {
            let distance = match rule.metric {
                CoreDistanceMetric::Euclidean => {
                    let first = request.seat_positions[*first_seat];
                    let second = request.seat_positions[*second_seat];
                    seat_distance(first[0], first[1], second[0], second[1])
                }
                CoreDistanceMetric::Graph => graph_distance(&adjacency, *first_seat, *second_seat)
                    .map(|distance| distance as f64),
            };
            // A disconnected graph pair has no finite distance: the Python
            // oracle treats it as infinite (inf < d is false), and the
            // runtime checker `assigned_students_meet_distance` treats it as
            // satisfied. Only a measured distance below the threshold is a
            // static conflict.
            if distance.is_some_and(|value| value < rule.distance) {
                return Err(
                    "conflicting hard rules: fixed seats violate a min_distance rule".to_string(),
                );
            }
        }
    }
    // Resolving groups surfaces unknown-member references, mirroring strict
    // rule compilation; the derived pairs are validated by the caller.
    // Soft-weight gate: negative weights are a documented input error and
    // oversized weights overflow the integer cost arithmetic downstream.
    if let Some(rules) = &request.rules {
        const MAX_SOFT_RULE_WEIGHT: i32 = 1_000_000;
        let soft = &rules.soft;
        let weighted_rules = [
            (
                "vision_front",
                soft.vision_front.enabled,
                soft.vision_front.weight,
            ),
            (
                "height_back",
                soft.height_back.enabled,
                soft.height_back.weight,
            ),
            ("randomize", soft.randomize.enabled, soft.randomize.weight),
            (
                "score_balance",
                soft.score_balance.enabled,
                soft.score_balance.weight,
            ),
            (
                "score_position",
                soft.score_position.enabled,
                soft.score_position.weight,
            ),
            (
                "score_distribution",
                soft.score_distribution.enabled,
                soft.score_distribution.weight,
            ),
            (
                "mentor_pairing",
                soft.mentor_pairing.enabled,
                soft.mentor_pairing.weight,
            ),
            (
                "fair_rotation",
                soft.fair_rotation.enabled,
                soft.fair_rotation.weight,
            ),
            (
                "avoid_recent_neighbors",
                soft.avoid_recent_neighbors.enabled,
                soft.avoid_recent_neighbors.weight,
            ),
            ("cooling", soft.cooling.enabled, soft.cooling.weight),
        ];
        for (name, enabled, weight) in weighted_rules {
            if !enabled {
                continue;
            }
            if weight < 0 {
                return Err(format!(
                    "invalid soft rule {name}: weight must be a non-negative integer, \
                     got {weight}"
                ));
            }
            if weight > MAX_SOFT_RULE_WEIGHT {
                return Err(format!(
                    "invalid soft rule {name}: weight {weight} exceeds the supported \
                     maximum of {MAX_SOFT_RULE_WEIGHT}"
                ));
            }
        }
        let mentor = &soft.mentor_pairing;
        if mentor.enabled
            && (!mentor.mentor_percentile.is_finite()
                || !mentor.learner_percentile.is_finite()
                || !(0.0..=1.0).contains(&mentor.mentor_percentile)
                || !(0.0..=1.0).contains(&mentor.learner_percentile))
        {
            return Err(
                "invalid mentor_pairing percentiles: must be finite values between 0 and 1"
                    .to_string(),
            );
        }
        for (name, lookback) in [
            ("fair_rotation", soft.fair_rotation.lookback),
            (
                "avoid_recent_neighbors",
                soft.avoid_recent_neighbors.lookback,
            ),
            ("mentor_pairing", Some(mentor.history_lookback)),
        ] {
            if lookback.is_some_and(|value| value < 0) {
                return Err(format!("invalid {name} lookback: must be non-negative"));
            }
        }
        if soft.avoid_recent_neighbors.max_recent_count < 0 {
            return Err(
                "invalid avoid_recent_neighbors max_recent_count: must be non-negative".to_string(),
            );
        }
        if soft.cooling.enabled && soft.cooling.weight > 0 && soft.cooling.cooling_period <= 0 {
            return Err("invalid cooling_period: must be positive".to_string());
        }
        const RELATIONS: [&str; 6] = [
            "desk_mate",
            "horizontal",
            "vertical",
            "diagonal",
            "adjacent_any",
            "within_distance",
        ];
        for (name, relations) in [
            (
                "avoid_recent_neighbors",
                &soft.avoid_recent_neighbors.relation_types,
            ),
            ("cooling", &soft.cooling.relation_types),
        ] {
            if let Some(relation) = relations
                .iter()
                .find(|relation| !RELATIONS.contains(&relation.as_str()))
            {
                return Err(format!("unrecognized {name} relation type: {relation:?}"));
            }
        }
    }
    if let Some(history) = &request.history {
        if history.history_count < 0
            || history
                .students
                .values()
                .any(|student| student.category_counts.values().any(|count| *count < 0))
        {
            return Err("invalid seat history: counts must be non-negative".to_string());
        }
        for student in history.students.values() {
            let indexed = student
                .records
                .iter()
                .any(|record| record.period_index.is_some());
            let mut previous = 0;
            for record in &student.records {
                if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
                    return Ok(Some(reason));
                }
                if indexed {
                    let index = record.period_index.ok_or_else(|| "invalid seat history: cannot mix indexed periods and legacy occurrence records for one student".to_string())?;
                    if index <= previous || index > history.history_count {
                        return Err("invalid seat history: period_index must increase within 1..=history_count".to_string());
                    }
                    previous = index;
                }
            }
        }
    }
    if let Some(history) = &request.pair_history {
        if history.history_count < 0 {
            return Err("invalid pair history: history_count must be non-negative".to_string());
        }
        for pair in history.pairs.values() {
            if let Some(reason) = run.and_then(SolveRunControl::stop_reason) {
                return Ok(Some(reason));
            }
            let mut previous = 0;
            let indexed = pair
                .records
                .iter()
                .any(|record| record.period_index.is_some());
            for record in &pair.records {
                if indexed {
                    let index = record.period_index.ok_or_else(|| "invalid pair history: cannot mix indexed periods and legacy occurrence records in one pair".to_string())?;
                    if index <= previous || index > history.history_count {
                        return Err("invalid pair history: period_index must increase within 1..=history_count".to_string());
                    }
                    previous = index;
                }
            }
        }
    }
    if resolve_group_rules_controlled(request, run)?.is_none() {
        return Ok(run.and_then(SolveRunControl::stop_reason));
    }
    Ok(None)
}

fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for index in (1..items.len()).rev() {
        let swap_with = rng.next_usize(index + 1);
        items.swap(index, swap_with);
    }
}
