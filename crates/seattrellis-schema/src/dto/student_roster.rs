//! `StudentRoster` v2 payload: the ordered student list of a class.
//!
//! Mirrors the v1 student record (schemas/student.schema.json) plus the
//! roster wrapper. Only one stable identifier is required: `student_id` or
//! `name`; project-specific columns live in `attributes`.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StudentRoster {
    pub students: Vec<RosterStudent>,
}

/// A single student record (v1 fields, v2 typing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RosterStudent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub student_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_cm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// String rendering of `str | float | None` (e.g. "0.8" or "poor").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    /// Project-specific columns; namespaced by convention.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_chinese_names_and_missing_fields() {
        let roster = StudentRoster {
            students: vec![RosterStudent {
                student_id: Some("STU001".into()),
                name: Some("学生一".into()),
                gender: None,
                height_cm: None,
                score: Some(92.5),
                vision: Some("poor".into()),
                notes: None,
                tags: vec![],
                needs: vec!["vision_front".into()],
                attributes: HashMap::new(),
            }],
        };
        let json = serde_json::to_string(&roster).unwrap();
        let parsed: StudentRoster = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, roster);
    }

    #[test]
    fn unknown_student_fields_are_rejected() {
        let json = r#"{"students":[{"student_id":"S1","mystery":true}]}"#;
        assert!(serde_json::from_str::<StudentRoster>(json).is_err());
    }
}
