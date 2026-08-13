//! Durable history-archive payload (`ArtifactKind::HistoryArchive`).
//!
//! The archive keeps the ordered, full seating snapshots from which seat and
//! pair history can be rebuilt.  The v2 envelope owns the artifact version;
//! every nested snapshot retains its frozen snapshot payload version.  The
//! shape is deliberately closed: product-specific additions belong in the
//! envelope's namespaced `extensions` object, not in an untyped map here.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::snapshot::SeatingSnapshotArtifact;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryArchiveArtifact {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default)]
    pub snapshots: Vec<ArchivedSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// One ordered snapshot entry.  `snapshot_id` is stable inside the archive;
/// `captured_at` is an RFC 3339 timestamp when the source supplied one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArchivedSnapshot {
    pub snapshot_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    pub snapshot: SeatingSnapshotArtifact,
}

impl HistoryArchiveArtifact {
    /// Enforce invariants that JSON Schema cannot express portably without a
    /// custom keyword: stable ids are non-empty and unique.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for entry in &self.snapshots {
            if entry.snapshot_id.trim().is_empty() {
                return Err("history archive snapshot_id cannot be empty".to_string());
            }
            if !ids.insert(entry.snapshot_id.as_str()) {
                return Err(format!(
                    "history archive snapshot_id must be unique: {}",
                    entry.snapshot_id
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{ArtifactEnvelope, ArtifactKind};

    use super::*;

    fn sample() -> HistoryArchiveArtifact {
        let snapshot = serde_json::from_str(
            r#"{
                "schema_version":"1.0",
                "created_at":"2026-08-13T00:00:00Z",
                "seed":42,
                "students":[{"student_id":"S1","name":"林晓雨"}],
                "layout":{"layout_id":"room","seats":[{"seat_id":"R1C1","row":1,"col":1}]},
                "rules":{"schema_version":1,"seed":42},
                "assignments":[{"student_key":"S1","student_name":"林晓雨","seat_id":"R1C1"}],
                "solver_status":"Solved"
            }"#,
        )
        .expect("sample snapshot parses");
        HistoryArchiveArtifact {
            name: "2026 秋季一班".into(),
            created_at: Some("2026-08-13T00:00:00Z".into()),
            snapshots: vec![ArchivedSnapshot {
                snapshot_id: "period-1".into(),
                captured_at: Some("2026-08-13T00:00:00Z".into()),
                snapshot,
            }],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn history_archive_envelope_round_trips() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::HistoryArchive, sample());
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: ArtifactEnvelope<HistoryArchiveArtifact> =
            serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert!(decoded.data.validate().is_ok());
    }

    #[test]
    fn history_archive_rejects_unknown_fields() {
        let mut value = serde_json::to_value(ArtifactEnvelope::new(
            ArtifactKind::HistoryArchive,
            sample(),
        ))
        .unwrap();
        value["data"]["snapshots"][0]["mystery"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ArtifactEnvelope<HistoryArchiveArtifact>>(value).is_err());
    }

    #[test]
    fn history_archive_rejects_duplicate_snapshot_ids() {
        let mut archive = sample();
        archive.snapshots.push(archive.snapshots[0].clone());
        assert!(archive.validate().unwrap_err().contains("must be unique"));
    }
}
