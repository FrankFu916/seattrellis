// ---------------------------------------------------------------------------
// candidates.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Candidate generation: seed derivation, exclusion, distance, recommendation.
// ---------------------------------------------------------------------------

use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::engine::validate_solve_request;
use crate::scoring::score_assignment_json;
use crate::solver::{
    parse_core_solve_request, solve_problem_internal, validate_solve_response, SolveControl,
    SolveStatus,
};
/// `candidate_count` caps the set (1..=20); `attempt_limit` bounds the
/// generation loop. Mirrors the Python `candidates.generate_candidate_set`
/// strategy (seeded repeated solve + exclusion).
use crate::NATIVE_API_VERSION;

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
    generate_candidates_json_with_latest_snapshot(request_json, candidate_count, "")
}

/// Like [`generate_candidates_json`], but also accepts a `latest_snapshot`
/// document so the per-candidate PlanScore activates the `stability_score`
/// dimension (the fixed-assignment scoring path covers the parity evidence;
/// this wires the same code into candidate generation). An empty string
/// keeps `stability_score` `not_available`, matching the Python CLI which
/// does not pass a latest snapshot either.
pub fn generate_candidates_json_with_latest_snapshot(
    request_json: &str,
    candidate_count: usize,
    latest_snapshot_json: &str,
) -> Result<String, String> {
    generate_candidates_json_with_latest_snapshot_and_control(
        request_json,
        candidate_count,
        latest_snapshot_json,
        &SolveControl::new(),
    )
}

/// Candidate generation shares cancellation and one wall-clock budget across
/// the entire set; it never grants a fresh full budget to every retry.
pub fn generate_candidates_json_with_latest_snapshot_and_control(
    request_json: &str,
    candidate_count: usize,
    latest_snapshot_json: &str,
    control: &SolveControl,
) -> Result<String, String> {
    if !(1..=20).contains(&candidate_count) {
        return Err(format!(
            "invalid candidate_count {candidate_count}: expected a value between 1 and 20"
        ));
    }
    let mut request = parse_core_solve_request(request_json)?;
    let started = Instant::now();
    let deadline = request
        .time_limit_seconds
        .map(|seconds| {
            Duration::try_from_secs_f64(seconds)
                .ok()
                .and_then(|duration| started.checked_add(duration))
                .ok_or_else(|| "invalid time_limit_seconds: duration is out of range".to_string())
        })
        .transpose()?;
    validate_solve_request(&request)?;
    let base_seed = request.seed;
    let attempt_limit = candidate_count * 12 + 8;

    let mut candidates: Vec<GeneratedCandidate> = Vec::new();
    let mut seen: Vec<Vec<usize>> = Vec::new();
    let mut failed_attempts = 0;
    let mut stop_status = None;

    for attempt_index in 0..attempt_limit {
        if candidates.len() >= candidate_count {
            break;
        }
        if control.is_cancelled() {
            stop_status = Some(SolveStatus::Cancelled);
            break;
        }
        if let Some(deadline) = deadline {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_secs_f64();
            if remaining <= 0.0 {
                stop_status = Some(SolveStatus::Timeout);
                break;
            }
            request.time_limit_seconds = Some(remaining);
        }
        // Seed derivation is independent for every attempt; never feed the
        // previous derived seed back into the next derivation.
        request.seed = derive_candidate_seed(base_seed, attempt_index);
        let response = solve_problem_internal(&request, control, &seen)?;
        if !response.feasible {
            failed_attempts += 1;
            if matches!(
                response.status,
                SolveStatus::Cancelled | SolveStatus::Timeout
            ) {
                stop_status = Some(response.status);
                break;
            }
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
        if let Some(status) = stop_status {
            return Err(format!(
                "candidate generation stopped with status {} before finding a feasible plan",
                status.as_str()
            ));
        }
        return Err("candidate generation did not produce any feasible plan".to_string());
    }
    let mut warnings: Vec<String> = Vec::new();
    if let Some(status) = stop_status {
        warnings.push(format!("candidate generation stopped with status {}; returning the feasible plans already found", status.as_str()));
    }
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
    // assignment distance to every other candidate; stability activates
    // only when a latest snapshot is supplied (the Python CLI also leaves
    // it not_available, and the fixed-assignment scoring path carries the
    // parity evidence).
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
            latest_snapshot_json,
            (candidates.len() > 1).then_some(diversities[index] * 100.0),
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
                    candidates[*right_index]
                        .candidate_id
                        .cmp(&candidates[*left_index].candidate_id)
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
