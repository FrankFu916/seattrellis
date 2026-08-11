// ---------------------------------------------------------------------------
// candidates.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Candidate generation: seed derivation, exclusion, distance, recommendation.
// ---------------------------------------------------------------------------

use serde_json::{json, Value};

use crate::engine::validate_solve_request;
use crate::scoring::score_assignment_json;
use crate::solver::{
    solve_problem_internal, validate_solve_response, CoreSolveRequest, SolveControl, SolveStatus,
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
