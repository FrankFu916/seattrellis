//! Sentence templates for the rule builder (M4 PD-D3-RULEBUILDER).
//!
//! The React rule builder renders each template as a natural-language
//! sentence with clickable slots; filling every required slot and compiling
//! goes through [`compile_sentence`], so the rule representation is always a
//! Rust-produced artifact (PD-D3-ADJ-1: JSON is read-only, never hand-edited
//! in the UI). Each template carries the parameter bindings that turn its
//! slots into the canonical `hard_rules` / `rules_overlay` entry the
//! generation pipeline parses (`class_generation::resolve_hard_rules`).

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::RuleCategory;

/// Bilingual copy served verbatim to the UI (same contract as catalogs).
#[derive(Debug, Clone, Serialize)]
pub struct LocalizedKeys {
    pub zh: String,
    pub en: String,
}

impl LocalizedKeys {
    fn new(zh: &str, en: &str) -> Self {
        Self {
            zh: zh.to_string(),
            en: en.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    /// One student from the roster.
    Student,
    /// Several students (comma-separated) mapped to an array parameter.
    Students,
    /// One seat id of the current layout.
    Seat,
    /// Free-form text (e.g. a group name).
    Text,
    /// A bounded number.
    Number,
    /// One of a fixed set of display options.
    Choice,
}

/// A fillable slot inside a template sentence. `param_path` is a
/// slash-separated path into the template's default entry JSON
/// (`students/0` sets the first array element); choice options may carry an
/// explicit `param_value` (e.g. the string "separate" stored as `true`).
#[derive(Debug, Clone, Serialize)]
pub struct LocalizedOption {
    pub value: String,
    pub param_value: Option<Value>,
    pub label: LocalizedKeys,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotSpec {
    pub key: String,
    pub kind: SlotKind,
    pub label: LocalizedKeys,
    pub placeholder: Option<LocalizedKeys>,
    pub param_path: Option<String>,
    pub required: bool,
    pub options: Option<Vec<LocalizedOption>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentenceTemplate {
    pub id: String,
    /// The rule id this template compiles into (registry `rule_spec` id for
    /// soft rules; the `hard_rules` container key for hard rules).
    pub rule_id: String,
    pub category: RuleCategory,
    pub label: LocalizedKeys,
    /// Natural-language sentence; slots appear as `{key}` placeholders.
    pub sentence: LocalizedKeys,
    pub slots: Vec<SlotSpec>,
    /// The entry JSON the filled slots are bound into.
    pub defaults: Value,
}

/// A successfully compiled rule, ready to insert into `hard_rules` (hard,
/// appended to the rule-id array) or `rules_overlay` (soft, keyed by id).
#[derive(Debug, Clone, Serialize)]
pub struct CompiledRule {
    pub category: RuleCategory,
    pub rule_id: String,
    pub entry: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub code: &'static str,
    pub slot: Option<String>,
    pub message: String,
}

impl CompileError {
    fn new(code: &'static str, slot: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            code,
            slot: slot.map(str::to_string),
            message: message.into(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn template(
    id: &str,
    rule_id: &str,
    category: RuleCategory,
    zh_label: &str,
    en_label: &str,
    zh_sentence: &str,
    en_sentence: &str,
    slots: Vec<SlotSpec>,
    defaults: Value,
) -> SentenceTemplate {
    SentenceTemplate {
        id: id.to_string(),
        rule_id: rule_id.to_string(),
        category,
        label: LocalizedKeys::new(zh_label, en_label),
        sentence: LocalizedKeys::new(zh_sentence, en_sentence),
        slots,
        defaults,
    }
}

fn student_slot(key: &str, path: &str, zh_label: &str, en_label: &str) -> SlotSpec {
    SlotSpec {
        key: key.to_string(),
        kind: SlotKind::Student,
        label: LocalizedKeys::new(zh_label, en_label),
        placeholder: Some(LocalizedKeys::new("选择学生…", "Choose student…")),
        param_path: Some(path.to_string()),
        required: true,
        options: None,
        min: None,
        max: None,
        step: None,
        default: None,
    }
}

/// The preset sentence templates (D3: 学生距离 / 固定座位 / 标签分组 / 区域 /
/// 成绩平衡 / 历史目标). The template list itself is the single source the
/// workbench renders; adding a template here extends the builder without
/// touching the UI.
pub fn sentence_templates() -> Vec<SentenceTemplate> {
    vec![
        template(
            "student_distance",
            "min_distance",
            RuleCategory::Hard,
            "学生距离",
            "Student distance",
            "让 {student_a} 与 {student_b} 的距离 ≥ {distance}（座）",
            "Keep {student_a} and {student_b} at least {distance} seats apart",
            vec![
                student_slot("student_a", "students/0", "学生 A", "Student A"),
                student_slot("student_b", "students/1", "学生 B", "Student B"),
                SlotSpec {
                    key: "distance".to_string(),
                    kind: SlotKind::Number,
                    label: LocalizedKeys::new("最小距离", "Minimum distance"),
                    placeholder: None,
                    param_path: Some("distance".to_string()),
                    required: true,
                    options: None,
                    min: Some(0.1),
                    max: None,
                    step: Some(0.1),
                    default: Some(json!(2.0)),
                },
            ],
            json!({ "students": ["", ""], "distance": 2.0, "metric": "graph" }),
        ),
        template(
            "fixed_seat",
            "fixed_seats",
            RuleCategory::Hard,
            "固定座位",
            "Fixed seat",
            "让 {student} 固定坐在 {seat}",
            "Keep {student} at seat {seat}",
            vec![
                student_slot("student", "student", "学生", "Student"),
                SlotSpec {
                    key: "seat".to_string(),
                    kind: SlotKind::Seat,
                    label: LocalizedKeys::new("座位", "Seat"),
                    placeholder: Some(LocalizedKeys::new("选择座位…", "Choose seat…")),
                    param_path: Some("seat_id".to_string()),
                    required: true,
                    options: None,
                    min: None,
                    max: None,
                    step: None,
                    default: None,
                },
            ],
            json!({ "student": "", "seat_id": "" }),
        ),
        template(
            "must_be_adjacent",
            "must_be_adjacent",
            RuleCategory::Hard,
            "必须相邻",
            "Must sit together",
            "让 {student_a} 与 {student_b} 相邻而坐",
            "Seat {student_a} and {student_b} next to each other",
            vec![
                student_slot("student_a", "students/0", "学生 A", "Student A"),
                student_slot("student_b", "students/1", "学生 B", "Student B"),
            ],
            json!({ "students": ["", ""] }),
        ),
        template(
            "cannot_be_adjacent",
            "cannot_be_adjacent",
            RuleCategory::Hard,
            "禁止相邻",
            "Must not sit together",
            "让 {student_a} 与 {student_b} 不要相邻",
            "Keep {student_a} and {student_b} apart",
            vec![
                student_slot("student_a", "students/0", "学生 A", "Student A"),
                student_slot("student_b", "students/1", "学生 B", "Student B"),
            ],
            json!({ "students": ["", ""] }),
        ),
        template(
            "student_group",
            "groups",
            RuleCategory::Hard,
            "标签分组",
            "Student group",
            "把 {students} 编为一组：{mode}",
            "Group {students} together: {mode}",
            vec![
                SlotSpec {
                    key: "name".to_string(),
                    kind: SlotKind::Text,
                    label: LocalizedKeys::new("小组名称", "Group name"),
                    placeholder: Some(LocalizedKeys::new("例如：第一组", "e.g. Group 1")),
                    param_path: Some("name".to_string()),
                    required: true,
                    options: None,
                    min: None,
                    max: None,
                    step: None,
                    default: None,
                },
                SlotSpec {
                    key: "students".to_string(),
                    kind: SlotKind::Students,
                    label: LocalizedKeys::new("组员", "Members"),
                    placeholder: Some(LocalizedKeys::new("选择学生…", "Choose students…")),
                    param_path: Some("students".to_string()),
                    required: true,
                    options: None,
                    min: None,
                    max: None,
                    step: None,
                    default: None,
                },
                SlotSpec {
                    key: "mode".to_string(),
                    kind: SlotKind::Choice,
                    label: LocalizedKeys::new("分组方式", "Group mode"),
                    placeholder: None,
                    param_path: Some("separate".to_string()),
                    required: true,
                    options: Some(vec![
                        LocalizedOption {
                            value: "separate".to_string(),
                            param_value: Some(json!(true)),
                            label: LocalizedKeys::new("组内成员分开坐", "Members sit apart"),
                        },
                        LocalizedOption {
                            value: "together".to_string(),
                            param_value: Some(json!(false)),
                            label: LocalizedKeys::new("组内成员坐在一起", "Members sit together"),
                        },
                    ]),
                    min: None,
                    max: None,
                    step: None,
                    default: Some(json!("separate")),
                },
            ],
            json!({ "name": "", "students": [], "separate": true }),
        ),
        template(
            "vision_front",
            "vision_front",
            RuleCategory::Soft,
            "视力需求靠前",
            "Vision needs front",
            "让标记「需要前排」的学生坐在教室前排",
            "Seat students marked \"need front\" near the front of the room",
            vec![],
            json!({ "enabled": true, "weight": 20 }),
        ),
        template(
            "score_balance",
            "score_distribution",
            RuleCategory::Soft,
            "成绩搭配",
            "Score balance",
            "让相邻座位的成绩尽量拉开差距（好带弱）",
            "Mix neighboring scores so stronger students sit with weaker ones",
            vec![],
            json!({ "enabled": true, "weight": 18, "scope": "row" }),
        ),
    ]
}

pub fn sentence_template(id: &str) -> Option<SentenceTemplate> {
    sentence_templates().into_iter().find(|item| item.id == id)
}

fn set_path(entry: &mut Value, path: &str, value: Value) -> Result<(), CompileError> {
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if segments.is_empty() {
        return Err(CompileError::new(
            "invalid_path",
            None,
            format!("slot path {path:?} is empty"),
        ));
    }
    let mut current = entry;
    for (index, segment) in segments.iter().enumerate() {
        let last = index == segments.len() - 1;
        let parsed = segment.parse::<usize>().ok();
        if let Some(array_index) = parsed {
            let array = current.as_array_mut().ok_or_else(|| {
                CompileError::new(
                    "invalid_path",
                    None,
                    format!("path segment {segment:?} is not an array"),
                )
            })?;
            if last {
                if array_index >= array.len() {
                    return Err(CompileError::new(
                        "invalid_path",
                        None,
                        format!("array index {array_index} out of range"),
                    ));
                }
                array[array_index] = value;
                return Ok(());
            }
            current = array.get_mut(array_index).ok_or_else(|| {
                CompileError::new(
                    "invalid_path",
                    None,
                    format!("array index {array_index} out of range"),
                )
            })?;
        } else {
            let object = current.as_object_mut().ok_or_else(|| {
                CompileError::new(
                    "invalid_path",
                    None,
                    format!("path segment {segment:?} is not an object"),
                )
            })?;
            if last {
                object.insert(segment.to_string(), value);
                return Ok(());
            }
            current = object.entry(segment.to_string()).or_insert(Value::Null);
        }
    }
    Ok(())
}

/// Fill a template's slots and produce the canonical rule entry. The entry is
/// validated structurally (required slots, choice values, number bounds);
/// semantic checks (unknown student/seat) stay at solve/precheck time.
pub fn compile_sentence(
    template_id: &str,
    slots: &Map<String, Value>,
) -> Result<CompiledRule, CompileError> {
    let template = sentence_template(template_id).ok_or_else(|| {
        CompileError::new(
            "unknown_template",
            None,
            format!("unknown template {template_id:?}"),
        )
    })?;
    let mut entry = template.defaults.clone();

    for slot in &template.slots {
        let raw = slots.get(&slot.key);
        let value = match raw {
            None | Some(Value::Null) => {
                if let Some(default) = &slot.default {
                    default.clone()
                } else if slot.required {
                    return Err(CompileError::new(
                        "missing_slot",
                        Some(&slot.key),
                        format!("required slot '{}' is not filled", slot.key),
                    ));
                } else {
                    continue;
                }
            }
            Some(value) => value.clone(),
        };

        match slot.kind {
            SlotKind::Student | SlotKind::Seat | SlotKind::Text => {
                let text = value.as_str().map(str::trim).unwrap_or("");
                if text.is_empty() {
                    if slot.required {
                        return Err(CompileError::new(
                            "missing_slot",
                            Some(&slot.key),
                            format!("required slot '{}' is empty", slot.key),
                        ));
                    }
                    continue;
                }
                if let Some(path) = &slot.param_path {
                    set_path(&mut entry, path, json!(text))?;
                }
            }
            SlotKind::Students => {
                let text = value.as_str().map(str::trim).unwrap_or("");
                if text.is_empty() {
                    if slot.required {
                        return Err(CompileError::new(
                            "missing_slot",
                            Some(&slot.key),
                            format!("required slot '{}' is empty", slot.key),
                        ));
                    }
                    continue;
                }
                let members: Vec<String> = text
                    .split(',')
                    .map(|part| part.trim().to_string())
                    .filter(|part| !part.is_empty())
                    .collect();
                if members.len() < 2 {
                    return Err(CompileError::new(
                        "invalid_value",
                        Some(&slot.key),
                        "a group needs at least two students".to_string(),
                    ));
                }
                if let Some(path) = &slot.param_path {
                    set_path(&mut entry, path, json!(members))?;
                }
            }
            SlotKind::Number => {
                let number = value
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| {
                        CompileError::new(
                            "invalid_value",
                            Some(&slot.key),
                            format!("slot '{}' must be a number", slot.key),
                        )
                    })?;
                if let Some(min) = slot.min {
                    if number < min {
                        return Err(CompileError::new(
                            "invalid_value",
                            Some(&slot.key),
                            format!("slot '{}' must be at least {min}", slot.key),
                        ));
                    }
                }
                if let Some(max) = slot.max {
                    if number > max {
                        return Err(CompileError::new(
                            "invalid_value",
                            Some(&slot.key),
                            format!("slot '{}' must be at most {max}", slot.key),
                        ));
                    }
                }
                if let Some(path) = &slot.param_path {
                    set_path(&mut entry, path, json!(number))?;
                }
            }
            SlotKind::Choice => {
                let text = value.as_str().unwrap_or("");
                let options = slot.options.as_deref().ok_or_else(|| {
                    CompileError::new(
                        "invalid_template",
                        Some(&slot.key),
                        "choice without options",
                    )
                })?;
                let option = options
                    .iter()
                    .find(|option| option.value == text)
                    .ok_or_else(|| {
                        CompileError::new(
                            "invalid_choice",
                            Some(&slot.key),
                            format!("slot '{}' has no option {text:?}", slot.key),
                        )
                    })?;
                if let Some(path) = &slot.param_path {
                    let bound = option
                        .param_value
                        .clone()
                        .unwrap_or_else(|| json!(option.value));
                    set_path(&mut entry, path, bound)?;
                }
            }
        }
    }

    Ok(CompiledRule {
        category: template.category,
        rule_id: template.rule_id,
        entry,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn templates_cover_the_builder_presets() {
        let templates = sentence_templates();
        let ids: Vec<&str> = templates.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "student_distance",
                "fixed_seat",
                "must_be_adjacent",
                "cannot_be_adjacent",
                "student_group",
                "vision_front",
                "score_balance",
            ]
        );
        for template in &templates {
            assert!(!template.sentence.zh.contains("TODO"));
            assert!(!template.sentence.en.contains("TODO"));
            for slot in &template.slots {
                if slot.param_path.is_some() {
                    assert!(!slot.key.is_empty());
                }
            }
        }
    }

    #[test]
    fn student_distance_compiles_into_the_min_distance_entry() {
        let mut slots = Map::new();
        slots.insert("student_a".to_string(), json!("S01"));
        slots.insert("student_b".to_string(), json!("S02"));
        slots.insert("distance".to_string(), json!(2.5));
        let compiled = compile_sentence("student_distance", &slots).expect("compiles");
        assert_eq!(compiled.category, RuleCategory::Hard);
        assert_eq!(compiled.rule_id, "min_distance");
        assert_eq!(
            compiled.entry,
            json!({
                "students": ["S01", "S02"],
                "distance": 2.5,
                "metric": "graph",
            })
        );
    }

    #[test]
    fn missing_required_slot_is_rejected() {
        let mut slots = Map::new();
        slots.insert("student_a".to_string(), json!("S01"));
        let error = compile_sentence("student_distance", &slots).expect_err("rejects");
        assert_eq!(error.code, "missing_slot");
        assert_eq!(error.slot.as_deref(), Some("student_b"));
    }

    #[test]
    fn number_bounds_are_enforced() {
        let mut slots = Map::new();
        slots.insert("student_a".to_string(), json!("S01"));
        slots.insert("student_b".to_string(), json!("S02"));
        slots.insert("distance".to_string(), json!(0.0));
        let error = compile_sentence("student_distance", &slots).expect_err("rejects");
        assert_eq!(error.code, "invalid_value");
    }

    #[test]
    fn fixed_seat_binds_student_and_seat() {
        let mut slots = Map::new();
        slots.insert("student".to_string(), json!("S01"));
        slots.insert("seat".to_string(), json!("R1C1"));
        let compiled = compile_sentence("fixed_seat", &slots).expect("compiles");
        assert_eq!(
            compiled.entry,
            json!({ "student": "S01", "seat_id": "R1C1" })
        );
    }

    #[test]
    fn group_mode_maps_display_value_to_the_bool_param() {
        let mut slots = Map::new();
        slots.insert("name".to_string(), json!("第一组"));
        slots.insert("students".to_string(), json!("S01, S02, S03"));
        slots.insert("mode".to_string(), json!("separate"));
        let compiled = compile_sentence("student_group", &slots).expect("compiles");
        assert_eq!(
            compiled.entry,
            json!({
                "name": "第一组",
                "students": ["S01", "S02", "S03"],
                "separate": true,
            })
        );

        slots.insert("mode".to_string(), json!("together"));
        let compiled = compile_sentence("student_group", &slots).expect("compiles");
        assert_eq!(compiled.entry["separate"], json!(false));
    }

    #[test]
    fn invalid_choice_is_rejected() {
        let mut slots = Map::new();
        slots.insert("name".to_string(), json!("第一组"));
        slots.insert("students".to_string(), json!("S01, S02"));
        slots.insert("mode".to_string(), json!("diagonal"));
        let error = compile_sentence("student_group", &slots).expect_err("rejects");
        assert_eq!(error.code, "invalid_choice");
    }

    #[test]
    fn slotless_soft_templates_compile_from_defaults() {
        let compiled = compile_sentence("vision_front", &Map::new()).expect("compiles");
        assert_eq!(compiled.category, RuleCategory::Soft);
        assert_eq!(compiled.rule_id, "vision_front");
        assert_eq!(compiled.entry, json!({ "enabled": true, "weight": 20 }));
    }

    #[test]
    fn unknown_template_is_rejected() {
        let error = compile_sentence("nope", &Map::new()).expect_err("rejects");
        assert_eq!(error.code, "unknown_template");
    }
}
