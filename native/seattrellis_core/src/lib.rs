pub mod cost;
pub mod models;
pub mod objectives;
pub mod rng;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use crate::cost::{
    avoid_recent_neighbors_cost, build_adjacency_edges, classify_seat_position,
    detect_neighbor_relation_types, fair_rotation_cost, individual_cost, normalize_edge,
    student_needs_front,
};
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

/// Thread-safe, one-shot cancellation control for a running solve.
///
/// Clones share the same atomic flag, so a caller may keep one clone on the
/// request thread and call [`SolveControl::cancel`] from another thread. A
/// cancelled control is intentionally not resettable; create a fresh control
/// for the next solve.
#[derive(Clone, Debug, Default)]
pub struct SolveControl {
    cancelled: Arc<AtomicBool>,
}

impl SolveControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
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
                let first_index = index_by_key
                    .get(members[first_offset])
                    .copied()
                    .ok_or_else(|| {
                        format!("Unknown student reference: {:?}.", members[first_offset])
                    })?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    Deadline,
    Cancelled,
}

struct SolveRunControl<'a> {
    deadline: Option<Instant>,
    cancellation: &'a SolveControl,
}

impl SolveRunControl<'_> {
    fn stop_reason(&self) -> Option<StopReason> {
        if self.cancellation.is_cancelled() {
            Some(StopReason::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Some(StopReason::Deadline)
        } else {
            None
        }
    }
}

fn stopped_response(reason: StopReason, attempts_used: usize) -> CoreSolveResponse {
    CoreSolveResponse {
        api_version: NATIVE_API_VERSION,
        feasible: false,
        status: match reason {
            StopReason::Deadline => SolveStatus::Timeout,
            StopReason::Cancelled => SolveStatus::Cancelled,
        },
        assignment: Vec::new(),
        attempts_used,
        hard_constraints_satisfied: false,
        total_cost: None,
    }
}

pub fn solve_problem(request: &CoreSolveRequest) -> Result<CoreSolveResponse, String> {
    let control = SolveControl::new();
    solve_problem_with_control(request, &control)
}

/// Solve with cooperative cancellation while preserving the frozen response
/// vocabulary. Cancellation before any legal incumbent returns `Cancelled`;
/// once an incumbent exists it remains a valid `Solved` result and
/// cancellation merely stops further search/optimization.
pub fn solve_problem_with_control(
    request: &CoreSolveRequest,
    control: &SolveControl,
) -> Result<CoreSolveResponse, String> {
    solve_problem_internal(request, control, &[])
}

fn solve_problem_internal(
    request: &CoreSolveRequest,
    cancellation: &SolveControl,
    excluded_assignments: &[Vec<usize>],
) -> Result<CoreSolveResponse, String> {
    let response = solve_problem_unchecked(request, cancellation, excluded_assignments)?;
    validate_solve_response_consistency(request, &response)?;
    Ok(response)
}

fn solve_problem_unchecked(
    request: &CoreSolveRequest,
    cancellation: &SolveControl,
    excluded_assignments: &[Vec<usize>],
) -> Result<CoreSolveResponse, String> {
    validate_solve_request(request)?;
    let duration = request
        .time_limit_seconds
        .map(Duration::try_from_secs_f64)
        .transpose()
        .map_err(|_| "invalid time_limit_seconds: duration is out of range".to_string())?;
    let deadline =
        match duration {
            Some(duration) => Some(Instant::now().checked_add(duration).ok_or_else(|| {
                "invalid time_limit_seconds: deadline is out of range".to_string()
            })?),
            None => None,
        };
    let run = SolveRunControl {
        deadline,
        cancellation,
    };
    if let Some(reason) = run.stop_reason() {
        return Ok(stopped_response(reason, 0));
    }

    let resolved = resolve_group_rules(request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    if let Some(reason) = run.stop_reason() {
        return Ok(stopped_response(reason, 0));
    }

    // Candidate-domain precheck (plan §6.1 second layer): a student with no
    // legal seat is a sound infeasibility proof — occupancy never *adds*
    // candidates. This is the only ProvenInfeasible the greedy path may emit.
    let domains = build_candidate_domains(request, &resolved, &adjacency, &graph_distances);
    if let Some(reason) = run.stop_reason() {
        return Ok(stopped_response(reason, 0));
    }
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
    let matching_size = maximum_candidate_matching(&domains);
    if let Some(reason) = run.stop_reason() {
        return Ok(stopped_response(reason, 0));
    }
    if matching_size < request.student_count {
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

    // Mirror the Python fallback: attempt 0 seats each student on the cheapest
    // seat, later attempts sample randomly among the top-3 cheap seats, and the
    // lowest-total-cost complete assignment across every attempt wins. The
    // loop exits early when the best plan stops improving (stagnation), so
    // interactive sizes stay responsive without losing quality.
    let mut best: Option<(Vec<usize>, f64, usize)> = None;
    let mut attempts_used = 0;
    let mut stagnation = 0usize;
    for attempt in 0..attempts {
        if let Some(reason) = run.stop_reason() {
            if best.is_some() {
                break;
            }
            return Ok(stopped_response(reason, attempts_used));
        }
        attempts_used = attempt + 1;
        match greedy_attempt_controlled(
            request,
            &resolved,
            &adjacency,
            &graph_distances,
            &mut rng,
            &ctx,
            attempt,
            &run,
            excluded_assignments,
        ) {
            GreedyOutcome::Found(assignment) => {
                let total_cost = full_solution_total_cost(&assignment, &adjacency, &ctx);
                if best
                    .as_ref()
                    .is_none_or(|(_, best_cost, _)| total_cost < *best_cost)
                {
                    best = Some((assignment, total_cost, attempt + 1));
                    stagnation = 0;
                } else {
                    stagnation += 1;
                    if stagnation >= GREEDY_STAGNATION_LIMIT {
                        break;
                    }
                }
            }
            GreedyOutcome::DeadEnd => {
                stagnation += 1;
                if stagnation >= GREEDY_STAGNATION_LIMIT {
                    break;
                }
            }
            GreedyOutcome::Stopped(reason) => {
                if best.is_some() {
                    break;
                }
                return Ok(stopped_response(reason, attempts_used));
            }
        }
    }

    if let Some((assignment, _total_cost, best_attempts)) = best {
        // Soft optimization (plan §6.2): hill-climb on the greedy best; the
        // result still passes the independent validation gate below.
        let assignment = local_search_controlled(
            request,
            &resolved,
            &adjacency,
            &graph_distances,
            &assignment,
            &ctx,
            &mut rng,
            &run,
            excluded_assignments,
        );
        let total_cost = full_solution_total_cost(&assignment, &adjacency, &ctx);
        // Independent validation gate (M3-05): a solver bug must surface as
        // InternalError, never as a silently "feasible" result.
        validate_assignment(
            request,
            &resolved,
            &adjacency,
            &graph_distances,
            &assignment,
        )?;
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
    let outcome = hard_search_controlled(
        request,
        &resolved,
        &adjacency,
        &graph_distances,
        HARD_SEARCH_NODE_BUDGET,
        &run,
        excluded_assignments,
    );
    match outcome {
        SearchOutcome::Found(assignment) => {
            // Soft optimization (plan §6.2), then the same independent
            // validation gate as the greedy path (M3-05).
            let assignment = local_search_controlled(
                request,
                &resolved,
                &adjacency,
                &graph_distances,
                &assignment,
                &ctx,
                &mut rng,
                &run,
                excluded_assignments,
            );
            validate_assignment(
                request,
                &resolved,
                &adjacency,
                &graph_distances,
                &assignment,
            )?;
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
                attempts_used,
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
            attempts_used,
            hard_constraints_satisfied: false,
            total_cost: None,
        }),
        SearchOutcome::BudgetExceeded => Ok(CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: false,
            // Budget spent without a complete state-space sweep: honest
            // status is Timeout when a time budget was given, Unknown
            // otherwise — never ProvenInfeasible (M1-03).
            status: SolveStatus::Unknown,
            assignment: Vec::new(),
            attempts_used,
            hard_constraints_satisfied: false,
            total_cost: None,
        }),
        SearchOutcome::DeadlineExceeded => {
            Ok(stopped_response(StopReason::Deadline, attempts_used))
        }
        SearchOutcome::Cancelled => Ok(stopped_response(StopReason::Cancelled, attempts_used)),
    }
}

/// Validate a successful solve response against the complete request contract.
///
/// This is a consumer-side boundary check: it distrusts the response flags and
/// assignment indices, reconstructs the student-indexed assignment, then
/// independently re-checks every hard rule (including group-derived rules).
pub fn validate_solve_response(
    request: &CoreSolveRequest,
    response: &CoreSolveResponse,
) -> Result<(), String> {
    validate_solve_request(request)?;

    if response.api_version != NATIVE_API_VERSION {
        return Err(format!(
            "solve response api_version {} does not match {}",
            response.api_version, NATIVE_API_VERSION
        ));
    }
    if response.status != SolveStatus::Solved {
        return Err(format!(
            "solve response status must be Solved, got {}",
            response.status.as_str()
        ));
    }
    if !response.feasible {
        return Err("Solved response must set feasible=true".to_string());
    }
    if !response.hard_constraints_satisfied {
        return Err("Solved response must set hard_constraints_satisfied=true".to_string());
    }
    if response.assignment.len() != request.student_count {
        return Err(format!(
            "solve response assignment contains {} entries for {} students",
            response.assignment.len(),
            request.student_count
        ));
    }

    let mut assignment_by_student: Vec<Option<usize>> = vec![None; request.student_count];
    let mut occupied_seats = vec![false; request.seat_positions.len()];
    for [student, seat] in &response.assignment {
        if *student >= request.student_count {
            return Err(format!(
                "solve response references out-of-range student {student}"
            ));
        }
        if *seat >= request.seat_positions.len() {
            return Err(format!(
                "solve response references out-of-range seat {seat}"
            ));
        }
        if assignment_by_student[*student].replace(*seat).is_some() {
            return Err(format!(
                "solve response assigns student {student} more than once"
            ));
        }
        if std::mem::replace(&mut occupied_seats[*seat], true) {
            return Err(format!("solve response assigns seat {seat} more than once"));
        }
    }

    let assignment: Vec<usize> = assignment_by_student
        .into_iter()
        .enumerate()
        .map(|(student, seat)| {
            seat.ok_or_else(|| format!("solve response omits student {student}"))
        })
        .collect::<Result<_, _>>()?;
    let resolved = resolve_group_rules(request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    validate_assignment(
        request,
        &resolved,
        &adjacency,
        &graph_distances,
        &assignment,
    )
    .map_err(|error| format!("solve response failed independent validation: {error}"))
}

fn validate_solve_response_consistency(
    request: &CoreSolveRequest,
    response: &CoreSolveResponse,
) -> Result<(), String> {
    if response.status == SolveStatus::Solved {
        return validate_solve_response(request, response);
    }
    validate_solve_request(request)?;
    if response.api_version != NATIVE_API_VERSION {
        return Err(format!(
            "solve response api_version {} does not match {}",
            response.api_version, NATIVE_API_VERSION
        ));
    }
    if response.feasible {
        return Err(format!(
            "non-Solved response {} must set feasible=false",
            response.status.as_str()
        ));
    }
    if !response.assignment.is_empty() {
        return Err(format!(
            "non-Solved response {} must have an empty assignment",
            response.status.as_str()
        ));
    }
    if response.hard_constraints_satisfied {
        return Err(format!(
            "non-Solved response {} must set hard_constraints_satisfied=false",
            response.status.as_str()
        ));
    }
    if response.total_cost.is_some() {
        return Err(format!(
            "non-Solved response {} must not report total_cost",
            response.status.as_str()
        ));
    }
    Ok(())
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
            (
                "infeasible",
                Some(format!(
                    "student {} has no legal seat ({why})",
                    empty.student
                )),
            )
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
pub fn audit_report_json(request_json: &str, assignment: &[[usize; 2]]) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);

    // Rebuild the student->seat probe and validate independently (M3-05),
    // so the audit never blesses an illegal assignment. The assignment must
    // be complete and conflict-free: a missing or duplicated student would
    // otherwise be silently masked here.
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    let mut occupied = vec![false; request.seat_positions.len()];
    for [student, seat] in assignment {
        if *student >= request.student_count || *seat >= request.seat_positions.len() {
            return Err(format!(
                "assignment references unknown student {student} or seat {seat}"
            ));
        }
        if probe[*student].replace(*seat).is_some() {
            return Err(format!(
                "assignment assigns student {student} more than once"
            ));
        }
        if occupied[*seat] {
            return Err(format!(
                "assignment assigns seat {seat} to more than one student"
            ));
        }
        occupied[*seat] = true;
    }
    if let Some(missing) = probe.iter().position(Option::is_none) {
        return Err(format!(
            "assignment is incomplete: student index {} has no seat",
            missing + 1
        ));
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
            assigned_students_meet_distance(&request.seat_positions, &probe, &graph_distances, rule)
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

    // Top soft-cost contributors (plan §6.5 "最大贡献者"): each student's
    // soft contribution = individual cost + half of every pair cost
    // involving the student (score_balance + avoid_recent_neighbors). The
    // randomize term uses a fresh fixed-seed rng so the ranking is
    // deterministic.
    let neighbor_rule = effective_neighbor_rule(&ctx.rules);
    let mut contributions: Vec<(String, f64)> = Vec::new();
    for student in 0..assignment_vec.len() {
        let mut contribution = individual_cost(
            &ctx.students[student],
            &ctx.layout.seats[assignment_vec[student]],
            &ctx.layout,
            &ctx.rules,
            ctx.history.as_ref(),
            &mut SplitMix64::new(0),
            ctx.min_row,
            ctx.max_row,
        ) as f64;
        for other in 0..assignment_vec.len() {
            if other == student {
                continue;
            }
            let first_seat = assignment_vec[student];
            let second_seat = assignment_vec[other];
            if ctx.rules.soft.score_balance.enabled && ctx.rules.soft.score_balance.weight != 0 {
                if let (Some(first_score), Some(second_score)) =
                    (ctx.students[student].score, ctx.students[other].score)
                {
                    if adjacency[first_seat].contains(&second_seat) {
                        contribution -= 0.5
                            * ctx.rules.soft.score_balance.weight as f64
                            * (first_score - second_score).abs();
                    }
                }
            }
            if neighbor_rule.enabled && neighbor_rule.weight != 0 {
                contribution += 0.5
                    * avoid_recent_neighbors_cost(
                        &ctx.students[student].key,
                        &ctx.students[other].key,
                        &ctx.layout.seats[first_seat],
                        &ctx.layout.seats[second_seat],
                        &ctx.layout,
                        &neighbor_rule,
                        ctx.pair_history.as_ref(),
                        Some(&ctx.adjacency_edges),
                    ) as f64;
            }
        }
        contributions.push((ctx.students[student].key.clone(), contribution));
    }
    contributions.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let top_contributors: Vec<Value> = contributions
        .iter()
        .take(3)
        .map(|(student_key, contribution)| {
            json!({ "student_key": student_key, "contribution": contribution })
        })
        .collect();

    let report = json!({
        "api_version": NATIVE_API_VERSION,
        "hard_rules": hard_rules,
        "soft_objectives": {
            "losses": evaluation.losses,
            "weighted_costs": evaluation.weighted_costs,
            "warnings": evaluation.warnings,
            "top_contributors": top_contributors,
        },
        "total_cost": full_solution_total_cost(&assignment_vec, &adjacency, &ctx),
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize audit report: {error}"))
}

// ---------------------------------------------------------------------------
// PlanScore (plan §6.2/§6.6): Python-parity per-assignment score breakdown
// ---------------------------------------------------------------------------

/// `_rating` from `scoring.py`: a coarse qualitative band over the 0..100
/// score.
fn score_rating(score: f64) -> &'static str {
    if score >= 75.0 {
        "high"
    } else if score >= 50.0 {
        "medium"
    } else {
        "low"
    }
}

/// `_available_dimension` from `scoring.py`: a finite score clamped to
/// 0..100 and rounded to two decimals, with the rating band.
fn score_dimension(
    score: f64,
    raw_value: Option<f64>,
    weight: f64,
    details: serde_json::Value,
) -> serde_json::Value {
    let score = (score.clamp(0.0, 100.0) * 100.0).round() / 100.0;
    serde_json::json!({
        "status": "available",
        "score": score,
        "raw_value": raw_value,
        "weight": weight,
        "rating": score_rating(score),
        "details": details,
    })
}

/// `_not_available` from `scoring.py`.
fn not_available_dimension(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "not_available",
        "score": null,
        "raw_value": null,
        "weight": 0,
        "rating": "not_available",
        "details": { "reason": reason },
    })
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Score a fixed assignment exactly like Python's `score_snapshot`
/// (plan §6.6 item 4: Rust/Python objective breakdown parity on the same
/// assignment). The breakdown mirrors `ScoreBreakdown` field-for-field:
/// seven named dimensions plus `rule_scores` (score_position /
/// score_distribution / mentor_pairing) and the hard-constraint summary.
///
/// `latest_snapshot_json` is a snapshot document (or `[]`) used by the
/// stability dimension; `diversity_score` is the caller-provided mean
/// assignment distance for candidate sets.
pub fn score_assignment_json(
    request_json: &str,
    assignment_pairs: &[[usize; 2]],
    latest_snapshot_json: &str,
    diversity_score: Option<f64>,
) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;

    // Rebuild the student->seat probe (same completeness checks as the
    // audit): a partial or duplicated assignment cannot be scored.
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    let mut occupied = vec![false; request.seat_positions.len()];
    for [student, seat] in assignment_pairs {
        if *student >= request.student_count || *seat >= request.seat_positions.len() {
            return Err(format!(
                "assignment references unknown student {student} or seat {seat}"
            ));
        }
        if probe[*student].replace(*seat).is_some() {
            return Err(format!(
                "assignment assigns student {student} more than once"
            ));
        }
        if occupied[*seat] {
            return Err(format!(
                "assignment assigns seat {seat} to more than one student"
            ));
        }
        occupied[*seat] = true;
    }
    if let Some(missing) = probe.iter().position(Option::is_none) {
        return Err(format!(
            "assignment is incomplete: student index {} has no seat",
            missing + 1
        ));
    }

    let ctx = build_cost_context(&request);
    let assignment_vec: Vec<usize> = probe.iter().map(|seat| seat.unwrap()).collect();
    let by_key = assignment_by_key(&probe, &ctx);
    let adjacency_edges = build_adjacency_edges(&ctx.layout);
    let seat_by_id: HashMap<&str, &Seat> = ctx
        .layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();
    let student_count = assignment_vec.len();

    // --- fair_rotation_score -------------------------------------------------
    let fair_rule = &ctx.rules.soft.fair_rotation;
    let fair_rotation_score = if !fair_rule.enabled || fair_rule.weight == 0 {
        not_available_dimension("fair_rotation is disabled.")
    } else if ctx
        .history
        .as_ref()
        .is_none_or(|history| history.history_count == 0)
    {
        not_available_dimension("No history snapshots were supplied.")
    } else {
        let penalty: i64 = assignment_vec
            .iter()
            .enumerate()
            .map(|(student, seat)| {
                fair_rotation_cost(
                    &ctx.students[student],
                    &ctx.layout.seats[*seat],
                    &ctx.layout,
                    fair_rule,
                    ctx.history.as_ref(),
                )
            })
            .sum();
        let penalty_units = penalty as f64 / (fair_rule.weight.max(1) * 100) as f64;
        let score = 100.0 / (1.0 + penalty_units / student_count.max(1) as f64);
        score_dimension(
            score,
            Some(penalty as f64),
            fair_rule.weight as f64,
            serde_json::json!({
                "penalty_cost": penalty,
                "history_count": ctx.history.as_ref().map(|h| h.history_count).unwrap_or(0),
                "lookback": fair_rule.lookback,
                "lower_penalty_is_better": true,
            }),
        )
    };

    // --- avoid_recent_neighbors_score ---------------------------------------
    let neighbor_rule = effective_neighbor_rule(&ctx.rules);
    let avoid_recent_neighbors_score = if !neighbor_rule.enabled || neighbor_rule.weight == 0 {
        not_available_dimension("avoid_recent_neighbors is disabled.")
    } else if ctx
        .pair_history
        .as_ref()
        .is_none_or(|pair_history| pair_history.history_count == 0)
    {
        not_available_dimension("No pair history was supplied.")
    } else {
        let selected_relations: std::collections::HashSet<&str> = neighbor_rule
            .relation_types
            .iter()
            .map(String::as_str)
            .collect();
        let mut penalty: i64 = 0;
        let mut relevant_pairs = 0usize;
        for first in 0..student_count {
            for second in (first + 1)..student_count {
                let first_seat = &ctx.layout.seats[assignment_vec[first]];
                let second_seat = &ctx.layout.seats[assignment_vec[second]];
                let current_relations = detect_neighbor_relation_types(
                    first_seat,
                    second_seat,
                    &ctx.layout,
                    Some(&adjacency_edges),
                    neighbor_rule.within_distance,
                );
                if current_relations
                    .iter()
                    .any(|relation| selected_relations.contains(relation.as_str()))
                {
                    relevant_pairs += 1;
                }
                penalty += avoid_recent_neighbors_cost(
                    &ctx.students[first].key,
                    &ctx.students[second].key,
                    first_seat,
                    second_seat,
                    &ctx.layout,
                    &neighbor_rule,
                    ctx.pair_history.as_ref(),
                    Some(&adjacency_edges),
                );
            }
        }
        let excess_units = penalty as f64 / (neighbor_rule.weight.max(1) * 100) as f64;
        let score = 100.0 / (1.0 + excess_units / relevant_pairs.max(1) as f64);
        score_dimension(
            score,
            Some(penalty as f64),
            neighbor_rule.weight as f64,
            serde_json::json!({
                "penalty_cost": penalty,
                "relevant_current_pairs": relevant_pairs,
                "history_count": ctx.pair_history.as_ref().map(|h| h.history_count).unwrap_or(0),
                "lookback": neighbor_rule.lookback,
                "lower_penalty_is_better": true,
            }),
        )
    };

    // --- score_balance_score -------------------------------------------------
    let balance_rule = &ctx.rules.soft.score_balance;
    let score_balance_score = if !balance_rule.enabled || balance_rule.weight == 0 {
        not_available_dimension("score_balance is disabled.")
    } else {
        let scores: Vec<f64> = ctx
            .students
            .iter()
            .filter_map(|student| student.score)
            .collect();
        let distinct: std::collections::HashSet<u64> =
            scores.iter().map(|score| score.to_bits()).collect();
        if scores.len() < 2 || distinct.len() < 2 {
            not_available_dimension("At least two different student scores are required.")
        } else {
            let min_score = scores.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_score = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let score_by_key: HashMap<&str, f64> = ctx
                .students
                .iter()
                .filter_map(|student| student.score.map(|score| (student.key.as_str(), score)))
                .collect();
            let mut gaps: Vec<f64> = Vec::new();
            for (first_seat, second_seat) in &adjacency_edges {
                let first_key = seat_by_id
                    .get(first_seat.as_str())
                    .and_then(|seat| by_key.iter().find(|(_, id)| *id == &seat.seat_id));
                let second_key = seat_by_id
                    .get(second_seat.as_str())
                    .and_then(|seat| by_key.iter().find(|(_, id)| *id == &seat.seat_id));
                if let (Some((first_student, _)), Some((second_student, _))) =
                    (first_key, second_key)
                {
                    if let (Some(first_score), Some(second_score)) = (
                        score_by_key.get(first_student.as_str()),
                        score_by_key.get(second_student.as_str()),
                    ) {
                        gaps.push((first_score - second_score).abs());
                    }
                }
            }
            if gaps.is_empty() {
                not_available_dimension("No adjacent assigned pairs have score data.")
            } else {
                let gap_mean = mean(&gaps);
                let score_range = max_score - min_score;
                let normalized = (gap_mean / score_range * 100.0).min(100.0);
                score_dimension(
                    normalized,
                    Some(gap_mean),
                    balance_rule.weight as f64,
                    serde_json::json!({
                        "mean_adjacent_score_gap": gap_mean,
                        "score_range": score_range,
                        "adjacent_pair_count": gaps.len(),
                        "meaning": "Higher values indicate stronger mixing of different score levels across adjacent seats.",
                    }),
                )
            }
        }
    };

    // --- height_preference_score ---------------------------------------------
    let height_rule = &ctx.rules.soft.height_back;
    let height_preference_score = if !height_rule.enabled || height_rule.weight == 0 {
        not_available_dimension("height_back is disabled.")
    } else {
        let heights: Vec<f64> = ctx
            .students
            .iter()
            .filter_map(|student| student.height_cm)
            .collect();
        let distinct_heights: std::collections::HashSet<u64> =
            heights.iter().map(|height| height.to_bits()).collect();
        let min_row = ctx.min_row;
        let max_row = ctx.max_row;
        if heights.len() < 2 || distinct_heights.len() < 2 || min_row == max_row {
            not_available_dimension("Different heights and more than one seat row are required.")
        } else {
            let height_by_key: HashMap<&str, f64> = ctx
                .students
                .iter()
                .filter_map(|student| {
                    student
                        .height_cm
                        .map(|height| (student.key.as_str(), height))
                })
                .collect();
            let min_height = heights.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_height = heights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mut errors: Vec<f64> = Vec::new();
            for (student, seat) in assignment_vec.iter().enumerate() {
                let Some(height) = height_by_key.get(ctx.students[student].key.as_str()) else {
                    continue;
                };
                let seat = &ctx.layout.seats[*seat];
                let height_position = (height - min_height) / (max_height - min_height);
                let row_position = (seat.row - min_row) as f64 / (max_row - min_row) as f64;
                errors.push((height_position - row_position).abs());
            }
            if errors.is_empty() {
                not_available_dimension("No assignments have height data.")
            } else {
                let error_mean = mean(&errors);
                let score = (100.0 * (1.0 - error_mean)).max(0.0);
                score_dimension(
                    score,
                    Some(error_mean),
                    height_rule.weight as f64,
                    serde_json::json!({
                        "mean_normalized_position_error": error_mean,
                        "lower_error_is_better": true,
                    }),
                )
            }
        }
    };

    // --- vision_preference_score ---------------------------------------------
    let vision_rule = &ctx.rules.soft.vision_front;
    let vision_preference_score = if !vision_rule.enabled || vision_rule.weight == 0 {
        not_available_dimension("vision_front is disabled.")
    } else {
        let needing_front: Vec<&str> = ctx
            .students
            .iter()
            .filter(|student| student_needs_front(student))
            .map(|student| student.key.as_str())
            .collect();
        if needing_front.is_empty() {
            not_available_dimension("No students are marked as needing a front seat.")
        } else {
            let min_row = ctx.min_row;
            let max_row = ctx.max_row;
            let mut positions: Vec<f64> = Vec::new();
            for (student, seat) in assignment_vec.iter().enumerate() {
                if !needing_front.contains(&ctx.students[student].key.as_str()) {
                    continue;
                }
                let seat = &ctx.layout.seats[*seat];
                let normalized = if min_row == max_row {
                    0.0
                } else {
                    (seat.row - min_row) as f64 / (max_row - min_row) as f64
                };
                positions.push(normalized);
            }
            if positions.is_empty() {
                not_available_dimension("No front-seat preference assignments could be evaluated.")
            } else {
                let position_mean = mean(&positions);
                let score = (100.0 * (1.0 - position_mean)).max(0.0);
                score_dimension(
                    score,
                    Some(position_mean),
                    vision_rule.weight as f64,
                    serde_json::json!({
                        "students_needing_front": needing_front.len(),
                        "mean_normalized_row": position_mean,
                        "lower_row_value_is_better": true,
                    }),
                )
            }
        }
    };

    // --- diversity_score ------------------------------------------------------
    let diversity_score = match diversity_score {
        Some(score) => score_dimension(
            score,
            Some(score),
            ctx.rules.soft.randomize.weight.max(1) as f64,
            serde_json::json!({
                "meaning": "Mean percentage of students seated differently from the other candidates.",
            }),
        ),
        None => not_available_dimension("Diversity requires at least two generated candidates."),
    };

    // --- stability_score ------------------------------------------------------
    let stability_score = if latest_snapshot_json.trim().is_empty() {
        not_available_dimension("No previous snapshot was supplied.")
    } else {
        let snapshot: Value = serde_json::from_str(latest_snapshot_json)
            .map_err(|error| format!("invalid latest snapshot document: {error}"))?;
        let mut previous: HashMap<String, String> = HashMap::new();
        if let Some(assignments) = snapshot.get("assignments").and_then(Value::as_array) {
            for assignment in assignments {
                if let (Some(student), Some(seat)) = (
                    assignment.get("student_key").and_then(Value::as_str),
                    assignment.get("seat_id").and_then(Value::as_str),
                ) {
                    previous.insert(student.to_string(), seat.to_string());
                }
            }
        }
        if previous.is_empty() {
            not_available_dimension("The previous snapshot has no comparable students.")
        } else {
            let mut comparable = 0usize;
            let mut unchanged = 0usize;
            for (student, seat) in assignment_vec.iter().enumerate() {
                if let Some(previous_seat) = previous.get(ctx.students[student].key.as_str()) {
                    comparable += 1;
                    if previous_seat == &ctx.layout.seats[*seat].seat_id {
                        unchanged += 1;
                    }
                }
            }
            if comparable == 0 {
                not_available_dimension("The previous snapshot has no comparable students.")
            } else {
                let score = unchanged as f64 / comparable as f64 * 100.0;
                score_dimension(
                    score,
                    Some(unchanged as f64),
                    1.0,
                    serde_json::json!({
                        "unchanged_students": unchanged,
                        "changed_students": comparable - unchanged,
                        "comparable_students": comparable,
                        "meaning": "Higher values preserve more seats from the latest historical snapshot.",
                    }),
                )
            }
        }
    };

    // --- rule_scores ----------------------------------------------------------
    let evaluation = evaluate_soft_objectives(&by_key, &ctx.objective_context, &ctx.rules);
    let mut rule_scores = serde_json::Map::new();
    for (name, unavailable_reason) in [
        (
            "score_position",
            "At least two students with different scores are required.",
        ),
        (
            "score_distribution",
            "At least two populated rows or groups with score data are required.",
        ),
        (
            "mentor_pairing",
            "No deterministic mentor/learner pairs could be formed from the score data.",
        ),
    ] {
        let (enabled, weight) = match name {
            "score_position" => (
                ctx.rules.soft.score_position.enabled,
                ctx.rules.soft.score_position.weight,
            ),
            "score_distribution" => (
                ctx.rules.soft.score_distribution.enabled,
                ctx.rules.soft.score_distribution.weight,
            ),
            _ => (
                ctx.rules.soft.mentor_pairing.enabled,
                ctx.rules.soft.mentor_pairing.weight,
            ),
        };
        let dimension = if !enabled || weight == 0 {
            not_available_dimension(&format!("{name} is disabled."))
        } else {
            match evaluation.losses.get(name).copied().flatten() {
                Some(loss) => {
                    let mut details = evaluation
                        .details
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    if !evaluation.warnings.is_empty() {
                        details["warnings"] = serde_json::Value::Array(
                            evaluation
                                .warnings
                                .iter()
                                .map(|warning| serde_json::Value::String(warning.clone()))
                                .collect(),
                        );
                    }
                    score_dimension((1.0 - loss) * 100.0, Some(loss), weight as f64, details)
                }
                None => {
                    let reason = if name == "score_distribution" && !evaluation.warnings.is_empty()
                    {
                        evaluation.warnings[0].clone()
                    } else {
                        unavailable_reason.to_string()
                    };
                    not_available_dimension(&reason)
                }
            }
        };
        rule_scores.insert(format!("{name}_score"), dimension);
    }

    // --- hard_constraint_summary ---------------------------------------------
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    let mut violations: Vec<String> = Vec::new();
    let mut checked = 3usize;
    let mut student_seats: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut seat_owners: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for [student, seat] in assignment_pairs {
        student_seats.insert(*student);
        seat_owners.insert(*seat);
    }
    if student_seats.len() != assignment_pairs.len() {
        violations.push("A student is assigned more than once.".to_string());
    }
    if seat_owners.len() != assignment_pairs.len() {
        violations.push("A seat is assigned more than once.".to_string());
    }
    if student_seats.len() != request.student_count {
        violations
            .push("Assignments do not contain every current student exactly once.".to_string());
    }
    for [student, seat] in &request.fixed_seats {
        checked += 1;
        if probe[*student] != Some(*seat) {
            violations.push(format!(
                "fixed_seats is not satisfied for student {student}."
            ));
        }
    }
    for [first, second] in &resolved.must_be_adjacent {
        checked += 1;
        if !assigned_students_are_adjacent(&probe, &adjacency, *first, *second) {
            violations.push(format!(
                "must_be_adjacent is not satisfied for students {first}, {second}."
            ));
        }
    }
    for [first, second] in &resolved.cannot_be_adjacent {
        checked += 1;
        if assigned_students_are_adjacent(&probe, &adjacency, *first, *second) {
            violations.push(format!(
                "cannot_be_adjacent is not satisfied for students {first}, {second}."
            ));
        }
    }
    for rule in &request.min_distance {
        checked += 1;
        if !assigned_students_meet_distance(&request.seat_positions, &probe, &graph_distances, rule)
        {
            violations.push(format!(
                "min_distance is not satisfied for students {:?}.",
                rule.students
            ));
        }
    }
    let hard_constraint_summary = serde_json::json!({
        "satisfied": violations.is_empty(),
        "checked_rule_count": checked,
        "violation_count": violations.len(),
        "violations": violations,
        "details": {},
    });

    // --- total ---------------------------------------------------------------
    let mut available: Vec<(f64, f64)> = Vec::new();
    for dimension in [
        &fair_rotation_score,
        &avoid_recent_neighbors_score,
        &score_balance_score,
        &height_preference_score,
        &vision_preference_score,
        &diversity_score,
        &stability_score,
    ] {
        if dimension.get("status").and_then(Value::as_str) == Some("available")
            && dimension.get("score").and_then(Value::as_f64).is_some()
            && dimension
                .get("weight")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                > 0.0
        {
            available.push((
                dimension["score"].as_f64().unwrap(),
                dimension["weight"].as_f64().unwrap(),
            ));
        }
    }
    for dimension in rule_scores.values() {
        if dimension.get("status").and_then(Value::as_str) == Some("available")
            && dimension.get("score").and_then(Value::as_f64).is_some()
            && dimension
                .get("weight")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                > 0.0
        {
            available.push((
                dimension["score"].as_f64().unwrap(),
                dimension["weight"].as_f64().unwrap(),
            ));
        }
    }
    let total = if !violations.is_empty() {
        0.0
    } else if available.is_empty() {
        100.0
    } else {
        let total_weight: f64 = available.iter().map(|(_, weight)| weight).sum();
        let weighted: f64 = available.iter().map(|(score, weight)| score * weight).sum();
        (weighted / total_weight * 100.0).round() / 100.0
    };

    let breakdown = serde_json::json!({
        "fair_rotation_score": fair_rotation_score,
        "avoid_recent_neighbors_score": avoid_recent_neighbors_score,
        "score_balance_score": score_balance_score,
        "height_preference_score": height_preference_score,
        "vision_preference_score": vision_preference_score,
        "diversity_score": diversity_score,
        "stability_score": stability_score,
        "rule_scores": serde_json::Value::Object(rule_scores),
        "hard_constraint_summary": hard_constraint_summary,
    });
    let report = serde_json::json!({
        "total": total,
        "breakdown": breakdown,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize plan score: {error}"))
}

/// Re-solve a snapshot while preserving requested local anchors (M2 parity,
/// ledger D.11 / plan §5.4 local repair), mirroring Python's
/// `compute_repair`:
///
/// - `locked_students` keep their current seat (fixed);
/// - `locked_seats` keep their current occupant fixed (an empty locked seat
///   is rejected — the Python reserve-empty-seat variant is a known gap);
/// - `affected_students` bounds the re-solve scope: those students plus
///   everyone connected by a hard pair rule are movable, everyone else is
///   fixed; without `affected_students` only the locks are fixed and the
///   rest may re-arrange globally.
///
/// Returns the repaired snapshot document (`assignments` + `solver_status`)
/// plus a short summary of moved/unseated students.
#[derive(Debug, Clone)]
struct ParsedSnapshotAssignment {
    student_key: String,
    seat_id: String,
}

fn parse_snapshot_assignments(
    snapshot: &Value,
    context: &str,
) -> Result<Vec<ParsedSnapshotAssignment>, String> {
    let object = snapshot
        .as_object()
        .ok_or_else(|| format!("invalid {context}: expected a JSON object"))?;
    let assignments = object
        .get("assignments")
        .ok_or_else(|| format!("invalid {context}: missing assignments"))?
        .as_array()
        .ok_or_else(|| format!("invalid {context}: assignments must be an array"))?;
    assignments
        .iter()
        .enumerate()
        .map(|(index, assignment)| {
            let assignment = assignment.as_object().ok_or_else(|| {
                format!("invalid {context}: assignments[{index}] must be an object")
            })?;
            let student_key = assignment
                .get("student_key")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "invalid {context}: assignments[{index}].student_key must be a non-empty string"
                    )
                })?;
            let seat_id = assignment
                .get("seat_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "invalid {context}: assignments[{index}].seat_id must be a non-empty string"
                    )
                })?;
            Ok(ParsedSnapshotAssignment {
                student_key: student_key.to_string(),
                seat_id: seat_id.to_string(),
            })
        })
        .collect()
}

fn request_seat_ids(request: &CoreSolveRequest) -> Vec<String> {
    (0..request.seat_positions.len())
        .map(|index| {
            request
                .layout
                .as_ref()
                .and_then(|layout| layout.seats.get(index))
                .map(|seat| seat.seat_id.clone())
                .unwrap_or_else(|| format!("seat-{}", index + 1))
        })
        .collect()
}

pub fn repair_json(
    request_json: &str,
    snapshot_json: &str,
    affected_students: &[String],
    locked_students: &[String],
    locked_seats: &[String],
) -> Result<String, String> {
    let mut request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let snapshot: Value = serde_json::from_str(snapshot_json)
        .map_err(|error| format!("invalid snapshot document: {error}"))?;
    let snapshot_assignments = parse_snapshot_assignments(&snapshot, "repair snapshot")?;

    // Student keys -> indices from the request.
    let students = effective_students(&request);
    let index_by_key: HashMap<&str, usize> = students
        .iter()
        .enumerate()
        .map(|(index, student)| (student.key.as_str(), index))
        .collect();
    let seat_ids = request_seat_ids(&request);
    let seat_index_by_id: HashMap<&str, usize> = seat_ids
        .iter()
        .enumerate()
        .map(|(index, seat_id)| (seat_id.as_str(), index))
        .collect();

    // Current assignment: student key -> seat id (and reverse). Repair uses
    // strict semantics: malformed, unknown, or duplicate references must not
    // silently change which anchors are preserved.
    let mut seat_by_student: HashMap<String, String> = HashMap::new();
    let mut student_by_seat: HashMap<String, String> = HashMap::new();
    for assignment in snapshot_assignments {
        if !index_by_key.contains_key(assignment.student_key.as_str()) {
            return Err(format!(
                "Repair snapshot references unknown student: {}.",
                assignment.student_key
            ));
        }
        if !seat_index_by_id.contains_key(assignment.seat_id.as_str()) {
            return Err(format!(
                "Repair snapshot references unknown seat: {}.",
                assignment.seat_id
            ));
        }
        if seat_by_student
            .insert(assignment.student_key.clone(), assignment.seat_id.clone())
            .is_some()
        {
            return Err(format!(
                "Repair snapshot contains duplicate assignments for student: {}.",
                assignment.student_key
            ));
        }
        if student_by_seat
            .insert(assignment.seat_id.clone(), assignment.student_key.clone())
            .is_some()
        {
            return Err(format!(
                "Repair snapshot assigns seat {} more than once.",
                assignment.seat_id
            ));
        }
    }

    // Validate the anchor sets.
    let unknown_affected: Vec<&str> = affected_students
        .iter()
        .map(String::as_str)
        .filter(|key| !index_by_key.contains_key(key))
        .collect();
    if !unknown_affected.is_empty() {
        return Err(format!(
            "Affected students are unknown: {}.",
            unknown_affected.join(", ")
        ));
    }
    for student in locked_students {
        if !index_by_key.contains_key(student.as_str()) {
            return Err(format!("Locked student is unknown: {student}."));
        }
        if !seat_by_student.contains_key(student) {
            return Err(format!(
                "Locked students must have a current seat before re-solving: {student}."
            ));
        }
    }
    for seat in locked_seats {
        if !seat_index_by_id.contains_key(seat.as_str()) {
            return Err(format!("Locked seat is unknown: {seat}."));
        }
        if !student_by_seat.contains_key(seat) {
            return Err(format!(
                "Locked seat must be occupied before re-solving: {seat}."
            ));
        }
    }
    for student in affected_students {
        if locked_students.contains(student) {
            return Err(format!(
                "Affected students cannot also be locked: {student}."
            ));
        }
        if let Some(seat) = seat_by_student.get(student) {
            if locked_seats.contains(seat) {
                return Err(format!("Affected students occupy locked seats: {student}."));
            }
        }
    }

    // Fixed set: locked students + locked-seat occupants + (when a local
    // scope is requested) every student outside the affected closure.
    let mut fixed_students: Vec<usize> = Vec::new();
    for student in locked_students {
        let index = index_by_key[student.as_str()];
        if !fixed_students.contains(&index) {
            fixed_students.push(index);
        }
    }
    for seat in locked_seats {
        let occupant = student_by_seat[seat.as_str()].clone();
        let index = index_by_key[occupant.as_str()];
        if !fixed_students.contains(&index) {
            fixed_students.push(index);
        }
    }
    if !affected_students.is_empty() {
        let mut affected_indices: Vec<usize> = affected_students
            .iter()
            .map(|student| index_by_key[student.as_str()])
            .collect();
        // One-hop closure via hard pair rules.
        let pair_rules: Vec<[usize; 2]> = request
            .must_be_adjacent
            .iter()
            .chain(request.cannot_be_adjacent.iter())
            .copied()
            .collect();
        let mut grew = true;
        while grew {
            grew = false;
            for pair in &pair_rules {
                if affected_indices.contains(&pair[0]) && !affected_indices.contains(&pair[1]) {
                    affected_indices.push(pair[1]);
                    grew = true;
                }
                if affected_indices.contains(&pair[1]) && !affected_indices.contains(&pair[0]) {
                    affected_indices.push(pair[0]);
                    grew = true;
                }
            }
        }
        for index in 0..request.student_count {
            if !affected_indices.contains(&index) && !fixed_students.contains(&index) {
                fixed_students.push(index);
            }
        }
    }

    // Express repair anchors as additional fixed seats. The request's original
    // fixed-seat rules remain authoritative even when the fixed student is in
    // the affected (movable) set.
    let original_fixed_seats = request.fixed_seats.clone();
    let original_fixed_by_student: HashMap<usize, usize> = original_fixed_seats
        .iter()
        .map(|[student, seat]| (*student, *seat))
        .collect();
    let original_fixed_by_seat: HashMap<usize, usize> = original_fixed_seats
        .iter()
        .map(|[student, seat]| (*seat, *student))
        .collect();
    let mut repair_anchors: Vec<[usize; 2]> = Vec::new();
    for index in fixed_students {
        let student_key = students[index].key.clone();
        let seat_id = seat_by_student
            .get(&student_key)
            .ok_or_else(|| format!("Student has no current seat: {student_key}."))?;
        let seat_index = seat_index_by_id
            .get(seat_id.as_str())
            .ok_or_else(|| format!("Current seat is unknown: {seat_id}."))?;

        if let Some(original_seat) = original_fixed_by_student.get(&index) {
            if original_seat != seat_index {
                return Err(format!(
                    "Repair anchor conflicts with the original fixed-seat rule: student \
                     {student_key} is fixed to seat index {original_seat}, but the repair \
                     anchor requires {seat_id} (index {seat_index})."
                ));
            }
            // The identical pair already exists in `original_fixed_seats`.
            continue;
        }
        if let Some(original_student) = original_fixed_by_seat.get(seat_index) {
            let original_student_key = &students[*original_student].key;
            return Err(format!(
                "Repair anchor conflicts with the original fixed-seat rule: seat {seat_id} \
                 (index {seat_index}) is fixed to student {original_student_key}, but the \
                 repair anchor requires student {student_key}."
            ));
        }
        repair_anchors.push([index, *seat_index]);
    }
    request.fixed_seats = original_fixed_seats;
    request.fixed_seats.extend(repair_anchors);
    // Re-run static conflict detection now that repair anchors have been
    // merged with the original hard rules (also catches anchor/anchor clashes).
    validate_solve_request(&request)
        .map_err(|error| format!("Repair constraints are invalid: {error}"))?;

    let response = solve_problem(&request)?;
    if !response.feasible {
        return Err(format!(
            "Repair solve did not find a legal seating (status {}).",
            response.status.as_str()
        ));
    }
    // Boundary validation is intentionally repeated here: repair must not
    // publish a snapshot unless the response satisfies both the original hard
    // rules and every repair anchor in the merged request.
    validate_solve_response(&request, &response)
        .map_err(|error| format!("Repair solve returned an invalid result: {error}"))?;

    // Build the repaired snapshot (frontend shape) + summary.
    let mut assignments: Vec<Value> = Vec::new();
    let mut moved = 0;
    let mut unseated = 0;
    for [student, seat] in &response.assignment {
        let student_key = students[*student].key.clone();
        let seat_id = seat_ids[*seat].clone();
        let display_name = students[*student]
            .display_name
            .clone()
            .unwrap_or_else(|| student_key.clone());
        if let Some(previous) = seat_by_student.get(&student_key) {
            if previous != &seat_id {
                moved += 1;
            }
        } else {
            unseated += 1;
        }
        assignments.push(json!({
            "student_key": student_key,
            "student_name": display_name,
            "seat_id": seat_id,
        }));
    }

    let repaired = json!({
        "assignments": assignments,
        "solver_status": response.status.as_str(),
        "seed": request.seed,
        "summary": {
            "moved_students": moved,
            "unseated_students": unseated,
            "locked_students": locked_students.len(),
            "locked_seats": locked_seats.len(),
        },
    });
    serde_json::to_string(&repaired)
        .map_err(|error| format!("could not serialize repair result: {error}"))
}

const REPORT_POSITION_CATEGORIES: [&str; 10] = [
    "front",
    "back",
    "middle",
    "side",
    "corner",
    "near_window",
    "near_door",
    "near_platform",
    "near_ac",
    "unknown",
];
const REPORT_PAIR_RELATIONS: [&str; 6] = [
    "desk_mate",
    "horizontal",
    "vertical",
    "diagonal",
    "adjacent_any",
    "within_distance",
];
const PAIR_REPORT_RECENT_LOOKBACK: usize = 4;

struct HistoryStudentAccumulator {
    student_name: Option<String>,
    total_assignments: u64,
    seat_counts: BTreeMap<String, u64>,
    category_counts: BTreeMap<String, u64>,
    records: Vec<Value>,
}

/// Fairness report over historical snapshots, retaining all current students
/// even when no snapshot contains an assignment for them. Malformed snapshot
/// entries are rejected instead of silently disappearing; semantic history
/// gaps are reported as warnings, matching the Python report contract.
pub fn history_report_json(request_json: &str, snapshots_json: &str) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let students = effective_students(&request);
    let layout = effective_layout(&request);
    let snapshots: Vec<Value> = serde_json::from_str(snapshots_json)
        .map_err(|error| format!("invalid snapshots document: {error}"))?;
    let seat_by_id: HashMap<&str, &Seat> = layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();
    let known_students: HashSet<&str> = students
        .iter()
        .map(|student| student.key.as_str())
        .collect();
    let mut per_student: BTreeMap<String, HistoryStudentAccumulator> = students
        .iter()
        .map(|student| {
            (
                student.key.clone(),
                HistoryStudentAccumulator {
                    student_name: student.display_name.clone(),
                    total_assignments: 0,
                    seat_counts: BTreeMap::new(),
                    category_counts: BTreeMap::new(),
                    records: Vec::new(),
                },
            )
        })
        .collect();
    let mut totals: BTreeMap<String, u64> = REPORT_POSITION_CATEGORIES
        .iter()
        .map(|category| ((*category).to_string(), 0))
        .collect();
    let mut warnings: Vec<String> = Vec::new();

    for (snapshot_offset, snapshot) in snapshots.iter().enumerate() {
        let snapshot_index = snapshot_offset + 1;
        let parsed =
            parse_snapshot_assignments(snapshot, &format!("history snapshot {snapshot_index}"))?;
        let mut assignments: BTreeMap<String, String> = BTreeMap::new();
        let mut seat_owner: HashMap<String, String> = HashMap::new();
        for assignment in parsed {
            if !known_students.contains(assignment.student_key.as_str()) {
                warnings.push(format!(
                    "history snapshot {snapshot_index} references unknown student {:?}; skipped",
                    assignment.student_key
                ));
                continue;
            }
            if assignments
                .insert(assignment.student_key.clone(), assignment.seat_id.clone())
                .is_some()
            {
                warnings.push(format!(
                    "history snapshot {snapshot_index} contains duplicate assignments for student {:?}; the last one was used",
                    assignment.student_key
                ));
            }
            if let Some(previous) =
                seat_owner.insert(assignment.seat_id.clone(), assignment.student_key.clone())
            {
                warnings.push(format!(
                    "history snapshot {snapshot_index} assigns seat {:?} to both {:?} and {:?}",
                    assignment.seat_id, previous, assignment.student_key
                ));
            }
        }
        let missing: Vec<&str> = known_students
            .iter()
            .copied()
            .filter(|student| !assignments.contains_key(*student))
            .collect();
        if !missing.is_empty() {
            warnings.push(format!(
                "history snapshot {snapshot_index} is missing {} current student(s)",
                missing.len()
            ));
        }

        for (student_key, seat_id) in assignments {
            let student = per_student
                .get_mut(&student_key)
                .expect("known student accumulators are pre-initialized");
            let (categories, unknown_seat, disabled_seat) = match seat_by_id.get(seat_id.as_str()) {
                None => {
                    warnings.push(format!(
                        "history snapshot {snapshot_index} references unknown seat_id {seat_id:?} for student {student_key:?}; marked as unknown"
                    ));
                    (vec!["unknown".to_string()], true, false)
                }
                Some(seat) if !seat.enabled => {
                    warnings.push(format!(
                        "history snapshot {snapshot_index} references disabled seat_id {seat_id:?} for student {student_key:?}; categories skipped"
                    ));
                    (Vec::new(), false, true)
                }
                Some(seat) => {
                    let mut categories: Vec<String> =
                        classify_seat_position(seat, &layout).into_iter().collect();
                    categories.sort();
                    (categories, false, false)
                }
            };
            student.total_assignments += 1;
            *student.seat_counts.entry(seat_id.clone()).or_default() += 1;
            for category in &categories {
                *student.category_counts.entry(category.clone()).or_default() += 1;
                *totals.entry(category.clone()).or_default() += 1;
            }
            student.records.push(json!({
                "snapshot_index": snapshot_index,
                "seat_id": seat_id,
                "categories": categories,
                "unknown_seat": unknown_seat,
                "disabled_seat": disabled_seat,
            }));
        }
    }

    let student_values: Vec<Value> = per_student
        .iter()
        .map(|(student_key, student)| {
            json!({
                "student_key": student_key,
                "student_name": student.student_name,
                "total_assignments": student.total_assignments,
                "seat_counts": student.seat_counts,
                "category_counts": student.category_counts,
                "records": student.records,
            })
        })
        .collect();
    let mut category_spread: BTreeMap<String, Value> = BTreeMap::new();
    for category in REPORT_POSITION_CATEGORIES {
        let counts: Vec<u64> = per_student
            .values()
            .map(|student| student.category_counts.get(category).copied().unwrap_or(0))
            .collect();
        let min = counts.iter().copied().min().unwrap_or(0);
        let max = counts.iter().copied().max().unwrap_or(0);
        category_spread.insert(
            category.to_string(),
            json!({ "min": min, "max": max, "spread": max - min }),
        );
    }

    let report = json!({
        "history_count": snapshots.len(),
        "student_count": request.student_count,
        "category_totals": totals,
        "students": student_values,
        "summary": {
            "category_spread": category_spread,
            "warning_count": warnings.len(),
        },
        "warnings": warnings,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize history report: {error}"))
}

struct PairReportAccumulator {
    pair_key: String,
    first_student_key: String,
    second_student_key: String,
    first_student_name: Option<String>,
    second_student_name: Option<String>,
    total_occurrences: u64,
    recent_occurrences: u64,
    relation_counts: BTreeMap<String, u64>,
    records: Vec<Value>,
}

fn pair_report_value(pair: &PairReportAccumulator) -> Value {
    json!({
        "pair_key": pair.pair_key,
        "first_student_key": pair.first_student_key,
        "second_student_key": pair.second_student_key,
        "first_student_name": pair.first_student_name,
        "second_student_name": pair.second_student_name,
        "total_occurrences": pair.total_occurrences,
        "relation_counts": pair.relation_counts,
        "records": pair.records,
    })
}

fn rank_pairs_for_relation<'a>(
    pairs: impl Iterator<Item = &'a PairReportAccumulator>,
    relation: &str,
    top: usize,
) -> Vec<&'a PairReportAccumulator> {
    let mut ranked: Vec<&PairReportAccumulator> = pairs
        .filter(|pair| pair.relation_counts.get(relation).copied().unwrap_or(0) > 0)
        .collect();
    ranked.sort_by(|left, right| {
        right
            .relation_counts
            .get(relation)
            .copied()
            .unwrap_or(0)
            .cmp(&left.relation_counts.get(relation).copied().unwrap_or(0))
            .then_with(|| right.total_occurrences.cmp(&left.total_occurrences))
            .then_with(|| left.pair_key.cmp(&right.pair_key))
    });
    ranked.truncate(top);
    ranked
}

/// Pair-history report with Python-compatible pair records and relation
/// rankings. `top` and `within_distance` are contract inputs, so invalid
/// values are rejected rather than silently coerced.
pub fn pair_report_json(
    request_json: &str,
    snapshots_json: &str,
    top: usize,
    within_distance: i32,
) -> Result<String, String> {
    if top == 0 {
        return Err("invalid top: expected a positive value".to_string());
    }
    if within_distance <= 0 {
        return Err("invalid within_distance: expected a positive value".to_string());
    }
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let students = effective_students(&request);
    let student_names: HashMap<&str, Option<String>> = students
        .iter()
        .map(|student| (student.key.as_str(), student.display_name.clone()))
        .collect();
    let known_students: HashSet<&str> = student_names.keys().copied().collect();
    let layout = effective_layout(&request);
    let snapshots: Vec<Value> = serde_json::from_str(snapshots_json)
        .map_err(|error| format!("invalid snapshots document: {error}"))?;
    let seat_by_id: HashMap<&str, &Seat> = layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();
    let mut pairs: BTreeMap<String, PairReportAccumulator> = BTreeMap::new();
    let mut relation_totals: BTreeMap<String, u64> = REPORT_PAIR_RELATIONS
        .iter()
        .map(|relation| ((*relation).to_string(), 0))
        .collect();
    let mut warnings: Vec<String> = Vec::new();
    let recent_start = snapshots.len().saturating_sub(PAIR_REPORT_RECENT_LOOKBACK) + 1;

    for (snapshot_offset, snapshot) in snapshots.iter().enumerate() {
        let snapshot_index = snapshot_offset + 1;
        let parsed = parse_snapshot_assignments(
            snapshot,
            &format!("pair-history snapshot {snapshot_index}"),
        )?;
        let mut by_student: BTreeMap<String, String> = BTreeMap::new();
        for assignment in parsed {
            if !known_students.contains(assignment.student_key.as_str()) {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} references unknown student {:?}; skipped",
                    assignment.student_key
                ));
                continue;
            }
            if by_student
                .insert(assignment.student_key.clone(), assignment.seat_id)
                .is_some()
            {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} contains duplicate assignments for student {:?}; the last one was used",
                    assignment.student_key
                ));
            }
        }
        let missing = known_students
            .iter()
            .filter(|student| !by_student.contains_key(**student))
            .count();
        if missing > 0 {
            warnings.push(format!(
                "pair-history snapshot {snapshot_index} is missing {missing} current student(s)"
            ));
        }

        let mut known: Vec<(&str, &str, &Seat)> = Vec::new();
        let mut occupied: HashMap<&str, &str> = HashMap::new();
        for (student_key, seat_id) in &by_student {
            let Some(seat) = seat_by_id.get(seat_id.as_str()).copied() else {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} references unknown seat_id {seat_id:?} for student {student_key:?}; pair relations skipped"
                ));
                continue;
            };
            if let Some(previous) = occupied.insert(seat_id.as_str(), student_key.as_str()) {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} assigns seat {seat_id:?} to both {previous:?} and {student_key:?}; later assignment skipped"
                ));
                continue;
            }
            known.push((student_key.as_str(), seat_id.as_str(), seat));
        }
        for first in 0..known.len() {
            for second in (first + 1)..known.len() {
                let (first_key, first_seat_id, first_seat) = known[first];
                let (second_key, second_seat_id, second_seat) = known[second];
                let mut relations: Vec<String> = detect_neighbor_relation_types(
                    first_seat,
                    second_seat,
                    &layout,
                    None,
                    within_distance,
                )
                .into_iter()
                .collect();
                if relations.is_empty() {
                    continue;
                }
                relations.sort();
                let pair_key = format!("{first_key}|{second_key}");
                let pair = pairs
                    .entry(pair_key.clone())
                    .or_insert_with(|| PairReportAccumulator {
                        pair_key,
                        first_student_key: first_key.to_string(),
                        second_student_key: second_key.to_string(),
                        first_student_name: student_names.get(first_key).cloned().flatten(),
                        second_student_name: student_names.get(second_key).cloned().flatten(),
                        total_occurrences: 0,
                        recent_occurrences: 0,
                        relation_counts: BTreeMap::new(),
                        records: Vec::new(),
                    });
                pair.total_occurrences += 1;
                if snapshot_index >= recent_start {
                    pair.recent_occurrences += 1;
                }
                for relation in &relations {
                    *pair.relation_counts.entry(relation.clone()).or_default() += 1;
                    *relation_totals.entry(relation.clone()).or_default() += 1;
                }
                let row_delta = (first_seat.row - second_seat.row).unsigned_abs();
                let col_delta = (first_seat.col - second_seat.col).unsigned_abs();
                pair.records.push(json!({
                    "snapshot_index": snapshot_index,
                    "first_seat_id": first_seat_id,
                    "second_seat_id": second_seat_id,
                    "relations": relations,
                    "row_delta": row_delta,
                    "col_delta": col_delta,
                    "chebyshev_distance": row_delta.max(col_delta),
                    "manhattan_distance": row_delta + col_delta,
                    "first_seat_disabled": !first_seat.enabled,
                    "second_seat_disabled": !second_seat.enabled,
                }));
            }
        }
    }

    let pair_values: Vec<Value> = pairs.values().map(pair_report_value).collect();
    let top_desk_mates: Vec<Value> = rank_pairs_for_relation(pairs.values(), "desk_mate", top)
        .into_iter()
        .map(pair_report_value)
        .collect();
    let top_adjacent_pairs: Vec<Value> =
        rank_pairs_for_relation(pairs.values(), "adjacent_any", top)
            .into_iter()
            .map(pair_report_value)
            .collect();
    let mut ranked: Vec<&PairReportAccumulator> = pairs.values().collect();
    ranked.sort_by(|left, right| {
        right
            .total_occurrences
            .cmp(&left.total_occurrences)
            .then_with(|| left.pair_key.cmp(&right.pair_key))
    });
    let repeated = ranked
        .iter()
        .filter(|pair| pair.total_occurrences > 1)
        .count();
    let max_occurrences = ranked
        .first()
        .map(|pair| pair.total_occurrences)
        .unwrap_or(0);

    // Retain the compact anonymized legacy view for current Rust consumers,
    // but compute "recent" from the Python-default four-snapshot lookback.
    let mut anonymized_students: BTreeMap<String, usize> = BTreeMap::new();
    let mut legacy_top_pairs: Vec<Value> = Vec::new();
    for pair in ranked.iter().take(top) {
        let next_first = anonymized_students.len() + 1;
        let first = *anonymized_students
            .entry(pair.first_student_key.clone())
            .or_insert(next_first);
        let next_second = anonymized_students.len() + 1;
        let second = *anonymized_students
            .entry(pair.second_student_key.clone())
            .or_insert(next_second);
        legacy_top_pairs.push(json!({
            "student_a": format!("student-{first}"),
            "student_b": format!("student-{second}"),
            "total_occurrences": pair.total_occurrences,
            "recent_occurrences": pair.recent_occurrences,
        }));
    }

    let report = json!({
        "history_count": snapshots.len(),
        "student_count": request.student_count,
        "pair_count": pairs.len(),
        "within_distance_metric": "chebyshev",
        "within_distance": within_distance,
        "relation_totals": relation_totals,
        "top_desk_mates": top_desk_mates,
        "top_adjacent_pairs": top_adjacent_pairs,
        "pairs": pair_values,
        "repeated_pair_count": repeated,
        "max_occurrences": max_occurrences,
        "top_pairs": legacy_top_pairs,
        "summary": {
            "warning_count": warnings.len(),
            "within_distance_metric": "chebyshev",
            "within_distance": within_distance,
            "recent_lookback": PAIR_REPORT_RECENT_LOOKBACK,
        },
        "warnings": warnings,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize pair report: {error}"))
}

/// Candidate set generation (plan §6.3): repeated seeded solves with exact
/// assignment exclusion, so candidates are not just different seeds of the
/// same plan. Every candidate is hard-validated before it enters the set;
/// the report carries per-candidate seed, cost, and assignment distance to
/// the recommended plan plus reproducibility metadata.
///
/// `candidate_count` caps the set (1..=20); `attempt_limit` bounds the
/// generation loop. Mirrors the Python `candidates.generate_candidate_set`
/// strategy (seeded repeated solve + exclusion).
#[derive(Debug)]
struct GeneratedCandidate {
    candidate_id: String,
    seed: u64,
    attempts_used: usize,
    total_cost: Option<f64>,
    assignment: Vec<usize>,
    assignment_pairs: Vec<[usize; 2]>,
}

fn assignment_distance(first: &[usize], second: &[usize]) -> f64 {
    if first.is_empty() {
        return 0.0;
    }
    first
        .iter()
        .zip(second.iter())
        .filter(|(left, right)| left != right)
        .count() as f64
        / first.len() as f64
}

fn derive_candidate_seed(base_seed: u64, attempt_index: usize) -> u64 {
    base_seed.wrapping_add(attempt_index as u64)
}

pub fn generate_candidates_json(
    request_json: &str,
    candidate_count: usize,
) -> Result<String, String> {
    if !(1..=20).contains(&candidate_count) {
        return Err(format!(
            "invalid candidate_count {candidate_count}: expected a value between 1 and 20"
        ));
    }
    let mut request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let base_seed = request.seed;
    let attempt_limit = candidate_count * 12 + 8;

    let mut candidates: Vec<GeneratedCandidate> = Vec::new();
    let mut seen: Vec<Vec<usize>> = Vec::new();
    let mut failed_attempts = 0;

    for attempt_index in 0..attempt_limit {
        if candidates.len() >= candidate_count {
            break;
        }
        // Seed derivation is independent for every attempt; never feed the
        // previous derived seed back into the next derivation.
        request.seed = derive_candidate_seed(base_seed, attempt_index);
        let control = SolveControl::new();
        let response = solve_problem_internal(&request, &control, &seen)?;
        if !response.feasible {
            failed_attempts += 1;
            // With exact no-goods installed, exhaustive infeasibility means
            // there are no additional distinct assignments to generate.
            if response.status == SolveStatus::ProvenInfeasible {
                break;
            }
            continue;
        }
        validate_solve_response(&request, &response)?;
        let mut assignment: Vec<usize> = vec![usize::MAX; request.student_count];
        for [student, seat] in &response.assignment {
            assignment[*student] = *seat;
        }
        if seen.iter().any(|existing| existing == &assignment) {
            return Err(
                "candidate solver violated exact-assignment exclusion by returning a duplicate"
                    .to_string(),
            );
        }
        seen.push(assignment.clone());
        candidates.push(GeneratedCandidate {
            candidate_id: format!("candidate_{:02}", candidates.len() + 1),
            seed: request.seed,
            attempts_used: response.attempts_used,
            total_cost: response.total_cost,
            assignment,
            assignment_pairs: response.assignment,
        });
    }

    if candidates.is_empty() {
        return Err("candidate generation did not produce any feasible plan".to_string());
    }
    let mut warnings: Vec<String> = Vec::new();
    if candidates.len() < candidate_count {
        warnings.push(format!(
            "requested {candidate_count} candidates but generated {} distinct feasible plans",
            candidates.len()
        ));
    }
    if failed_attempts > 0 {
        warnings.push(format!(
            "{failed_attempts} generation attempts did not produce an additional distinct feasible plan"
        ));
    }

    // PlanScore per candidate (plan §6.2/§6.6): mirror Python's
    // `apply_diversity_scores` + `score_snapshot`. Diversity is the mean
    // assignment distance to every other candidate; stability stays
    // not_available here (the core history model keeps categories, not raw
    // seat ids — the fixed-assignment scoring path covers stability parity).
    let request_json = request_json.to_string();
    let mut diversities = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        let distances: f64 = candidates
            .iter()
            .filter(|other| other.candidate_id != candidate.candidate_id)
            .map(|other| assignment_distance(&candidate.assignment, &other.assignment))
            .sum();
        let mean_distance = if candidates.len() > 1 {
            distances / (candidates.len() - 1) as f64
        } else {
            0.0
        };
        diversities.push(mean_distance);
    }
    let mut plan_scores: Vec<Value> = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let score = score_assignment_json(
            &request_json,
            &candidate.assignment_pairs,
            "",
            Some(diversities[index]),
        )
        .map_err(|error| format!("candidate {index} could not be scored: {error}"))?;
        plan_scores.push(serde_json::from_str(&score).map_err(|error| {
            format!("candidate {index} produced a malformed plan score: {error}")
        })?);
    }

    // Recommend the highest PlanScore total, mirroring Python's
    // `refresh_recommendation` (sorted by -total_score, then candidate_id),
    // then calculate every distance against that actual recommendation.
    let recommended_index = plan_scores
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            let left_total = left["total"].as_f64().unwrap_or(0.0);
            let right_total = right["total"].as_f64().unwrap_or(0.0);
            left_total
                .partial_cmp(&right_total)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    candidates[*left_index]
                        .candidate_id
                        .cmp(&candidates[*right_index].candidate_id)
                })
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    let recommended = candidates[recommended_index].candidate_id.clone();
    let recommended_assignment = candidates[recommended_index].assignment.clone();
    let candidate_values: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            json!({
                "candidate_id": candidate.candidate_id,
                "seed": candidate.seed,
                "attempts_used": candidate.attempts_used,
                "total_cost": candidate.total_cost,
                "hard_constraints_satisfied": true,
                "distance_to_best": assignment_distance(
                    &candidate.assignment,
                    &recommended_assignment,
                ),
                "plan_score": plan_scores[index],
                "assignment": candidate.assignment_pairs,
            })
        })
        .collect();

    let report = json!({
        "api_version": NATIVE_API_VERSION,
        "candidate_count": candidate_values.len(),
        "requested_candidate_count": candidate_count,
        "base_seed": base_seed,
        "generation_method": "seeded repeated solve with exact-assignment exclusion",
        "recommended_candidate_id": recommended,
        "candidates": candidate_values,
        "warnings": warnings,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize candidate report: {error}"))
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
fn adjacency_edges_by_seat_id(layout: &Layout, edges: &[[usize; 2]]) -> HashSet<(String, String)> {
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

/// Stop the greedy attempt loop after this many consecutive attempts that
/// did not improve the best plan (plan §6.6 interactive response). The loop
/// stays deterministic: the attempt order is seed-driven, so the same
/// version/input/seed reproduces the same result, while easy problems stop
/// after a short plateau instead of spending the full `n*12` attempts.
const GREEDY_STAGNATION_LIMIT: usize = 48;

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
fn local_search(
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
fn local_search_controlled(
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
enum GreedyOutcome {
    Found(Vec<usize>),
    DeadEnd,
    Stopped(StopReason),
}

#[cfg(test)]
fn greedy_attempt(
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
fn greedy_attempt_controlled(
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
        domains.push(CandidateDomain {
            student,
            seats,
            excluded,
        });
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
    DeadlineExceeded,
    Cancelled,
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
#[cfg(test)]
fn hard_search_with_budget(
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

fn hard_search_controlled(
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
    let mut domains: Vec<Vec<usize>> =
        build_candidate_domains(request, resolved, adjacency, graph_distances)
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

fn validate_solve_request(request: &CoreSolveRequest) -> Result<(), String> {
    if request.api_version != NATIVE_API_VERSION {
        return Err(format!(
            "unsupported native solve api_version {}; expected {}",
            request.api_version, NATIVE_API_VERSION
        ));
    }
    if request.student_count == 0 {
        return Err("native solve requires at least one student".to_string());
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
    if !request.students.is_empty() {
        let mut student_keys: HashSet<&str> = HashSet::new();
        for student in &request.students {
            if student.key.trim().is_empty() {
                return Err("students require non-empty keys".to_string());
            }
            if !student_keys.insert(student.key.as_str()) {
                return Err(format!("duplicate student key: {:?}", student.key));
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
        .any(|score| !score.is_finite())
    {
        return Err("student scores must be finite numbers".to_string());
    }
    if request
        .time_limit_seconds
        .is_some_and(|seconds| !seconds.is_finite() || seconds <= 0.0)
    {
        return Err(
            "invalid time_limit_seconds: expected a finite value greater than zero".to_string(),
        );
    }
    if let Some(layout) = &request.layout {
        if layout.seats.len() < seat_count {
            return Err(
                "layout must describe at least as many seats as seat_positions".to_string(),
            );
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
        if pair[0] == pair[1] {
            return Err("invalid pair rule: must reference two different students".to_string());
        }
    }
    for rule in &request.min_distance {
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
        assigned_students_meet_distance, assignment_is_unique, audit_report_json,
        build_candidate_domains, build_cost_context, build_graph_distance_matrix,
        build_index_adjacency, classify_solve_error, evaluate_problem_json,
        full_solution_total_cost, generate_candidates_json, greedy_attempt,
        hard_search_with_budget, history_report_json, local_search, maximum_candidate_matching,
        pair_report_json, precheck_report_json, repair_json, resolve_group_rules,
        score_assignment_json, seat_distance, solve_problem_json, solve_problem_with_control,
        validate_assignment, validate_solve_request, validate_solve_request_json,
        validate_solve_response, CoreEvaluationResponse, CoreSolveRequest, CoreSolveResponse,
        SearchOutcome, SolveControl, SolveStatus, SplitMix64, HARD_SEARCH_NODE_BUDGET,
        NATIVE_API_VERSION,
    };
    use serde_json::{json, Value};

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
    fn validate_rejects_empty_class_and_invalid_student_keys() {
        let empty_class = r#"{
            "api_version": 2,
            "student_count": 0,
            "seat_positions": [[0.0, 0.0]]
        }"#;
        let error = validate_solve_request_json(empty_class).unwrap_err();
        assert!(error.contains("at least one student"), "{error}");

        let empty_key = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "students": [{"key": "   "}]
        }"#;
        let error = validate_solve_request_json(empty_key).unwrap_err();
        assert!(error.contains("non-empty keys"), "{error}");

        let duplicate_key = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "students": [{"key": "same"}, {"key": "same"}]
        }"#;
        let error = validate_solve_request_json(duplicate_key).unwrap_err();
        assert!(error.contains("duplicate student key"), "{error}");
    }

    #[test]
    fn validate_rejects_non_positive_or_non_finite_time_limit() {
        let negative = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "time_limit_seconds": -1.0
        }"#;
        let error = validate_solve_request_json(negative).unwrap_err();
        assert!(error.contains("time_limit_seconds"), "{error}");

        let mut request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 1,
                "seat_positions": [[0.0, 0.0]]
            }"#,
        )
        .unwrap();
        request.time_limit_seconds = Some(f64::NAN);
        let error = validate_solve_request(&request).unwrap_err();
        assert!(error.contains("finite"), "{error}");

        request.time_limit_seconds = Some(0.0);
        let error = validate_solve_request(&request).unwrap_err();
        assert!(error.contains("greater than zero"), "{error}");
    }

    #[test]
    fn validate_rejects_self_referential_pair_and_distance_rules() {
        let pair = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "must_be_adjacent": [[0, 0]]
        }"#;
        let error = validate_solve_request_json(pair).unwrap_err();
        assert!(error.contains("two different students"), "{error}");

        let distance = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "min_distance": [
                {"students": [0, 0], "distance": 1.0, "metric": "graph"}
            ]
        }"#;
        let error = validate_solve_request_json(distance).unwrap_err();
        assert!(error.contains("two different students"), "{error}");
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
        let mut seats = response
            .assignment
            .iter()
            .map(|pair| pair[1])
            .collect::<Vec<_>>();
        seats.sort_unstable();
        assert_eq!(seats, vec![0, 1, 2]);
    }

    #[test]
    fn solves_single_student_single_seat_without_local_search_panic() {
        let request = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "students": [{"key": "only"}],
            "seed": 1
        }"#;

        let response_json = solve_problem_json(request).expect("single-student solve succeeds");
        let response: CoreSolveResponse = serde_json::from_str(&response_json).unwrap();
        assert_eq!(response.status, SolveStatus::Solved);
        assert_eq!(response.assignment, vec![[0, 0]]);
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
        assert!(
            total_cost.as_f64().is_some(),
            "feasible solve reports a number"
        );

        let infeasible_request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]],
            "seed": 1
        }"#;
        let response_json =
            solve_problem_json(infeasible_request).expect("request should be valid");
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert!(value
            .get("total_cost")
            .unwrap_or(&serde_json::Value::Null)
            .is_null());
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
        let payload_text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("cannot read 40-parity.json at {}: {error}", path.display())
        });
        let payload: serde_json::Value =
            serde_json::from_str(&payload_text).expect("reference payload should be valid JSON");
        let problem = payload
            .get("problem")
            .expect("reference has a problem block");
        let problem_json = serde_json::to_string(problem).expect("problem block serializes");

        let response_json = solve_problem_json(&problem_json).expect("native solve should run");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(
            response.feasible,
            "40-person parity problem must be feasible"
        );
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.assignment.len(), 40);
        let total_cost = response
            .total_cost
            .expect("feasible solve reports total_cost");
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
        assert_eq!(
            resolved.must_be_adjacent,
            vec![[0, 1], [0, 1], [0, 2], [1, 2]]
        );
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
        assert!(
            response.feasible,
            "groups A/B together and C/D apart must be feasible"
        );
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
        assert!(
            !response.feasible,
            "A and B are pinned apart but must sit together"
        );
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

    fn response_validation_request() -> CoreSolveRequest {
        serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
                "edges": [[0, 1]],
                "students": [{"key": "A"}, {"key": "B"}]
            }"#,
        )
        .unwrap()
    }

    fn structurally_valid_solved_response() -> CoreSolveResponse {
        CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: true,
            status: SolveStatus::Solved,
            assignment: vec![[0, 0], [1, 1]],
            attempts_used: 1,
            hard_constraints_satisfied: true,
            total_cost: Some(0.0),
        }
    }

    #[test]
    fn solve_response_validation_rejects_forged_success_flags() {
        let request = response_validation_request();
        let mut response = structurally_valid_solved_response();
        assert!(validate_solve_response(&request, &response).is_ok());

        response.api_version = 1;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("api_version"), "{error}");

        response = structurally_valid_solved_response();
        response.status = SolveStatus::Unknown;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("status must be Solved"), "{error}");

        response = structurally_valid_solved_response();
        response.feasible = false;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("feasible=true"), "{error}");

        response = structurally_valid_solved_response();
        response.hard_constraints_satisfied = false;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("hard_constraints_satisfied=true"), "{error}");
    }

    #[test]
    fn solve_response_validation_rejects_duplicate_and_out_of_range_indices() {
        let request = response_validation_request();

        let mut response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [0, 1]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("student 0 more than once"), "{error}");

        response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [1, 0]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("seat 0 more than once"), "{error}");

        response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [2, 1]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("out-of-range student 2"), "{error}");

        response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [1, 2]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("out-of-range seat 2"), "{error}");
    }

    #[test]
    fn solve_response_validation_rechecks_group_derived_hard_rules() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
                "edges": [[0, 1], [1, 2]],
                "students": [{"key": "A"}, {"key": "B"}],
                "rules": {
                    "groups": [
                        {"name": "together", "students": ["A", "B"], "together": true}
                    ]
                }
            }"#,
        )
        .unwrap();
        let response = CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: true,
            status: SolveStatus::Solved,
            assignment: vec![[0, 0], [1, 2]],
            attempts_used: 1,
            hard_constraints_satisfied: true,
            total_cost: Some(0.0),
        };
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("violates a hard rule"), "{error}");
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
        assert!(
            error.contains("do not satisfy a must_be_adjacent rule"),
            "{error}"
        );

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
        assert!(
            error.contains("violate a cannot_be_adjacent rule"),
            "{error}"
        );

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

        let outcome = hard_search_with_budget(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            200_000,
            None,
        );
        let SearchOutcome::Found(assignment) = outcome else {
            panic!("hard search should find the far-apart placement, got {outcome:?}");
        };
        // Student 0 and 1 must be >= 3 hops apart: only opposite corners work
        // in this 2x3 ladder (e.g. 0->seat 0 and 1->seat 5 is 3 hops).
        let probe: Vec<Option<usize>> = assignment.iter().map(|seat| Some(*seat)).collect();
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
        let outcome =
            hard_search_with_budget(&request, &resolved, &adjacency, &graph_distances, 1, None);
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

    #[test]
    fn cancelled_control_reports_cancelled_before_any_incumbent() {
        // Cooperative cancellation (plan §6.1): a control cancelled before
        // the solve starts must terminate with the Cancelled status and
        // never produce an incumbent.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 8,
                "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
                "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
                "seed": 7
            }"#,
        )
        .expect("request should parse");
        let control = SolveControl::new();
        control.cancel();
        let response =
            solve_problem_with_control(&request, &control).expect("solve should terminate");
        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::Cancelled);
        assert!(response.assignment.is_empty());
        // A fresh control on the same request still solves normally.
        let response =
            solve_problem_with_control(&request, &SolveControl::new()).expect("solve should run");
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

        let initial = greedy_attempt(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &mut rng,
            &ctx,
            0,
        )
        .expect("greedy should seat everyone");
        let before = full_solution_total_cost(&initial, &adjacency, &ctx);

        let improved = local_search(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &initial,
            &ctx,
            &mut rng,
        );
        let after = full_solution_total_cost(&improved, &adjacency, &ctx);

        assert!(after <= before + 1e-9, "cost worsened: {before} -> {after}");
        validate_assignment(&request, &resolved, &adjacency, &graph_distances, &improved)
            .expect("local search must keep the assignment legal");

        // Determinism: same seed, same input -> identical output. Replay the
        // same RNG consumption (greedy first, then local search).
        let mut rng2 = SplitMix64::new(42);
        let _ = greedy_attempt(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &mut rng2,
            &ctx,
            0,
        )
        .expect("greedy should seat everyone");
        let rerun = local_search(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &initial,
            &ctx,
            &mut rng2,
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
        let report: serde_json::Value =
            serde_json::from_str(&audit_report_json(request, &assignment).unwrap()).unwrap();

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
    #[test]
    fn candidate_set_is_diverse_and_fully_validated() {
        let request = r#"{
            "api_version": 2,
            "student_count": 10,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0],[0.0,2.0],[1.0,2.0],[2.0,2.0],[3.0,2.0]],
            "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[8,9],[9,10],[10,11],[0,4],[1,5],[2,6],[3,7],[4,8],[5,9],[6,10],[7,11]],
            "students": [
                {"key":"s0","score":100.0},{"key":"s1","score":10.0},{"key":"s2","score":95.0},{"key":"s3","score":15.0},
                {"key":"s4","score":90.0},{"key":"s5","score":20.0},{"key":"s6","score":85.0},{"key":"s7","score":25.0},
                {"key":"s8","score":80.0},{"key":"s9","score":30.0}
            ],
            "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}},
            "seed": 42
        }"#;
        let report: serde_json::Value =
            serde_json::from_str(&generate_candidates_json(request, 3).unwrap()).unwrap();

        let candidates = report["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 3, "requested 3 candidates");
        assert_eq!(report["requested_candidate_count"], 3);
        assert!(report["recommended_candidate_id"].is_string());
        assert_eq!(report["base_seed"], 42);
        assert_eq!(
            report["generation_method"],
            "seeded repeated solve with exact-assignment exclusion"
        );

        // Every candidate is distinct, hard-validated, and carries the
        // reproducibility + diversity metadata.
        let mut assignments: Vec<Vec<[usize; 2]>> = Vec::new();
        for candidate in candidates {
            assert_eq!(candidate["hard_constraints_satisfied"], true);
            assert!(candidate["seed"].is_u64());
            assert!(candidate["total_cost"].is_number());
            assert!(candidate["distance_to_best"].is_number());
            let assignment: Vec<[usize; 2]> =
                serde_json::from_value(candidate["assignment"].clone()).unwrap();
            assert!(
                !assignments.contains(&assignment),
                "candidates must be distinct"
            );
            assignments.push(assignment);
        }
    }
    #[test]
    fn history_report_counts_categories_with_identifiers() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]],
            "students": [{"key":"S1","display_name":"Alice"},{"key":"S2","display_name":"Bob"}],
            "layout": {"layout_id": "l", "name": "l", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 0.0, "y": 0.0, "zone": "front", "enabled": true},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 1.0, "y": 0.0, "zone": "front", "enabled": true}
            ], "adjacency": {"edges": [["R1C1","R1C2"]]}}
        }"#;
        let snapshots = r#"[
            {"assignments": [{"student_key":"S1","seat_id":"R1C1"},{"student_key":"S2","seat_id":"R1C2"}]},
            {"assignments": [{"student_key":"S1","seat_id":"R1C2"},{"student_key":"S2","seat_id":"R1C1"}]}
        ]"#;
        let report: Value =
            serde_json::from_str(&history_report_json(request, snapshots).unwrap()).unwrap();
        assert_eq!(report["history_count"], 2);
        assert_eq!(report["student_count"], 2);
        // Both students sat in the front zone in both periods.
        assert!(report["category_totals"]["front"].as_u64().unwrap() >= 4);
        // Teacher-side report: identifiers are present (oracle contract,
        // mirroring Python's StudentSeatHistory). Anonymization happens at
        // the export/display boundary (teacher vs public templates), not in
        // the core report.
        let students = report["students"].as_array().unwrap();
        assert_eq!(students.len(), 2);
        assert!(report["students"][0]["student_key"].as_str().is_some());
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(serialized.contains("Alice") && serialized.contains("S1"));
    }

    #[test]
    fn plan_score_matches_python_semantics_for_a_fixed_assignment() {
        // The breakdown mirrors Python's `score_snapshot`: three enabled soft
        // rules produce available dimensions with Python's formulas, disabled
        // rules report not_available with the exact reasons, the weighted
        // total matches, and the hard summary counts every rule.
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "students": [
                {"key":"S1","display_name":"A","score":100.0,"height_cm":150.0,"vision":"poor"},
                {"key":"S2","display_name":"B","score":90.0,"height_cm":160.0,"vision":"poor"},
                {"key":"S3","display_name":"C","score":80.0,"height_cm":170.0,"vision":"normal"},
                {"key":"S4","display_name":"D","score":70.0,"height_cm":180.0,"vision":"normal"}
            ],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}},
            "rules": {"seed": 42, "soft": {
                "score_balance": {"enabled": true, "weight": 1},
                "height_back": {"enabled": true, "weight": 1},
                "vision_front": {"enabled": true, "weight": 20}
            }}
        }"#;
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2], [3, 3]];
        let report: Value =
            serde_json::from_str(&score_assignment_json(request, &assignment, "", None).unwrap())
                .unwrap();
        let breakdown = &report["breakdown"];
        // Enabled dimensions are available with a 0..100 score.
        for key in [
            "score_balance_score",
            "height_preference_score",
            "vision_preference_score",
        ] {
            assert_eq!(breakdown[key]["status"], "available", "{key}");
            let score = breakdown[key]["score"].as_f64().unwrap();
            assert!((0.0..=100.0).contains(&score), "{key}: {score}");
        }
        // Disabled / missing-input dimensions report the exact Python reason.
        assert_eq!(
            breakdown["fair_rotation_score"]["details"]["reason"],
            "fair_rotation is disabled."
        );
        assert_eq!(
            breakdown["stability_score"]["details"]["reason"],
            "No previous snapshot was supplied."
        );
        assert_eq!(
            breakdown["rule_scores"]["mentor_pairing_score"]["details"]["reason"],
            "mentor_pairing is disabled."
        );
        // The weighted total is a plain weighted average of available dims.
        let total = report["total"].as_f64().unwrap();
        let mut weighted = 0.0;
        let mut total_weight = 0.0;
        for key in [
            "score_balance_score",
            "height_preference_score",
            "vision_preference_score",
        ] {
            weighted += breakdown[key]["score"].as_f64().unwrap()
                * breakdown[key]["weight"].as_f64().unwrap();
            total_weight += breakdown[key]["weight"].as_f64().unwrap();
        }
        assert!(
            (total - weighted / total_weight).abs() < 0.01,
            "total {total} vs weighted average {}",
            weighted / total_weight
        );
        // The hard summary is satisfied with the base three integrity checks.
        assert_eq!(breakdown["hard_constraint_summary"]["satisfied"], true);
        assert_eq!(
            breakdown["hard_constraint_summary"]["checked_rule_count"],
            3
        );
        assert_eq!(breakdown["hard_constraint_summary"]["violation_count"], 0);

        // A rule-violating assignment is flagged: total 0 + a counted
        // violation (integrity failures like duplicate seats are rejected
        // outright by the completeness checks, so a fixed-seat violation is
        // the honest path here).
        let fixed_request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0, 1]],
            "students": [
                {"key":"S1","display_name":"A","score":100.0,"height_cm":150.0,"vision":"poor"},
                {"key":"S2","display_name":"B","score":90.0,"height_cm":160.0,"vision":"poor"},
                {"key":"S3","display_name":"C","score":80.0,"height_cm":170.0,"vision":"normal"},
                {"key":"S4","display_name":"D","score":70.0,"height_cm":180.0,"vision":"normal"}
            ],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}},
            "rules": {"seed": 42, "soft": {
                "score_balance": {"enabled": true, "weight": 1},
                "height_back": {"enabled": true, "weight": 1},
                "vision_front": {"enabled": true, "weight": 20}
            }}
        }"#;
        let violating: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2], [3, 3]];
        let report: Value = serde_json::from_str(
            &score_assignment_json(fixed_request, &violating, "", None).unwrap(),
        )
        .unwrap();
        assert_eq!(report["total"], 0.0);
        assert_eq!(
            report["breakdown"]["hard_constraint_summary"]["violation_count"],
            1
        );
        assert_eq!(
            report["breakdown"]["hard_constraint_summary"]["checked_rule_count"],
            4
        );
    }

    #[test]
    fn candidate_report_carries_plan_score_and_recommends_max_total() {
        let request = json!({
            "api_version": 2,
            "student_count": 8,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
            "students": (0..8).map(|index| json!({"key": format!("S{index}")})).collect::<Vec<_>>(),
            "rules": {"seed": 42, "soft": {}},
            "seed": 42
        });
        let report: Value =
            serde_json::from_str(&generate_candidates_json(&request.to_string(), 3).unwrap())
                .unwrap();
        let candidates = report["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 3);
        // Every candidate carries a plan_score; diversity is available
        // (multiple candidates), stability is not (no history in the core
        // request), and the recommended candidate has the max total.
        let totals: Vec<f64> = candidates
            .iter()
            .map(|candidate| candidate["plan_score"]["total"].as_f64().unwrap())
            .collect();
        for candidate in candidates {
            assert_eq!(
                candidate["plan_score"]["breakdown"]["diversity_score"]["status"],
                "available"
            );
            assert_eq!(
                candidate["plan_score"]["breakdown"]["stability_score"]["status"],
                "not_available"
            );
        }
        let recommended = report["recommended_candidate_id"].as_str().unwrap();
        let recommended_index = candidates
            .iter()
            .position(|candidate| candidate["candidate_id"].as_str() == Some(recommended))
            .unwrap();
        assert_eq!(
            totals[recommended_index],
            totals.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        );
    }

    #[test]
    fn pair_report_counts_repeated_pairs_and_relations() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0]],
            "edges": [[0,1],[1,2]],
            "students": [{"key":"S1"},{"key":"S2"},{"key":"S3"}],
            "layout": {"layout_id": "l", "name": "l", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 0.0, "y": 0.0, "zone": "front", "enabled": true},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 1.0, "y": 0.0, "zone": "front", "enabled": true},
                {"seat_id": "R1C3", "row": 1, "col": 3, "x": 2.0, "y": 0.0, "zone": "front", "enabled": true}
            ], "adjacency": {"edges": [["R1C1","R1C2"],["R1C2","R1C3"]]}}
        }"#;
        // S1-S2 sit adjacent in both periods: repeated pair with occurrences 2.
        let snapshots = r#"[
            {"assignments": [{"student_key":"S1","seat_id":"R1C1"},{"student_key":"S2","seat_id":"R1C2"},{"student_key":"S3","seat_id":"R1C3"}]},
            {"assignments": [{"student_key":"S1","seat_id":"R1C1"},{"student_key":"S2","seat_id":"R1C2"},{"student_key":"S3","seat_id":"R1C3"}]}
        ]"#;
        let report: Value =
            serde_json::from_str(&pair_report_json(request, snapshots, 10, 2).unwrap()).unwrap();
        assert_eq!(report["history_count"], 2);
        assert!(report["pair_count"].as_u64().unwrap() >= 1);
        assert!(report["repeated_pair_count"].as_u64().unwrap() >= 1);
        assert_eq!(report["max_occurrences"], 2);
        // Top pair is anonymized.
        let top = report["top_pairs"][0].clone();
        assert!(top["student_a"].as_str().unwrap().starts_with("student-"));
        assert_eq!(top["total_occurrences"], 2);
        // Desk-mate relations were counted.
        assert!(report["relation_totals"]["desk_mate"].as_u64().unwrap() >= 2);
    }
    #[test]
    fn repair_keeps_locked_student_seated() {
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "students": [
                {"key":"S1","display_name":"Alice"},{"key":"S2"},
                {"key":"S3"},{"key":"S4"}
            ],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C4","row":1,"col":4,"x":3.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"],["R1C3","R1C4"]]}}
        }"#;
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","student_name":"Alice","seat_id":"R1C1"},
                {"student_key":"S2","student_name":"Bob","seat_id":"R1C2"},
                {"student_key":"S3","student_name":"Carol","seat_id":"R1C3"},
                {"student_key":"S4","student_name":"Dan","seat_id":"R1C4"}
            ],
            "solver_status": "FEASIBLE"
        }"#;
        // Lock S1 in place; everything else may re-arrange.
        let repaired = repair_json(request, snapshot, &[], &["S1".to_string()], &[])
            .expect("repair should succeed");
        let value: Value = serde_json::from_str(&repaired).unwrap();
        let assignments = value["assignments"].as_array().unwrap();
        assert_eq!(assignments.len(), 4);
        let s1 = assignments
            .iter()
            .find(|a| a["student_key"] == "S1")
            .unwrap();
        assert_eq!(s1["seat_id"], "R1C1", "locked student must keep its seat");
        assert!(value["summary"]["moved_students"].as_u64().is_some());
    }

    #[test]
    fn repair_preserves_original_fixed_seat_for_affected_student_non_identity() {
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0,2]],
            "seed": 2,
            "students": [{"key":"S1"},{"key":"S2"},{"key":"S3"},{"key":"S4"}],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C4","row":1,"col":4,"x":3.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"],["R1C3","R1C4"]]}}
        }"#;
        // Deliberately non-identity: S1 is student index 0 but occupies seat
        // index 2. S1 and S2 are both movable, so dropping the original fixed
        // rule would let the deterministic seed move S1 to R1C1.
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","seat_id":"R1C3"},
                {"student_key":"S2","seat_id":"R1C1"},
                {"student_key":"S3","seat_id":"R1C4"},
                {"student_key":"S4","seat_id":"R1C2"}
            ]
        }"#;

        let repaired = repair_json(
            request,
            snapshot,
            &["S1".to_string(), "S2".to_string()],
            &[],
            &[],
        )
        .expect("repair should preserve the original fixed-seat rule");
        let value: Value = serde_json::from_str(&repaired).unwrap();
        let s1 = value["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|assignment| assignment["student_key"] == "S1")
            .unwrap();
        assert_eq!(s1["seat_id"], "R1C3");
    }

    #[test]
    fn repair_rejects_anchors_conflicting_with_original_fixed_seat() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0]],
            "edges": [[0,1],[1,2]],
            "fixed_seats": [[0,2]],
            "students": [{"key":"S1"},{"key":"S2"},{"key":"S3"}],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"]]}}
        }"#;
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","seat_id":"R1C1"},
                {"student_key":"S2","seat_id":"R1C3"},
                {"student_key":"S3","seat_id":"R1C2"}
            ]
        }"#;

        // Same student, different seat.
        let error = repair_json(request, snapshot, &[], &["S1".to_string()], &[]).unwrap_err();
        assert!(error.contains("original fixed-seat rule"), "{error}");
        assert!(error.contains("student S1"), "{error}");

        // Different anchored student attempts to occupy the original fixed
        // student's seat when S1 is the local affected student.
        let error = repair_json(request, snapshot, &["S1".to_string()], &[], &[]).unwrap_err();
        assert!(error.contains("original fixed-seat rule"), "{error}");
        assert!(error.contains("student S2"), "{error}");
    }

    #[test]
    fn repair_rejects_invalid_anchor_combinations() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]],
            "students": [{"key":"S1"},{"key":"S2"}],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"]]}}
        }"#;
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","seat_id":"R1C1"},
                {"student_key":"S2","seat_id":"R1C2"}
            ]
        }"#;

        // Locking an unknown student is an error.
        let err = repair_json(request, snapshot, &[], &["S9".to_string()], &[]).unwrap_err();
        assert!(err.contains("unknown"), "{err}");

        // Affected and locked cannot overlap.
        let err = repair_json(
            request,
            snapshot,
            &["S1".to_string()],
            &["S1".to_string()],
            &[],
        )
        .unwrap_err();
        assert!(err.contains("cannot also be locked"), "{err}");

        // A locked seat must be known.
        let err = repair_json(request, snapshot, &[], &[], &["R1C9".to_string()]).unwrap_err();
        assert!(err.contains("unknown"), "{err}");
    }
}
