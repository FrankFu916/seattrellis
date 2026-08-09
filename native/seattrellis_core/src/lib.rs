pub mod cost;
pub mod models;
pub mod objectives;
pub mod rng;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
    /// Optional wall-clock budget in seconds (M3-04). When set, the solve
    /// must return by the deadline: greedy attempts stop, the hard search
    /// checks the clock per node, and a budget spent without a complete
    /// state-space sweep reports `Timeout` (not `Unknown`). Absent => the
    /// hard search falls back to its node budget.
    #[serde(default)]
    pub time_limit_seconds: Option<f64>,
}

/// Frozen v2 SolveStatus vocabulary (plan §四.1, M1-03). Serialized with
/// PascalCase so the wire values match the contract text verbatim
/// (`Solved`, `ProvenInfeasible`, `Timeout`, `Unknown`, `InvalidInput`,
/// `Cancelled`, `InternalError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum SolveStatus {
    Solved,
    ProvenInfeasible,
    Timeout,
    /// Default: honest status for legacy wire data and heuristic exhaustion.
    #[default]
    Unknown,
    InvalidInput,
    Cancelled,
    InternalError,
}

impl SolveStatus {
    /// The frozen wire spelling (identical to `serde` PascalCase output).
    pub fn as_str(self) -> &'static str {
        match self {
            SolveStatus::Solved => "Solved",
            SolveStatus::ProvenInfeasible => "ProvenInfeasible",
            SolveStatus::Timeout => "Timeout",
            SolveStatus::Unknown => "Unknown",
            SolveStatus::InvalidInput => "InvalidInput",
            SolveStatus::Cancelled => "Cancelled",
            SolveStatus::InternalError => "InternalError",
        }
    }
}

/// Classify a solver/domain error message onto the frozen vocabulary.
///
/// The core reports validation failures as `Err(String)`; callers (CLI,
/// server) use this to distinguish `InvalidInput` from `InternalError`.
/// Heuristic exhaustion is never classified as `ProvenInfeasible` here:
/// without a sound proof the honest status is `Unknown` (M1-03).
pub fn classify_solve_error(message: &str) -> SolveStatus {
    let low = message.to_ascii_lowercase();
    const INVALID_TOKENS: [&str; 12] = [
        "invalid",
        "unknown",
        "require",
        "duplicate",
        "missing",
        "not enough",
        "at least",
        "cannot seat",
        "more students",
        "unrecognized",
        "unsupported",
        "conflicting",
    ];
    if INVALID_TOKENS.iter().any(|token| low.contains(token)) {
        SolveStatus::InvalidInput
    } else {
        SolveStatus::InternalError
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoreSolveResponse {
    pub api_version: u32,
    pub feasible: bool,
    /// Frozen v2 SolveStatus (M1-03). Greedy exhaustion is `Unknown`, never
    /// `ProvenInfeasible` (no sound proof exists until M3).
    #[serde(default)]
    pub status: SolveStatus,
    /// ``[student_index, seat_index]`` pairs; empty when infeasible.
    pub assignment: Vec<[usize; 2]>,
    pub attempts_used: usize,
    pub hard_constraints_satisfied: bool,
    /// Mirrors `_fallback_total_cost`; `None` when infeasible.
    #[serde(default)]
    pub total_cost: Option<f64>,
}

/// Hard-rule index pairs after `rules.groups` are expanded into pairwise
/// constraints. Mirrors `rule_compiler.compile_hard_rules`: the request's
/// explicit pair lists come first, group-derived pairs after, so duplicate
/// pairs are checked exactly as often as Python checks them.
#[derive(Debug, Default)]
pub struct ResolvedHardRules {
    pub must_be_adjacent: Vec<[usize; 2]>,
    pub cannot_be_adjacent: Vec<[usize; 2]>,
}

/// Expand `rules.groups` (separate/together) into pairwise hard-rule index
/// pairs, merged with the request's explicit pair lists.
///
/// Mirrors `rule_compiler._expand_group_rules` (every member pair of each
/// group; `together` → must_be_adjacent, `separate` → cannot_be_adjacent) and
/// strict `_require_student` resolution (unknown members are a hard error).
/// Members are matched by student `key` and deduplicated in order, mirroring
/// `tuple(dict.fromkeys(...))`. A group with fewer than two distinct members
/// contributes no constraints, exactly like `itertools.combinations`.
pub fn resolve_group_rules(request: &CoreSolveRequest) -> Result<ResolvedHardRules, String> {
    let mut resolved = ResolvedHardRules {
        must_be_adjacent: request.must_be_adjacent.clone(),
        cannot_be_adjacent: request.cannot_be_adjacent.clone(),
    };
    let Some(rules) = &request.rules else {
        return Ok(resolved);
    };
    if rules.groups.is_empty() {
        return Ok(resolved);
    }
    let students = effective_students(request);
    let index_by_key: HashMap<&str, usize> = students
        .iter()
        .enumerate()
        .map(|(index, student)| (student.key.as_str(), index))
        .collect();
    for group in &rules.groups {
        let mut members: Vec<&str> = Vec::new();
        for member in &group.students {
            if !members.contains(&member.as_str()) {
                members.push(member);
            }
        }
        for first_offset in 0..members.len() {
            for second in &members[first_offset + 1..] {
                let first_index = index_by_key.get(members[first_offset]).copied().ok_or_else(
                    || format!("Unknown student reference: {:?}.", members[first_offset]),
                )?;
                let second_index = index_by_key
                    .get(second)
                    .copied()
                    .ok_or_else(|| format!("Unknown student reference: {second:?}."))?;
                if first_index == second_index {
                    return Err("A pair rule must reference two different students.".to_string());
                }
                let pair = [first_index.min(second_index), first_index.max(second_index)];
                if group.together {
                    resolved.must_be_adjacent.push(pair);
                }
                if group.separate {
                    resolved.cannot_be_adjacent.push(pair);
                }
            }
        }
    }
    Ok(resolved)
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
    let resolved = resolve_group_rules(request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);

    // Candidate-domain precheck (plan §6.1 second layer): a student with no
    // legal seat is a sound infeasibility proof — occupancy never *adds*
    // candidates. This is the only ProvenInfeasible the greedy path may emit.
    let domains = build_candidate_domains(request, &resolved, &adjacency, &graph_distances);
    if domains.iter().any(|domain| domain.seats.is_empty()) {
        return Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: false,
            status: SolveStatus::ProvenInfeasible,
            assignment: Vec::new(),
            attempts_used: 0,
            hard_constraints_satisfied: false,
            total_cost: None,
        });
    }

    // Global matching precheck (plan §6.1 third layer): even when every
    // student has candidates, they may not be jointly seatable. A maximum
    // bipartite matching smaller than the class size is a sound proof of
    // infeasibility (Hall's theorem); a full matching proves nothing yet.
    if maximum_candidate_matching(&domains) < request.student_count {
        return Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: false,
            status: SolveStatus::ProvenInfeasible,
            assignment: Vec::new(),
            attempts_used: 0,
            hard_constraints_satisfied: false,
            total_cost: None,
        });
    }

    let attempts = (request.student_count * 12).max(40);
    let mut rng = SplitMix64::new(request.seed);
    let ctx = build_cost_context(request);

    // Optional wall-clock budget (M3-04): once set, the whole solve (greedy
    // attempts + hard search) must honor it. A valid incumbent found before
    // the deadline still returns Solved; running out of time without one
    // reports Timeout instead of Unknown.
    let deadline: Option<std::time::Instant> = request
        .time_limit_seconds
        .map(|seconds| std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds));

    // Mirror the Python fallback: attempt 0 seats each student on the cheapest
    // seat, later attempts sample randomly among the top-3 cheap seats, and the
    // lowest-total-cost complete assignment across every attempt wins.
    let mut best: Option<(Vec<usize>, f64, usize)> = None;
    for attempt in 0..attempts {
        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            break;
        }
        if let Some(assignment) = greedy_attempt(
            request,
            &resolved,
            &adjacency,
            &graph_distances,
            &mut rng,
            &ctx,
            attempt,
        ) {
            let total_cost = full_solution_total_cost(&assignment, &adjacency, &ctx);
            if best
                .as_ref()
                .is_none_or(|(_, best_cost, _)| total_cost < *best_cost)
            {
                best = Some((assignment, total_cost, attempt + 1));
            }
        }
    }

    if let Some((assignment, _total_cost, best_attempts)) = best {
        // Soft optimization (plan §6.2): hill-climb on the greedy best; the
        // result still passes the independent validation gate below.
        let assignment = local_search(
            request,
            &resolved,
            &adjacency,
            &graph_distances,
            &assignment,
            &ctx,
            &mut rng,
        );
        let total_cost = full_solution_total_cost(&assignment, &adjacency, &ctx);
        // Independent validation gate (M3-05): a solver bug must surface as
        // InternalError, never as a silently "feasible" result.
        validate_assignment(request, &resolved, &adjacency, &graph_distances, &assignment)?;
        let pairs: Vec<[usize; 2]> = assignment
            .iter()
            .enumerate()
            .map(|(student, seat)| [student, *seat])
            .collect();
        return Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: true,
            status: SolveStatus::Solved,
            assignment: pairs,
            attempts_used: best_attempts,
            hard_constraints_satisfied: true,
            total_cost: Some(total_cost),
        });
    }

    // Full hard search (plan §6.1 fourth layer): when every greedy attempt
    // fails, backtracking with MRV + forward checking either finds a legal
    // seating, proves infeasibility by exhausting the whole state space, or
    // spends its budget (node budget without a time limit; the clock with
    // one — then the honest status is Timeout/Unknown, never ProvenInfeasible).
    let outcome = hard_search_with_budget(
        request,
        &resolved,
        &adjacency,
        &graph_distances,
        deadline.map_or(HARD_SEARCH_NODE_BUDGET, |_| HARD_SEARCH_NODE_BUDGET),
        deadline,
    );
    match outcome {
        SearchOutcome::Found(assignment) => {
            // Soft optimization (plan §6.2), then the same independent
            // validation gate as the greedy path (M3-05).
            let assignment = local_search(
                request,
                &resolved,
                &adjacency,
                &graph_distances,
                &assignment,
                &ctx,
                &mut rng,
            );
            validate_assignment(request, &resolved, &adjacency, &graph_distances, &assignment)?;
            let total_cost = full_solution_total_cost(&assignment, &adjacency, &ctx);
            let pairs: Vec<[usize; 2]> = assignment
                .iter()
                .enumerate()
                .map(|(student, seat)| [student, *seat])
                .collect();
            Ok(CoreSolveResponse {
                api_version: NATIVE_API_VERSION,
                feasible: true,
                status: SolveStatus::Solved,
                assignment: pairs,
                attempts_used: attempts,
                hard_constraints_satisfied: true,
                total_cost: Some(total_cost),
            })
        }
        SearchOutcome::ProvenInfeasible => Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: false,
            // Exhaustive search over the full state space: this is a sound
            // proof, so ProvenInfeasible is honest here (M1-03 / plan §四.1).
            status: SolveStatus::ProvenInfeasible,
            assignment: Vec::new(),
            attempts_used: attempts,
            hard_constraints_satisfied: false,
            total_cost: None,
        }),
        SearchOutcome::BudgetExceeded => Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: false,
            // Budget spent without a complete state-space sweep: honest
            // status is Timeout when a time budget was given, Unknown
            // otherwise — never ProvenInfeasible (M1-03).
            status: if deadline.is_some() {
                SolveStatus::Timeout
            } else {
                SolveStatus::Unknown
            },
            assignment: Vec::new(),
            attempts_used: attempts,
            hard_constraints_satisfied: false,
            total_cost: None,
        }),
    }
}

pub fn solve_problem_json(request_json: &str) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    let response = solve_problem(&request)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize native solve response: {error}"))
}

/// Feasibility precheck report (M3-06, plan §6.1 layer 2 + §6.5): candidate
/// seat domains with per-seat exclusion reasons, the most constrained
/// student, and the global matching size. The UI consumes this to explain
/// *why* a problem is hard or infeasible before/without running a search.
///
/// The report never runs the solver; `precheck` is `"clean"` when static
/// conflicts, empty domains, and matching all pass — that is a diagnostic,
/// not a feasibility proof (the hard search decides that).
pub fn precheck_report_json(request_json: &str) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    let domains = build_candidate_domains(&request, &resolved, &adjacency, &graph_distances);
    let matching_size = maximum_candidate_matching(&domains);

    let (precheck, reason): (&str, Option<String>) =
        if let Some(empty) = domains.iter().find(|domain| domain.seats.is_empty()) {
            let why = empty
                .excluded
                .first()
                .map(|(seat, reason)| format!("seat {seat}: {reason}"))
                .unwrap_or_else(|| "no legal seat".to_string());
            ("infeasible", Some(format!("student {} has no legal seat ({why})", empty.student)))
        } else if matching_size < request.student_count {
            (
                "infeasible",
                Some(format!(
                    "matching seats {} of {} students",
                    matching_size, request.student_count
                )),
            )
        } else {
            ("clean", None)
        };

    let most_constrained = domains
        .iter()
        .min_by_key(|domain| (domain.seats.len(), domain.student))
        .map(|domain| {
            json!({
                "student": domain.student,
                "candidate_count": domain.seats.len(),
            })
        });

    let students: Vec<Value> = domains
        .iter()
        .map(|domain| {
            json!({
                "student": domain.student,
                "candidate_count": domain.seats.len(),
                "seats": domain.seats,
                "excluded": domain.excluded.iter().map(|(seat, reason)| {
                    json!({ "seat": seat, "reason": reason })
                }).collect::<Vec<_>>(),
            })
        })
        .collect();

    let report = json!({
        "api_version": NATIVE_API_VERSION,
        "precheck": precheck,
        "infeasible_reason": reason,
        "student_count": request.student_count,
        "seat_count": request.seat_positions.len(),
        "matching_size": matching_size,
        "most_constrained_student": most_constrained,
        "students": students,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize precheck report: {error}"))
}

/// Solution audit report (plan §6.5): per hard-rule check status and the
/// soft-objective breakdown for a solved assignment.
///
/// The UI consumes this to explain a candidate: which hard rules were
/// checked and satisfied, each soft objective's raw loss / weighted cost,
/// and warnings for rules that could not participate (missing data).
pub fn audit_report_json(
    request_json: &str,
    assignment: &[[usize; 2]],
) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);

    // Rebuild the student->seat probe and validate independently (M3-05),
    // so the audit never blesses an illegal assignment.
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    for [student, seat] in assignment {
        if *student >= request.student_count || *seat >= request.seat_positions.len() {
            return Err(format!(
                "assignment references unknown student {student} or seat {seat}"
            ));
        }
        probe[*student] = Some(*seat);
    }
    if !solve_partial_assignment_valid(&request, &resolved, &probe, &adjacency, &graph_distances) {
        return Err("assignment violates a hard rule".to_string());
    }

    // Hard-rule audit: how many rules of each kind were checked, and how
    // many hold. A full assignment makes every rule checkable.
    let fixed_ok = request
        .fixed_seats
        .iter()
        .filter(|[student, seat]| probe[*student] == Some(*seat))
        .count();
    let must_ok = resolved
        .must_be_adjacent
        .iter()
        .filter(|[a, b]| assigned_students_are_adjacent(&probe, &adjacency, *a, *b))
        .count();
    let cannot_ok = resolved
        .cannot_be_adjacent
        .iter()
        .filter(|[a, b]| !assigned_students_are_adjacent(&probe, &adjacency, *a, *b))
        .count();
    let distance_ok = request
        .min_distance
        .iter()
        .filter(|rule| {
            assigned_students_meet_distance(
                &request.seat_positions,
                &probe,
                &graph_distances,
                rule,
            )
        })
        .count();

    let hard_rules = json!({
        "fixed_seats": { "checked": request.fixed_seats.len(), "satisfied": fixed_ok },
        "must_be_adjacent": { "checked": resolved.must_be_adjacent.len(), "satisfied": must_ok },
        "cannot_be_adjacent": { "checked": resolved.cannot_be_adjacent.len(), "satisfied": cannot_ok },
        "min_distance": { "checked": request.min_distance.len(), "satisfied": distance_ok },
    });

    // Soft-objective breakdown (raw loss / weight / weighted cost per rule).
    let ctx = build_cost_context(&request);
    let assignment_vec: Vec<usize> = probe.iter().map(|seat| seat.unwrap()).collect();
    let mut evaluation = evaluate_soft_objectives(
        &assignment_by_key(&probe, &ctx),
        &ctx.objective_context,
        &ctx.rules,
    );
    // score_balance is folded into full_solution_total_cost directly (not
    // through evaluate_soft_objectives); surface its breakdown here so the
    // audit covers every soft objective (plan §6.5).
    if ctx.rules.soft.score_balance.enabled && ctx.rules.soft.score_balance.weight != 0 {
        let mut loss = 0.0;
        for first in 0..assignment_vec.len() {
            let Some(first_score) = ctx.students[first].score else {
                continue;
            };
            for second in (first + 1)..assignment_vec.len() {
                let Some(second_score) = ctx.students[second].score else {
                    continue;
                };
                if adjacency[assignment_vec[first]].contains(&assignment_vec[second]) {
                    loss += (first_score - second_score).abs();
                }
            }
        }
        let weight = ctx.rules.soft.score_balance.weight as f64;
        evaluation
            .losses
            .insert("score_balance".to_string(), Some(loss));
        evaluation
            .weighted_costs
            .insert("score_balance".to_string(), -loss * weight);
    }

    let report = json!({
        "api_version": NATIVE_API_VERSION,
        "hard_rules": hard_rules,
        "soft_objectives": {
            "losses": evaluation.losses,
            "weighted_costs": evaluation.weighted_costs,
            "warnings": evaluation.warnings,
        },
        "total_cost": full_solution_total_cost(&assignment_vec, &adjacency, &ctx),
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize audit report: {error}"))
}

/// Map a student->seat probe to `student key -> seat id`, the shape the soft
/// objective evaluator consumes.
fn assignment_by_key(
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
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)
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
            seat_id: format!("seat_{index}"),
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

/// Local search budget: candidate moves per optimization run (plan §6.2).
const LOCAL_SEARCH_ITERATIONS: usize = 2_000;

/// Stop after this many consecutive non-improving moves (stagnation
/// detection, plan §6.2).
const LOCAL_SEARCH_STAGNATION_LIMIT: usize = 250;

/// Soft optimization (plan §6.2): hill-climbing local search on top of a
/// legal assignment. Swaps two students' seats or moves a student to an
/// empty seat; every candidate move is re-validated against the hard rules
/// before acceptance, so hard correctness is never broken. Moves are sampled
/// with the shared deterministic RNG — same seed, same result.
///
/// Only strictly-improving moves are accepted; after `STAGNATION_LIMIT`
/// consecutive failures the search stops. Returns the best assignment found
/// (may be the input itself).
fn local_search(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    assignment: &[usize],
    ctx: &CostContext,
    rng: &mut SplitMix64,
) -> Vec<usize> {
    let mut current = assignment.to_vec();
    let mut current_cost = full_solution_total_cost(&current, adjacency, ctx);
    let mut stagnation = 0;

    for _ in 0..LOCAL_SEARCH_ITERATIONS {
        let candidate = random_neighbor(&current, ctx, rng);
        let Ok(probe) = validate_candidate_move(
            request,
            resolved,
            adjacency,
            graph_distances,
            candidate,
        ) else {
            continue;
        };
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

/// A candidate neighbor assignment: either a swap of two students' seats or
/// a move of one student into an empty seat (sampled deterministically).
fn random_neighbor(
    assignment: &[usize],
    ctx: &CostContext,
    rng: &mut SplitMix64,
) -> Vec<usize> {
    let mut neighbor = assignment.to_vec();
    let seat_count = ctx.layout.seats.len();
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
    if candidate.iter().any(|seat| candidate.iter().filter(|other| other == &seat).count() > 1) {
        return Err("candidate move duplicates a seat".to_string());
    }
    Ok(candidate)
}

fn greedy_attempt(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
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
                resolved,
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

fn valid_candidate_seats(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
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
        let ok =
            solve_partial_assignment_valid(request, resolved, assignment, adjacency, graph_distances);
        assignment[student] = None;
        if ok {
            candidates.push(seat);
        }
    }
    candidates
}

fn solve_partial_assignment_valid(
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
        domains.push(CandidateDomain { student, seats, excluded });
    }
    domains
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
    let seat_count = domains
        .iter()
        .flat_map(|domain| domain.seats.iter().copied())
        .max()
        .map_or(0, |max| max + 1);
    let mut seat_owner: Vec<Option<usize>> = vec![None; seat_count];
    let mut matched = 0;
    for student in 0..domains.len() {
        let mut visited = vec![false; seat_count];
        if augment_matching(student, domains, &mut visited, &mut seat_owner) {
            matched += 1;
        }
    }
    matched
}

/// Kuhn's augmenting-path step: try to reassign seats so `student` gets one.
fn augment_matching(
    student: usize,
    domains: &[CandidateDomain],
    visited: &mut [bool],
    seat_owner: &mut [Option<usize>],
) -> bool {
    for &seat in &domains[student].seats {
        if visited[seat] {
            continue;
        }
        visited[seat] = true;
        if let Some(previous) = seat_owner[seat] {
            if augment_matching(previous, domains, visited, seat_owner) {
                seat_owner[seat] = Some(student);
                return true;
            }
        } else {
            seat_owner[seat] = Some(student);
            return true;
        }
    }
    false
}

/// Exhaustive hard search outcome (plan §6.1 fourth layer).
///
/// [`SearchOutcome::ProvenInfeasible`] is only returned when the entire state
/// space was swept without a legal seating; hitting the node budget returns
/// [`SearchOutcome::BudgetExceeded`] so callers can keep the honest `Unknown`
/// status (M1-03: heuristic exhaustion is never ProvenInfeasible).
#[derive(Debug, PartialEq, Eq)]
enum SearchOutcome {
    Found(Vec<usize>),
    ProvenInfeasible,
    BudgetExceeded,
}

/// Node budget for one hard search. Classes are small (<= 60 students) and
/// MRV + forward checking prune hard, so 200k nodes is generous for the
/// full sweep while still bounding worst-case time.
const HARD_SEARCH_NODE_BUDGET: usize = 200_000;

/// Full hard search: MRV student selection with degree tie-break, forward
/// checking over the candidate domains, deterministic (fixed order) branch
/// exploration. Fixed students are pre-placed; their domains are singletons
/// so MRV picks them first. `budget` bounds nodes; `deadline` (optional
/// wall-clock, M3-04) bounds time — whichever hits first stops the sweep.
fn hard_search_with_budget(
    request: &CoreSolveRequest,
    resolved: &ResolvedHardRules,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    budget: usize,
    deadline: Option<std::time::Instant>,
) -> SearchOutcome {
    let mut assignment: Vec<Option<usize>> = vec![None; request.student_count];
    for [student, seat] in &request.fixed_seats {
        assignment[*student] = Some(*seat);
    }
    let mut domains: Vec<Vec<usize>> = build_candidate_domains(request, resolved, adjacency, graph_distances)
        .into_iter()
        .map(|domain| domain.seats)
        .collect();
    let mut budget = budget;
    backtrack(
        request,
        resolved,
        adjacency,
        graph_distances,
        &mut assignment,
        &mut domains,
        &mut budget,
        deadline,
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
    deadline: Option<std::time::Instant>,
) -> SearchOutcome {
    if *budget == 0 {
        return SearchOutcome::BudgetExceeded;
    }
    *budget -= 1;
    if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
        return SearchOutcome::BudgetExceeded;
    }

    // Every student assigned: complete assignment found.
    if assignment.iter().all(Option::is_some) {
        let complete = assignment.iter().map(|seat| seat.unwrap()).collect();
        return SearchOutcome::Found(complete);
    }

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
        // Deterministic seat order; skip seats already taken.
        if assignment.contains(&Some(seat)) {
            continue;
        }

        // Forward checking: assign student -> seat and filter every other
        // student's domain. Any empty domain prunes this branch.
        let mut next_domains = domains.to_vec();
        let mut pruned = false;
        for other in 0..request.student_count {
            if assignment[other].is_some() || other == student {
                continue;
            }
            next_domains[other].retain(|candidate| {
                *candidate != seat
                    && partial_pair_valid(
                        request,
                        resolved,
                        adjacency,
                        graph_distances,
                        assignment,
                        student,
                        seat,
                        other,
                        *candidate,
                    )
            });
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
            deadline,
        );
        assignment[student] = None;
        match result {
            SearchOutcome::Found(_) => return result,
            SearchOutcome::BudgetExceeded => return result,
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
        if *student_index == student
            && probe[*student_index] != Some(*seat_index)
        {
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
                other = if *first_student == student { second_student } else { first_student }
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
                other = if *first_student == student { second_student } else { first_student }
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
    }
    for rule in &request.min_distance {
        if rule.students[0] >= request.student_count || rule.students[1] >= request.student_count {
            return Err("min_distance references an unknown student".to_string());
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
        if fixed_by_student.insert(*student_index, *seat_index).is_some() {
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
    let graph_distances = build_graph_distance_matrix(&adjacency);
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
                CoreDistanceMetric::Graph => {
                    graph_distances[*first_seat][*second_seat].map(|distance| distance as f64)
                }
            };
            if distance.is_none_or(|value| value < rule.distance) {
                return Err(
                    "conflicting hard rules: fixed seats violate a min_distance rule".to_string(),
                );
            }
        }
    }
    // Resolving groups surfaces unknown-member references, mirroring strict
    // rule compilation; the derived pairs are validated by the caller.
    resolve_group_rules(request)?;
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
        assignment_is_unique, assigned_students_meet_distance, build_candidate_domains,
        build_cost_context, build_graph_distance_matrix, build_index_adjacency,
        classify_solve_error, full_solution_total_cost, greedy_attempt, local_search,
        evaluate_problem_json, hard_search_with_budget, maximum_candidate_matching,
        resolve_group_rules, seat_distance, solve_problem_json, HARD_SEARCH_NODE_BUDGET,
        audit_report_json, precheck_report_json, validate_assignment,
        validate_solve_request_json,
        SplitMix64,
        CoreEvaluationResponse, CoreSolveRequest, CoreSolveResponse, SearchOutcome,
        SolveStatus, NATIVE_API_VERSION,
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
    fn validates_solve_request_without_running_search() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0]]
        }"#;
        assert!(validate_solve_request_json(request).is_ok());

        // Regression: a fixed seat whose seat index is >= student_count must
        // validate (seat indexes are independent of student_count). The merged
        // hard-rule loop used to mistake the seat slot for a student index and
        // reject this valid request.
        let fixed_high_seat = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "fixed_seats": [[0, 0], [1, 2]]
        }"#;
        assert!(
            validate_solve_request_json(fixed_high_seat).is_ok(),
            "fixed seat index 2 with 2 students must be accepted"
        );
        let fixed_out_of_range = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0], [1, 5]]
        }"#;
        let error = validate_solve_request_json(fixed_out_of_range).unwrap_err();
        assert!(error.contains("unknown student or seat"), "{error}");

        let invalid = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]]
        }"#;
        let error = validate_solve_request_json(invalid).unwrap_err();
        assert!(error.contains("more students than available seats"));
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

    // -----------------------------------------------------------------------
    // Group rules (RuleSet.groups): expanded into pairwise must/cannot-be-
    // adjacent constraints exactly like `rule_compiler._expand_group_rules`.
    // -----------------------------------------------------------------------

    #[test]
    fn expands_group_rules_into_pairwise_hard_rules() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 4,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                "must_be_adjacent": [[0, 1]],
                "students": [
                    {"key": "A"}, {"key": "B"}, {"key": "C"}, {"key": "D"}
                ],
                "rules": {
                    "seed": 1,
                    "groups": [
                        {"name": "buddies", "students": ["A", "B", "C"], "together": true},
                        {"name": "rivals", "students": ["C", "D"], "separate": true},
                        {"name": "solo", "students": ["D"], "together": true},
                        {"name": "dupe", "students": ["A", "A", "B"], "separate": true}
                    ]
                }
            }"#,
        )
        .expect("request parses");

        let resolved = resolve_group_rules(&request).expect("groups resolve");
        // Explicit pairs first, then group-derived pairs in member order:
        // buddies(A,B,C) together → (A,B),(A,C),(B,C).
        assert_eq!(resolved.must_be_adjacent, vec![[0, 1], [0, 1], [0, 2], [1, 2]]);
        // rivals(C,D) separate → (C,D); dupe dedupes to (A,B) → (A,B).
        assert_eq!(resolved.cannot_be_adjacent, vec![[2, 3], [0, 1]]);
    }

    #[test]
    fn group_member_references_resolve_by_student_key() {
        // Members may appear in any order and are paired by index, not by the
        // order the student records appear in the request.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 3,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
                "students": [
                    {"key": "first", "display_name": "Alpha"},
                    {"key": "second", "display_name": "Beta"},
                    {"key": "third", "display_name": "Gamma"}
                ],
                "rules": {
                    "groups": [
                        {"name": "trio", "students": ["third", "first"], "together": true}
                    ]
                }
            }"#,
        )
        .expect("request parses");
        let resolved = resolve_group_rules(&request).expect("groups resolve");
        assert_eq!(resolved.must_be_adjacent, vec![[0, 2]]);
        assert!(resolved.cannot_be_adjacent.is_empty());
    }

    #[test]
    fn rejects_group_member_that_is_not_a_student() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
                "students": [{"key": "A"}, {"key": "B"}],
                "rules": {
                    "groups": [{"name": "g", "students": ["A", "GHOST"], "together": true}]
                }
            }"#,
        )
        .expect("request parses");
        let error = resolve_group_rules(&request).unwrap_err();
        assert!(error.contains("Unknown student reference"), "{error}");
        assert!(error.contains("GHOST"), "{error}");
    }

    #[test]
    fn validate_rejects_unknown_group_member() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "students": [{"key": "A"}, {"key": "B"}],
            "rules": {"groups": [{"name": "g", "students": ["A", "GHOST"], "together": true}]}
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(error.contains("Unknown student reference"), "{error}");
    }

    /// The solver must honor group rules end-to-end: a `together` group is
    /// seated adjacently and a `separate` group is kept apart, using only the
    /// top-level `rules.groups` (no explicit pairwise lists).
    #[test]
    fn solver_enforces_group_together_and_separate() {
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
            "edges": [[0, 1], [1, 2], [2, 3]],
            "students": [
                {"key": "A", "score": 90.0},
                {"key": "B", "score": 80.0},
                {"key": "C", "score": 70.0},
                {"key": "D", "score": 60.0}
            ],
            "rules": {
                "seed": 7,
                "soft": {"randomize": {"enabled": false, "weight": 1}},
                "groups": [
                    {"name": "buddy", "students": ["A", "B"], "together": true},
                    {"name": "rival", "students": ["C", "D"], "separate": true}
                ]
            }
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(response.feasible, "groups A/B together and C/D apart must be feasible");
        assert!(response.hard_constraints_satisfied);

        let seat_of = |student: usize| -> usize {
            response
                .assignment
                .iter()
                .find(|pair| pair[0] == student)
                .map(|pair| pair[1])
                .expect("student is assigned")
        };
        let adjacent = |first: usize, second: usize| {
            (seat_of(first) as i64 - seat_of(second) as i64).abs() == 1
        };
        assert!(adjacent(0, 1), "A and B must sit together");
        assert!(!adjacent(2, 3), "C and D must sit apart");
    }

    /// An infeasible group combination must be reported as infeasible rather
    /// than silently ignored.
    #[test]
    fn solver_reports_infeasible_group_as_not_found() {
        // Three seats in a line, A fixed to seat 0 and B to seat 2; the
        // `together` group demands adjacency, which the fixed seats rule out.
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0], [1, 2]],
            "students": [{"key": "A"}, {"key": "B"}],
            "rules": {
                "seed": 3,
                "groups": [{"name": "g", "students": ["A", "B"], "together": true}]
            }
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(!response.feasible, "A and B are pinned apart but must sit together");
        assert!(response.assignment.is_empty());
    }

    // ------------------------------------------------------------------
    // M1-03: frozen SolveStatus contract (plan §四.1)
    // ------------------------------------------------------------------

    #[test]
    fn solved_reports_solved_status() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]],
            "seed": 0
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert_eq!(response.status, SolveStatus::Solved);
        assert!(response.feasible);

        // The wire value must be the frozen PascalCase spelling.
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert_eq!(value["status"], "Solved");
    }

    /// Exhaustive search proves a fully-constrained 2x2 grid infeasible
    /// (M3-04: the status upgrades from Unknown to ProvenInfeasible once the
    /// whole state space is swept; see
    /// `hard_search_budget_exhaustion_stays_unknown` for the honest-Unknown
    /// case).
    #[test]
    fn greedy_exhaustion_reports_unknown_status() {
        // 2x2 grid, every seat pair forbidden from adjacency: the request
        // passes static validation but no complete assignment exists.
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
            "cannot_be_adjacent": [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]],
            "seed": 7
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::ProvenInfeasible);

        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert_eq!(value["status"], "ProvenInfeasible");
    }

    #[test]
    fn validation_errors_are_err_and_classify_as_invalid_input() {
        let request = r#"{
            "api_version": 99,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]]
        }"#;
        let err = solve_problem_json(request).expect_err("unsupported api_version must fail");
        assert_eq!(classify_solve_error(&err), SolveStatus::InvalidInput);
    }

    #[test]
    fn classify_solve_error_distinguishes_input_from_internal() {
        for message in [
            "unsupported api_version 99",
            "native solve requires at least one seat",
            "native solve cannot seat more students than available seats",
            "Duplicate student identifiers: STU001",
            "unknown rule kind",
        ] {
            assert_eq!(
                classify_solve_error(message),
                SolveStatus::InvalidInput,
                "message {message:?} should be InvalidInput",
            );
        }
        for message in [
            "solver panicked while ranking candidates",
            "could not serialize the response",
            "internal store is poisoned",
        ] {
            assert_eq!(
                classify_solve_error(message),
                SolveStatus::InternalError,
                "message {message:?} should be InternalError",
            );
        }
    }

    /// The status vocabulary is frozen: every variant serializes to exactly
    /// the plan's spelling, and deserialization round-trips.
    #[test]
    fn solve_status_vocabulary_is_frozen_on_the_wire() {
        let cases = [
            (SolveStatus::Solved, "Solved"),
            (SolveStatus::ProvenInfeasible, "ProvenInfeasible"),
            (SolveStatus::Timeout, "Timeout"),
            (SolveStatus::Unknown, "Unknown"),
            (SolveStatus::InvalidInput, "InvalidInput"),
            (SolveStatus::Cancelled, "Cancelled"),
            (SolveStatus::InternalError, "InternalError"),
        ];
        for (status, wire) in cases {
            let encoded = serde_json::to_string(&status).unwrap();
            assert_eq!(encoded, format!("\"{wire}\""));
            let decoded: SolveStatus = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, status);
        }
    }

    /// Responses without a `status` field (pre-M1-03 wire data) must
    /// deserialize with the honest default `Unknown`.
    #[test]
    fn legacy_response_without_status_defaults_to_unknown() {
        let legacy = r#"{
            "api_version": 2,
            "feasible": true,
            "assignment": [[0, 0]],
            "attempts_used": 1,
            "hard_constraints_satisfied": true
        }"#;
        let response: CoreSolveResponse = serde_json::from_str(legacy).unwrap();
        assert_eq!(response.status, SolveStatus::Unknown);
    }

    // ---- M3-02: static conflict layer (plan §6.1 first layer) ----

    #[test]
    fn static_conflict_student_fixed_to_two_seats_is_invalid() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "fixed_seats": [[0, 0], [0, 2]]
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(
            error.contains("fixed to more than one seat"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn static_conflict_seat_fixed_to_two_students_is_invalid() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0], [1, 0]]
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(
            error.contains("fixed to more than one student"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn static_conflict_same_pair_in_must_and_cannot_is_invalid() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "must_be_adjacent": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(
            error.contains("appears in both must_be_adjacent and cannot_be_adjacent"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn static_conflict_fixed_seats_contradict_pair_rules() {
        // Fixed seats 0 and 2 are not adjacent, but must_be_adjacent demands
        // adjacency: unsolvable before any search.
        let must_violated = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0], [1, 2]],
            "must_be_adjacent": [[0, 1]]
        }"#;
        let error = validate_solve_request_json(must_violated).unwrap_err();
        assert!(error.contains("do not satisfy a must_be_adjacent rule"), "{error}");

        // Fixed seats 0 and 1 are adjacent, but cannot_be_adjacent forbids it.
        let cannot_violated = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "fixed_seats": [[0, 0], [1, 1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;
        let error = validate_solve_request_json(cannot_violated).unwrap_err();
        assert!(error.contains("violate a cannot_be_adjacent rule"), "{error}");

        // Fixed seats 0 and 1 violate a graph min_distance of 2 (they are 1 hop).
        let distance_violated = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "fixed_seats": [[0, 0], [1, 1]],
            "min_distance": [{"students": [0, 1], "distance": 2.0, "metric": "graph"}]
        }"#;
        let error = validate_solve_request_json(distance_violated).unwrap_err();
        assert!(error.contains("violate a min_distance rule"), "{error}");
    }

    #[test]
    fn conflicting_errors_classify_as_invalid_input() {
        assert_eq!(
            classify_solve_error("conflicting hard rules: fixed seats violate a min_distance rule"),
            SolveStatus::InvalidInput
        );
    }

    // ---- M3-02: candidate domains (plan §6.1 second layer) ----

    #[test]
    fn candidate_domains_respect_fixed_and_pair_rules() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 3,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
                "edges": [[0, 1], [1, 2], [2, 3]],
                "fixed_seats": [[0, 0]],
                "must_be_adjacent": [[0, 1]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);
        let domains = build_candidate_domains(&request, &resolved, &adjacency, &graph_distances);
        // Student 0 is fixed to seat 0: domain is exactly {0}.
        assert_eq!(domains[0].seats, vec![0]);
        assert!(domains[0].excluded.iter().all(|(seat, _)| *seat != 0));

        // Student 1 must sit adjacent to student 0: only seat 1 is legal.
        assert_eq!(domains[1].seats, vec![1]);
        assert!(domains[1]
            .excluded
            .iter()
            .any(|(seat, reason)| *seat == 3 && reason.contains("adjacent")));

        // Student 2 is unconstrained: every seat is legal.
        assert_eq!(domains[2].seats.len(), 4);
    }

    #[test]
    fn empty_candidate_domain_is_proven_infeasible() {
        // Student 1 must sit at graph distance >= 3 from the fixed student 0
        // (seat 0), but every seat is closer than 3 hops on this line graph:
        // no legal seat exists for student 1, a sound infeasibility proof.
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0]],
            "min_distance": [{"students": [0, 1], "distance": 3.0, "metric": "graph"}]
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::ProvenInfeasible);
        assert_eq!(response.attempts_used, 0);
        assert!(response.assignment.is_empty());
    }

    // ---- M3-03: global matching precheck (plan §6.1 third layer) ----

    #[test]
    fn maximum_matching_counts_jointly_seatable_students() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 3,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
                "edges": [[0, 1], [1, 2], [2, 3]],
                "fixed_seats": [[0, 0]],
                "must_be_adjacent": [[0, 1]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);
        let domains = build_candidate_domains(&request, &resolved, &adjacency, &graph_distances);
        // domains: student0={0}, student1={1}, student2={0,1,2,3} — a full
        // matching of size 3 exists (0->0, 1->1, 2->2).
        assert_eq!(maximum_candidate_matching(&domains), 3);
    }

    #[test]
    fn matching_precheck_proves_infeasibility_when_seats_are_overbooked() {
        // Three students, three seats. Students 0/1 are fixed to seats 0/1;
        // student 2 must not sit adjacent to student 0, which rules out seat
        // 2 (its only neighbor) but not seats 0/1 (no edges). Every domain is
        // non-empty, yet seats 0/1 are both taken: maximum matching = 2 < 3.
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 2]],
            "fixed_seats": [[0, 0], [1, 1]],
            "cannot_be_adjacent": [[0, 2]]
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::ProvenInfeasible);
        assert_eq!(response.attempts_used, 0);
    }
    // ---- M3-04: exhaustive hard search (plan §6.1 fourth layer) ----

    #[test]
    fn hard_search_finds_legal_assignment_when_greedy_fails() {
        // A 4-cycle of must_be_adjacent pairs: students 0-1, 1-2, 2-3, 3-0
        // must all sit adjacent, but seat 2 is disabled... no: use a layout
        // where the only legal seating is a specific rotation the random
        // greedy misses. Here a 2x3 grid with a min_distance pair between
        // students 0 and 1 (>= 2 graph hops): greedy attempt 0 pins 0 and 1
        // on adjacent cheap seats and every randomized attempt fails to
        // escape; the search finds the far-apart placement.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [0.0, 1.0], [1.0, 1.0], [2.0, 1.0]],
                "edges": [[0, 1], [1, 2], [3, 4], [4, 5], [0, 3], [1, 4], [2, 5]],
                "min_distance": [{"students": [0, 1], "distance": 3.0, "metric": "graph"}],
                "seed": 1
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);

        let outcome = hard_search_with_budget(&request, &resolved, &adjacency, &graph_distances, 200_000, None);
        let SearchOutcome::Found(assignment) = outcome else {
            panic!("hard search should find the far-apart placement, got {outcome:?}");
        };
        // Student 0 and 1 must be >= 3 hops apart: only opposite corners work
        // in this 2x3 ladder (e.g. 0->seat 0 and 1->seat 5 is 3 hops).
        let probe: Vec<Option<usize>> =
            assignment.iter().map(|seat| Some(*seat)).collect();
        assert!(assigned_students_meet_distance(
            &request.seat_positions,
            &probe,
            &graph_distances,
            &request.min_distance[0],
        ));
        assert_eq!(assignment.len(), 2);
        assert!(assignment[0] != assignment[1]);
    }

    #[test]
    fn hard_search_budget_exhaustion_stays_unknown() {
        // The 2x2 fully-forbidden grid is proven infeasible in a few nodes;
        // with a tiny budget the sweep cannot complete and the honest status
        // must stay Unknown (never ProvenInfeasible).
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 4,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
                "cannot_be_adjacent": [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);

        // A budget of 1 node cannot sweep anything.
        let outcome = hard_search_with_budget(&request, &resolved, &adjacency, &graph_distances, 1, None);
        assert_eq!(outcome, SearchOutcome::BudgetExceeded);

        // The full budget proves it (and solve_problem reports that).
        let outcome = hard_search_with_budget(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            HARD_SEARCH_NODE_BUDGET,
            None,
        );
        assert_eq!(outcome, SearchOutcome::ProvenInfeasible);
    }
    #[test]
    fn independent_validator_rejects_violating_assignments() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
                "edges": [[0, 1], [1, 2]],
                "cannot_be_adjacent": [[0, 1]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);

        // Adjacent seats 0/1 violate the pair rule: must be rejected.
        let error = validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0, 1])
            .expect_err("adjacent placement must violate cannot_be_adjacent");
        assert!(error.contains("violates a hard rule"), "{error}");

        // Duplicate seat: must be rejected.
        let error = validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0, 0])
            .expect_err("duplicate seat must be rejected");
        assert!(error.contains("duplicate seat"), "{error}");

        // Missing students: must be rejected.
        let error = validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0])
            .expect_err("short assignment must be rejected");
        assert!(error.contains("students"), "{error}");

        // Seats 0 and 2 are not adjacent: a legal pairing passes.
        validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0, 2])
            .expect("non-adjacent pairing must pass");
    }
    #[test]
    fn precheck_report_lists_domains_and_reasons() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
            "edges": [[0, 1], [1, 2], [2, 3]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]]
        }"#;
        let report: serde_json::Value =
            serde_json::from_str(&precheck_report_json(request).unwrap()).unwrap();
        assert_eq!(report["precheck"], "clean");
        assert!(report["infeasible_reason"].is_null());
        assert_eq!(report["matching_size"], 3);
        assert_eq!(report["students"][0]["candidate_count"], 1);
        assert_eq!(report["students"][0]["seats"][0], 0);
        assert_eq!(report["students"][1]["candidate_count"], 1);
        assert_eq!(report["students"][1]["seats"][0], 1);
        // The exclusion reason names the pair rule.
        let excluded = &report["students"][1]["excluded"];
        assert!(
            excluded
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["reason"].as_str().unwrap().contains("adjacent")),
            "excluded reasons: {excluded}"
        );
    }

    #[test]
    fn precheck_report_flags_empty_domain_with_reason() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0]],
            "min_distance": [{"students": [0, 1], "distance": 3.0, "metric": "graph"}]
        }"#;
        let report: serde_json::Value =
            serde_json::from_str(&precheck_report_json(request).unwrap()).unwrap();
        assert_eq!(report["precheck"], "infeasible");
        let reason = report["infeasible_reason"].as_str().unwrap();
        assert!(reason.contains("student 1 has no legal seat"), "{reason}");
    }
    #[test]
    fn time_limit_reports_timeout_when_budget_is_spent() {
        // The fully-forbidden 2x2 grid is provably infeasible, but with a
        // sub-millisecond budget the search cannot sweep it: the honest
        // status is Timeout (a time budget was given), never ProvenInfeasible.
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
            "cannot_be_adjacent": [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]],
            "time_limit_seconds": 0.000001
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::Timeout);
    }

    #[test]
    fn time_limit_with_incumbent_still_reports_solved() {
        // A trivial problem solved by greedy attempt 0 within the budget:
        // the incumbent wins even though the budget is tiny.
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "time_limit_seconds": 0.001
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        assert_eq!(response.status, SolveStatus::Solved);
    }
    // ---- M3 6.2: soft optimization (local search) ----

    #[test]
    fn local_search_never_worsens_cost_and_keeps_legality() {
        // Skewed scores + enabled score_balance give the hill climber room
        // to improve on the raw greedy output.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 8,
                "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
                "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
                "students": [
                    {"key":"s0","score":100.0},{"key":"s1","score":10.0},
                    {"key":"s2","score":95.0},{"key":"s3","score":15.0},
                    {"key":"s4","score":90.0},{"key":"s5","score":20.0},
                    {"key":"s6","score":85.0},{"key":"s7","score":25.0}
                ],
                "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}},
                "seed": 42
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);
        let ctx = build_cost_context(&request);
        let mut rng = SplitMix64::new(42);

        let initial = greedy_attempt(&request, &resolved, &adjacency, &graph_distances, &mut rng, &ctx, 0)
            .expect("greedy should seat everyone");
        let before = full_solution_total_cost(&initial, &adjacency, &ctx);

        let improved = local_search(
            &request, &resolved, &adjacency, &graph_distances, &initial, &ctx, &mut rng,
        );
        let after = full_solution_total_cost(&improved, &adjacency, &ctx);

        assert!(after <= before + 1e-9, "cost worsened: {before} -> {after}");
        validate_assignment(&request, &resolved, &adjacency, &graph_distances, &improved)
            .expect("local search must keep the assignment legal");

        // Determinism: same seed, same input -> identical output. Replay the
        // same RNG consumption (greedy first, then local search).
        let mut rng2 = SplitMix64::new(42);
        let _ = greedy_attempt(&request, &resolved, &adjacency, &graph_distances, &mut rng2, &ctx, 0)
            .expect("greedy should seat everyone");
        let rerun = local_search(
            &request, &resolved, &adjacency, &graph_distances, &initial, &ctx, &mut rng2,
        );
        assert_eq!(improved, rerun, "local search must be deterministic");
    }

    #[test]
    fn solve_applies_local_search_without_breaking_parity_status() {
        // End-to-end: the solver still reports Solved with a legal assignment
        // (the local search path runs inside solve_problem).
        let request = r#"{
            "api_version": 2,
            "student_count": 8,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
            "students": [
                {"key":"s0","score":100.0},{"key":"s1","score":10.0},
                {"key":"s2","score":95.0},{"key":"s3","score":15.0},
                {"key":"s4","score":90.0},{"key":"s5","score":20.0},
                {"key":"s6","score":85.0},{"key":"s7","score":25.0}
            ],
            "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}},
            "seed": 42
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(response.feasible);
        assert_eq!(response.status, SolveStatus::Solved);
        assert!(response.total_cost.unwrap().is_finite());
    }
    #[test]
    fn audit_report_breaks_down_hard_and_soft_rules() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]],
            "students": [
                {"key":"s0","score":100.0},{"key":"s1","score":10.0},{"key":"s2","score":90.0}
            ],
            "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}}
        }"#;
        // Legal assignment: s0->0 (fixed), s1->1 (adjacent to s0), s2->2.
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2]];
        let report: serde_json::Value = serde_json::from_str(
            &audit_report_json(request, &assignment).unwrap(),
        )
        .unwrap();

        assert_eq!(report["hard_rules"]["fixed_seats"]["satisfied"], 1);
        assert_eq!(report["hard_rules"]["must_be_adjacent"]["satisfied"], 1);
        assert_eq!(report["hard_rules"]["cannot_be_adjacent"]["satisfied"], 0);
        // The soft breakdown must carry the score_balance weighted cost.
        let weighted = &report["soft_objectives"]["weighted_costs"];
        assert!(
            weighted.as_object().unwrap().contains_key("score_balance"),
            "weighted_costs: {weighted}"
        );
        assert!(report["total_cost"].is_number());
    }

    #[test]
    fn audit_rejects_illegal_assignments() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;
        // Adjacent seats violate the pair rule: the audit must refuse.
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1]];
        let error = audit_report_json(request, &assignment).unwrap_err();
        assert!(error.contains("violates a hard rule"), "{error}");
    }
}
