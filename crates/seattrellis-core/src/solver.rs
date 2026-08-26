// ---------------------------------------------------------------------------
// solver.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Solve contract (CoreSolveRequest / SolveStatus / SolveControl) and solver entry points.
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use crate::models::{Layout, PairHistory, RuleSet, SeatHistory, Student};
use crate::objectives::SoftObjectiveContext;
use crate::rng::SplitMix64;

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

use crate::engine::{
    build_candidate_domains, build_cost_context, effective_students, full_solution_total_cost,
    greedy_attempt_controlled, hard_search_controlled, local_search_controlled,
    maximum_candidate_matching, validate_assignment, validate_solve_request, GreedyOutcome,
    SearchOutcome, GREEDY_STAGNATION_LIMIT, HARD_SEARCH_NODE_BUDGET,
};
use crate::evaluation::{build_graph_distance_matrix, build_index_adjacency, CoreMinDistanceRule};
use crate::NATIVE_API_VERSION;

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
    const INVALID_TOKENS: [&str; 21] = [
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
        // Validation phrasing used by `validate_solve_request` and the
        // repair/editing parsers ("students must be empty or match
        // student_count", "edges must reference two different known
        // seats", "seat positions must contain finite numbers", ...).
        // Every non-test "must be/reference/contain" message in this
        // crate describes rejected input, never an internal fault.
        "must be",
        "must reference",
        "must contain",
        // CLI/io surface messages: unreadable or malformed inputs are
        // InvalidInput (exit 2), not internal failures (exit 70) — the
        // CLI arg sweep (ledger §19.33) pinned these classes.
        "cannot read",
        "not valid json",
        "is not a json",
        "project file not found",
        "no such file",
        "could not read",
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
pub(crate) struct CostContext {
    pub(crate) students: Vec<Student>,
    pub(crate) layout: Layout,
    pub(crate) rules: RuleSet,
    pub(crate) history: Option<SeatHistory>,
    pub(crate) pair_history: Option<PairHistory>,
    pub(crate) adjacency_edges: HashSet<(String, String)>,
    pub(crate) objective_context: SoftObjectiveContext,
    pub(crate) min_row: i32,
    pub(crate) max_row: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopReason {
    Deadline,
    Cancelled,
}

pub(crate) struct SolveRunControl<'a> {
    pub(crate) deadline: Option<Instant>,
    pub(crate) cancellation: &'a SolveControl,
}

impl SolveRunControl<'_> {
    pub(crate) fn stop_reason(&self) -> Option<StopReason> {
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

pub(crate) fn solve_problem_internal(
    request: &CoreSolveRequest,
    cancellation: &SolveControl,
    excluded_assignments: &[Vec<usize>],
) -> Result<CoreSolveResponse, String> {
    let response = solve_problem_unchecked(request, cancellation, excluded_assignments)?;
    // Every product leaving the core passes the independent validators:
    // Solved responses clear the full consumer-side `validate_solve_response`,
    // every other status must carry the exact non-Solved shape.
    if response.status == SolveStatus::Solved {
        validate_solve_response(request, &response)?;
    } else {
        validate_non_solved_response_shape(request, &response)?;
    }
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

/// Shape check for the non-Solved statuses (Solved responses take the full
/// [`validate_solve_response`] path instead).
fn validate_non_solved_response_shape(
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
    let request = parse_core_solve_request(request_json)?;
    let response = solve_problem(&request)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize native solve response: {error}"))
}

/// Soft-rule names the cost functions actually read (`models/rules.py`
/// `SoftRules`). Any other name in `rules.soft` would be silently dropped by
/// serde, which changes the plan the teacher asked for, so requests carrying
/// one are rejected instead.
pub(crate) const KNOWN_SOFT_RULES: [&str; 10] = [
    "vision_front",
    "height_back",
    "randomize",
    "score_balance",
    "score_position",
    "score_distribution",
    "mentor_pairing",
    "fair_rotation",
    "avoid_recent_neighbors",
    "cooling",
];

/// Parse a native solve request document with the two contract guards serde
/// cannot express on [`CoreSolveRequest`]:
///
/// 1. a non-empty `rules.hard` block is rejected — the string-reference hard
///    rules inside it are never consumed by the native path (the solver only
///    reads the top-level index-pair form and `rules.groups`), so accepting
///    the request would report `hard_constraints_satisfied: true` while
///    ignoring the teacher's constraints;
/// 2. unrecognized `rules.soft` rule names are rejected (docs/rules.zh.md:
///    "未识别的 soft rule 名称也会报错").
pub(crate) fn parse_core_solve_request(request_json: &str) -> Result<CoreSolveRequest, String> {
    let value: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    if let Some(hard) = value.get("rules").and_then(|rules| rules.get("hard")) {
        if json_value_has_data(hard) {
            return Err(
                "unsupported field rules.hard: native solve requests must express hard \
                 constraints in the top-level index-pair form (fixed_seats / must_be_adjacent \
                 / cannot_be_adjacent / min_distance); rules.hard is not consumed by \
                 the native solve path"
                    .to_string(),
            );
        }
    }
    if let Some(soft) = value
        .get("rules")
        .and_then(|rules| rules.get("soft"))
        .and_then(serde_json::Value::as_object)
    {
        let unknown: Vec<&str> = soft
            .keys()
            .map(String::as_str)
            .filter(|name| !KNOWN_SOFT_RULES.contains(name))
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "unrecognized soft rule name(s) in rules.soft: {}; supported soft rules: {}",
                unknown.join(", "),
                KNOWN_SOFT_RULES.join(", ")
            ));
        }
    }
    serde_json::from_value(value).map_err(|error| format!("invalid native solve request: {error}"))
}

/// Does this JSON value carry any actual data (as opposed to being null, an
/// empty object/array, or a container of only empty values)?
fn json_value_has_data(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Object(object) => object.values().any(json_value_has_data),
        serde_json::Value::Array(items) => items.iter().any(json_value_has_data),
        _ => true,
    }
}

#[cfg(test)]
mod exit_classification_tests {
    use super::*;

    #[test]
    fn validation_phrasing_classifies_as_invalid_input_not_internal() {
        let messages = [
            "students must be empty or match student_count",
            "student_scores must be empty or match student_count",
            "edges must reference two different known seats",
            "seat positions must contain finite numbers",
            "min_distance values must be positive and finite",
            "student scores must be finite numbers",
            "layout must describe at least as many seats as seat_positions",
        ];
        for message in messages {
            assert_eq!(
                classify_solve_error(message),
                SolveStatus::InvalidInput,
                "misclassified: {message}"
            );
        }
    }

    #[test]
    fn genuine_internal_faults_stay_internal() {
        let messages = [
            "candidate generation failed: solver produced no plans",
            "panicked while ranking candidates",
            "solver store lock poisoned",
        ];
        for message in messages {
            assert_eq!(
                classify_solve_error(message),
                SolveStatus::InternalError,
                "misclassified: {message}"
            );
        }
    }
}
