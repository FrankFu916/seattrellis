//! Durable editing-operation log payload (`ArtifactKind::EditingOperationLog`).
//!
//! The log reuses the frozen editor protocol's typed operation enum, keeping
//! every replayable mutation explicit and strictly parsed.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::editor_protocol::{EditingOperation, EditorProtocolVersion};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditingOperationLogArtifact {
    pub protocol_version: EditorProtocolVersion,
    pub draft_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String>,
    pub base_revision: u64,
    pub final_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default)]
    pub operations: Vec<EditingOperation>,
}

impl EditingOperationLogArtifact {
    pub fn validate(&self) -> Result<(), String> {
        if self.draft_id.trim().is_empty() {
            return Err("editing operation log draft_id cannot be empty".to_string());
        }
        if self.final_revision < self.base_revision {
            return Err("editing operation log final_revision precedes base_revision".to_string());
        }
        if self.operations.len() > 100 {
            return Err("editing operation log may contain at most 100 operations".to_string());
        }
        for operation in &self.operations {
            operation.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::dto::editor_protocol::{
        EditingOperation, EditorProtocolVersion, StudentSeatPayload,
    };
    use crate::{ArtifactEnvelope, ArtifactKind};

    use super::*;

    fn sample() -> EditingOperationLogArtifact {
        EditingOperationLogArtifact {
            protocol_version: EditorProtocolVersion::V1,
            draft_id: "draft-1".into(),
            candidate_id: Some("candidate-1".into()),
            base_revision: 3,
            final_revision: 4,
            created_at: Some("2026-08-13T00:00:00Z".into()),
            operations: vec![EditingOperation::MoveStudent(StudentSeatPayload {
                student_key: "S1".into(),
                seat_id: "R1C1".into(),
            })],
        }
    }

    #[test]
    fn editing_operation_log_envelope_round_trips() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::EditingOperationLog, sample());
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: ArtifactEnvelope<EditingOperationLogArtifact> =
            serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert!(decoded.data.validate().is_ok());
    }

    #[test]
    fn editing_operation_log_rejects_unknown_payload_fields() {
        let mut value = serde_json::to_value(ArtifactEnvelope::new(
            ArtifactKind::EditingOperationLog,
            sample(),
        ))
        .unwrap();
        value["data"]["operations"][0]["payload"]["force"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<ArtifactEnvelope<EditingOperationLogArtifact>>(value).is_err()
        );
    }

    #[test]
    fn editing_operation_log_rejects_revision_regression() {
        let mut log = sample();
        log.final_revision = 2;
        assert!(log.validate().unwrap_err().contains("precedes"));
    }
}
