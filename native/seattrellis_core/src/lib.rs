pub mod cost;
pub mod models;
pub mod objectives;
pub mod rng;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::cost::{avoid_recent_neighbors_cost, individual_cost, normalize_edge};
use crate::models::{
    effective_neighbor_rule, Layout, PairHistory, RuleSet, Seat, SeatHistory, Student,
};
use crate::objectives::{compile_soft_objectives, evaluate_soft_objectives, SoftObjectiveContext};
use crate::rng::SplitMix64;

pub const NATIVE_API_VERSION: u32 = 2;

#[derive(Debug, Deserialize)]
pub struct CoreEvaluationRequest {
    pub api_version: u32,
    pub student_count: usize,
    pub seat_positions: Vec<[f64; 2]>,
    #[serde(default)]
    pub edges: Vec<[usize; 2]>,
    pub assignments: Vec<[usize; 2]>,
    #[serde(default)]
    pub fixed_seats: Vec<[usize; 2]>,
    #[serde(default)]
    pub must_be_adjacent: Vec<[usize; 2]>,
    #[serde(default)]
    pub cannot_be_adjacent: Vec<[usize; 2]>,
    #[serde(default)]
    pub min_distance: Vec<CoreMinDistanceRule>,
    #[serde(default)]
    pub student_scores: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct CoreMinDistanceRule {
    pub students: [usize; 2],
    pub distance: f64,
    pub metric: CoreDistanceMetric,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreDistanceMetric {
    Euclidean,
    Graph,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CoreEvaluationResponse {
    pub api_version: u32,
    pub assignment_unique: bool,
    pub hard_constraints_satisfied: bool,
    pub checked_rule_count: usize,
    pub violation_count: usize,
    pub violation_codes: Vec<String>,
    pub graph_distance_matrix: Vec<Vec<Option<u32>>>,
    pub peer_mixing_gap_sum: f64,
    pub peer_mixing_pair_count: usize,
    pub peer_mixing_mean_gap: Option<f64>,
}

pub fn assignment_is_unique(
    student_count: usize,
    seat_count: usize,
    assignments: &[(usize, usize)],
) -> bool {
    if assignments.len() != student_count {
        return false;
    }
    let mut seen_students = vec![false; student_count];
    let mut seen_seats = vec![false; seat_count];
    for &(student_index, seat_index) in assignments {
        if student_index >= student_count || seat_index >= seat_count {
            return false;
        }
        if seen_students[student_index] || seen_seats[seat_index] {
            return false;
        }
        seen_students[student_index] = true;
        seen_seats[seat_index] = true;
    }
    seen_students.into_iter().all(|seen| seen)
}

pub fn seat_distance(first_x: f64, first_y: f64, second_x: f64, second_y: f64) -> Option<f64> {
    if !(first_x.is_finite() && first_y.is_finite() && second_x.is_finite() && second_y.is_finite())
    {
        return None;
    }
    let x_delta = first_x - second_x;
    let y_delta = first_y - second_y;
    Some((x_delta * x_delta + y_delta * y_delta).sqrt())
}

pub fn evaluate_problem_json(request_json: &str) -> Result<String, String> {
    let request: CoreEvaluationRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native evaluation request: {error}"))?;
    let response = evaluate_problem(&request)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize native evaluation response: {error}"))
}

pub fn evaluate_problem(request: &CoreEvaluationRequest) -> Result<CoreEvaluationResponse, String> {
    validate_request(request)?;
    let seat_count = request.seat_positions.len();
    let adjacency = build_index_adjacency(seat_count, &request.edges);
    let graph_distance_matrix = build_graph_distance_matrix(&adjacency);
    let assignments: Vec<(usize, usize)> = request
        .assignments
        .iter()
        .map(|pair| (pair[0], pair[1]))
        .collect();
    let assignment_unique = assignment_is_unique(request.student_count, seat_count, &assignments);
    let assignment_by_student =
        assignment_by_student(request.student_count, seat_count, &assignments);
    let mut violation_codes = Vec::new();
    if !assignment_unique {
        violation_codes.push("assignment_not_unique".to_string());
    }
    for [student_index, seat_index] in &request.fixed_seats {
        if assignment_by_student[*student_index] != Some(*seat_index) {
            violation_codes.push("fixed_seat".to_string());
        }
    }
    for [first_student, second_student] in &request.must_be_adjacent {
        if !assigned_students_are_adjacent(
            &assignment_by_student,
            &adjacency,
            *first_student,
            *second_student,
        ) {
            violation_codes.push("must_be_adjacent".to_string());
        }
    }
    for [first_student, second_student] in &request.cannot_be_adjacent {
        if assigned_students_are_adjacent(
            &assignment_by_student,
            &adjacency,
            *first_student,
            *second_student,
        ) {
            violation_codes.push("cannot_be_adjacent".to_string());
        }
    }
    for rule in &request.min_distance {
        if !assigned_students_meet_distance(
            &request.seat_positions,
            &assignment_by_student,
            &graph_distance_matrix,
            rule,
        ) {
            violation_codes.push("min_distance".to_string());
        }
    }
    let (peer_mixing_gap_sum, peer_mixing_pair_count) = peer_mixing_score(request, &assignments);
    let peer_mixing_mean_gap =
        (peer_mixing_pair_count > 0).then_some(peer_mixing_gap_sum / peer_mixing_pair_count as f64);
    let checked_rule_count = 3
        + request.fixed_seats.len()
        + request.must_be_adjacent.len()
        + request.cannot_be_adjacent.len()
        + request.min_distance.len();
    Ok(CoreEvaluationResponse {
        api_version: NATIVE_API_VERSION,
        assignment_unique,
        hard_constraints_satisfied: violation_codes.is_empty(),
        checked_rule_count,
        violation_count: violation_codes.len(),
        violation_codes,
        graph_distance_matrix,
        peer_mixing_gap_sum,
        peer_mixing_pair_count,
        peer_mixing_mean_gap,
    })
}

fn validate_request(request: &CoreEvaluationRequest) -> Result<(), String> {
    if request.api_version != NATIVE_API_VERSION {
        return Err(format!(
            "unsupported native evaluation api_version {}; expected {}",
            request.api_version, NATIVE_API_VERSION
        ));
    }
    if request.seat_positions.is_empty() {
        return Err("native evaluation requires at least one seat".to_string());
    }
    if request
        .seat_positions
        .iter()
        .flatten()
        .any(|coordinate| !coordinate.is_finite())
    {
        return Err("seat positions must contain finite numbers".to_string());
    }
    if !request.student_scores.is_empty() && request.student_scores.len() != request.student_count {
        return Err("student_scores must be empty or match student_count".to_string());
    }
    if request
        .student_scores
        .iter()
        .flatten()
        .any(|score| !score.is_finite())
    {
        return Err("student scores must be finite numbers".to_string());
    }
    let seat_count = request.seat_positions.len();
    for [first_seat, second_seat] in &request.edges {
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
    }
    for rule in &request.min_distance {
        if rule.students[0] >= request.student_count || rule.students[1] >= request.student_count {
            return Err("min_distance references an unknown student".to_string());
        }
        if !rule.distance.is_finite() || rule.distance <= 0.0 {
            return Err("min_distance values must be positive and finite".to_string());
        }
    }
    Ok(())
}

fn build_index_adjacency(seat_count: usize, edges: &[[usize; 2]]) -> Vec<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); seat_count];
    for [first_seat, second_seat] in edges {
        if !adjacency[*first_seat].contains(second_seat) {
            adjacency[*first_seat].push(*second_seat);
        }
        if !adjacency[*second_seat].contains(first_seat) {
            adjacency[*second_seat].push(*first_seat);
        }
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    adjacency
}

fn build_graph_distance_matrix(adjacency: &[Vec<usize>]) -> Vec<Vec<Option<u32>>> {
    let mut matrix = Vec::with_capacity(adjacency.len());
    for source_index in 0..adjacency.len() {
        let mut distances = vec![None; adjacency.len()];
        distances[source_index] = Some(0);
        let mut queue = VecDeque::from([source_index]);
        while let Some(seat_index) = queue.pop_front() {
            let next_distance = distances[seat_index].unwrap_or(0) + 1;
            for neighbor_index in &adjacency[seat_index] {
                if distances[*neighbor_index].is_none() {
                    distances[*neighbor_index] = Some(next_distance);
                    queue.push_back(*neighbor_index);
                }
            }
        }
        matrix.push(distances);
    }
    matrix
}

fn assignment_by_student(
    student_count: usize,
    seat_count: usize,
    assignments: &[(usize, usize)],
) -> Vec<Option<usize>> {
    let mut assignment_by_student = vec![None; student_count];
    for (student_index, seat_index) in assignments {
        if *student_index < student_count && *seat_index < seat_count {
            assignment_by_student[*student_index] = Some(*seat_index);
        }
    }
    assignment_by_student
}

fn assigned_students_are_adjacent(
    assignment_by_student: &[Option<usize>],
    adjacency: &[Vec<usize>],
    first_student: usize,
    second_student: usize,
) -> bool {
    match (
        assignment_by_student[first_student],
        assignment_by_student[second_student],
    ) {
        (Some(first_seat), Some(second_seat)) => adjacency[first_seat].contains(&second_seat),
        _ => false,
    }
}

fn assigned_students_meet_distance(
    seat_positions: &[[f64; 2]],
    assignment_by_student: &[Option<usize>],
    graph_distances: &[Vec<Option<u32>>],
    rule: &CoreMinDistanceRule,
) -> bool {
    let (Some(first_seat), Some(second_seat)) = (
        assignment_by_student[rule.students[0]],
        assignment_by_student[rule.students[1]],
    ) else {
        return false;
    };
    let distance = match rule.metric {
        CoreDistanceMetric::Euclidean => {
            let first = seat_positions[first_seat];
            let second = seat_positions[second_seat];
            seat_distance(first[0], first[1], second[0], second[1])
        }
        CoreDistanceMetric::Graph => {
            graph_distances[first_seat][second_seat].map(|distance| distance as f64)
        }
    };
    distance.is_none_or(|value| value >= rule.distance)
}

fn peer_mixing_score(
    request: &CoreEvaluationRequest,
    assignments: &[(usize, usize)],
) -> (f64, usize) {
    if request.student_scores.len() != request.student_count {
        return (0.0, 0);
    }
    let mut occupant_by_seat = vec![None; request.seat_positions.len()];
    for (student_index, seat_index) in assignments {
        if *student_index < request.student_count && *seat_index < occupant_by_seat.len() {
            occupant_by_seat[*seat_index] = Some(*student_index);
        }
    }
    let mut gap_sum = 0.0;
    let mut pair_count = 0;
    for [first_seat, second_seat] in &request.edges {
        let (Some(first_student), Some(second_student)) = (
            occupant_by_seat[*first_seat],
            occupant_by_seat[*second_seat],
        ) else {
            continue;
        };
        let (Some(first_score), Some(second_score)) = (
            request.student_scores[first_student],
            request.student_scores[second_student],
        ) else {
            continue;
        };
        gap_sum += (first_score - second_score).abs();
        pair_count += 1;
    }
    (gap_sum, pair_count)
}

// ---------------------------------------------------------------------------
// Cost-ranked greedy seat generator.
//
// This is the solver piece ported from the Python fallback backend
// (`solver/fallback_backend.py`). It produces a complete,
// hard-constraint-satisfying assignment (or reports infeasibility) using a
// most-constrained-first greedy where each candidate seat is ranked by the
// same cost formula Python uses (`_fallback_candidate_cost`):
//
//     individual_cost + avoid_recent_neighbors_cost + soft-objective cost
//
// Attempt 0 picks the cheapest seat; later attempts pick randomly among the
// top-3 cheapest (mirroring `rng.choice(candidates[:min(3, len)])`). The
// full solution is scored with `_fallback_total_cost` semantics and reported
// as `CoreSolveResponse.total_cost`.
//
// All cost inputs on the request are optional and degrade to "no data" when
// absent: an empty student list produces placeholder students (no vision /
// height / score), missing rules fall back to the pydantic defaults, and a
// missing layout is derived from the grid `seat_positions`.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CoreSolveRequest {
    pub api_version: u32,
    pub student_count: usize,
    pub seat_positions: Vec<[f64; 2]>,
    #[serde(default)]
    pub edges: Vec<[usize; 2]>,
    #[serde(default)]
    pub fixed_seats: Vec<[usize; 2]>,
    #[serde(default)]
    pub must_be_adjacent: Vec<[usize; 2]>,
    #[serde(default)]
    pub cannot_be_adjacent: Vec<[usize; 2]>,
    #[serde(default)]
    pub min_distance: Vec<CoreMinDistanceRule>,
    #[serde(default)]
    pub seed: u64,
    // ---- cost-ranking data (all optional; degrade when absent) ----
    /// Full student records, indexed by student. Empty => placeholder students.
    #[serde(default)]
    pub students: Vec<Student>,
    /// Score-only view used when `students` is empty.
    #[serde(default)]
    pub student_scores: Vec<Option<f64>>,
    /// Soft rules driving the cost functions. Missing => pydantic defaults.
    #[serde(default)]
    pub rules: Option<RuleSet>,
    /// Seat records aligned with `seat_positions`. Missing => derived grid.
    #[serde(default)]
    pub layout: Option<Layout>,
    /// Student seat history for the fair-rotation cost.
    #[serde(default)]
    pub history: Option<SeatHistory>,
    /// Pair history for the recent-neighbor cost and mentor pairing.
    #[serde(default)]
    pub pair_history: Option<PairHistory>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoreSolveResponse {
    pub api_version: u32,
    pub feasible: bool,
    /// ``[student_index, seat_index]`` pairs; empty when infeasible.
    pub assignment: Vec<[usize; 2]>,
    pub attempts_used: usize,
    pub hard_constraints_satisfied: bool,
    /// Mirrors `_fallback_total_cost`; `None` when infeasible.
    #[serde(default)]
    pub total_cost: Option<f64>,
}

/// Everything the cost-ranked greedy needs to score candidate seats and full
/// solutions. Built once per solve.
struct CostContext {
    students: Vec<Student>,
    layout: Layout,
    rules: RuleSet,
    history: Option<SeatHistory>,
    pair_history: Option<PairHistory>,
    adjacency_edges: HashSet<(String, String)>,
    objective_context: SoftObjectiveContext,
    min_row: i32,
    max_row: i32,
}

pub fn solve_problem(request: &CoreSolveRequest) -> Result<CoreSolveResponse, String> {
    validate_solve_request(request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    let attempts = (request.student_count * 12).max(40);
    let mut rng = SplitMix64::new(request.seed);
    let ctx = build_cost_context(request);

    // Mirror the Python fallback: attempt 0 seats each student on the cheapest
    // seat, later attempts sample randomly among the top-3 cheap seats, and the
    // lowest-total-cost complete assignment across every attempt wins.
    let mut best: Option<(Vec<usize>, f64, usize)> = None;
    for attempt in 0..attempts {
        if let Some(assignment) =
            greedy_attempt(request, &adjacency, &graph_distances, &mut rng, &ctx, attempt)
        {
            let total_cost = full_solution_total_cost(&assignment, &adjacency, &ctx);
            if best
                .as_ref()
                .is_none_or(|(_, best_cost, _)| total_cost < *best_cost)
            {
                best = Some((assignment, total_cost, attempt + 1));
            }
        }
    }

    if let Some((assignment, total_cost, attempts_used)) = best {
        let pairs: Vec<[usize; 2]> = assignment
            .iter()
            .enumerate()
            .map(|(student, seat)| [student, *seat])
            .collect();
        return Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: true,
            assignment: pairs,
            attempts_used,
            hard_constraints_satisfied: true,
            total_cost: Some(total_cost),
        });
    }

    Ok(CoreSolveResponse {
        api_version: NATIVE_API_VERSION,
        feasible: false,
        assignment: Vec::new(),
        attempts_used: attempts,
        hard_constraints_satisfied: false,
        total_cost: None,
    })
}

pub fn solve_problem_json(request_json: &str) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    let response = solve_problem(&request)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize native solve response: {error}"))
}

/// Build the cost context for a solve request, degrading gracefully when the
/// optional cost data is absent.
fn build_cost_context(request: &CoreSolveRequest) -> CostContext {
    let students = effective_students(request);
    let layout = effective_layout(request);
    let rules = request.rules.clone().unwrap_or_default();
    let adjacency_edges = adjacency_edges_by_seat_id(&layout, &request.edges);
    let objective_context =
        compile_soft_objectives(&students, &layout, &rules, request.pair_history.as_ref());
    let enabled_seats = layout.enabled_seats();
    let min_row = enabled_seats.iter().map(|seat| seat.row).min().unwrap_or(1);
    let max_row = enabled_seats.iter().map(|seat| seat.row).max().unwrap_or(1);
    CostContext {
        students,
        layout,
        rules,
        history: request.history.clone(),
        pair_history: request.pair_history.clone(),
        adjacency_edges,
        objective_context,
        min_row,
        max_row,
    }
}

/// Placeholder students when the request carries no records: index-derived keys
/// with scores taken from `student_scores` when provided.
fn effective_students(request: &CoreSolveRequest) -> Vec<Student> {
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
fn effective_layout(request: &CoreSolveRequest) -> Layout {
    if let Some(layout) = &request.layout {
        return layout.clone();
    }
    let seats: Vec<Seat> = request
        .seat_positions
        .iter()
        .enumerate()
        .map(|(index, position)| Seat {
            seat_id: format!("seat_{}", index),
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
fn adjacency_edges_by_seat_id(
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

    let neighbor_rule = effective_neighbor_rule(&ctx.rules);
    if neighbor_rule.enabled && neighbor_rule.weight != 0 {
        for (assigned_index, assigned_seat_index) in assignment.iter().enumerate() {
            if let Some(assigned_seat_index) = assigned_seat_index {
                cost += avoid_recent_neighbors_cost(
                    &student.key,
                    &ctx.students[assigned_index].key,
                    seat,
                    &ctx.layout.seats[*assigned_seat_index],
                    &ctx.layout,
                    &neighbor_rule,
                    ctx.pair_history.as_ref(),
                    Some(&ctx.adjacency_edges),
                ) as f64;
            }
        }
    }

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
    cost += evaluate_soft_objectives(&prospective, &ctx.objective_context, &ctx.rules).total_cost();
    cost
}

/// Total cost of a complete assignment, mirroring `_fallback_total_cost`:
/// individual costs (with one seeded RNG), the score-balance adjacent reward,
/// the recent-neighbor penalty over every pair, and the soft-objective cost.
fn full_solution_total_cost(
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

    let neighbor_rule = effective_neighbor_rule(&ctx.rules);
    if neighbor_rule.enabled && neighbor_rule.weight != 0 {
        for first_index in 0..assignment.len() {
            for second_index in (first_index + 1)..assignment.len() {
                cost += avoid_recent_neighbors_cost(
                    &ctx.students[first_index].key,
                    &ctx.students[second_index].key,
                    &ctx.layout.seats[assignment[first_index]],
                    &ctx.layout.seats[assignment[second_index]],
                    &ctx.layout,
                    &neighbor_rule,
                    ctx.pair_history.as_ref(),
                    Some(&ctx.adjacency_edges),
                ) as f64;
            }
        }
    }

    let mut assignment_by_key: HashMap<String, String> = HashMap::new();
    for (student_index, seat_index) in assignment.iter().enumerate() {
        assignment_by_key.insert(
            ctx.students[student_index].key.clone(),
            ctx.layout.seats[*seat_index].seat_id.clone(),
        );
    }
    cost += evaluate_soft_objectives(&assignment_by_key, &ctx.objective_context, &ctx.rules)
        .total_cost();
    cost
}

fn greedy_attempt(
    request: &CoreSolveRequest,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    rng: &mut SplitMix64,
    ctx: &CostContext,
    attempt: usize,
) -> Option<Vec<usize>> {
    let student_count = request.student_count;
    let seat_count = request.seat_positions.len();
    let mut assignment: Vec<Option<usize>> = vec![None; student_count];
    let mut used: Vec<bool> = vec![false; seat_count];
    let mut order: Vec<usize> = (0..student_count).collect();
    shuffle(&mut order, rng);

    loop {
        // Pick the unassigned student with the fewest valid candidate seats.
        let mut best: Option<(usize, Vec<usize>)> = None;
        for &student in &order {
            if assignment[student].is_some() {
                continue;
            }
            let candidates = valid_candidate_seats(
                request,
                &mut assignment,
                &used,
                adjacency,
                graph_distances,
                student,
            );
            if candidates.is_empty() {
                return None;
            }
            if best
                .as_ref()
                .is_none_or(|(_, existing)| candidates.len() < existing.len())
            {
                best = Some((student, candidates));
            }
        }
        let (student, candidates) = best?;

        // Rank candidates by cost; attempt 0 takes the cheapest, later attempts
        // sample uniformly from the top-3 (mirrors Python `rng.choice`).
        let mut ranked: Vec<(f64, usize)> = candidates
            .iter()
            .map(|seat| (candidate_ranking_cost(student, *seat, &assignment, ctx), *seat))
            .collect();
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
            return Some(assignment.into_iter().map(|seat| seat.unwrap()).collect());
        }
    }
}

fn valid_candidate_seats(
    request: &CoreSolveRequest,
    assignment: &mut [Option<usize>],
    used: &[bool],
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    student: usize,
) -> Vec<usize> {
    let fixed = request
        .fixed_seats
        .iter()
        .find(|pair| pair[0] == student)
        .map(|pair| pair[1]);
    let mut candidates = Vec::new();
    for (seat, &is_used) in used.iter().enumerate() {
        if is_used {
            continue;
        }
        if let Some(fixed_seat) = fixed {
            if seat != fixed_seat {
                continue;
            }
        }
        assignment[student] = Some(seat);
        let ok = solve_partial_assignment_valid(request, assignment, adjacency, graph_distances);
        assignment[student] = None;
        if ok {
            candidates.push(seat);
        }
    }
    candidates
}

fn solve_partial_assignment_valid(
    request: &CoreSolveRequest,
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
    for [first_student, second_student] in &request.must_be_adjacent {
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
    for [first_student, second_student] in &request.cannot_be_adjacent {
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

fn validate_solve_request(request: &CoreSolveRequest) -> Result<(), String> {
    if request.api_version != NATIVE_API_VERSION {
        return Err(format!(
            "unsupported native solve api_version {}; expected {}",
            request.api_version, NATIVE_API_VERSION
        ));
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
    if !request.students.is_empty() && request.students.len() != request.student_count {
        return Err("students must be empty or match student_count".to_string());
    }
    if !request.student_scores.is_empty() && request.student_scores.len() != request.student_count {
        return Err("student_scores must be empty or match student_count".to_string());
    }
    if request
        .student_scores
        .iter()
        .flatten()
        .any(|score| !score.is_finite())
    {
        return Err("student scores must be finite numbers".to_string());
    }
    if let Some(layout) = &request.layout {
        if layout.seats.len() < seat_count {
            return Err("layout must describe at least as many seats as seat_positions".to_string());
        }
    }
    for [first_seat, second_seat] in &request.edges {
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
        if pair[0] >= request.student_count || pair[1] >= request.student_count {
            return Err("hard rules reference an unknown student".to_string());
        }
    }
    for [_student_index, seat_index] in &request.fixed_seats {
        if *seat_index >= seat_count {
            return Err("fixed_seats reference an unknown seat".to_string());
        }
    }
    for rule in &request.min_distance {
        if rule.students[0] >= request.student_count || rule.students[1] >= request.student_count {
            return Err("min_distance references an unknown student".to_string());
        }
        if !rule.distance.is_finite() || rule.distance <= 0.0 {
            return Err("min_distance values must be positive and finite".to_string());
        }
    }
    Ok(())
}

fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for index in (1..items.len()).rev() {
        let swap_with = rng.next_usize(index + 1);
        items.swap(index, swap_with);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        assignment_is_unique, evaluate_problem_json, seat_distance, solve_problem_json,
        CoreEvaluationResponse, CoreSolveResponse, NATIVE_API_VERSION,
    };

    #[test]
    fn exposes_expected_native_api_version() {
        assert_eq!(NATIVE_API_VERSION, 2);
    }

    #[test]
    fn accepts_complete_unique_assignment() {
        let assignments = vec![(0, 1), (1, 0), (2, 2)];
        assert!(assignment_is_unique(3, 3, &assignments));
    }

    #[test]
    fn rejects_duplicate_student_or_seat() {
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (0, 1)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (1, 0)]));
    }

    #[test]
    fn rejects_missing_or_out_of_bounds_assignment() {
        assert!(!assignment_is_unique(2, 2, &[(0, 0)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (2, 1)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (1, 2)]));
    }

    #[test]
    fn computes_euclidean_distance() {
        assert_eq!(seat_distance(1.0, 1.0, 4.0, 5.0), Some(5.0));
        assert_eq!(seat_distance(f64::NAN, 1.0, 4.0, 5.0), None);
    }

    #[test]
    fn evaluates_versioned_problem_dto() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "assignments": [[0, 0], [1, 1], [2, 2]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]],
            "cannot_be_adjacent": [[0, 2]],
            "min_distance": [
                {"students": [0, 2], "distance": 2.0, "metric": "graph"}
            ],
            "student_scores": [90.0, 60.0, 30.0]
        }"#;

        let response_json = evaluate_problem_json(request).expect("request should be valid");
        let response: CoreEvaluationResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.assignment_unique);
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.checked_rule_count, 7);
        assert_eq!(response.violation_count, 0);
        assert_eq!(response.graph_distance_matrix[0][2], Some(2));
        assert_eq!(response.peer_mixing_gap_sum, 60.0);
        assert_eq!(response.peer_mixing_pair_count, 2);
        assert_eq!(response.peer_mixing_mean_gap, Some(30.0));
    }

    #[test]
    fn reports_hard_rule_violations_without_identity_data() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "assignments": [[0, 0], [1, 1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;

        let response_json = evaluate_problem_json(request).expect("request should be valid");
        let response: CoreEvaluationResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.hard_constraints_satisfied);
        assert_eq!(response.violation_codes, vec!["cannot_be_adjacent"]);
    }

    #[test]
    fn rejects_incompatible_problem_dto_versions() {
        let request = r#"{
            "api_version": 3,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "assignments": [[0, 0]]
        }"#;

        let error = evaluate_problem_json(request).expect_err("version should be rejected");
        assert!(error.contains("expected 2"));
    }

    #[test]
    fn solves_a_simple_feasible_class() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "seed": 7
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.assignment.len(), 3);
        let mut seats = response.assignment.iter().map(|pair| pair[1]).collect::<Vec<_>>();
        seats.sort_unstable();
        assert_eq!(seats, vec![0, 1, 2]);
    }

    #[test]
    fn respects_fixed_seats() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[1, 2]],
            "seed": 3
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        let seat_of_one = response
            .assignment
            .iter()
            .find(|pair| pair[0] == 1)
            .map(|pair| pair[1]);
        assert_eq!(seat_of_one, Some(2));
    }

    #[test]
    fn places_must_be_adjacent_students_near_each_other() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "must_be_adjacent": [[0, 1]],
            "seed": 5
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        let seat_of = |student: usize| {
            response
                .assignment
                .iter()
                .find(|pair| pair[0] == student)
                .map(|pair| pair[1])
                .unwrap()
        };
        let (first, second) = (seat_of(0), seat_of(1));
        assert!((first as isize - second as isize).unsigned_abs() == 1);
    }

    #[test]
    fn reports_infeasible_when_no_placement_satisfies_hard_rules() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]],
            "seed": 1
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert!(response.assignment.is_empty());
        assert!(!response.hard_constraints_satisfied);
    }

    #[test]
    fn rejects_too_many_students_for_the_room() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]]
        }"#;

        let error = solve_problem_json(request).expect_err("capacity should be rejected");
        assert!(error.contains("cannot seat more students"));
    }

    // -----------------------------------------------------------------------
    // Cost-ranked greedy: the ranking must prefer the cheaper seat, and the
    // response must carry the new total_cost field.
    // -----------------------------------------------------------------------

    /// Two students, two seats in different rows. Student 0 has poor vision
    /// (needs the front), student 1 is short (no height penalty). With
    /// vision_front enabled and randomize disabled, cost ranking must seat
    /// student 0 in the front row regardless of greedy placement order.
    #[test]
    fn cost_ranking_prefers_cheaper_front_seat_for_vision_student() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 6.0]],
            "seed": 0,
            "students": [
                {"key": "STU001", "vision": "poor", "height_cm": null, "tags": [], "needs": []},
                {"key": "STU002", "vision": null, "height_cm": 150.0, "tags": [], "needs": []}
            ],
            "layout": {
                "layout_id": "t",
                "name": "T",
                "seats": [
                    {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
                    {"seat_id": "R6C1", "row": 6, "col": 1, "enabled": true}
                ]
            },
            "rules": {
                "seed": 0,
                "soft": {
                    "vision_front": {"enabled": true, "weight": 20},
                    "height_back": {"enabled": true, "weight": 1},
                    "randomize": {"enabled": false, "weight": 1},
                    "score_balance": {"enabled": false, "weight": 1},
                    "fair_rotation": {"enabled": false, "weight": 10},
                    "avoid_recent_neighbors": {"enabled": false, "weight": 10}
                }
            }
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        assert!(response.hard_constraints_satisfied);
        let seat_of_student0 = response
            .assignment
            .iter()
            .find(|pair| pair[0] == 0)
            .map(|pair| pair[1])
            .expect("student 0 is assigned");
        // Student 0 (vision "poor") must be seated in the front row, seat 0.
        assert_eq!(seat_of_student0, 0);
        assert!(response.total_cost.is_some());
    }

    /// The new `total_cost` field must serialize: present and finite for a
    /// feasible solve, `null` for an infeasible one.
    #[test]
    fn solve_response_serializes_total_cost() {
        let feasible_request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]],
            "seed": 0
        }"#;
        let response_json = solve_problem_json(feasible_request).expect("request should be valid");
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        let total_cost = value.get("total_cost").expect("total_cost is serialized");
        assert!(total_cost.as_f64().is_some(), "feasible solve reports a number");

        let infeasible_request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]],
            "seed": 1
        }"#;
        let response_json = solve_problem_json(infeasible_request).expect("request should be valid");
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert!(value.get("total_cost").unwrap_or(&serde_json::Value::Null).is_null());
    }

    /// Cross-check against the frozen 40-student parity reference: the native
    /// solver must report feasible=true and the returned assignment must pass
    /// the native hard-constraint evaluator. Python's reference cost is
    /// recorded (59975.0) for comparison; exact agreement is not required.
    ///
    /// Ignored by default because it runs the full 480-attempt cost-ranked
    /// solve in debug mode (~9s). Run explicitly with
    /// `cargo test -p seattrellis_core -- --ignored`.
    #[test]
    #[ignore = "runs the full 480-attempt cost-ranked solve (~9s); opt-in"]
    fn solves_forty_parity_reference_feasibly() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/reference/40-parity.json");
        let payload_text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read 40-parity.json at {}: {error}", path.display()));
        let payload: serde_json::Value = serde_json::from_str(&payload_text)
            .expect("reference payload should be valid JSON");
        let problem = payload.get("problem").expect("reference has a problem block");
        let problem_json = serde_json::to_string(problem).expect("problem block serializes");

        let response_json = solve_problem_json(&problem_json).expect("native solve should run");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible, "40-person parity problem must be feasible");
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.assignment.len(), 40);
        let total_cost = response.total_cost.expect("feasible solve reports total_cost");
        assert!(total_cost.is_finite());

        // Feed the same problem plus the solved assignment to the native
        // hard-constraint evaluator for an independent verification.
        let mut eval_request: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&problem_json).expect("problem is an object");
        eval_request.insert(
            "assignments".to_string(),
            serde_json::Value::Array(
                response
                    .assignment
                    .iter()
                    .map(|pair| {
                        serde_json::Value::Array(vec![
                            serde_json::Value::from(pair[0]),
                            serde_json::Value::from(pair[1]),
                        ])
                    })
                    .collect(),
            ),
        );
        let eval_json = serde_json::Value::Object(eval_request).to_string();
        let eval_response_json = evaluate_problem_json(&eval_json).expect("evaluation should run");
        let eval_response: CoreEvaluationResponse =
            serde_json::from_str(&eval_response_json).expect("evaluation response JSON");

        assert!(eval_response.assignment_unique);
        assert!(
            eval_response.hard_constraints_satisfied,
            "native assignment must satisfy all hard rules, violations: {:?}",
            eval_response.violation_codes
        );

        let python_cost = payload
            .pointer("/python_reference/total_cost")
            .and_then(serde_json::Value::as_f64);
        assert!(python_cost.is_some(), "reference records a python cost");
        eprintln!(
            "40-parity: native feasible=true total_cost={total_cost:.1} python_reference_cost={:.1}",
            python_cost.unwrap()
        );
    }
}
