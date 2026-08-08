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

/// One sensitive field found during a scan, with its JSON path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrivacyFinding {
    /// JSON pointer path, e.g. `/students/0/score`.
    pub path: String,
    /// The matched key.
    pub key: String,
}

/// Field names considered sensitive, mirrored from the Python oracle's
/// `_SENSITIVE_KEYS` (src/seattrellis/project_bundle.py:27) plus the
/// `*_name` suffix rule. Educational privacy: grades, notes, special needs,
/// height, vision; identity: ids, names, email, phone.
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
    "vision",
    "email",
    "phone",
    "name",
];

/// Whether a key is sensitive. Any key ending in `_name` is sensitive
/// (mirrors `normalized.endswith("_name")` in the Python oracle).
pub fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    SENSITIVE_KEYS.contains(&normalized.as_str()) || normalized.ends_with("_name")
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
    if !completed {
        return PrivacyVerdict::Indeterminate;
    }
    if findings.is_empty() {
        PrivacyVerdict::Safe
    } else {
        PrivacyVerdict::Unsafe
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
    fn sensitive_key_list_matches_the_oracle() {
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
            "vision",
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
    fn scan_finds_nested_sensitive_fields_with_paths() {
        let document = serde_json::json!({
            "students": [
                { "student_id": "STU001", "name": "Alice", "score": 92.5,
                  "notes": "quiet", "attributes": { "vision": "0.8" } },
                { "student_id": "STU002", "tags": ["leader"] }
            ],
            "layout": { "seats": [ { "seat_id": "R1C1", "row": 1 } ] }
        });
        let findings = scan_document(&document);
        let keys: Vec<&str> = findings.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"student_id"));
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"score"));
        assert!(keys.contains(&"notes"));
        assert!(keys.contains(&"vision"));
        assert!(!keys.contains(&"row"));
        assert!(!keys.contains(&"seat_id"));
        assert!(findings.iter().any(|f| f.path == "/students/0/attributes/vision"));
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
        assert_eq!(serde_json::to_string(&PrivacyVerdict::Safe).unwrap(), "\"Safe\"");
    }
}
