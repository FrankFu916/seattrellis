//! Unified privacy policy (M2-06).
//!
//! Sensitive fields are defined by ONE Rust policy — renderers never decide
//! what counts as sensitive. The classification is a three-state verdict:
//!
//! - `Safe`: the document was fully scanned and no sensitive field was found.
//! - `Unsafe`: the document contains sensitive fields (for public exports).
//! - `Indeterminate`: the scan did not complete (unparseable, oversized,
//!   unknown binary, or simply not scanned). Anything not fully scanned can
//!   never be `Safe` (plan §八 privacy).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The three-state privacy verdict (plan: Safe/Unsafe/Indeterminate only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum PrivacyVerdict {
    Safe,
    Unsafe,
    Indeterminate,
}

/// Privacy decisions must fail closed when a verdict is absent (for example,
/// while reading an older manifest that predates the three-state policy).
impl Default for PrivacyVerdict {
    fn default() -> Self {
        Self::Indeterminate
    }
}

impl PrivacyVerdict {
    /// Public sharing is allowed only after a complete scan proves the input
    /// safe. `Unsafe` and `Indeterminate` are both deliberately false.
    pub const fn is_safe_for_public_sharing(self) -> bool {
        matches!(self, Self::Safe)
    }
}

/// One sensitive field found during a scan, with its JSON path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrivacyFinding {
    /// JSON pointer path, e.g. `/students/0/score`.
    pub path: String,
    /// The matched key.
    pub key: String,
}

/// Field names considered sensitive. This starts with the Python oracle's
/// `_SENSITIVE_KEYS` (src/seattrellis/project_bundle.py:27), adds the actual v2
/// roster spellings (`height_cm`, `needs`), and keeps the `*_name` suffix rule.
/// Educational privacy: grades, notes, special needs, height, vision;
/// identity: ids, names, email, phone.
const SENSITIVE_KEYS: &[&str] = &[
    "student_id",
    "student_key",
    "student_name",
    "score",
    "grade",
    "notes",
    "note",
    "special_needs",
    "special_need",
    "height",
    "height_cm",
    "vision",
    "needs",
    "email",
    "phone",
    "name",
];

/// Whether a key is sensitive. Any key ending in `_name` is sensitive
/// (mirrors `normalized.endswith("_name")` in the Python oracle).
pub fn is_sensitive_key(key: &str) -> bool {
    // CSV headers commonly contain surrounding whitespace or use spaces and
    // hyphens where the durable JSON field uses underscores. Keep this
    // normalization in the central policy so every scanner makes the same
    // decision.
    let normalized = key
        .trim()
        .trim_start_matches('\u{feff}')
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    SENSITIVE_KEYS.contains(&normalized.as_str()) || normalized.ends_with("_name")
}

/// Combine complete per-input verdicts into one fail-closed verdict.
///
/// A single incomplete input makes the aggregate `Indeterminate`, even when
/// another input was already known to be unsafe: the aggregate scan did not
/// cover the whole input set. An empty input set is likewise not proof of
/// safety.
pub fn aggregate_verdicts(verdicts: impl IntoIterator<Item = PrivacyVerdict>) -> PrivacyVerdict {
    let mut saw_input = false;
    let mut saw_unsafe = false;
    for verdict in verdicts {
        saw_input = true;
        match verdict {
            PrivacyVerdict::Indeterminate => return PrivacyVerdict::Indeterminate,
            PrivacyVerdict::Unsafe => saw_unsafe = true,
            PrivacyVerdict::Safe => {}
        }
    }
    if !saw_input {
        PrivacyVerdict::Indeterminate
    } else if saw_unsafe {
        PrivacyVerdict::Unsafe
    } else {
        PrivacyVerdict::Safe
    }
}

/// Scan a JSON document for sensitive fields. Completes on any parseable
/// document; callers decide whether the document was scannable at all
/// (oversized/unknown-binary inputs must be reported as not scanned).
pub fn scan_document(document: &Value) -> Vec<PrivacyFinding> {
    let mut findings = Vec::new();
    walk(document, "/", &mut findings);
    findings
}

fn walk(value: &Value, path: &str, findings: &mut Vec<PrivacyFinding>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path == "/" {
                    format!("/{key}")
                } else {
                    format!("{path}/{key}")
                };
                if is_sensitive_key(key) {
                    findings.push(PrivacyFinding {
                        path: child_path.clone(),
                        key: key.clone(),
                    });
                }
                walk(child, &child_path, findings);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let child_path = format!("{path}/{index}");
                walk(child, &child_path, findings);
            }
        }
        _ => {}
    }
}

/// Classify a scan result. The verdict is `Indeterminate` unless the scan
/// actually completed over the whole document.
pub fn classify_scan(completed: bool, findings: &[PrivacyFinding]) -> PrivacyVerdict {
    classify_findings(completed, !findings.is_empty())
}

/// Classify any scanner's result using the shared fail-closed policy. This is
/// used by non-JSON scanners (for example CSV headers) that do not naturally
/// produce JSON-pointer [`PrivacyFinding`] values.
pub const fn classify_findings(completed: bool, has_sensitive_findings: bool) -> PrivacyVerdict {
    if !completed {
        return PrivacyVerdict::Indeterminate;
    }
    if has_sensitive_findings {
        PrivacyVerdict::Unsafe
    } else {
        PrivacyVerdict::Safe
    }
}

/// Convenience: scan a parseable document and classify it. Documents that
/// could not be scanned (unparseable, oversized, unknown binary) must be
/// reported as `Indeterminate` via [`classify_scan`] with `completed=false`.
pub fn classify_document(document: &Value) -> PrivacyVerdict {
    classify_scan(true, &scan_document(document))
}

/// A document that was never scanned can never be Safe (plan §八).
pub fn classify_unscanned() -> PrivacyVerdict {
    PrivacyVerdict::Indeterminate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_key_list_covers_the_oracle_and_v2_roster() {
        for key in [
            "student_id",
            "student_key",
            "student_name",
            "score",
            "grade",
            "notes",
            "note",
            "special_needs",
            "special_need",
            "height",
            "height_cm",
            "vision",
            "needs",
            "email",
            "phone",
            "name",
        ] {
            assert!(is_sensitive_key(key), "{key} should be sensitive");
        }
        for key in ["row", "col", "seat_id", "layout_id", "zone", "enabled"] {
            assert!(!is_sensitive_key(key), "{key} should not be sensitive");
        }
    }

    #[test]
    fn name_suffix_rule_is_sensitive() {
        assert!(is_sensitive_key("teacher_name"));
        assert!(is_sensitive_key("student_name"));
        // *_name suffix matches anything, mirroring the Python oracle rule.
        assert!(is_sensitive_key("classroom_name"));
        assert!(!is_sensitive_key("room_id"));
    }

    #[test]
    fn key_normalization_is_shared_by_json_and_tabular_scanners() {
        assert!(is_sensitive_key(" Height-Cm "));
        assert!(is_sensitive_key("special needs"));
        assert!(is_sensitive_key("NEEDS"));
        assert!(is_sensitive_key("\u{feff}student_id"));
    }

    #[test]
    fn scan_finds_nested_sensitive_fields_with_paths() {
        let document = serde_json::json!({
            "students": [
                { "student_id": "STU001", "name": "Alice", "score": 92.5,
                  "height_cm": 168, "needs": ["front"], "notes": "quiet",
                  "attributes": { "vision": "0.8" } },
                { "student_id": "STU002", "tags": ["leader"] }
            ],
            "layout": { "seats": [ { "seat_id": "R1C1", "row": 1 } ] }
        });
        let findings = scan_document(&document);
        let keys: Vec<&str> = findings.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"student_id"));
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"score"));
        assert!(keys.contains(&"height_cm"));
        assert!(keys.contains(&"needs"));
        assert!(keys.contains(&"notes"));
        assert!(keys.contains(&"vision"));
        assert!(!keys.contains(&"row"));
        assert!(!keys.contains(&"seat_id"));
        assert!(findings
            .iter()
            .any(|f| f.path == "/students/0/attributes/vision"));
    }

    #[test]
    fn classification_is_three_state() {
        let clean = serde_json::json!({ "layout_id": "r1", "seats": [] });
        let dirty = serde_json::json!({ "students": [{ "score": 90 }] });
        assert_eq!(classify_document(&clean), PrivacyVerdict::Safe);
        assert_eq!(classify_document(&dirty), PrivacyVerdict::Unsafe);
        assert_eq!(classify_unscanned(), PrivacyVerdict::Indeterminate);
        assert_eq!(
            classify_scan(false, &scan_document(&clean)),
            PrivacyVerdict::Indeterminate,
            "an incomplete scan can never be Safe"
        );
    }

    #[test]
    fn verdict_wire_spelling_is_frozen() {
        assert_eq!(
            serde_json::to_string(&PrivacyVerdict::Indeterminate).unwrap(),
            "\"Indeterminate\""
        );
        assert_eq!(
            serde_json::to_string(&PrivacyVerdict::Safe).unwrap(),
            "\"Safe\""
        );
    }

    #[test]
    fn aggregate_verdict_is_fail_closed() {
        assert_eq!(aggregate_verdicts([]), PrivacyVerdict::Indeterminate);
        assert_eq!(
            aggregate_verdicts([PrivacyVerdict::Safe, PrivacyVerdict::Safe]),
            PrivacyVerdict::Safe
        );
        assert_eq!(
            aggregate_verdicts([PrivacyVerdict::Safe, PrivacyVerdict::Unsafe]),
            PrivacyVerdict::Unsafe
        );
        assert_eq!(
            aggregate_verdicts([PrivacyVerdict::Unsafe, PrivacyVerdict::Indeterminate]),
            PrivacyVerdict::Indeterminate
        );
        assert_eq!(PrivacyVerdict::default(), PrivacyVerdict::Indeterminate);
        assert!(PrivacyVerdict::Safe.is_safe_for_public_sharing());
        assert!(!PrivacyVerdict::Unsafe.is_safe_for_public_sharing());
        assert!(!PrivacyVerdict::Indeterminate.is_safe_for_public_sharing());
    }
}
