//! Candidate-engine gate (plan §6.3/§6.7): 20/40/50/60/80 students ×
//! 1/5/20 requested candidates must produce exactly the requested number of
//! DISTINCT feasible plans, every candidate must pass the independent
//! consumer-side validator, and the same request + seed must reproduce the
//! report exactly.
//!
//! This is deliberately independent of the generator's internal checks: each
//! candidate assignment is rebuilt into a `CoreSolveResponse` and re-validated
//! through the public `validate_solve_response` entry point.

use std::collections::HashSet;

use seattrellis_core::{
    generate_candidates_json, validate_solve_response, CoreSolveRequest, CoreSolveResponse,
    SolveStatus, NATIVE_API_VERSION,
};
use serde_json::{json, Value};

fn grid_request(count: usize, seed: u64) -> Value {
    let positions: Vec<[f64; 2]> = (0..count)
        .map(|index| [(index % 6) as f64, (index / 6) as f64])
        .collect();
    let mut edges = Vec::new();
    for first in 0..positions.len() {
        for second in (first + 1)..positions.len() {
            let dx = (positions[first][0] - positions[second][0]).abs();
            let dy = (positions[first][1] - positions[second][1]).abs();
            if (dx == 1.0 && dy == 0.0) || (dx == 0.0 && dy == 1.0) {
                edges.push([first, second]);
            }
        }
    }
    json!({
        "api_version": 2,
        "student_count": count,
        "seat_positions": positions,
        "edges": edges,
        "students": (0..count)
            .map(|index| json!({ "key": format!("S{index:03}") }))
            .collect::<Vec<_>>(),
        "seed": seed,
        "rules": { "seed": seed, "soft": {} },
    })
}

/// Rebuild a `CoreSolveResponse` from a candidate report entry so the
/// consumer-side validator can re-check it independently.
fn candidate_response(candidate: &Value) -> CoreSolveResponse {
    let assignment = candidate["assignment"]
        .as_array()
        .expect("candidate assignment array")
        .iter()
        .map(|pair| {
            let list = pair.as_array().expect("assignment pair");
            [
                list[0].as_u64().expect("student index") as usize,
                list[1].as_u64().expect("seat index") as usize,
            ]
        })
        .collect();
    CoreSolveResponse {
        api_version: NATIVE_API_VERSION,
        feasible: true,
        status: SolveStatus::Solved,
        assignment,
        attempts_used: 0,
        hard_constraints_satisfied: true,
        total_cost: None,
    }
}

/// The gate is release-only: the largest combination (80 students × 20
/// candidates) costs minutes in debug builds. CI runs it explicitly with
/// `cargo test --release -p seattrellis_core --test candidates_gate --
/// --ignored` (rust.yml, ubuntu job).
#[test]
#[ignore = "expensive: run in release mode via the CI candidates-gate step"]
fn candidate_gate_sizes_x_counts_distinct_validated_reproducible() {
    let mut checked = 0usize;
    for count in [20, 40, 50, 60, 80] {
        for requested in [1, 5, 20] {
            let request_value = grid_request(count, 0x00C0_FFEE);
            let request_text = serde_json::to_string(&request_value).unwrap();
            let report_text = generate_candidates_json(&request_text, requested)
                .unwrap_or_else(|error| panic!("count={count} candidates={requested}: {error}"));
            // Reproducibility: the same request + seed reproduces the report
            // exactly (three-platform reproducibility is covered by CI).
            let rerun = generate_candidates_json(&request_text, requested).unwrap();
            assert_eq!(
                report_text, rerun,
                "candidate report is not reproducible for count={count} candidates={requested}"
            );

            let report: Value = serde_json::from_str(&report_text).unwrap();
            let candidates = report["candidates"].as_array().expect("candidates array");
            assert_eq!(
                candidates.len(),
                requested,
                "count={count} candidates={requested}: expected exactly {requested} distinct plans"
            );
            assert_eq!(report["requested_candidate_count"], json!(requested));
            assert_eq!(report["base_seed"], json!(0x00C0_FFEE));

            let request: CoreSolveRequest = serde_json::from_value(request_value).unwrap();
            let mut seen: HashSet<Vec<[usize; 2]>> = HashSet::new();
            for candidate in candidates {
                let response = candidate_response(candidate);
                validate_solve_response(&request, &response).unwrap_or_else(|error| {
                    panic!(
                        "candidate failed independent validation \
                         (count={count}, candidates={requested}): {error}"
                    )
                });
                assert!(
                    seen.insert(response.assignment.clone()),
                    "duplicate candidate assignment (count={count}, candidates={requested})"
                );
            }
            // The recommended candidate must be one of the generated ones.
            let recommended = report["recommended_candidate_id"]
                .as_str()
                .expect("recommended_candidate_id");
            assert!(
                candidates
                    .iter()
                    .any(|candidate| candidate["candidate_id"].as_str() == Some(recommended)),
                "recommended {recommended} is not among the candidates (count={count})"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 15);
}
