//! SeatTrellisProject artifact DTO (M2-01 follow-up): the portable project
//! workspace document, mirroring the Python `SeatTrellisProject` model
//! (models/project.py) field-for-field. Strict parsing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The v2 project artifact (`kind = "seattrellis_project"`): references to
/// the sibling students/layout/rules files plus defaults for generation and
/// export. Relative paths are validated by callers against the workspace
/// root (mirrors Python's relative-path validation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeatTrellisProjectArtifact {
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_name")]
    pub name: String,
    pub students: String,
    pub layout: String,
    pub rules: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_dir: Option<String>,
    #[serde(default = "default_outputs_dir")]
    pub outputs_dir: String,
    #[serde(default = "default_candidates")]
    pub default_candidates: u32,
    #[serde(default = "default_candidate")]
    pub default_candidate: String,
    #[serde(default = "default_export_format")]
    pub default_export_format: ExportFormat,
}

fn default_kind() -> String {
    "seattrellis_project".to_string()
}

fn default_schema_version() -> u32 {
    1
}

fn default_name() -> String {
    "SeatTrellis Project".to_string()
}

fn default_outputs_dir() -> String {
    "outputs".to_string()
}

fn default_candidates() -> u32 {
    5
}

fn default_candidate() -> String {
    "recommended".to_string()
}

fn default_export_format() -> ExportFormat {
    ExportFormat::Html
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ExportFormat {
    #[serde(rename = "html")]
    Html,
    #[serde(rename = "excel")]
    Excel,
    #[serde(rename = "png")]
    Png,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_roundtrips_with_defaults() {
        let document = r#"{
            "kind": "seattrellis_project",
            "schema_version": 1,
            "name": "Demo",
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "history_dir": "history",
            "outputs_dir": "outputs",
            "default_candidates": 5,
            "default_candidate": "recommended",
            "default_export_format": "html"
        }"#;
        let parsed: SeatTrellisProjectArtifact = serde_json::from_str(document).unwrap();
        assert_eq!(parsed.name, "Demo");
        assert_eq!(parsed.history_dir.as_deref(), Some("history"));
        assert_eq!(parsed.default_export_format, ExportFormat::Html);

        let encoded = serde_json::to_string(&parsed).unwrap();
        let roundtrip: SeatTrellisProjectArtifact = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip, parsed);
    }

    #[test]
    fn project_defaults_match_python() {
        let minimal =
            r#"{"students": "students.csv", "layout": "layout.json", "rules": "rules.json"}"#;
        let parsed: SeatTrellisProjectArtifact = serde_json::from_str(minimal).unwrap();
        assert_eq!(parsed.kind, "seattrellis_project");
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.name, "SeatTrellis Project");
        assert_eq!(parsed.outputs_dir, "outputs");
        assert_eq!(parsed.default_candidates, 5);
        assert_eq!(parsed.default_candidate, "recommended");
        assert_eq!(parsed.default_export_format, ExportFormat::Html);
        assert!(parsed.history_dir.is_none());
    }

    #[test]
    fn project_rejects_unknown_fields() {
        let document = r#"{
            "students": "students.csv",
            "layout": "layout.json",
            "rules": "rules.json",
            "mystery": true
        }"#;
        let error = serde_json::from_str::<SeatTrellisProjectArtifact>(document).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
    }
}
