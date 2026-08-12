//! Export defaults memory (PD-D9-EXPORTPANEL, M5-A5).
//!
//! "Last used" export parameters are remembered globally (user config
//! directory) so the quick-export menu can export with the previous
//! settings; per-class overrides land when the project workflow provides a
//! project context (tracked in the M5 plan). Writes are atomic (temp +
//! rename); a malformed memory file is ignored, never fatal.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The remembered export parameters (subset of `ExportRequest`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExportDefaults {
    pub template: String,
    pub hide_scores: bool,
    pub hide_notes: bool,
    pub hide_special_needs: bool,
    pub anonymize: bool,
    pub show_height: bool,
    pub show_vision: bool,
    pub orientation: Option<String>,
    pub paper_size: String,
    pub page_scale: f64,
    pub margin_mm: f64,
    pub locale: String,
    pub show_student_ids: bool,
}

impl Default for ExportDefaults {
    fn default() -> Self {
        // Matches the export-defaults draft: teacher template, sensitive
        // fields hidden, height/vision shown, portrait default except
        // print-html (format-level default handles that).
        ExportDefaults {
            template: "teacher".to_string(),
            hide_scores: true,
            hide_notes: true,
            hide_special_needs: true,
            anonymize: false,
            show_height: true,
            show_vision: true,
            orientation: None,
            paper_size: "a4".to_string(),
            page_scale: 1.0,
            margin_mm: 12.0,
            locale: "zh".to_string(),
            show_student_ids: false,
        }
    }
}

impl ExportDefaults {
    /// The user-level memory file.
    pub fn global_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("seattrellis").join("export-defaults.json")
    }

    /// Read the memory file; `None` when absent or malformed (never fatal).
    pub fn load_from(path: &Path) -> Option<ExportDefaults> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Read the global memory file.
    pub fn load_global() -> Option<ExportDefaults> {
        Self::load_from(&Self::global_path())
    }

    /// Atomically write the memory file (temp + rename in the same dir).
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "could not create export-defaults directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("could not serialize export defaults: {error}"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes).map_err(|error| {
            format!(
                "could not write export defaults to {}: {error}",
                tmp.display()
            )
        })?;
        std::fs::rename(&tmp, path).map_err(|error| {
            format!(
                "could not move export defaults into place at {}: {error}",
                path.display()
            )
        })?;
        Ok(())
    }

    /// Atomically write the global memory file.
    pub fn save_global(&self) -> Result<(), String> {
        self.save_to(&Self::global_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_and_atomic_write() {
        let dir = std::env::temp_dir().join(format!("seattrellis-ed-{}", std::process::id()));
        let path = dir.join("export-defaults.json");
        let defaults = ExportDefaults {
            template: "teacher".to_string(),
            anonymize: true,
            orientation: Some("landscape".to_string()),
            paper_size: "a3".to_string(),
            ..ExportDefaults::default()
        };
        defaults.save_to(&path).expect("save");
        let loaded = ExportDefaults::load_from(&path).expect("load");
        assert_eq!(loaded, defaults);
        assert!(
            !dir.join("export-defaults.json.tmp").exists(),
            "no temp leftover"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_memory_file_is_ignored_not_fatal() {
        let dir = std::env::temp_dir().join(format!("seattrellis-ed-bad-{}", std::process::id()));
        let path = dir.join("export-defaults.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "not json{{").unwrap();
        assert!(ExportDefaults::load_from(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn defaults_serialize_with_all_fields() {
        let json = serde_json::to_value(ExportDefaults::default()).unwrap();
        for key in [
            "template",
            "hide_scores",
            "hide_notes",
            "hide_special_needs",
            "anonymize",
            "show_height",
            "show_vision",
            "orientation",
            "paper_size",
            "page_scale",
            "margin_mm",
            "locale",
            "show_student_ids",
        ] {
            assert!(json.get(key).is_some(), "missing field {key}");
        }
    }
}
