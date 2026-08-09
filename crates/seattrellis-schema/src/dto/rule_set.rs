//! RuleSet artifact DTO (M2-01 follow-up): the v2 rule-set document, mirroring
//! the Python `RuleSet` model (models/rules.py) field-for-field so artifact
//! round-trips are lossless. Strict parsing: unknown fields are rejected.

use serde::{Deserialize, Serialize};

/// The v2 rule-set artifact (`kind = "ruleset"`). Every field mirrors the
/// Python model; defaults match Python's `default_factory` values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSetArtifact {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub hard: HardRules,
    #[serde(default)]
    pub soft: SoftRules,
    #[serde(default)]
    pub groups: Vec<GroupRule>,
}

fn default_schema_version() -> u32 {
    1
}

fn default_seed() -> u64 {
    42
}

/// Hard constraints: fixed seats and pairwise adjacency/distance rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct HardRules {
    #[serde(default)]
    pub fixed_seats: Vec<FixedSeatRule>,
    #[serde(default)]
    pub must_be_adjacent: Vec<PairRule>,
    #[serde(default)]
    pub cannot_be_adjacent: Vec<PairRule>,
    #[serde(default)]
    pub min_distance: Vec<MinDistanceRule>,
}

/// Pin one student to one seat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedSeatRule {
    pub student: String,
    pub seat_id: String,
}

/// A pairwise student constraint (keys).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairRule {
    /// Exactly two student references.
    pub students: [String; 2],
}

/// Minimum graph/Euclidean distance between two students.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinDistanceRule {
    pub students: [String; 2],
    pub distance: f64,
    #[serde(default = "default_metric")]
    pub metric: DistanceMetric,
}

fn default_metric() -> DistanceMetric {
    DistanceMetric::Euclidean
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistanceMetric {
    #[serde(rename = "euclidean")]
    Euclidean,
    #[serde(rename = "graph")]
    Graph,
}

/// A named hard group rule: `separate` keeps every member pair apart,
/// `together` requires every member pair to be adjacent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRule {
    pub name: String,
    #[serde(default)]
    pub students: Vec<String>,
    #[serde(default)]
    pub separate: bool,
    #[serde(default)]
    pub together: bool,
}

/// Soft objectives. Defaults mirror Python's `SoftRules` factories
/// (vision_front/height_back/randomize enabled; the rest disabled).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftRules {
    #[serde(default = "default_vision_front")]
    pub vision_front: WeightedRule,
    #[serde(default = "default_height_back")]
    pub height_back: WeightedRule,
    #[serde(default = "default_randomize")]
    pub randomize: WeightedRule,
    #[serde(default)]
    pub score_balance: WeightedRule,
    #[serde(default)]
    pub score_position: ScorePositionRule,
    #[serde(default)]
    pub score_distribution: ScoreDistributionRule,
    #[serde(default)]
    pub mentor_pairing: MentorPairingRule,
    #[serde(default)]
    pub fair_rotation: FairRotationRule,
    #[serde(default)]
    pub avoid_recent_neighbors: AvoidRecentNeighborsRule,
    #[serde(default)]
    pub cooling: CoolingRule,
}

impl Default for SoftRules {
    fn default() -> Self {
        SoftRules {
            vision_front: default_vision_front(),
            height_back: default_height_back(),
            randomize: default_randomize(),
            score_balance: WeightedRule::default(),
            score_position: ScorePositionRule::default(),
            score_distribution: ScoreDistributionRule::default(),
            mentor_pairing: MentorPairingRule::default(),
            fair_rotation: FairRotationRule::default(),
            avoid_recent_neighbors: AvoidRecentNeighborsRule::default(),
            cooling: CoolingRule::default(),
        }
    }
}

fn default_vision_front() -> WeightedRule {
    WeightedRule { enabled: true, weight: 20 }
}

fn default_height_back() -> WeightedRule {
    WeightedRule { enabled: true, weight: 1 }
}

fn default_randomize() -> WeightedRule {
    WeightedRule { enabled: true, weight: 1 }
}

/// An enabled flag + non-negative weight shared by most soft rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WeightedRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
}

fn default_weight() -> i32 {
    1
}

/// Place score ranks toward the front or back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScorePositionRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_direction")]
    pub direction: ScoreDirection,
}

impl Default for ScorePositionRule {
    fn default() -> Self {
        ScorePositionRule {
            enabled: false,
            weight: default_weight(),
            direction: default_direction(),
        }
    }
}

fn default_direction() -> ScoreDirection {
    ScoreDirection::HighFront
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreDirection {
    #[serde(rename = "high_front")]
    HighFront,
    #[serde(rename = "high_back")]
    HighBack,
}

/// Balance score-rank means across physical rows or named seat groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreDistributionRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_scope")]
    pub scope: ScoreScope,
}

impl Default for ScoreDistributionRule {
    fn default() -> Self {
        ScoreDistributionRule {
            enabled: false,
            weight: default_weight(),
            scope: default_scope(),
        }
    }
}

fn default_scope() -> ScoreScope {
    ScoreScope::Row
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreScope {
    #[serde(rename = "row")]
    Row,
    #[serde(rename = "group")]
    Group,
}

/// Pair high- and low-ranked students through a soft proximity goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MentorPairingRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_mentor_percentile")]
    pub mentor_percentile: f64,
    #[serde(default = "default_learner_percentile")]
    pub learner_percentile: f64,
    #[serde(default = "default_mentor_relation")]
    pub relation: MentorRelation,
    #[serde(default = "default_true")]
    pub avoid_recent_repeats: bool,
    #[serde(default = "default_lookback")]
    pub history_lookback: i32,
}

impl Default for MentorPairingRule {
    fn default() -> Self {
        MentorPairingRule {
            enabled: false,
            weight: default_weight(),
            mentor_percentile: default_mentor_percentile(),
            learner_percentile: default_learner_percentile(),
            relation: default_mentor_relation(),
            avoid_recent_repeats: default_true(),
            history_lookback: default_lookback(),
        }
    }
}

fn default_mentor_percentile() -> f64 {
    0.75
}

fn default_learner_percentile() -> f64 {
    0.25
}

fn default_mentor_relation() -> MentorRelation {
    MentorRelation::DeskMate
}

fn default_true() -> bool {
    true
}

fn default_lookback() -> i32 {
    4
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MentorRelation {
    #[serde(rename = "desk_mate")]
    DeskMate,
    #[serde(rename = "adjacent_any")]
    AdjacentAny,
}

/// Fair-rotation: avoid repeating seat position categories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FairRotationRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_categories")]
    pub avoid_repeating_categories: Vec<String>,
    #[serde(default = "default_lookback")]
    pub lookback: i32,
}

impl Default for FairRotationRule {
    fn default() -> Self {
        FairRotationRule {
            enabled: false,
            weight: default_weight(),
            avoid_repeating_categories: default_categories(),
            lookback: default_lookback(),
        }
    }
}

fn default_categories() -> Vec<String> {
    vec![
        "front".to_string(),
        "back".to_string(),
        "side".to_string(),
        "corner".to_string(),
        "near_window".to_string(),
        "near_door".to_string(),
        "near_ac".to_string(),
    ]
}

/// Penalize recent desk-mate / neighbor pairings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvoidRecentNeighborsRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_relation_types")]
    pub relation_types: Vec<String>,
    #[serde(default = "default_lookback")]
    pub lookback: i32,
    #[serde(default = "default_max_recent_count")]
    pub max_recent_count: i32,
    #[serde(default = "default_within_distance")]
    pub within_distance: i32,
}

impl Default for AvoidRecentNeighborsRule {
    fn default() -> Self {
        AvoidRecentNeighborsRule {
            enabled: false,
            weight: default_weight(),
            relation_types: default_relation_types(),
            lookback: default_lookback(),
            max_recent_count: default_max_recent_count(),
            within_distance: default_within_distance(),
        }
    }
}

fn default_relation_types() -> Vec<String> {
    vec!["desk_mate".to_string(), "adjacent_any".to_string()]
}

fn default_max_recent_count() -> i32 {
    1
}

fn default_within_distance() -> i32 {
    2
}

/// Cooling period between repeated assignments (strict recent-neighbor form).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoolingRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_cooling_period")]
    pub cooling_period: i32,
    #[serde(default = "default_relation_types")]
    pub relation_types: Vec<String>,
    #[serde(default = "default_within_distance")]
    pub within_distance: i32,
}

impl Default for CoolingRule {
    fn default() -> Self {
        CoolingRule {
            enabled: false,
            weight: default_weight(),
            cooling_period: default_cooling_period(),
            relation_types: default_relation_types(),
            within_distance: default_within_distance(),
        }
    }
}

fn default_cooling_period() -> i32 {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_roundtrips_with_defaults() {
        let document = r#"{
            "schema_version": 1,
            "seed": 42,
            "hard": {
                "fixed_seats": [{"student": "s1", "seat_id": "R1C1"}],
                "must_be_adjacent": [{"students": ["s1", "s2"]}],
                "cannot_be_adjacent": [{"students": ["s3", "s4"]}],
                "min_distance": [{"students": ["s5", "s6"], "distance": 2.0, "metric": "graph"}]
            },
            "soft": {
                "vision_front": {"enabled": true, "weight": 20},
                "mentor_pairing": {"enabled": true, "weight": 5}
            },
            "groups": [{"name": "team-a", "students": ["s1", "s2"], "together": true}]
        }"#;
        let parsed: RuleSetArtifact = serde_json::from_str(document).unwrap();
        assert_eq!(parsed.hard.fixed_seats[0].seat_id, "R1C1");
        assert_eq!(parsed.hard.min_distance[0].metric, DistanceMetric::Graph);
        assert_eq!(parsed.soft.vision_front.weight, 20);
        assert_eq!(parsed.soft.height_back, default_height_back());
        assert_eq!(parsed.soft.cooling.cooling_period, 3);
        assert!(parsed.groups[0].together);

        let encoded = serde_json::to_string(&parsed).unwrap();
        let roundtrip: RuleSetArtifact = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip, parsed);
    }

    #[test]
    fn ruleset_rejects_unknown_fields() {
        let document = r#"{
            "schema_version": 1,
            "seed": 42,
            "unknown_rule_kind": true
        }"#;
        let error = serde_json::from_str::<RuleSetArtifact>(document).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
    }

    #[test]
    fn ruleset_rejects_unknown_nested_fields() {
        let document = r#"{
            "schema_version": 1,
            "seed": 42,
            "soft": {"vision_front": {"enabled": true, "weight": 20, "bogus": 1}}
        }"#;
        let error = serde_json::from_str::<RuleSetArtifact>(document).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
    }
}
