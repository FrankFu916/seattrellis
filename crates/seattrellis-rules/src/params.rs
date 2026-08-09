//! Typed rule parameters (M3-01). Each struct mirrors the Python rule model
//! (src/seattrellis/models/rules.py) so the RuleSpec parameter schema is
//! accurate; schemars derives the JSON Schema consumed by the React UI.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Hard rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixedSeatParam {
    pub student: String,
    pub seat_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PairStudentsParam {
    pub students: [String; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    Euclidean,
    Graph,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MinDistanceParam {
    pub students: [String; 2],
    /// Must be > 0 (validated at rule resolution, M3-03 precheck).
    pub distance: f64,
    #[serde(default = "default_graph")]
    pub metric: DistanceMetric,
}

fn default_graph() -> DistanceMetric {
    DistanceMetric::Graph
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupParam {
    pub name: String,
    pub students: Vec<String>,
    /// `true` = members must sit apart; `false` = members must sit together.
    #[serde(default)]
    pub separate: bool,
    // Note: the default `students` list is a template (empty); membership
    // validation (>= 2 distinct members) happens at rule resolution.
}

// ---------------------------------------------------------------------------
// Soft rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeightedRuleParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
}

fn default_weight() -> i32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FairRotationParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_categories")]
    pub avoid_repeating_categories: Vec<String>,
    #[serde(default = "default_lookback")]
    pub lookback: i32,
}

pub(crate) fn default_categories() -> Vec<String> {
    [
        "front",
        "back",
        "side",
        "corner",
        "near_window",
        "near_door",
        "near_ac",
    ]
    .iter()
    .map(|value| value.to_string())
    .collect()
}

fn default_lookback() -> i32 {
    4
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AvoidRecentNeighborsParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_relations")]
    pub relation_types: Vec<String>,
    #[serde(default = "default_lookback")]
    pub lookback: i32,
    #[serde(default = "default_max_recent")]
    pub max_recent_count: i32,
    #[serde(default = "default_within_distance")]
    pub within_distance: i32,
}

pub(crate) fn default_relations() -> Vec<String> {
    ["desk_mate", "adjacent_any"]
        .iter()
        .map(|value| value.to_string())
        .collect()
}

fn default_max_recent() -> i32 {
    1
}

fn default_within_distance() -> i32 {
    2
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoolingParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cooling_weight")]
    pub weight: i32,
    #[serde(default = "default_cooling_period")]
    pub cooling_period: i32,
    #[serde(default = "default_relations")]
    pub relation_types: Vec<String>,
    #[serde(default = "default_within_distance")]
    pub within_distance: i32,
}

fn default_cooling_weight() -> i32 {
    5
}

fn default_cooling_period() -> i32 {
    3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScoreDirection {
    #[default]
    HighFront,
    HighBack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScorePositionParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default)]
    pub direction: ScoreDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum DistributionScope {
    #[default]
    Row,
    Group,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoreDistributionParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default)]
    pub scope: DistributionScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum PairRelation {
    #[default]
    DeskMate,
    AdjacentAny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MentorPairingParam {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_mentor_percentile")]
    pub mentor_percentile: f64,
    #[serde(default = "default_learner_percentile")]
    pub learner_percentile: f64,
    #[serde(default)]
    pub relation: PairRelation,
    #[serde(default = "default_avoid_repeats")]
    pub avoid_recent_repeats: bool,
    #[serde(default = "default_lookback")]
    pub history_lookback: i32,
}

fn default_mentor_percentile() -> f64 {
    0.75
}

fn default_learner_percentile() -> f64 {
    0.25
}

fn default_avoid_repeats() -> bool {
    true
}
