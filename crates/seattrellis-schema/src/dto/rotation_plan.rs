//! Durable rotation-plan payload (`ArtifactKind::RotationPlan`).
//!
//! The payload preserves the frozen v1 `schema_version = "1.0"` document
//! inside the v2 artifact envelope.  Period snapshots accept either the full
//! historical snapshot contract or the compact, independently validated
//! snapshot emitted by the Rust rotation use case.  Both variants are typed
//! and closed; there is no opaque JSON field in this module.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::snapshot::{SeatAssignment, SeatingSnapshotArtifact};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotationPlanArtifact {
    pub schema_version: String,
    pub kind: RotationPlanKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub name: String,
    pub periods: Vec<RotationPeriod>,
    #[serde(default)]
    pub base_history_count: u64,
    pub fairness_summary: RotationFairnessSummary,
    pub pair_repeat_summary: PairRepeatSummary,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub metadata: RotationPlanMetadata,
}

impl RotationPlanArtifact {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != "1.0" {
            return Err(format!(
                "rotation payload schema_version must be 1.0, got {}",
                self.schema_version
            ));
        }
        if self.name.trim().is_empty() {
            return Err("rotation plan name cannot be empty".to_string());
        }
        if self.periods.is_empty() {
            return Err("rotation plan must contain at least one period".to_string());
        }
        let mut periods = std::collections::HashSet::new();
        for period in &self.periods {
            if period.period == 0 {
                return Err("rotation period numbers start at 1".to_string());
            }
            if period.label.trim().is_empty() {
                return Err(format!(
                    "rotation period {} has an empty label",
                    period.period
                ));
            }
            if !periods.insert(period.period) {
                return Err(format!(
                    "rotation period numbers must be unique: {}",
                    period.period
                ));
            }
        }
        if self.metadata.period_count != self.periods.len() as u32 {
            return Err(format!(
                "rotation metadata period_count ({}) does not match periods ({})",
                self.metadata.period_count,
                self.periods.len()
            ));
        }
        Ok(())
    }
}

/// A unit enum freezes the legacy payload discriminator to `rotation_plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RotationPlanKind {
    #[serde(rename = "rotation_plan")]
    RotationPlan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotationPeriod {
    pub period: u32,
    pub label: String,
    pub snapshot: RotationSnapshot,
}

/// Full oracle snapshots and compact Rust rotation snapshots are both formal
/// wire variants.  The untagged representation matches the existing JSON.
/// `Full` is boxed (serde-transparent) to keep the variant size small.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RotationSnapshot {
    Full(Box<SeatingSnapshotArtifact>),
    Compact(CompactRotationSnapshot),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompactRotationSnapshot {
    pub assignments: Vec<SeatAssignment>,
    pub solver_status: String,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub metadata: CompactRotationSnapshotMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CompactRotationSnapshotMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_period: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_label: Option<String>,
}

fn default_seed() -> u64 {
    42
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotationFairnessSummary {
    pub history_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub student_count: Option<u64>,
    #[serde(default)]
    pub category_totals: BTreeMap<String, u64>,
    #[serde(default)]
    pub summary: RotationFairnessStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RotationFairnessStats {
    #[serde(default)]
    pub category_spread: BTreeMap<String, CategorySpread>,
    #[serde(default)]
    pub warning_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategorySpread {
    pub min: u64,
    pub max: u64,
    pub spread: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PairRepeatSummary {
    pub history_count: u64,
    pub pair_count: u64,
    pub repeated_pair_count: u64,
    pub max_occurrences: u64,
    #[serde(default)]
    pub relation_totals: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeated_pairs: Vec<RepeatedPairSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepeatedPairSummary {
    pub pair_key: String,
    pub first_student_key: String,
    pub second_student_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_student_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_student_name: Option<String>,
    pub total_occurrences: u64,
    #[serde(default)]
    pub relation_counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub records: Vec<PairOccurrenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PairOccurrenceRecord {
    pub snapshot_index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub first_seat_id: String,
    pub second_seat_id: String,
    #[serde(default)]
    pub relations: Vec<String>,
    #[serde(default)]
    pub row_delta: u64,
    #[serde(default)]
    pub col_delta: u64,
    #[serde(default)]
    pub chebyshev_distance: u64,
    #[serde(default)]
    pub manhattan_distance: u64,
    #[serde(default)]
    pub first_seat_disabled: bool,
    #[serde(default)]
    pub second_seat_disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotationPlanMetadata {
    pub period_count: u32,
    pub backend: String,
    pub seed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::{ArtifactEnvelope, ArtifactKind};

    use super::*;

    fn sample() -> RotationPlanArtifact {
        RotationPlanArtifact {
            schema_version: "1.0".into(),
            kind: RotationPlanKind::RotationPlan,
            created_at: None,
            name: "三期轮换".into(),
            periods: vec![RotationPeriod {
                period: 1,
                label: "第一周".into(),
                snapshot: RotationSnapshot::Compact(CompactRotationSnapshot {
                    assignments: vec![SeatAssignment {
                        student_key: "S1".into(),
                        student_name: "林晓雨".into(),
                        seat_id: "R1C1".into(),
                    }],
                    solver_status: "Solved".into(),
                    seed: 42,
                    metadata: CompactRotationSnapshotMetadata {
                        rotation_period: Some(1),
                        rotation_label: Some("第一周".into()),
                    },
                }),
            }],
            base_history_count: 0,
            fairness_summary: RotationFairnessSummary {
                history_count: 1,
                student_count: Some(1),
                category_totals: BTreeMap::from([("front".into(), 1)]),
                summary: RotationFairnessStats {
                    category_spread: BTreeMap::from([(
                        "front".into(),
                        CategorySpread {
                            min: 1,
                            max: 1,
                            spread: 0,
                        },
                    )]),
                    warning_count: 0,
                },
            },
            pair_repeat_summary: PairRepeatSummary {
                history_count: 1,
                pair_count: 0,
                repeated_pair_count: 0,
                max_occurrences: 0,
                relation_totals: BTreeMap::new(),
                repeated_pairs: Vec::new(),
            },
            warnings: Vec::new(),
            metadata: RotationPlanMetadata {
                period_count: 1,
                backend: "native".into(),
                seed: 42,
                saved_at: None,
                saved_from: None,
                restored_at: None,
                restored_from: None,
            },
        }
    }

    #[test]
    fn rotation_plan_envelope_round_trips() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::RotationPlan, sample());
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: ArtifactEnvelope<RotationPlanArtifact> =
            serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert!(decoded.data.validate().is_ok());
    }

    #[test]
    fn rotation_plan_accepts_the_current_compact_rust_snapshot_shape() {
        let document = r#"{
            "schema_version":"1.0",
            "kind":"rotation_plan",
            "name":"Current Rust shape",
            "periods":[{"period":1,"label":"Period 1","snapshot":{
                "assignments":[],"solver_status":"Solved","seed":42,
                "metadata":{"rotation_period":1,"rotation_label":"Period 1"}
            }}],
            "base_history_count":0,
            "fairness_summary":{"history_count":1,"student_count":0,"category_totals":{},"summary":{"warning_count":0}},
            "pair_repeat_summary":{"history_count":1,"pair_count":0,"repeated_pair_count":0,"max_occurrences":0,"relation_totals":{}},
            "warnings":[],
            "metadata":{"period_count":1,"backend":"native","seed":42}
        }"#;
        let plan: RotationPlanArtifact = serde_json::from_str(document).unwrap();
        assert!(plan.validate().is_ok());
        assert!(matches!(
            plan.periods[0].snapshot,
            RotationSnapshot::Compact(_)
        ));
    }

    #[test]
    fn rotation_plan_rejects_unknown_nested_fields() {
        let mut value =
            serde_json::to_value(ArtifactEnvelope::new(ArtifactKind::RotationPlan, sample()))
                .unwrap();
        value["data"]["periods"][0]["snapshot"]["metadata"]["mystery"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ArtifactEnvelope<RotationPlanArtifact>>(value).is_err());
    }

    #[test]
    fn rotation_plan_validates_period_count() {
        let mut plan = sample();
        plan.metadata.period_count = 2;
        assert!(plan.validate().unwrap_err().contains("does not match"));
    }
}
