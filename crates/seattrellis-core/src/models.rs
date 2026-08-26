//! Domain types shared by the cost functions and the score soft objectives.
//!
//! This is the merged model layer for the two self-contained Rust ports that
//! were folded into this crate:
//!
//! * the cost module port (`cost.rs`, mirroring `solver/backend_common.py` and
//!   `history.py`), and
//! * the soft-objectives module port (`objectives.rs`, mirroring
//!   `solver/soft_objectives.py`).
//!
//! Types are mapped field-for-field from the Python pydantic models
//! (`models/student.py`, `models/layout.py`, `models/rules.py`,
//! `models/history.py`). Only the fields the cost / objective computations read
//! are kept, and defaults mirror the Python pydantic defaults exactly. Every
//! field that has a Python default carries `#[serde(default)]` so partial JSON
//! deserializes exactly like pydantic.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Student (models/student.py)
// ---------------------------------------------------------------------------

/// A student record. `key` mirrors the Python `student_id or name or ""`.
///
/// `vision` is stored as its string rendering to mirror `str | float | None`:
/// a numeric vision like `0.8` is stored as `"0.8"`, a keyword like `"poor"`
/// as `"poor"`. [`student_needs_front`](crate::cost::student_needs_front)
/// parses it on demand, exactly like Python.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Student {
    pub key: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub height_cm: Option<f64>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub vision: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub needs: Vec<String>,
}

impl Student {
    /// `Student.new` helper kept from the objectives port (`key`, `score`).
    pub fn new(key: impl Into<String>, score: Option<f64>) -> Self {
        Self {
            key: key.into(),
            display_name: None,
            height_cm: None,
            score,
            vision: None,
            tags: Vec::new(),
            needs: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Seat / Layout (models/layout.py)
// ---------------------------------------------------------------------------

fn default_enabled() -> bool {
    true
}

/// A seat node. `x`/`y` default to `col`/`row` when unset (the pydantic
/// `default_coordinates` model validator); they are only read by the
/// distance-based adjacency path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Seat {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub near_window: bool,
    #[serde(default)]
    pub near_door: bool,
    #[serde(default)]
    pub near_platform: bool,
    #[serde(default)]
    pub near_ac: bool,
}

impl Seat {
    /// `SeatNode.new(seat_id, row, col)` helper kept from the objectives port.
    pub fn new(seat_id: impl Into<String>, row: i32, col: i32) -> Self {
        Self {
            seat_id: seat_id.into(),
            row,
            col,
            x: None,
            y: None,
            enabled: true,
            zone: None,
            group_id: None,
            near_window: false,
            near_door: false,
            near_platform: false,
            near_ac: false,
        }
    }

    /// Default coordinates mirror the Python `SeatNode` model_validator:
    /// x defaults to col and y defaults to row.
    pub fn x_default(&self) -> f64 {
        self.x.unwrap_or(self.col as f64)
    }

    pub fn y_default(&self) -> f64 {
        self.y.unwrap_or(self.row as f64)
    }
}

/// `AdjacencyConfig` (`models/layout.py`) — drives the derived adjacency graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjacencyConfig {
    #[serde(default = "default_horizontal")]
    pub include_horizontal: bool,
    #[serde(default)]
    pub include_vertical: bool,
    #[serde(default)]
    pub include_diagonal: bool,
    #[serde(default = "default_one")]
    pub max_row_delta: i32,
    #[serde(default = "default_one")]
    pub max_col_delta: i32,
    #[serde(default)]
    pub max_distance: Option<f64>,
    #[serde(default = "default_use_xy")]
    pub use_xy_distance: bool,
    #[serde(default)]
    pub custom_edges: Vec<(String, String)>,
}

fn default_horizontal() -> bool {
    true
}

fn default_use_xy() -> bool {
    true
}

fn default_one() -> i32 {
    1
}

impl Default for AdjacencyConfig {
    fn default() -> Self {
        Self {
            include_horizontal: true,
            include_vertical: false,
            include_diagonal: false,
            max_row_delta: 1,
            max_col_delta: 1,
            max_distance: None,
            use_xy_distance: true,
            custom_edges: Vec::new(),
        }
    }
}

fn default_layout_id() -> String {
    "default-layout".to_string()
}

fn default_layout_name() -> String {
    "Classroom".to_string()
}

/// `ClassroomLayout` (`models/layout.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default = "default_layout_id")]
    pub layout_id: String,
    #[serde(default = "default_layout_name")]
    pub name: String,
    pub seats: Vec<Seat>,
    #[serde(default)]
    pub adjacency: AdjacencyConfig,
}

impl Layout {
    pub fn new(seats: Vec<Seat>) -> Self {
        Self {
            layout_id: default_layout_id(),
            name: default_layout_name(),
            seats,
            adjacency: AdjacencyConfig::default(),
        }
    }

    /// Enabled seats, in layout order (mirrors `enabled_seats`).
    pub fn enabled_seats(&self) -> Vec<&Seat> {
        self.seats.iter().filter(|seat| seat.enabled).collect()
    }

    pub fn seat_by_id(&self, seat_id: &str) -> Option<&Seat> {
        self.seats.iter().find(|seat| seat.seat_id == seat_id)
    }
}

// ---------------------------------------------------------------------------
// Rules (models/rules.py)
// ---------------------------------------------------------------------------

/// `ScorePositionRule.direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScoreDirection {
    #[serde(rename = "high_front")]
    #[default]
    HighFront,
    #[serde(rename = "high_back")]
    HighBack,
}

/// `ScoreDistributionRule.scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DistributionScope {
    #[serde(rename = "row")]
    #[default]
    Row,
    #[serde(rename = "group")]
    Group,
}

/// `MentorPairingRule.relation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PairRelation {
    #[serde(rename = "desk_mate")]
    #[default]
    DeskMate,
    #[serde(rename = "adjacent_any")]
    AdjacentAny,
}

/// `WeightedRule` base: `enabled: bool = False`, `weight: int = 1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeightedRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
}

impl Default for WeightedRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 1,
        }
    }
}

/// `ScorePositionRule` (`models/rules.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScorePositionRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub direction: ScoreDirection,
}

impl Default for ScorePositionRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 1,
            direction: ScoreDirection::HighFront,
        }
    }
}

/// `ScoreDistributionRule` (`models/rules.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreDistributionRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub scope: DistributionScope,
}

impl Default for ScoreDistributionRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 1,
            scope: DistributionScope::Row,
        }
    }
}

/// `MentorPairingRule` (`models/rules.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentorPairingRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub mentor_percentile: f64,
    #[serde(default)]
    pub learner_percentile: f64,
    #[serde(default)]
    pub relation: PairRelation,
    #[serde(default)]
    pub avoid_recent_repeats: bool,
    #[serde(default)]
    pub history_lookback: i32,
}

impl Default for MentorPairingRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 1,
            mentor_percentile: 0.75,
            learner_percentile: 0.25,
            relation: PairRelation::DeskMate,
            avoid_recent_repeats: true,
            history_lookback: 4,
        }
    }
}

/// `FairRotationRule` (`models/rules.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FairRotationRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub avoid_repeating_categories: Vec<String>,
    /// `None` means "all history", mirroring the cost port's `int | None`.
    #[serde(default)]
    pub lookback: Option<i32>,
}

impl Default for FairRotationRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 10,
            avoid_repeating_categories: vec![
                "front".to_string(),
                "back".to_string(),
                "side".to_string(),
                "corner".to_string(),
                "near_window".to_string(),
                "near_door".to_string(),
                "near_ac".to_string(),
            ],
            lookback: Some(4),
        }
    }
}

/// `AvoidRecentNeighborsRule` (`models/rules.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvoidRecentNeighborsRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub relation_types: Vec<String>,
    /// `None` means "all history", mirroring the cost port's `int | None`.
    #[serde(default)]
    pub lookback: Option<i32>,
    #[serde(default)]
    pub max_recent_count: i32,
    #[serde(default)]
    pub within_distance: i32,
}

impl Default for AvoidRecentNeighborsRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 10,
            relation_types: vec!["desk_mate".to_string(), "adjacent_any".to_string()],
            lookback: Some(4),
            max_recent_count: 1,
            within_distance: 2,
        }
    }
}

/// `CoolingRule` (`models/rules.py`) — consumed by `effective_neighbor_rule`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoolingRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: i32,
    #[serde(default)]
    pub cooling_period: i32,
    #[serde(default)]
    pub relation_types: Vec<String>,
    #[serde(default)]
    pub within_distance: i32,
}

impl Default for CoolingRule {
    fn default() -> Self {
        Self {
            enabled: false,
            weight: 5,
            cooling_period: 3,
            relation_types: vec!["desk_mate".to_string(), "adjacent_any".to_string()],
            within_distance: 2,
        }
    }
}

/// `SoftRules` (`models/rules.py`) — the full set read across the cost
/// functions and the score soft objectives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoftRules {
    #[serde(default)]
    pub vision_front: WeightedRule,
    #[serde(default)]
    pub height_back: WeightedRule,
    #[serde(default)]
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
        Self {
            vision_front: WeightedRule {
                enabled: true,
                weight: 20,
            },
            height_back: WeightedRule {
                enabled: true,
                weight: 1,
            },
            randomize: WeightedRule {
                enabled: true,
                weight: 1,
            },
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

/// `GroupRule` (`models/rules.py`) — a named hard group rule for separation or
/// togetherness. Membership is expanded into pairwise constraints by
/// [`crate::resolve_group_rules`]; `together` requires every member pair to be
/// adjacent while `separate` keeps every member pair apart, exactly like the
/// shared Python rule compiler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupRule {
    pub name: String,
    #[serde(default)]
    pub students: Vec<String>,
    #[serde(default)]
    pub separate: bool,
    #[serde(default)]
    pub together: bool,
}

/// `RuleSet` (`models/rules.py`) — the cost/objective-relevant fields.
///
/// `groups` is serialization-omitted when empty so goal-rule documents that
/// carry only `seed` + `soft` still round-trip unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSet {
    #[serde(default = "default_rule_seed")]
    pub seed: u64,
    #[serde(default)]
    pub soft: SoftRules,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupRule>,
}

fn default_rule_seed() -> u64 {
    42
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            seed: 42,
            soft: SoftRules::default(),
            groups: Vec::new(),
        }
    }
}

/// `effective_neighbor_rule` from `models/rules.py`.
///
/// Cooling is compiled into the existing recent-neighbor objective: when both
/// are enabled the effective rule uses the union of relation types, the longer
/// history window, the stricter repeat threshold, and the sum of weights.
pub fn effective_neighbor_rule(rules: &RuleSet) -> AvoidRecentNeighborsRule {
    let base = &rules.soft.avoid_recent_neighbors;
    let cooling = &rules.soft.cooling;
    if !cooling.enabled || cooling.weight == 0 {
        return base.clone();
    }

    let cooling_rule = AvoidRecentNeighborsRule {
        enabled: true,
        weight: cooling.weight,
        relation_types: cooling.relation_types.clone(),
        lookback: Some(cooling.cooling_period),
        max_recent_count: 0,
        within_distance: cooling.within_distance,
    };
    if !base.enabled || base.weight == 0 {
        return cooling_rule;
    }

    let mut relation_types = base.relation_types.clone();
    for relation in &cooling_rule.relation_types {
        if !relation_types.contains(relation) {
            relation_types.push(relation.clone());
        }
    }
    let base_lookback = base.lookback.unwrap_or(i32::MAX);
    let cooling_lookback = cooling_rule.lookback.unwrap_or(0);
    AvoidRecentNeighborsRule {
        enabled: true,
        // Saturating: both weights are user-supplied i32 values and the sum
        // must never overflow into a negative (huge-discount) weight.
        weight: base.weight.saturating_add(cooling_rule.weight),
        relation_types,
        lookback: Some(base_lookback.max(cooling_lookback)),
        max_recent_count: base.max_recent_count.min(cooling_rule.max_recent_count),
        within_distance: base.within_distance.max(cooling_rule.within_distance),
    }
}

// ---------------------------------------------------------------------------
// History (models/history.py)
// ---------------------------------------------------------------------------

/// One assignment record in a student's seat history
/// (`models/history.py` `SeatHistoryRecord`) — only the classified categories
/// matter for the cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatHistoryRecord {
    #[serde(default)]
    pub categories: Vec<String>,
}

/// `StudentSeatHistory` (`models/history.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentSeatHistory {
    #[serde(default)]
    pub category_counts: HashMap<String, i32>,
    #[serde(default)]
    pub records: Vec<SeatHistoryRecord>,
}

impl StudentSeatHistory {
    /// `recent_category_counts(lookback)`: counts of each category over the most
    /// recent `lookback` records (`None` = all). Mirrors the Python method.
    pub fn recent_category_counts(&self, lookback: Option<i32>) -> HashMap<String, i32> {
        let mut counts: HashMap<String, i32> = HashMap::new();
        let records: &[SeatHistoryRecord] = match lookback {
            Some(lb) if lb > 0 => {
                let start = self.records.len().saturating_sub(lb as usize);
                &self.records[start..]
            }
            Some(_) => return counts,
            None => &self.records[..],
        };
        for record in records {
            for category in &record.categories {
                *counts.entry(category.clone()).or_insert(0) += 1;
            }
        }
        counts
    }
}

/// `SeatHistory` (`models/history.py`) — the subset `fair_rotation_cost` reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatHistory {
    #[serde(default)]
    pub history_count: i32,
    #[serde(default)]
    pub students: HashMap<String, StudentSeatHistory>,
}

impl SeatHistory {
    pub fn new_empty() -> Self {
        Self {
            history_count: 0,
            students: HashMap::new(),
        }
    }
}

/// One occurrence record in a pair's history
/// (`models/history.py` `PairHistoryRecord`) — only the relations matter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairHistoryRecord {
    #[serde(default)]
    pub relations: Vec<String>,
}

impl PairHistoryRecord {
    pub fn new(relations: Vec<&str>) -> Self {
        Self {
            relations: relations.into_iter().map(str::to_string).collect(),
        }
    }
}

/// `StudentPairHistory` (`models/history.py`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentPairHistory {
    #[serde(default)]
    pub records: Vec<PairHistoryRecord>,
}

impl StudentPairHistory {
    pub fn new(records: Vec<PairHistoryRecord>) -> Self {
        Self { records }
    }

    /// `recent_occurrence_count(relation_types, lookback)`: number of the most
    /// recent `lookback` records whose relation set intersects `relation_types`
    /// (`None` = all). Mirrors the Python method.
    pub fn recent_occurrence_count(
        &self,
        relation_types: &HashSet<String>,
        lookback: Option<i32>,
    ) -> i32 {
        if relation_types.is_empty() {
            return 0;
        }
        let records: &[PairHistoryRecord] = match lookback {
            Some(lb) if lb > 0 => {
                let start = self.records.len().saturating_sub(lb as usize);
                &self.records[start..]
            }
            Some(_) => return 0,
            None => &self.records[..],
        };
        records
            .iter()
            .filter(|record| {
                record
                    .relations
                    .iter()
                    .any(|relation| relation_types.contains(relation))
            })
            .count() as i32
    }
}

/// `PairHistory` (`models/history.py`) — the subset the recent-neighbor cost
/// and the mentor-pairing objective read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairHistory {
    #[serde(default)]
    pub history_count: i32,
    #[serde(default)]
    pub within_distance_metric: String,
    #[serde(default)]
    pub within_distance: i32,
    #[serde(default)]
    pub pairs: HashMap<String, StudentPairHistory>,
}

impl PairHistory {
    pub fn new_empty() -> Self {
        Self {
            history_count: 0,
            within_distance_metric: "chebyshev".to_string(),
            within_distance: 2,
            pairs: HashMap::new(),
        }
    }
}
