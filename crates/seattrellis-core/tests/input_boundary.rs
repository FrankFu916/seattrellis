// ---------------------------------------------------------------------------
// input_boundary.rs — input-validation boundary audit (2026-08-12).
//
// Regression tests for the input boundary of the core solver and model:
// extreme-but-finite coordinates, non-finite student score/height fields,
// and overflow-safe row/col arithmetic (debug builds panic on i32 overflow;
// these tests pin the fixed behavior).
// ---------------------------------------------------------------------------

use seattrellis_core::{
    audit_report_json, pair_report_json, score_assignment_json, solve_problem_json,
    validate_solve_request_json, SolveStatus,
};

/// Extreme-but-finite coordinates (1e300) pass the finiteness validation and
/// must not panic anywhere in the solve pipeline. Before the fix, the
/// derived rows saturated to i32::MAX / i32::MIN and the row/col delta
/// arithmetic overflowed (debug panic) in the soft-objective adjacency pass.
#[test]
fn extreme_coordinates_do_not_panic_solve() {
    let request = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 1e300], [0.0, -1e300]],
        "seed": 1
    }"#;
    let response_json = solve_problem_json(request).expect("extreme coordinates must validate");
    let response: seattrellis_core::CoreSolveResponse =
        serde_json::from_str(&response_json).expect("response is valid JSON");
    assert_eq!(response.status, SolveStatus::Solved);
    assert_eq!(response.assignment.len(), 2);
}

/// The same extreme coordinates must not panic the audit entry point either.
#[test]
fn extreme_coordinates_do_not_panic_audit() {
    let request = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 1e300], [0.0, -1e300]],
        "students": [
            {"key": "S1", "vision": "poor"},
            {"key": "S2"}
        ],
        "rules": {"seed": 1, "soft": {"vision_front": {"enabled": true, "weight": 20}}}
    }"#;
    let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1]];
    let report = audit_report_json(request, &assignment).expect("audit must not panic");
    let value: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert!(value["total_cost"].is_number());
}

/// Extreme coordinates must not panic the fixed-assignment scorer (row
/// normalization arithmetic).
#[test]
fn extreme_coordinates_do_not_panic_scoring() {
    let request = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 1e300], [0.0, -1e300]],
        "students": [
            {"key": "S1", "height_cm": 150.0, "vision": "poor"},
            {"key": "S2", "height_cm": 180.0}
        ],
        "rules": {"seed": 1, "soft": {
            "height_back": {"enabled": true, "weight": 1},
            "vision_front": {"enabled": true, "weight": 20}
        }}
    }"#;
    let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1]];
    let report = score_assignment_json(request, &assignment, "", None).expect("scoring must not panic");
    let value: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert!(value["total"].is_number());
}

/// A non-finite `students[].score` must be rejected at validation (mirrors
/// the existing `student_scores` finiteness check). NaN/±inf are not
/// representable in JSON (out-of-range literals are already rejected at
/// parse), so the check protects the in-process Rust API where a DTO can be
/// built directly; before the fix a NaN score propagated into percentiles
/// and produced a NaN total_cost that serialized as JSON null.
#[test]
fn non_finite_student_score_is_rejected() {
    let base = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
        "students": [
            {"key": "S1", "score": 100.0},
            {"key": "S2", "score": 50.0}
        ],
        "rules": {"seed": 1, "soft": {"score_balance": {"enabled": true, "weight": 5}}}
    }"#;
    let mut request: seattrellis_core::CoreSolveRequest =
        serde_json::from_str(base).expect("base request parses");
    for bad_score in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        request.students[1].score = Some(bad_score);
        let error = seattrellis_core::solve_problem(&request).unwrap_err();
        assert!(error.contains("finite"), "unexpected error: {error}");
    }
    // The JSON wire rejects out-of-range literals before validation.
    let wire = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
        "students": [
            {"key": "S1", "score": 100.0},
            {"key": "S2", "score": 1e999}
        ]
    }"#;
    let error = validate_solve_request_json(wire).unwrap_err();
    assert!(error.contains("number out of range"), "unexpected error: {error}");
}

/// A non-finite `students[].height_cm` must be rejected at validation.
/// Before the fix NaN height silently degraded to cost 0 and a huge finite
/// height could overflow the i64 cost chain.
#[test]
fn non_finite_height_is_rejected() {
    let base = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
        "students": [
            {"key": "S1", "height_cm": 150.0},
            {"key": "S2", "height_cm": 160.0}
        ],
        "rules": {"seed": 1, "soft": {"height_back": {"enabled": true, "weight": 1}}}
    }"#;
    let mut request: seattrellis_core::CoreSolveRequest =
        serde_json::from_str(base).expect("base request parses");
    for bad_height in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        request.students[1].height_cm = Some(bad_height);
        let error = seattrellis_core::solve_problem(&request).unwrap_err();
        assert!(error.contains("finite"), "unexpected error: {error}");
    }
}

/// A huge-but-finite height (1e300) must not overflow the cost chain:
/// the solve completes and reports a finite total_cost. Before the fix the
/// i64 product weight * round(height) * row-penalty overflowed (debug panic).
#[test]
fn huge_finite_height_does_not_overflow_cost() {
    let request = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 0.0], [0.0, 2.0]],
        "students": [
            {"key": "S1", "height_cm": 1e300},
            {"key": "S2"}
        ],
        "rules": {"seed": 1, "soft": {"height_back": {"enabled": true, "weight": 3}}}
    }"#;
    let response_json = solve_problem_json(request).expect("huge height must validate");
    let response: seattrellis_core::CoreSolveResponse =
        serde_json::from_str(&response_json).expect("response is valid JSON");
    assert_eq!(response.status, SolveStatus::Solved);
    let total_cost = response.total_cost.expect("feasible solve reports total_cost");
    assert!(total_cost.is_finite(), "total_cost must be finite, got {total_cost}");
}

/// The pair report computes row/col deltas over every historical pair; the
/// delta arithmetic must be overflow-safe for extreme layout rows.
#[test]
fn extreme_rows_do_not_panic_pair_report() {
    let request = r#"{
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[0.0, 1e300], [0.0, -1e300]],
        "students": [{"key": "S1"}, {"key": "S2"}]
    }"#;
    let snapshots = r#"[
        {"assignments": [
            {"student_key": "S1", "seat_id": "seat_0"},
            {"student_key": "S2", "seat_id": "seat_1"}
        ]}
    ]"#;
    let report =
        pair_report_json(request, snapshots, 10, 2).expect("pair report must not panic");
    let value: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(value["history_count"], 1);
}

#[test]
fn oversized_seat_count_is_rejected() {
    // Core audit 2026-08-12: validation builds O(V^2) matrices, so an
    // unbounded seat count is a memory DoS surface on the loopback API.
    let seats: Vec<[f64; 2]> = (0..10_001)
        .map(|index| [(index % 200) as f64 * 0.5, (index / 200) as f64 * 0.5])
        .collect();
    let request = serde_json::json!({
        "api_version": 2,
        "student_count": 10,
        "seat_positions": seats,
        "seed": 0,
    });
    let error = solve_problem_json(&request.to_string()).unwrap_err();
    assert!(error.contains("at most 10000 seats"), "{error}");
}
