//! Silent-swallow gate (M·HIGH): the native solve path never consumes
//! `rules.hard` (string-reference form) and never sees soft-rule names it
//! does not know. Both shapes previously deserialized without error and were
//! silently dropped — a solve that ignored the teacher's constraints still
//! reported `hard_constraints_satisfied: true`.

use seattrellis_core::{classify_solve_error, solve_problem_json, validate_solve_request_json};
use serde_json::{json, Value};

fn base_request(rules: Value) -> String {
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
        "rules": rules
    })
    .to_string()
}

#[test]
fn non_empty_rules_hard_is_rejected_with_index_pair_guidance() {
    let rules = json!({
        "seed": 7,
        "soft": {},
        "groups": [],
        "hard": {
            "fixed_seats": [{"student": "s0", "seat_id": "seat_0"}]
        }
    });
    let request = base_request(rules);
    let error = solve_problem_json(&request).expect_err("rules.hard must not be silently dropped");
    assert!(
        error.contains("rules.hard") && error.contains("not consumed"),
        "error must name the unconsumed field: {error}"
    );
    for field in [
        "fixed_seats",
        "must_be_adjacent",
        "cannot_be_adjacent",
        "min_distance",
    ] {
        assert!(
            error.contains(field),
            "error must point at the index-pair alternative {field}: {error}"
        );
    }
    assert_eq!(
        classify_solve_error(&error),
        seattrellis_core::SolveStatus::InvalidInput
    );
    assert!(validate_solve_request_json(&request).is_err());
}

#[test]
fn empty_or_null_rules_hard_stays_accepted() {
    for hard in [json!({}), json!(null), json!({"fixed_seats": []})] {
        let request = base_request(json!({"seed": 7, "soft": {}, "hard": hard}));
        validate_solve_request_json(&request)
            .unwrap_or_else(|error| panic!("empty rules.hard {hard} must stay legal: {error}"));
    }
}

#[test]
fn unrecognized_soft_rule_name_lists_the_name() {
    let request = base_request(json!({
        "seed": 7,
        "soft": {
            "magic_seating": {"enabled": true, "weight": 5},
            "vision_front": {"enabled": true, "weight": 1}
        }
    }));
    let error = solve_problem_json(&request).expect_err("unknown soft rule names must be reported");
    assert!(
        error.contains("magic_seating"),
        "error must list the unknown rule name: {error}"
    );
    assert_eq!(
        classify_solve_error(&error),
        seattrellis_core::SolveStatus::InvalidInput
    );
}

#[test]
fn every_known_soft_rule_name_is_accepted() {
    let request = base_request(json!({
        "seed": 7,
        "groups": [],
        "soft": {
            "vision_front": {"enabled": false, "weight": 1},
            "height_back": {"enabled": false, "weight": 1},
            "randomize": {"enabled": false, "weight": 1},
            "score_balance": {"enabled": false, "weight": 1},
            "score_position": {"enabled": false, "weight": 1, "direction": "high_front"},
            "score_distribution": {"enabled": false, "weight": 1, "scope": "row"},
            "mentor_pairing": {
                "enabled": false,
                "weight": 1,
                "mentor_percentile": 0.75,
                "learner_percentile": 0.25,
                "relation": "desk_mate",
                "avoid_recent_repeats": true,
                "history_lookback": 4
            },
            "fair_rotation": {
                "enabled": false,
                "weight": 1,
                "avoid_repeating_categories": ["front"],
                "lookback": 4
            },
            "avoid_recent_neighbors": {
                "enabled": false,
                "weight": 1,
                "relation_types": ["desk_mate"],
                "lookback": 4,
                "max_recent_count": 1,
                "within_distance": 2
            },
            "cooling": {
                "enabled": false,
                "weight": 1,
                "cooling_period": 3,
                "relation_types": ["desk_mate"],
                "within_distance": 2
            }
        }
    }));
    validate_solve_request_json(&request)
        .unwrap_or_else(|error| panic!("known rule vocabulary must stay legal: {error}"));
}
