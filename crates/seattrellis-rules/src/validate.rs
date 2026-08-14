//! Rule document validation (M6-02).
//!
//! The React workbench shows a live diagnostic view of the raw custom rules
//! JSON in the advanced settings. The validation itself is Rust's job: this
//! module validates a whole RuleSet document against the rule registry's
//! parameter schemas (the same schemars-generated JSON Schemas the UI
//! renders), so the field taxonomy and range rules are never duplicated in
//! TypeScript. Diagnostic codes mirror the workbench's i18n catalog
//! (`generate.rulesDiagnostic*`) and are returned as structured
//! `{ path, code, detail? }` records.

use serde_json::Value;

use crate::rule_spec;

/// A single validation finding, keyed by the JSON path where it occurred.
///
/// `code` values are stable and map to workbench i18n keys:
/// `invalid_json` / `root_object` / `unknown_field` / `object_required` /
/// `array_required` / `pair_shape` / `fixed_seat_shape` / `distance_value` /
/// `group_shape` / `group_members` / `group_mode` / `unknown_student` /
/// `unknown_seat` / `value_type`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RuleDiagnostic {
    pub path: String,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Top-level fields of a RuleSet document (`models/rules.py` + the frontend
/// raw-JSON surface). Unknown top-level keys are rejected.
const TOP_LEVEL_FIELDS: [&str; 5] = ["schema_version", "seed", "hard", "soft", "groups"];

/// Hard-rule keys (`hard` object). Each value is an array of that rule's
/// parameter documents.
const HARD_RULE_KEYS: [&str; 4] = [
    "fixed_seats",
    "must_be_adjacent",
    "cannot_be_adjacent",
    "min_distance",
];

/// Soft-rule keys (`soft` object). Each value is a parameter object.
const SOFT_RULE_KEYS: [&str; 10] = [
    "vision_front",
    "height_back",
    "randomize",
    "score_balance",
    "score_position",
    "score_distribution",
    "mentor_pairing",
    "fair_rotation",
    "avoid_recent_neighbors",
    "cooling",
];

fn is_record(value: &Value) -> bool {
    value.is_object()
}

/// Validate a whole RuleSet JSON document. `student_ids` and `seat_ids` are
/// the roster/layout identifiers used to flag unknown references; pass empty
/// sets to skip reference checks (shape-only validation).
pub fn validate_rule_document(
    source: &str,
    student_ids: &[String],
    seat_ids: &[String],
) -> Vec<RuleDiagnostic> {
    let text = source.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let parsed: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            return vec![RuleDiagnostic {
                path: "$".to_string(),
                code: "invalid_json",
                detail: None,
            }]
        }
    };
    if !is_record(&parsed) {
        return vec![RuleDiagnostic {
            path: "$".to_string(),
            code: "root_object",
            detail: None,
        }];
    }

    let known_students: std::collections::HashSet<&str> =
        student_ids.iter().map(String::as_str).collect();
    let known_seats: std::collections::HashSet<&str> =
        seat_ids.iter().map(String::as_str).collect();

    let mut diagnostics = Vec::new();
    let object = parsed.as_object().expect("is_record checked");

    // Top-level field set + scalar typing of `schema_version` / `seed`.
    for (key, _) in object {
        if !TOP_LEVEL_FIELDS.contains(&key.as_str()) {
            diagnostics.push(RuleDiagnostic {
                path: key.clone(),
                code: "unknown_field",
                detail: None,
            });
        }
    }
    for field in ["schema_version", "seed"] {
        if let Some(value) = object.get(field) {
            if value.as_i64().is_none() {
                diagnostics.push(RuleDiagnostic {
                    path: field.to_string(),
                    code: "value_type",
                    detail: None,
                });
            }
        }
    }

    if let Some(hard) = object.get("hard") {
        validate_hard_rules(hard, &known_students, &known_seats, &mut diagnostics);
    }
    if let Some(soft) = object.get("soft") {
        validate_soft_rules(soft, &mut diagnostics);
    }
    if let Some(groups) = object.get("groups") {
        validate_groups(groups, &known_students, &mut diagnostics);
    }

    diagnostics
}

fn push_reference_errors(
    diagnostics: &mut Vec<RuleDiagnostic>,
    students: &Value,
    known_students: &std::collections::HashSet<&str>,
    base_path: &str,
) {
    if let Some(array) = students.as_array() {
        for (index, student) in array.iter().enumerate() {
            let name = student.as_str().unwrap_or("");
            if !name.is_empty() && !known_students.contains(name) {
                diagnostics.push(RuleDiagnostic {
                    path: format!("{base_path}[{index}]"),
                    code: "unknown_student",
                    detail: Some(name.to_string()),
                });
            }
        }
    }
}

fn validate_hard_rules(
    value: &Value,
    known_students: &std::collections::HashSet<&str>,
    known_seats: &std::collections::HashSet<&str>,
    diagnostics: &mut Vec<RuleDiagnostic>,
) {
    if !is_record(value) {
        diagnostics.push(RuleDiagnostic {
            path: "hard".to_string(),
            code: "object_required",
            detail: None,
        });
        return;
    }
    let object = value.as_object().expect("is_record checked");
    for (key, _) in object {
        if !HARD_RULE_KEYS.contains(&key.as_str()) {
            diagnostics.push(RuleDiagnostic {
                path: format!("hard.{key}"),
                code: "unknown_field",
                detail: None,
            });
        }
    }

    for key in HARD_RULE_KEYS {
        let Some(rules) = object.get(key) else {
            continue;
        };
        if !rules.is_array() {
            diagnostics.push(RuleDiagnostic {
                path: format!("hard.{key}"),
                code: "array_required",
                detail: None,
            });
            continue;
        }
        for (index, rule) in rules.as_array().expect("array checked").iter().enumerate() {
            validate_rule_entry(key, rule, index, known_students, known_seats, diagnostics);
        }
    }
}

fn validate_rule_entry(
    key: &str,
    rule: &Value,
    index: usize,
    known_students: &std::collections::HashSet<&str>,
    known_seats: &std::collections::HashSet<&str>,
    diagnostics: &mut Vec<RuleDiagnostic>,
) {
    let base = format!("hard.{key}[{index}]");
    if !is_record(rule) {
        diagnostics.push(RuleDiagnostic {
            path: base,
            code: "object_required",
            detail: None,
        });
        return;
    }
    let object = rule.as_object().expect("is_record checked");

    // Shape checks per hard rule kind (mirrors the frontend contract; the
    // registry schemas reinforce these at solve time, but we give the same
    // live, field-level feedback here).
    match key {
        "fixed_seats" => {
            let student = object.get("student").and_then(Value::as_str).unwrap_or("");
            let seat_id = object.get("seat_id").and_then(Value::as_str).unwrap_or("");
            if student.is_empty() || seat_id.is_empty() {
                diagnostics.push(RuleDiagnostic {
                    path: base.clone(),
                    code: "fixed_seat_shape",
                    detail: None,
                });
                return;
            }
            if !known_students.is_empty() && !known_students.contains(student) {
                diagnostics.push(RuleDiagnostic {
                    path: format!("{base}.student"),
                    code: "unknown_student",
                    detail: Some(student.to_string()),
                });
            }
            if !known_seats.is_empty() && !known_seats.contains(seat_id) {
                diagnostics.push(RuleDiagnostic {
                    path: format!("{base}.seat_id"),
                    code: "unknown_seat",
                    detail: Some(seat_id.to_string()),
                });
            }
        }
        "must_be_adjacent" | "cannot_be_adjacent" => {
            if !valid_pair(object.get("students")) {
                diagnostics.push(RuleDiagnostic {
                    path: format!("{base}.students"),
                    code: "pair_shape",
                    detail: None,
                });
            } else {
                push_reference_errors(
                    diagnostics,
                    object.get("students").expect("pair checked"),
                    known_students,
                    &format!("{base}.students"),
                );
            }
        }
        "min_distance" => {
            if !valid_pair(object.get("students")) {
                diagnostics.push(RuleDiagnostic {
                    path: format!("{base}.students"),
                    code: "pair_shape",
                    detail: None,
                });
            } else {
                push_reference_errors(
                    diagnostics,
                    object.get("students").expect("pair checked"),
                    known_students,
                    &format!("{base}.students"),
                );
            }
            if let Some(distance) = object.get("distance") {
                let ok = distance
                    .as_f64()
                    .map(|d| d.is_finite() && d > 0.0)
                    .unwrap_or(false);
                if !ok {
                    diagnostics.push(RuleDiagnostic {
                        path: format!("{base}.distance"),
                        code: "distance_value",
                        detail: None,
                    });
                }
            } else {
                diagnostics.push(RuleDiagnostic {
                    path: format!("{base}.distance"),
                    code: "distance_value",
                    detail: None,
                });
            }
            if let Some(metric) = object.get("metric") {
                if metric != "graph" && metric != "euclidean" {
                    diagnostics.push(RuleDiagnostic {
                        path: format!("{base}.metric"),
                        code: "distance_value",
                        detail: None,
                    });
                }
            }
        }
        _ => {}
    }

    // Unknown-field check using the registry spec's parameter schema (single
    // source of truth for allowed fields).
    if let Some(spec) = rule_spec(key) {
        if let Some(properties) = spec
            .param_schema
            .get("properties")
            .and_then(Value::as_object)
        {
            for (field, _) in object {
                if !properties.contains_key(field) {
                    diagnostics.push(RuleDiagnostic {
                        path: format!("{base}.{field}"),
                        code: "unknown_field",
                        detail: None,
                    });
                }
            }
        }
    }
}

fn valid_pair(value: Option<&Value>) -> bool {
    let Some(value) = value else { return false };
    let Some(array) = value.as_array() else {
        return false;
    };
    if array.len() != 2 {
        return false;
    }
    array
        .iter()
        .all(|item| item.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false))
}

fn validate_soft_rules(value: &Value, diagnostics: &mut Vec<RuleDiagnostic>) {
    if !is_record(value) {
        diagnostics.push(RuleDiagnostic {
            path: "soft".to_string(),
            code: "object_required",
            detail: None,
        });
        return;
    }
    let object = value.as_object().expect("is_record checked");
    for (key, _) in object {
        if !SOFT_RULE_KEYS.contains(&key.as_str()) {
            diagnostics.push(RuleDiagnostic {
                path: format!("soft.{key}"),
                code: "unknown_field",
                detail: None,
            });
        }
    }

    // Each soft rule value must be an object; nested unknown fields are
    // checked against the registry parameter schema, which is the single
    // source of truth for the allowed member set.
    for (key, entry) in object {
        if !entry.is_object() {
            diagnostics.push(RuleDiagnostic {
                path: format!("soft.{key}"),
                code: "object_required",
                detail: None,
            });
            continue;
        }
        if let Some(spec) = rule_spec(key) {
            if let Some(properties) = spec
                .param_schema
                .get("properties")
                .and_then(Value::as_object)
            {
                if let Some(entry_object) = entry.as_object() {
                    for (field, _) in entry_object {
                        if !properties.contains_key(field) {
                            diagnostics.push(RuleDiagnostic {
                                path: format!("soft.{key}.{field}"),
                                code: "unknown_field",
                                detail: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn validate_groups(
    value: &Value,
    known_students: &std::collections::HashSet<&str>,
    diagnostics: &mut Vec<RuleDiagnostic>,
) {
    if !value.is_array() {
        diagnostics.push(RuleDiagnostic {
            path: "groups".to_string(),
            code: "array_required",
            detail: None,
        });
        return;
    }
    let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (index, group) in value.as_array().expect("array checked").iter().enumerate() {
        let base = format!("groups[{index}]");
        if !is_record(group) {
            diagnostics.push(RuleDiagnostic {
                path: base,
                code: "group_shape",
                detail: None,
            });
            continue;
        }
        let object = group.as_object().expect("is_record checked");
        let name = object.get("name").and_then(Value::as_str).unwrap_or("");
        if name.trim().is_empty() || !names.insert(name.trim().to_string()) {
            diagnostics.push(RuleDiagnostic {
                path: format!("{base}.name"),
                code: "group_shape",
                detail: None,
            });
        }
        match object.get("students") {
            Some(students) if students.is_array() => {
                let members: Vec<&str> = students
                    .as_array()
                    .expect("array checked")
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                if members.len() < 2 {
                    diagnostics.push(RuleDiagnostic {
                        path: format!("{base}.students"),
                        code: "group_members",
                        detail: None,
                    });
                } else {
                    for (member_index, member) in members.iter().enumerate() {
                        if !known_students.is_empty() && !known_students.contains(*member) {
                            diagnostics.push(RuleDiagnostic {
                                path: format!("{base}.students[{member_index}]"),
                                code: "unknown_student",
                                detail: Some((*member).to_string()),
                            });
                        }
                    }
                }
            }
            _ => diagnostics.push(RuleDiagnostic {
                path: format!("{base}.students"),
                code: "group_members",
                detail: None,
            }),
        }
        for field in ["separate", "together"] {
            if let Some(v) = object.get(field) {
                if !v.is_boolean() {
                    diagnostics.push(RuleDiagnostic {
                        path: format!("{base}.{field}"),
                        code: "group_mode",
                        detail: None,
                    });
                }
            }
        }
        if matches!(
            (object.get("separate"), object.get("together")),
            (Some(Value::Bool(true)), Some(Value::Bool(true)))
        ) {
            diagnostics.push(RuleDiagnostic {
                path: base.clone(),
                code: "group_mode",
                detail: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<String> {
        ["S01", "S02", "S03"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn seats() -> Vec<String> {
        ["R1C1", "R1C2", "R1C3"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn empty_and_blank_are_clean() {
        assert!(validate_rule_document("", &ids(), &seats()).is_empty());
        assert!(validate_rule_document("   ", &ids(), &seats()).is_empty());
    }

    #[test]
    fn invalid_json_is_reported() {
        let diagnostics = validate_rule_document("{ not json", &ids(), &seats());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "invalid_json");
    }

    #[test]
    fn unknown_top_level_field_is_reported() {
        let diagnostics = validate_rule_document(r#"{"bogus": 1}"#, &ids(), &seats());
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "unknown_field" && d.path == "bogus"));
    }

    #[test]
    fn fixed_seat_validates_shape_and_references() {
        let source = r#"{"hard": {"fixed_seats": [{"student": "S01", "seat_id": "R9C9"}]}}"#;
        let diagnostics = validate_rule_document(source, &ids(), &seats());
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "unknown_seat" && d.path == "hard.fixed_seats[0].seat_id"));

        let ok = r#"{"hard": {"fixed_seats": [{"student": "S01", "seat_id": "R1C1"}]}}"#;
        assert!(validate_rule_document(ok, &ids(), &seats()).is_empty());
    }

    #[test]
    fn min_distance_validates_distance_and_metric() {
        let source = r#"{"hard": {"min_distance": [{"students": ["S01","S02"], "distance": 0, "metric": "bad"}]}}"#;
        let diagnostics = validate_rule_document(source, &ids(), &seats());
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "distance_value" && d.path == "hard.min_distance[0].distance"));
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "distance_value" && d.path == "hard.min_distance[0].metric"));
    }

    #[test]
    fn soft_rule_unknown_member_is_reported_against_registry() {
        let source = r#"{"soft": {"vision_front": {"enabled": true, "bogus": 1}}}"#;
        let diagnostics = validate_rule_document(source, &ids(), &seats());
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "unknown_field" && d.path == "soft.vision_front.bogus"));
    }

    #[test]
    fn groups_validate_membership_and_mode() {
        let source = r#"{"groups": [{"name": "g", "students": ["S01","S05"], "separate": true, "together": true}]}"#;
        let diagnostics = validate_rule_document(source, &ids(), &seats());
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "unknown_student" && d.path == "groups[0].students[1]"));
        assert!(diagnostics
            .iter()
            .any(|d| d.code == "group_mode" && d.path == "groups[0]"));
    }
}
