//! RuleSpec: the metadata contract for every official rule (M3-01).
//!
//! The Rust registry is the single source of truth: the React UI renders
//! controls from the emitted registry JSON and never hard-codes rule lists
//! (M6-02). Each spec carries the parameter schema (schemars), defaults,
//! localized keys and objective semantics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::params;

/// Bilingual keys for labels/help/explanations; the UI resolves them from its
/// i18n catalog (M6-04), the registry never embeds raw prose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalizedKeys {
    pub zh: String,
    pub en: String,
}

/// Whether a rule is a hard constraint or a soft objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    Hard,
    Soft,
}

/// The explanation code emitted when this rule contributes to an infeasible
/// or penalized outcome (M3-06 feasibility reports key off these).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExplanationCode {
    FixedSeatConflict,
    MustBeAdjacentUnsatisfied,
    CannotBeAdjacentViolated,
    MinDistanceViolated,
    GroupConflict,
    CapacityExceeded,
    StudentDomainEmpty,
    SoftObjective,
}

/// Objective semantics for soft rules (M4-01 audit will freeze the exact
/// cost contract; this metadata drives UI labels and explanation wording).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveMeta {
    /// Whether higher or lower cost is better (always "lower" today; kept
    /// explicit so the UI never guesses).
    pub direction: String,
    /// How the weight scales the contribution (relative weight).
    pub weight_semantics: String,
    /// Objective audit version, bumped when the cost contract changes.
    pub audit_version: u32,
}

/// The metadata record for one official rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleSpec {
    /// Stable rule ID (matches the wire key, e.g. `fixed_seats`).
    pub id: String,
    /// Spec version of this metadata record; bump on contract changes.
    pub spec_version: u32,
    pub category: RuleCategory,
    pub label: LocalizedKeys,
    pub help: LocalizedKeys,
    /// i18n keys for explanation wording (M3-06).
    pub explanation: LocalizedKeys,
    pub explanation_code: ExplanationCode,
    /// JSON Schema for the rule's parameters (schemars-generated).
    pub param_schema: Value,
    /// Default parameter document (what a fresh rule starts from).
    pub defaults: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective: Option<ObjectiveMeta>,
}

/// Build a spec from a typed param type: schema + defaults via schemars.
#[allow(clippy::too_many_arguments)] // registry table entries stay flat
fn spec_from<T: JsonSchema + serde::Serialize + Clone>(
    id: &str,
    spec_version: u32,
    category: RuleCategory,
    label: LocalizedKeys,
    help: LocalizedKeys,
    explanation: LocalizedKeys,
    explanation_code: ExplanationCode,
    defaults: T,
    objective: Option<ObjectiveMeta>,
) -> RuleSpec {
    let param_schema = serde_json::to_value(schemars::schema_for!(T)).expect("schema serializes");
    let defaults = serde_json::to_value(defaults).expect("defaults serialize");
    RuleSpec {
        id: id.to_string(),
        spec_version,
        category,
        label,
        help,
        explanation,
        explanation_code,
        param_schema,
        defaults,
        objective,
    }
}

fn keys(zh: &str, en: &str) -> LocalizedKeys {
    LocalizedKeys {
        zh: zh.to_string(),
        en: en.to_string(),
    }
}

/// The official rule registry (M3-01): one spec per rule, hard and soft.
pub fn rule_specs() -> Vec<RuleSpec> {
    vec![
        // ------------------------------------------------------------------
        // Hard rules
        // ------------------------------------------------------------------
        spec_from(
            "fixed_seats",
            1,
            RuleCategory::Hard,
            keys("rule.fixed_seats.label", "rule.fixed_seats.label"),
            keys("rule.fixed_seats.help", "rule.fixed_seats.help"),
            keys(
                "rule.fixed_seats.explanation",
                "rule.fixed_seats.explanation",
            ),
            ExplanationCode::FixedSeatConflict,
            params::FixedSeatParam {
                student: String::new(),
                seat_id: String::new(),
            },
            None,
        ),
        spec_from(
            "must_be_adjacent",
            1,
            RuleCategory::Hard,
            keys("rule.must_be_adjacent.label", "rule.must_be_adjacent.label"),
            keys("rule.must_be_adjacent.help", "rule.must_be_adjacent.help"),
            keys(
                "rule.must_be_adjacent.explanation",
                "rule.must_be_adjacent.explanation",
            ),
            ExplanationCode::MustBeAdjacentUnsatisfied,
            params::PairStudentsParam {
                students: [String::new(), String::new()],
            },
            None,
        ),
        spec_from(
            "cannot_be_adjacent",
            1,
            RuleCategory::Hard,
            keys(
                "rule.cannot_be_adjacent.label",
                "rule.cannot_be_adjacent.label",
            ),
            keys(
                "rule.cannot_be_adjacent.help",
                "rule.cannot_be_adjacent.help",
            ),
            keys(
                "rule.cannot_be_adjacent.explanation",
                "rule.cannot_be_adjacent.explanation",
            ),
            ExplanationCode::CannotBeAdjacentViolated,
            params::PairStudentsParam {
                students: [String::new(), String::new()],
            },
            None,
        ),
        spec_from(
            "min_distance",
            1,
            RuleCategory::Hard,
            keys("rule.min_distance.label", "rule.min_distance.label"),
            keys("rule.min_distance.help", "rule.min_distance.help"),
            keys(
                "rule.min_distance.explanation",
                "rule.min_distance.explanation",
            ),
            ExplanationCode::MinDistanceViolated,
            params::MinDistanceParam {
                students: [String::new(), String::new()],
                distance: 1.0,
                metric: params::DistanceMetric::Graph,
            },
            None,
        ),
        spec_from(
            "groups",
            1,
            RuleCategory::Hard,
            keys("rule.groups.label", "rule.groups.label"),
            keys("rule.groups.help", "rule.groups.help"),
            keys("rule.groups.explanation", "rule.groups.explanation"),
            ExplanationCode::GroupConflict,
            params::GroupParam {
                name: String::new(),
                students: Vec::new(),
                separate: false,
            },
            None,
        ),
        // ------------------------------------------------------------------
        // Soft rules
        // ------------------------------------------------------------------
        spec_from(
            "vision_front",
            1,
            RuleCategory::Soft,
            keys("rule.vision_front.label", "rule.vision_front.label"),
            keys("rule.vision_front.help", "rule.vision_front.help"),
            keys(
                "rule.vision_front.explanation",
                "rule.vision_front.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::WeightedRuleParam::default(),
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "height_back",
            1,
            RuleCategory::Soft,
            keys("rule.height_back.label", "rule.height_back.label"),
            keys("rule.height_back.help", "rule.height_back.help"),
            keys(
                "rule.height_back.explanation",
                "rule.height_back.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::WeightedRuleParam::default(),
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "randomize",
            1,
            RuleCategory::Soft,
            keys("rule.randomize.label", "rule.randomize.label"),
            keys("rule.randomize.help", "rule.randomize.help"),
            keys("rule.randomize.explanation", "rule.randomize.explanation"),
            ExplanationCode::SoftObjective,
            params::WeightedRuleParam::default(),
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "score_balance",
            1,
            RuleCategory::Soft,
            keys("rule.score_balance.label", "rule.score_balance.label"),
            keys("rule.score_balance.help", "rule.score_balance.help"),
            keys(
                "rule.score_balance.explanation",
                "rule.score_balance.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::WeightedRuleParam::default(),
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "fair_rotation",
            1,
            RuleCategory::Soft,
            keys("rule.fair_rotation.label", "rule.fair_rotation.label"),
            keys("rule.fair_rotation.help", "rule.fair_rotation.help"),
            keys(
                "rule.fair_rotation.explanation",
                "rule.fair_rotation.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::FairRotationParam {
                enabled: false,
                weight: 10,
                avoid_repeating_categories: params::default_categories(),
                lookback: 4,
            },
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "avoid_recent_neighbors",
            1,
            RuleCategory::Soft,
            keys(
                "rule.avoid_recent_neighbors.label",
                "rule.avoid_recent_neighbors.label",
            ),
            keys(
                "rule.avoid_recent_neighbors.help",
                "rule.avoid_recent_neighbors.help",
            ),
            keys(
                "rule.avoid_recent_neighbors.explanation",
                "rule.avoid_recent_neighbors.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::AvoidRecentNeighborsParam {
                enabled: false,
                weight: 10,
                relation_types: params::default_relations(),
                lookback: 4,
                max_recent_count: 1,
                within_distance: 2,
            },
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "cooling",
            1,
            RuleCategory::Soft,
            keys("rule.cooling.label", "rule.cooling.label"),
            keys("rule.cooling.help", "rule.cooling.help"),
            keys("rule.cooling.explanation", "rule.cooling.explanation"),
            ExplanationCode::SoftObjective,
            params::CoolingParam {
                enabled: false,
                weight: 5,
                cooling_period: 3,
                relation_types: params::default_relations(),
                within_distance: 2,
            },
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                // Ledger §8: both languages implement cooling as an
                // approximation merged into avoid_recent_neighbors; the
                // strong semantics need a v2 product decision.
                audit_version: 1,
            }),
        ),
        spec_from(
            "score_position",
            1,
            RuleCategory::Soft,
            keys("rule.score_position.label", "rule.score_position.label"),
            keys("rule.score_position.help", "rule.score_position.help"),
            keys(
                "rule.score_position.explanation",
                "rule.score_position.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::ScorePositionParam {
                enabled: false,
                weight: 1,
                direction: params::ScoreDirection::HighFront,
            },
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "score_distribution",
            1,
            RuleCategory::Soft,
            keys(
                "rule.score_distribution.label",
                "rule.score_distribution.label",
            ),
            keys(
                "rule.score_distribution.help",
                "rule.score_distribution.help",
            ),
            keys(
                "rule.score_distribution.explanation",
                "rule.score_distribution.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::ScoreDistributionParam {
                enabled: false,
                weight: 1,
                scope: params::DistributionScope::Row,
            },
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
        spec_from(
            "mentor_pairing",
            1,
            RuleCategory::Soft,
            keys("rule.mentor_pairing.label", "rule.mentor_pairing.label"),
            keys("rule.mentor_pairing.help", "rule.mentor_pairing.help"),
            keys(
                "rule.mentor_pairing.explanation",
                "rule.mentor_pairing.explanation",
            ),
            ExplanationCode::SoftObjective,
            params::MentorPairingParam {
                enabled: false,
                weight: 10,
                mentor_percentile: 0.75,
                learner_percentile: 0.25,
                relation: params::PairRelation::DeskMate,
                avoid_recent_repeats: true,
                history_lookback: 4,
            },
            Some(ObjectiveMeta {
                direction: "lower".into(),
                weight_semantics: "relative".into(),
                audit_version: 1,
            }),
        ),
    ]
}

/// Look up one rule spec by its stable ID.
pub fn rule_spec(id: &str) -> Option<RuleSpec> {
    rule_specs().into_iter().find(|spec| spec.id == id)
}

/// The whole registry as JSON (consumed by the React UI via the generated
/// client; drift-checked by `xtask contract check`).
pub fn rule_registry_json() -> Value {
    serde_json::to_value(rule_specs()).expect("registry serializes")
}

impl Default for params::WeightedRuleParam {
    fn default() -> Self {
        params::WeightedRuleParam {
            enabled: false,
            weight: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_official_rule_has_a_spec() {
        let specs = rule_specs();
        let ids: Vec<&str> = specs.iter().map(|spec| spec.id.as_str()).collect();
        for id in [
            "fixed_seats",
            "must_be_adjacent",
            "cannot_be_adjacent",
            "min_distance",
            "groups",
            "vision_front",
            "height_back",
            "randomize",
            "score_balance",
            "fair_rotation",
            "avoid_recent_neighbors",
            "cooling",
            "score_position",
            "score_distribution",
            "mentor_pairing",
        ] {
            assert!(ids.contains(&id), "missing rule spec: {id}");
        }
        assert_eq!(ids.len(), 15, "registry must not silently grow or shrink");
    }

    #[test]
    fn every_spec_has_complete_metadata() {
        for spec in rule_specs() {
            assert!(!spec.id.is_empty());
            assert!(spec.spec_version >= 1);
            assert!(!spec.label.zh.is_empty() && !spec.label.en.is_empty());
            assert!(!spec.help.zh.is_empty() && !spec.help.en.is_empty());
            assert!(!spec.explanation.zh.is_empty() && !spec.explanation.en.is_empty());
            assert!(
                spec.param_schema.get("properties").is_some(),
                "{}: param schema missing properties",
                spec.id
            );
            assert!(spec.defaults.is_object(), "{}: defaults missing", spec.id);
            if spec.category == RuleCategory::Soft {
                assert!(
                    spec.objective.is_some(),
                    "{}: soft rule must declare objective semantics",
                    spec.id
                );
            }
        }
    }

    #[test]
    fn defaults_validate_against_their_own_schema() {
        // Every spec's defaults document must validate against its param
        // schema (guards against spec/default drift).
        for spec in rule_specs() {
            let schema: serde_json::Value = spec.param_schema.clone();
            let validator = jsonschema::validator_for(&schema)
                .unwrap_or_else(|error| panic!("{}: bad param schema: {error}", spec.id));
            let result = validator.validate(&spec.defaults);
            assert!(
                result.is_ok(),
                "{}: defaults violate the param schema: {}",
                spec.id,
                result
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_default(),
            );
        }
    }

    #[test]
    fn registry_json_round_trips() {
        let registry = rule_registry_json();
        let specs: Vec<RuleSpec> = serde_json::from_value(registry).unwrap();
        assert_eq!(specs.len(), 15);
        assert_eq!(specs[0].id, "fixed_seats");
    }
}
