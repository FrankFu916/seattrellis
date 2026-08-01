//! Goal-id → RuleSet JSON mapping.
//!
//! The soft-objective configuration for each goal mirrors the Python presets
//! in `src/seattrellis/presets.py` (`_rules(...)` + each `PresetDefinition`).
//! Field names and values are byte-for-byte the same `model_dump(mode="json")`
//! output that Python produces for `preset.rules`, restricted to the fields the
//! Rust core model (`seattrellis_core::models::RuleSet`) exposes: `seed` and
//! `soft`. The Python `hard`/`groups`/`schema_version` fields are not part of
//! the core Rust model and are intentionally omitted.

use serde_json::{json, Value};

/// The goal ids accepted by [`goal_rules`], in canonical (lowercase-hyphen) form.
pub const GOAL_IDS: &[&str] = &[
    "daily-rotation",
    "quick-shuffle",
    "fair-shuffle",
    "peer-support",
];

const UNKNOWN_GOAL_HELP: &str =
    "Available goals: daily-rotation, quick-shuffle, fair-shuffle, peer-support.";

/// Build the `RuleSet` JSON for a seat-planning goal id.
///
/// Returns a document shaped like the `seattrellis_core::models::RuleSet`
/// model (`{"seed": 42, "soft": {...}}`) so it can be deserialized directly
/// by the core crate. Unknown goal ids return an `Err`.
///
/// Goal ids are matched case-insensitively after trimming whitespace and
/// normalizing `_` to `-` (mirroring `presets.get_preset`).
pub fn goal_rules(goal_id: &str) -> Result<Value, String> {
    let normalized = goal_id.trim().to_ascii_lowercase().replace('_', "-");
    let soft = match normalized.as_str() {
        "daily-rotation" => soft_rules(20, 4, 3, 4, 12, 12),
        "quick-shuffle" => soft_rules(0, 0, 10, 0, 0, 0),
        "fair-shuffle" => soft_rules(0, 0, 2, 0, 20, 0),
        "peer-support" => soft_rules(0, 0, 2, 18, 0, 0),
        _ => return Err(format!("Unknown goal {goal_id:?}. {UNKNOWN_GOAL_HELP}")),
    };
    Ok(json!({ "seed": 42, "soft": soft }))
}

/// A single `WeightedRule`: `{"enabled": ..., "weight": ...}`.
fn weighted(enabled: bool, weight: i32) -> Value {
    json!({ "enabled": enabled, "weight": weight })
}

/// Build the `soft` object, mirroring `presets._rules(...)`.
///
/// Parameters are the enabled weights, exactly like the Python helper:
/// `vision`, `height`, `randomize`, `score` (score_balance),
/// `fair_rotation`, `neighbors` (avoid_recent_neighbors). Rules given a
/// weight of 0 are serialized as `enabled: false, weight: 0`, and rules the
/// four goals never use (`score_position`, `score_distribution`,
/// `mentor_pairing`, `cooling`) stay at their pydantic defaults.
fn soft_rules(
    vision: i32,
    height: i32,
    randomize: i32,
    score: i32,
    fair_rotation: i32,
    neighbors: i32,
) -> Value {
    json!({
        "vision_front": weighted(vision > 0, vision),
        "height_back": weighted(height > 0, height),
        "randomize": weighted(randomize > 0, randomize),
        "score_balance": weighted(score > 0, score),
        "score_position": {
            "enabled": false,
            "weight": 1,
            "direction": "high_front",
        },
        "score_distribution": {
            "enabled": false,
            "weight": 1,
            "scope": "row",
        },
        "mentor_pairing": {
            "enabled": false,
            "weight": 1,
            "mentor_percentile": 0.75,
            "learner_percentile": 0.25,
            "relation": "desk_mate",
            "avoid_recent_repeats": true,
            "history_lookback": 4,
        },
        "fair_rotation": {
            "enabled": fair_rotation > 0,
            "weight": fair_rotation,
            "avoid_repeating_categories": [
                "front", "back", "side", "corner",
                "near_window", "near_door", "near_ac",
            ],
            "lookback": 4,
        },
        "avoid_recent_neighbors": {
            "enabled": neighbors > 0,
            "weight": neighbors,
            "relation_types": ["desk_mate", "adjacent_any"],
            "lookback": 4,
            "max_recent_count": 1,
            "within_distance": 2,
        },
        "cooling": {
            "enabled": false,
            "weight": 5,
            "cooling_period": 3,
            "relation_types": ["desk_mate", "adjacent_any"],
            "within_distance": 2,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;

    /// Expected JSON for `daily-rotation` — the real `get_preset("daily").rules`
    /// output captured from `.venv/bin/python` (`model_dump(mode="json")`,
    /// seed + soft only).
    const DAILY_EXPECTED: &str = r#"{"seed": 42, "soft": {"avoid_recent_neighbors": {"enabled": true, "lookback": 4, "max_recent_count": 1, "relation_types": ["desk_mate", "adjacent_any"], "weight": 12, "within_distance": 2}, "cooling": {"cooling_period": 3, "enabled": false, "relation_types": ["desk_mate", "adjacent_any"], "weight": 5, "within_distance": 2}, "fair_rotation": {"avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"], "enabled": true, "lookback": 4, "weight": 12}, "height_back": {"enabled": true, "weight": 4}, "mentor_pairing": {"avoid_recent_repeats": true, "enabled": false, "history_lookback": 4, "learner_percentile": 0.25, "mentor_percentile": 0.75, "relation": "desk_mate", "weight": 1}, "randomize": {"enabled": true, "weight": 3}, "score_balance": {"enabled": true, "weight": 4}, "score_distribution": {"enabled": false, "scope": "row", "weight": 1}, "score_position": {"direction": "high_front", "enabled": false, "weight": 1}, "vision_front": {"enabled": true, "weight": 20}}}"#;

    /// Expected JSON for `quick-shuffle` — `get_preset("random").rules`.
    const RANDOM_EXPECTED: &str = r#"{"seed": 42, "soft": {"avoid_recent_neighbors": {"enabled": false, "lookback": 4, "max_recent_count": 1, "relation_types": ["desk_mate", "adjacent_any"], "weight": 0, "within_distance": 2}, "cooling": {"cooling_period": 3, "enabled": false, "relation_types": ["desk_mate", "adjacent_any"], "weight": 5, "within_distance": 2}, "fair_rotation": {"avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"], "enabled": false, "lookback": 4, "weight": 0}, "height_back": {"enabled": false, "weight": 0}, "mentor_pairing": {"avoid_recent_repeats": true, "enabled": false, "history_lookback": 4, "learner_percentile": 0.25, "mentor_percentile": 0.75, "relation": "desk_mate", "weight": 1}, "randomize": {"enabled": true, "weight": 10}, "score_balance": {"enabled": false, "weight": 0}, "score_distribution": {"enabled": false, "scope": "row", "weight": 1}, "score_position": {"direction": "high_front", "enabled": false, "weight": 1}, "vision_front": {"enabled": false, "weight": 0}}}"#;

    /// Expected JSON for `fair-shuffle` — `get_preset("fair-rotation").rules`.
    const FAIR_ROTATION_EXPECTED: &str = r#"{"seed": 42, "soft": {"avoid_recent_neighbors": {"enabled": false, "lookback": 4, "max_recent_count": 1, "relation_types": ["desk_mate", "adjacent_any"], "weight": 0, "within_distance": 2}, "cooling": {"cooling_period": 3, "enabled": false, "relation_types": ["desk_mate", "adjacent_any"], "weight": 5, "within_distance": 2}, "fair_rotation": {"avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"], "enabled": true, "lookback": 4, "weight": 20}, "height_back": {"enabled": false, "weight": 0}, "mentor_pairing": {"avoid_recent_repeats": true, "enabled": false, "history_lookback": 4, "learner_percentile": 0.25, "mentor_percentile": 0.75, "relation": "desk_mate", "weight": 1}, "randomize": {"enabled": true, "weight": 2}, "score_balance": {"enabled": false, "weight": 0}, "score_distribution": {"enabled": false, "scope": "row", "weight": 1}, "score_position": {"direction": "high_front", "enabled": false, "weight": 1}, "vision_front": {"enabled": false, "weight": 0}}}"#;

    /// Expected JSON for `peer-support` — `get_preset("balanced").rules`.
    const BALANCED_EXPECTED: &str = r#"{"seed": 42, "soft": {"avoid_recent_neighbors": {"enabled": false, "lookback": 4, "max_recent_count": 1, "relation_types": ["desk_mate", "adjacent_any"], "weight": 0, "within_distance": 2}, "cooling": {"cooling_period": 3, "enabled": false, "relation_types": ["desk_mate", "adjacent_any"], "weight": 5, "within_distance": 2}, "fair_rotation": {"avoid_repeating_categories": ["front", "back", "side", "corner", "near_window", "near_door", "near_ac"], "enabled": false, "lookback": 4, "weight": 0}, "height_back": {"enabled": false, "weight": 0}, "mentor_pairing": {"avoid_recent_repeats": true, "enabled": false, "history_lookback": 4, "learner_percentile": 0.25, "mentor_percentile": 0.75, "relation": "desk_mate", "weight": 1}, "randomize": {"enabled": true, "weight": 2}, "score_balance": {"enabled": true, "weight": 18}, "score_distribution": {"enabled": false, "scope": "row", "weight": 1}, "score_position": {"direction": "high_front", "enabled": false, "weight": 1}, "vision_front": {"enabled": false, "weight": 0}}}"#;

    fn parse_expected(raw: &str) -> Value {
        from_str(raw).expect("embedded expected JSON must parse")
    }

    /// Field-by-field assertion: every key/value of the expected `soft`
    /// object must equal the produced one (order-independent, recursive).
    fn assert_soft_matches(goal_id: &str, expected_raw: &str) {
        let produced = goal_rules(goal_id).expect("known goal must succeed");
        let expected = parse_expected(expected_raw);
        let produced_soft = produced.get("soft").expect("produced JSON must have soft");
        let expected_soft = expected.get("soft").expect("expected JSON must have soft");
        // Recursively compare every leaf: `Value` equality is per-field and
        // ignores map key ordering.
        assert_eq!(produced_soft, expected_soft, "soft mismatch for {goal_id}");
        assert_eq!(
            produced.get("seed"),
            expected.get("seed"),
            "seed mismatch for {goal_id}"
        );
    }

    #[test]
    fn daily_rotation_matches_python_daily_preset() {
        assert_soft_matches("daily-rotation", DAILY_EXPECTED);
        // Spot-check the distinguishing weights.
        let soft = &goal_rules("daily-rotation").unwrap()["soft"];
        assert_eq!(soft["vision_front"]["weight"], 20);
        assert_eq!(soft["height_back"]["weight"], 4);
        assert_eq!(soft["randomize"]["weight"], 3);
        assert_eq!(soft["score_balance"]["weight"], 4);
        assert_eq!(soft["fair_rotation"]["weight"], 12);
        assert_eq!(soft["avoid_recent_neighbors"]["weight"], 12);
        assert_eq!(soft["fair_rotation"]["enabled"], true);
        assert_eq!(soft["avoid_recent_neighbors"]["enabled"], true);
    }

    #[test]
    fn quick_shuffle_matches_python_random_preset() {
        assert_soft_matches("quick-shuffle", RANDOM_EXPECTED);
        let soft = &goal_rules("quick-shuffle").unwrap()["soft"];
        assert_eq!(soft["randomize"]["weight"], 10);
        assert_eq!(soft["randomize"]["enabled"], true);
        assert_eq!(soft["vision_front"]["enabled"], false);
        assert_eq!(soft["height_back"]["enabled"], false);
        assert_eq!(soft["score_balance"]["enabled"], false);
        assert_eq!(soft["fair_rotation"]["enabled"], false);
        assert_eq!(soft["avoid_recent_neighbors"]["enabled"], false);
    }

    #[test]
    fn fair_shuffle_matches_python_fair_rotation_preset() {
        assert_soft_matches("fair-shuffle", FAIR_ROTATION_EXPECTED);
        let soft = &goal_rules("fair-shuffle").unwrap()["soft"];
        assert_eq!(soft["fair_rotation"]["weight"], 20);
        assert_eq!(soft["fair_rotation"]["enabled"], true);
        assert_eq!(soft["randomize"]["weight"], 2);
        assert_eq!(soft["avoid_recent_neighbors"]["enabled"], false);
    }

    #[test]
    fn peer_support_matches_python_balanced_preset() {
        assert_soft_matches("peer-support", BALANCED_EXPECTED);
        let soft = &goal_rules("peer-support").unwrap()["soft"];
        assert_eq!(soft["score_balance"]["weight"], 18);
        assert_eq!(soft["score_balance"]["enabled"], true);
        assert_eq!(soft["randomize"]["weight"], 2);
        assert_eq!(soft["fair_rotation"]["enabled"], false);
        assert_eq!(soft["avoid_recent_neighbors"]["enabled"], false);
    }

    #[test]
    fn direction_scope_percentile_and_relation_leaf_values_match_python() {
        // Exact-match evidence for the fields the recursive comparison covers
        // implicitly: direction, scope, mentor percentiles/relation, lookbacks.
        // Values are taken verbatim from `get_preset("daily").rules.soft`
        // (`model_dump(mode="json")`).
        let soft = &goal_rules("daily-rotation").unwrap()["soft"];

        assert_eq!(soft["score_position"]["direction"], "high_front");
        assert_eq!(soft["score_position"]["enabled"], false);
        assert_eq!(soft["score_position"]["weight"], 1);

        assert_eq!(soft["score_distribution"]["scope"], "row");
        assert_eq!(soft["score_distribution"]["enabled"], false);
        assert_eq!(soft["score_distribution"]["weight"], 1);

        assert_eq!(soft["mentor_pairing"]["mentor_percentile"], 0.75);
        assert_eq!(soft["mentor_pairing"]["learner_percentile"], 0.25);
        assert_eq!(soft["mentor_pairing"]["relation"], "desk_mate");
        assert_eq!(soft["mentor_pairing"]["avoid_recent_repeats"], true);
        assert_eq!(soft["mentor_pairing"]["history_lookback"], 4);
        assert_eq!(soft["mentor_pairing"]["enabled"], false);
        assert_eq!(soft["mentor_pairing"]["weight"], 1);

        assert_eq!(soft["fair_rotation"]["lookback"], 4);
        assert_eq!(
            soft["fair_rotation"]["avoid_repeating_categories"],
            serde_json::json!([
                "front",
                "back",
                "side",
                "corner",
                "near_window",
                "near_door",
                "near_ac",
            ])
        );

        assert_eq!(soft["avoid_recent_neighbors"]["lookback"], 4);
        assert_eq!(soft["avoid_recent_neighbors"]["max_recent_count"], 1);
        assert_eq!(soft["avoid_recent_neighbors"]["within_distance"], 2);
        assert_eq!(
            soft["avoid_recent_neighbors"]["relation_types"],
            serde_json::json!(["desk_mate", "adjacent_any"])
        );

        assert_eq!(soft["cooling"]["enabled"], false);
        assert_eq!(soft["cooling"]["weight"], 5);
        assert_eq!(soft["cooling"]["cooling_period"], 3);
        assert_eq!(soft["cooling"]["within_distance"], 2);
    }

    #[test]
    fn unknown_goal_is_an_error() {
        let err = goal_rules("warp-speed").unwrap_err();
        assert!(
            err.contains("warp-speed"),
            "error should name the goal: {err}"
        );
        assert!(
            err.contains("Available goals"),
            "error should list goals: {err}"
        );
        assert!(goal_rules("").is_err());
        assert!(goal_rules("   ").is_err());
    }

    #[test]
    fn goal_ids_are_normalized_case_and_underscore_insensitively() {
        for canonical in GOAL_IDS {
            let base = goal_rules(canonical).unwrap();
            assert_eq!(goal_rules(&canonical.to_uppercase()).unwrap(), base);
            assert_eq!(goal_rules(&format!("  {}  ", canonical)).unwrap(), base);
            assert_eq!(
                goal_rules(&canonical.replace('-', "_")).unwrap(),
                base,
                "underscore spelling should be accepted for {canonical}"
            );
        }
    }

    #[test]
    fn ruleset_json_deserializes_into_core_model() {
        use seattrellis_core::models::RuleSet;
        for goal in GOAL_IDS {
            let doc = goal_rules(goal).unwrap();
            let parsed: RuleSet = serde_json::from_value(doc.clone())
                .unwrap_or_else(|e| panic!("core RuleSet failed to deserialize {goal}: {e}"));
            assert_eq!(parsed.seed, 42, "seed for {goal}");
            // The produced document must round-trip through core exactly:
            // re-serializing the parsed model yields the same JSON we emitted.
            let round_trip = serde_json::to_value(&parsed).unwrap();
            assert_eq!(round_trip, doc, "core round-trip mismatch for {goal}");
        }
    }

    #[test]
    fn soft_object_has_exactly_the_core_soft_rule_keys() {
        let soft = goal_rules("daily-rotation").unwrap()["soft"].clone();
        let keys: std::collections::BTreeSet<&str> = soft
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let expected: std::collections::BTreeSet<&str> = [
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
        ]
        .into_iter()
        .collect();
        assert_eq!(keys, expected, "soft keys must match core SoftRules fields");
    }
}
