//! Durable export-preset payload (`ArtifactKind::ExportPreset`).
//!
//! This is the typed, reusable counterpart of the export request/defaults
//! contract.  Values that affect privacy, page layout, or output format are
//! explicit fields so presets cannot smuggle behavior through an opaque JSON
//! object.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPresetArtifact {
    pub name: String,
    pub format: ExportFormat,
    #[serde(default)]
    pub template: ExportTemplate,
    #[serde(default)]
    pub privacy: ExportPrivacyOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<ExportOrientation>,
    #[serde(default)]
    pub paper_size: PaperSize,
    #[serde(default = "default_page_scale")]
    pub page_scale: f64,
    #[serde(default = "default_margin_mm")]
    pub margin_mm: f64,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub show_student_ids: bool,
}

impl ExportPresetArtifact {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("export preset name cannot be empty".to_string());
        }
        if !self.page_scale.is_finite() || !(0.25..=4.0).contains(&self.page_scale) {
            return Err("export preset page_scale must be between 0.25 and 4.0".to_string());
        }
        if !self.margin_mm.is_finite() || !(5.0..=25.0).contains(&self.margin_mm) {
            return Err("export preset margin_mm must be between 5 and 25".to_string());
        }
        if self.locale.trim().is_empty() {
            return Err("export preset locale cannot be empty".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ExportFormat {
    Svg,
    Html,
    PrintHtml,
    Png,
    Pdf,
    Xlsx,
    Docx,
    Pptx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExportTemplate {
    Public,
    #[default]
    Teacher,
    Report,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PaperSize {
    #[default]
    A4,
    A3,
    Letter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ExportPrivacyOptions {
    #[serde(default = "default_true")]
    pub hide_scores: bool,
    #[serde(default = "default_true")]
    pub hide_notes: bool,
    #[serde(default = "default_true")]
    pub hide_special_needs: bool,
    #[serde(default)]
    pub anonymize: bool,
    #[serde(default = "default_true")]
    pub show_height: bool,
    #[serde(default = "default_true")]
    pub show_vision: bool,
}

fn default_true() -> bool {
    true
}

fn default_page_scale() -> f64 {
    1.0
}

fn default_margin_mm() -> f64 {
    12.0
}

fn default_locale() -> String {
    "zh".to_string()
}

#[cfg(test)]
mod tests {
    use crate::{ArtifactEnvelope, ArtifactKind};

    use super::*;

    fn sample() -> ExportPresetArtifact {
        ExportPresetArtifact {
            name: "教师打印".into(),
            format: ExportFormat::PrintHtml,
            template: ExportTemplate::Teacher,
            privacy: ExportPrivacyOptions {
                hide_scores: true,
                hide_notes: true,
                hide_special_needs: true,
                anonymize: false,
                show_height: true,
                show_vision: true,
            },
            orientation: Some(ExportOrientation::Landscape),
            paper_size: PaperSize::A4,
            page_scale: 1.0,
            margin_mm: 12.0,
            locale: "zh".into(),
            show_student_ids: false,
        }
    }

    #[test]
    fn export_preset_envelope_round_trips() {
        let envelope = ArtifactEnvelope::new(ArtifactKind::ExportPreset, sample());
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: ArtifactEnvelope<ExportPresetArtifact> =
            serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert!(decoded.data.validate().is_ok());
    }

    #[test]
    fn export_preset_rejects_unknown_privacy_fields() {
        let mut value =
            serde_json::to_value(ArtifactEnvelope::new(ArtifactKind::ExportPreset, sample()))
                .unwrap();
        value["data"]["privacy"]["student_names"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ArtifactEnvelope<ExportPresetArtifact>>(value).is_err());
    }

    #[test]
    fn export_preset_validates_layout_ranges() {
        let mut preset = sample();
        preset.margin_mm = 100.0;
        assert!(preset.validate().unwrap_err().contains("margin_mm"));
    }
}
