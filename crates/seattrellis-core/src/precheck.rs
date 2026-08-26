// ---------------------------------------------------------------------------
// precheck.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Feasibility precheck report.

use serde_json::{json, Value};

use crate::engine::{build_candidate_domains, maximum_candidate_matching, validate_solve_request};
use crate::evaluation::{build_graph_distance_matrix, build_index_adjacency};
use crate::solver::{parse_core_solve_request, resolve_group_rules};
use crate::NATIVE_API_VERSION;

pub fn precheck_report_json(request_json: &str) -> Result<String, String> {
    let request = parse_core_solve_request(request_json)?;
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
