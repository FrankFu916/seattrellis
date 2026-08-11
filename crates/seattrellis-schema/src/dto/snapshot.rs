//! SeatingSnapshot artifact DTO (M2-01 follow-up): the solved-plan snapshot
//! document, mirroring the Python `SeatingSnapshot` model (models/snapshot.py)
//! and composing the roster/layout/ruleset DTOs. Strict parsing.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::classroom_layout::ClassroomLayout;
use super::rule_set::RuleSetArtifact;
use super::student_roster::RosterStudent;

/// A solved-plan snapshot (`kind = "snapshot"`): the students, layout, rules
/// and assignment of one solver run, plus reproducibility metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatingSnapshotArtifact {
    pub schema_version: String,
    /// RFC 3339 UTC timestamp (absent in normalized oracle goldens).
    #[serde(default)]
    pub created_at: String,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
    pub students: Vec<RosterStudent>,
    pub layout: ClassroomLayout,
    pub rules: RuleSetArtifact,
    pub assignments: Vec<SeatAssignment>,
    pub solver_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective_value: Option<f64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metrics: HashMap<String, serde_json::Value>,
}

fn default_seed() -> u64 {
    42
}

/// One student->seat assignment inside a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatAssignment {
    pub student_key: String,
    pub student_name: String,
    pub seat_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrips() {
        let document = r#"{
            "schema_version": "0.2.2",
            "created_at": "2026-08-09T00:00:00Z",
            "seed": 42,
            "metadata": {"candidate_id": "candidate_01"},
            "students": [{"student_id": "S1", "name": "Alice"}],
            "layout": {"layout_id": "benchmark-5x8", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "zone": "front"}
            ]},
            "rules": {"schema_version": 1, "seed": 42},
            "assignments": [{"student_key": "S1", "student_name": "Alice", "seat_id": "R1C1"}],
            "solver_status": "FEASIBLE",
            "objective_value": 123.5,
            "metrics": {"solver": "fallback"}
        }"#;
        let parsed: SeatingSnapshotArtifact = serde_json::from_str(document).unwrap();
        assert_eq!(parsed.assignments[0].student_key, "S1");
        assert_eq!(parsed.objective_value, Some(123.5));
        assert_eq!(parsed.metrics["solver"], "fallback");

        let encoded = serde_json::to_string(&parsed).unwrap();
        let roundtrip: SeatingSnapshotArtifact = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip, parsed);
    }

    #[test]
    fn snapshot_rejects_unknown_fields() {
        let document = r#"{
            "schema_version": "0.2.2",
            "created_at": "2026-08-09T00:00:00Z",
            "students": [],
            "layout": {"layout_id": "x", "seats": []},
            "rules": {"schema_version": 1, "seed": 42},
            "assignments": [],
            "solver_status": "FEASIBLE",
            "mystery": true
        }"#;
        let error = serde_json::from_str::<SeatingSnapshotArtifact>(document).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
    }
}
