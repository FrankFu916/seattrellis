use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

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
    distance.map_or(true, |value| value >= rule.distance)
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
// Greedy constraint-satisfying seat generator.
//
// This is the first solver piece ported from the Python fallback backend: it
// produces a complete, hard-constraint-satisfying assignment (or reports
// infeasibility) using a most-constrained-first greedy with seeded
// tie-breaking.  Cost optimization and soft objectives are a later milestone;
// this establishes the native solve contract and the deterministic baseline.
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoreSolveResponse {
    pub api_version: u32,
    pub feasible: bool,
    /// ``[student_index, seat_index]`` pairs; empty when infeasible.
    pub assignment: Vec<[usize; 2]>,
    pub attempts_used: usize,
    pub hard_constraints_satisfied: bool,
}

pub fn solve_problem(request: &CoreSolveRequest) -> Result<CoreSolveResponse, String> {
    validate_solve_request(request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    let attempts = (request.student_count * 12).max(40);
    let mut rng = SplitMix64::new(request.seed);

    for attempt in 0..attempts {
        if let Some(assignment) =
            greedy_attempt(request, &adjacency, &graph_distances, &mut rng)
        {
            let pairs: Vec<[usize; 2]> = assignment
                .iter()
                .enumerate()
                .map(|(student, seat)| [student, *seat])
                .collect();
            return Ok(CoreSolveResponse {
                api_version: NATIVE_API_VERSION,
                feasible: true,
                assignment: pairs,
                attempts_used: attempt + 1,
                hard_constraints_satisfied: true,
            });
        }
    }

    Ok(CoreSolveResponse {
        api_version: NATIVE_API_VERSION,
        feasible: false,
        assignment: Vec::new(),
        attempts_used: attempts,
        hard_constraints_satisfied: false,
    })
}

pub fn solve_problem_json(request_json: &str) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    let response = solve_problem(&request)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize native solve response: {error}"))
}

fn greedy_attempt(
    request: &CoreSolveRequest,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    rng: &mut SplitMix64,
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
                .map_or(true, |(_, existing)| candidates.len() < existing.len())
            {
                best = Some((student, candidates));
            }
        }
        let (student, mut candidates) = best?;
        shuffle(&mut candidates, rng);
        let seat = candidates[0];
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
    for seat in 0..request.seat_positions.len() {
        if used[seat] {
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

/// Dependency-free deterministic PRNG (SplitMix64) so the core stays lean.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_usize(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        (self.next_u64() % bound as u64) as usize
    }
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
}
