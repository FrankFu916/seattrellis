//! v2 durable artifact contract (plan §四.3, M2-01).
//!
//! Every long-lived SeatTrellis artifact uses the envelope:
//!
//! ```json
//! { "kind": "<stable artifact identifier>",
//!   "schema_version": 2,
//!   "data": { ... typed payload ... },
//!   "extensions": { "<namespaced>": ... } }
//! ```
//!
//! Rules (frozen):
//! - `schema_version` is the *artifact* version, not the product SemVer.
//! - Unknown fields are rejected by default (strict parse); extensions are
//!   only allowed under the explicit `extensions` namespace.
//! - Readers must check `kind` and `schema_version` against the registry
//!   before touching `data`.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::ArtifactKind;

/// The v2 durable artifact envelope. `T` is the typed payload for the kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEnvelope<T> {
    /// Stable artifact identifier (see [`ArtifactKind`]).
    pub kind: ArtifactKind,
    /// Artifact schema version; 2 for all v2 artifacts.
    pub schema_version: u32,
    /// The typed payload.
    pub data: T,
    /// Optional namespaced extension fields; unknown top-level fields are
    /// rejected instead of being silently dropped.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, Value>,
}

impl<T> ArtifactEnvelope<T> {
    pub fn new(kind: ArtifactKind, data: T) -> Self {
        ArtifactEnvelope {
            kind,
            schema_version: 2,
            data,
            extensions: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::student_roster::{RosterStudent, StudentRoster};

    fn roster() -> StudentRoster {
        StudentRoster {
            students: vec![RosterStudent {
                student_id: Some("STU001".into()),
                name: Some("学生一".into()),
                gender: Some("F".into()),
                height_cm: Some(165.0),
                score: Some(88.5),
                vision: Some("0.8".into()),
                notes: None,
                tags: vec!["leader".into()],
                needs: vec!["vision_front".into()],
                attributes: HashMap::new(),
            }],
        }
    }

    #[test]
    fn envelope_round_trips() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::StudentRoster, roster());
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: ArtifactEnvelope<StudentRoster> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, envelope);
        assert_eq!(parsed.kind, ArtifactKind::StudentRoster);
        assert_eq!(parsed.schema_version, 2);
        assert!(parsed.extensions.is_empty());
    }

    #[test]
    fn unknown_top_level_fields_are_rejected() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::StudentRoster, roster());
        let mut json = serde_json::to_value(&envelope).unwrap();
        json["sneaky"] = Value::String("field".into());
        let result: Result<ArtifactEnvelope<StudentRoster>, _> = serde_json::from_value(json);
        assert!(result.is_err(), "unknown envelope fields must be rejected");
    }

    #[test]
    fn unknown_data_fields_are_rejected() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::StudentRoster, roster());
        let mut json = serde_json::to_value(&envelope).unwrap();
        json["data"]["mystery"] = Value::Bool(true);
        let result: Result<ArtifactEnvelope<StudentRoster>, _> = serde_json::from_value(json);
        assert!(result.is_err(), "unknown payload fields must be rejected");
    }

    #[test]
    fn extensions_round_trip_under_the_namespace() {
        let mut envelope = ArtifactEnvelope::new(ArtifactKind::StudentRoster, roster());
        envelope
            .extensions
            .insert("com.example.note".into(), Value::String("x".into()));
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: ArtifactEnvelope<StudentRoster> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.extensions["com.example.note"], "x");
    }
}
