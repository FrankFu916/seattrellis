//! Soft-rule weight gate (G·HIGH): negative weights are `InvalidInput`
//! (docs/rules.zh.md: "非负整数 weight … 负数权重会报错"), oversized weights
//! are rejected before they can overflow the cost arithmetic, and the
//! i32::MAX + cooling combination must not panic.

use seattrellis_core::{classify_solve_error, CoreSolveRequest, CoreSolveResponse};
use seattrellis_core::{solve_problem_json, validate_solve_request_json};
use serde_json::{json, Value};
use std::sync::Mutex;

fn base_request(soft: Value) -> String {
    json!({
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[1.0, 1.0], [2.0, 1.0]],
        "edges": [[0, 1]],
        "seed": 7,
        "students": [
            {"key": "s0", "score": 80.5},
            {"key": "s1", "score": 60.25}
        ],
        "rules": {"seed": 7, "soft": soft}
    })
    .to_string()
}

fn negative_weight_cases() -> Vec<(&'static str, Value)> {
    vec![
        ("vision_front", json!({"enabled": true, "weight": -1})),
        ("height_back", json!({"enabled": true, "weight": -5})),
        ("randomize", json!({"enabled": true, "weight": -2})),
        ("score_balance", json!({"enabled": true, "weight": -100})),
        (
            "score_position",
            json!({"enabled": true, "weight": -3, "direction": "high_back"}),
        ),
        (
            "score_distribution",
            json!({"enabled": true, "weight": -8, "scope": "row"}),
        ),
        (
            "mentor_pairing",
            json!({
                "enabled": true,
                "weight": -6,
                "mentor_percentile": 0.75,
                "learner_percentile": 0.25,
                "relation": "desk_mate",
                "avoid_recent_repeats": false,
                "history_lookback": 4
            }),
        ),
        (
            "fair_rotation",
            json!({"enabled": true, "weight": -9, "lookback": 4}),
        ),
        (
            "avoid_recent_neighbors",
            json!({"enabled": true, "weight": -10, "lookback": 4}),
        ),
        ("cooling", json!({"enabled": true, "weight": -12})),
    ]
}

#[test]
fn negative_enabled_soft_weights_are_invalid_input() {
    for (name, rule) in negative_weight_cases() {
        let soft = json!({ (name): rule });
        let request = base_request(soft);
        let error =
            validate_solve_request_json(&request).expect_err("negative weights must be rejected");
        assert!(
            error.contains("non-negative"),
            "rule {name}: unexpected error {error}"
        );
        assert_eq!(
            classify_solve_error(&error),
            seattrellis_core::SolveStatus::InvalidInput,
            "rule {name}: {error}"
        );
        assert!(
            solve_problem_json(&request).is_err(),
            "rule {name}: string API must surface the rejection"
        );
    }
}

#[test]
fn zero_weight_stays_legal_but_oversized_weight_is_rejected() {
    let ok = base_request(json!({"vision_front": {"enabled": true, "weight": 0}}));
    assert!(
        validate_solve_request_json(&ok).is_ok(),
        "weight 0 is legal"
    );

    let boundary = base_request(json!({"vision_front": {"enabled": true, "weight": 1_000_000}}));
    assert!(
        validate_solve_request_json(&boundary).is_ok(),
        "the documented maximum weight must stay legal"
    );

    let request = base_request(json!({"vision_front": {"enabled": true, "weight": 1_000_001}}));
    let error =
        validate_solve_request_json(&request).expect_err("weights above the maximum must fail");
    assert!(error.contains("maximum"), "unexpected error: {error}");
    assert_eq!(
        classify_solve_error(&error),
        seattrellis_core::SolveStatus::InvalidInput
    );
}

/// Serializes solves that use extreme weights; the heavy arithmetic must stay
/// bounded even when two runs race on shared CPU caches (paranoia guard for
/// debug-build overflow panics).
static SOLVE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn i32_max_weight_with_cooling_does_not_panic() {
    // Far above the supported maximum: must be rejected cleanly (InvalidInput),
    // never overflow or panic.
    let _guard = SOLVE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let extreme = json!({
        "avoid_recent_neighbors": {
            "enabled": true,
            "weight": 2147483647i64,
            "relation_types": ["desk_mate"],
            "lookback": 4,
            "max_recent_count": 0,
            "within_distance": 2
        },
        "cooling": {"enabled": true, "weight": 2147483647i64}
    });
    let request = base_request(extreme.clone());
    let error = validate_solve_request_json(&request)
        .expect_err("i32::MAX weights are beyond the supported maximum");
    assert!(error.contains("maximum"), "unexpected error: {error}");
    assert_eq!(
        classify_solve_error(&error),
        seattrellis_core::SolveStatus::InvalidInput
    );
}

#[test]
fn maxed_out_weights_with_cooling_pair_history_do_not_panic() {
    let _guard = SOLVE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // Both rules at the documented maximum: the combined recent-neighbor
    // weight saturates instead of overflowing, and the pair sat together
    // recently so the weighted penalty path actually executes.
    let with_history = json!({
        "api_version": 2,
        "student_count": 2,
        "seat_positions": [[1.0, 1.0], [2.0, 1.0]],
        "edges": [[0, 1]],
        "seed": 7,
        "students": [
            {"key": "s0", "score": 80.5},
            {"key": "s1", "score": 60.25}
        ],
        "pair_history": {
            "history_count": 1,
            "within_distance_metric": "chebyshev",
            "within_distance": 2,
            "pairs": {"s0|s1": {"records": [{"relations": ["desk_mate"]}]}}
        },
        "rules": {"seed": 7, "soft": {
            "avoid_recent_neighbors": {
                "enabled": true,
                "weight": 1_000_000i64,
                "relation_types": ["desk_mate"],
                "lookback": 4,
                "max_recent_count": 0,
                "within_distance": 2
            },
            "cooling": {"enabled": true, "weight": 1_000_000i64}
        }}
    })
    .to_string();

    let response = solve_problem_json(&with_history).expect("extreme weights must not panic");
    let parsed: CoreSolveResponse = serde_json::from_str(&response).expect("response JSON");
    let request_typed: CoreSolveRequest = serde_json::from_str(&with_history).expect("request");
    seattrellis_core::validate_solve_response(&request_typed, &parsed)
        .expect("response stays valid");
    if parsed.feasible {
        let total = parsed.total_cost.expect("feasible responses carry a cost");
        assert!(total.is_finite(), "total_cost must stay finite: {total}");
    }
}
