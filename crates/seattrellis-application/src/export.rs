//! Export orchestration (M1-02): rebuild the current plan from the editor
//! state and render the requested format. Business logic only.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::{AppError, SolveRequestStore};
use seattrellis_core::{CoreSolveRequest, CoreSolveResponse, SolveStatus};
use seattrellis_domain::editing::{self, EditorDraftStore};

/// The result of an export request: the rendered bytes plus the content
/// metadata the transport layer turns into headers (M1-02).
pub struct ExportOutcome {
    pub content_type: &'static str,
    pub content_disposition: String,
    pub body: Vec<u8>,
}

/// Orchestrate an export: locate the originating solve request, rebuild the
/// current plan from the editor state, render and return the artifact.
/// Business logic only - no HTTP types (M1-02).
pub fn export_draft(
    value: &Value,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Result<ExportOutcome, AppError> {
    let draft_id = value
        .get("draft_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if draft_id.is_empty() {
        return Err(AppError::bad_request(
            "export request is missing a 'draft_id'",
        ));
    }

    let request_value = match solve_requests.lock() {
        Ok(guard) => match guard.get(draft_id) {
            Some(value) => value.clone(),
            None => return Err(AppError::not_found("editor draft was not found")),
        },
        Err(_) => return Err(AppError::internal("solve request store is poisoned")),
    };
    // Fetching the state also validates that the draft still exists.
    let state = match editing::fetch_state(editor_store, draft_id) {
        Ok(state) => state,
        Err(_) => return Err(AppError::not_found("editor draft was not found")),
    };
    let response_value = export_response_value(&request_value, &state)?;

    let mut export_json = value.clone();
    if let Some(object) = export_json.as_object_mut() {
        // `print-html` has its own dedicated layout (print-layout-spec.md);
        // no normalization to `html` anymore (M5-A2).
        // Remembered export defaults fill in options the request did not
        // specify explicitly (PD-D9 "last used" semantics, M5-A5). The
        // memory file lives in the user config dir; malformed memory is
        // ignored and built-in defaults apply.
        if let Some(memory) = seattrellis_io::export_defaults::ExportDefaults::load_global() {
            let mut patch = serde_json::Map::new();
            for (key, value) in [
                ("template", memory.template.as_str()),
                ("paper_size", memory.paper_size.as_str()),
                ("locale", memory.locale.as_str()),
            ] {
                if !object.contains_key(key) {
                    patch.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
            }
            if !object.contains_key("orientation") {
                if let Some(orientation) = &memory.orientation {
                    patch.insert(
                        "orientation".to_string(),
                        serde_json::Value::String(orientation.clone()),
                    );
                }
            }
            for (key, value) in [
                ("hide_scores", memory.hide_scores),
                ("hide_notes", memory.hide_notes),
                ("hide_special_needs", memory.hide_special_needs),
                ("anonymize", memory.anonymize),
                ("show_height", memory.show_height),
                ("show_vision", memory.show_vision),
                ("show_student_ids", memory.show_student_ids),
            ] {
                if !object.contains_key("privacy") {
                    let privacy = patch
                        .entry("privacy".to_string())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
                        .as_object_mut()
                        .expect("privacy entry is an object");
                    privacy.insert(key.to_string(), serde_json::Value::Bool(value));
                }
            }
            for key in ["page_scale", "margin_mm"] {
                if !object.contains_key(key) {
                    let value = if key == "page_scale" {
                        memory.page_scale
                    } else {
                        memory.margin_mm
                    };
                    patch.insert(key.to_string(), serde_json::json!(value));
                }
            }
            for (key, value) in patch {
                object.insert(key, value);
            }
        }
        object.insert("request".to_string(), request_value);
        object.insert("response".to_string(), response_value);
    }
    let export_string = export_json.to_string();

    let format = match seattrellis_export::export::format_of(&export_string) {
        Ok(format) => format,
        Err(message) => return Err(AppError::bad_request(&message)),
    };
    let bytes = match seattrellis_export::export::export_plan(&export_string) {
        Ok(bytes) => bytes,
        Err(message) => return Err(AppError::bad_request(&message)),
    };

    // Remember the effective parameters for the next quick export.
    let _ = remember_defaults(&export_json);

    let filename = format!("seat-plan.{}", format.extension());
    Ok(ExportOutcome {
        content_type: format.mime(),
        content_disposition: format!("attachment; filename=\"{filename}\""),
        body: bytes,
    })
}

/// Revalidate an editor state against the originating solve request and return
/// a UI-consumable report. An invalid intermediate edit remains an editor
/// state (so undo/redo still works), but it is explicitly marked and cannot be
/// exported as a solved plan.
pub fn editor_validation_report(
    draft_id: &str,
    state: &editing::EditorState,
    solve_requests: &SolveRequestStore,
) -> Result<Value, AppError> {
    let request_value = match solve_requests.lock() {
        Ok(guard) => guard
            .get(draft_id)
            .cloned()
            .ok_or_else(|| AppError::internal("editor draft has no originating solve request"))?,
        Err(_) => return Err(AppError::internal("solve request store is poisoned")),
    };
    let (request, response) = editor_solve_response(&request_value, state)?;
    match seattrellis_core::validate_solve_response(&request, &response) {
        Ok(()) => Ok(json!({
            "valid": true,
            "hard_constraints_satisfied": true,
            "violations": [],
        })),
        Err(message) => {
            let rule_id = if message.contains("hard rule") {
                "hard_constraints"
            } else {
                "assignment.integrity"
            };
            Ok(json!({
                "valid": false,
                "hard_constraints_satisfied": false,
                "violations": [{
                    "rule_id": rule_id,
                    "entity": null,
                    "reason": message,
                    "witness": null,
                    "suggested_action": "undo_or_adjust",
                    "message_key": "validation.editor_assignment_invalid",
                }],
            }))
        }
    }
}

/// Reconstruct and independently validate the current editor assignment before
/// export. Manual edits are allowed to change the original solution, but they
/// must never bypass the original request's hard rules.
pub(crate) fn export_response_value(
    request_value: &Value,
    state: &editing::EditorState,
) -> Result<Value, AppError> {
    let (request, response) = editor_solve_response(request_value, state)?;
    seattrellis_core::validate_solve_response(&request, &response).map_err(|message| {
        AppError::unprocessable(
            "invalid_export_assignment",
            format!("the edited plan cannot be exported: {message}"),
        )
    })?;
    serde_json::to_value(response)
        .map_err(|error| AppError::internal(format!("could not encode export response: {error}")))
}

pub(crate) fn editor_solve_response(
    request_value: &Value,
    state: &editing::EditorState,
) -> Result<(CoreSolveRequest, CoreSolveResponse), AppError> {
    let request: CoreSolveRequest = serde_json::from_value(request_value.clone())
        .map_err(|_| AppError::internal("stored solve request is not a valid CoreSolveRequest"))?;

    // student key -> index, using the same fallback keys as the draft builder.
    let students = request_value.get("students").and_then(Value::as_array);
    let student_count = request_value
        .get("student_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut student_index: HashMap<String, usize> = HashMap::new();
    for index in 0..student_count {
        let key = students
            .and_then(|list| list.get(index))
            .and_then(|student| student.get("key"))
            .and_then(Value::as_str)
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| format!("student-{}", index + 1));
        student_index.insert(key, index);
    }

    // seat_id -> index, using the same seat ids as the draft builder.
    let layout_seats = request_value
        .get("layout")
        .and_then(|layout| layout.get("seats"))
        .and_then(Value::as_array);
    let seat_count = request_value
        .get("seat_positions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let mut seat_index: HashMap<String, usize> = HashMap::new();
    for index in 0..seat_count {
        let seat_id = layout_seats
            .and_then(|list| list.get(index))
            .and_then(|seat| seat.get("seat_id"))
            .and_then(Value::as_str)
            .map(|seat_id| seat_id.to_string())
            .unwrap_or_else(|| format!("seat-{}", index + 1));
        seat_index.insert(seat_id, index);
    }

    let mut assignment: Vec<[usize; 2]> = Vec::new();
    for student in &state.students {
        if let Some(seat_id) = &student.seat_id {
            if let (Some(&student_idx), Some(&seat_idx)) = (
                student_index.get(&student.student_key),
                seat_index.get(seat_id),
            ) {
                assignment.push([student_idx, seat_idx]);
            }
        }
    }

    let response = CoreSolveResponse {
        api_version: 2,
        feasible: true,
        status: SolveStatus::Solved,
        assignment,
        attempts_used: 1,
        hard_constraints_satisfied: true,
        total_cost: None,
    };
    Ok((request, response))
}

/// Persist the effective export parameters as the "last used" memory
/// (PD-D9). Best-effort: a failure to write the memory file never fails the
/// export itself.
fn remember_defaults(export_json: &Value) -> Result<(), String> {
    let object = export_json
        .as_object()
        .ok_or("export json is not an object")?;
    let privacy = object
        .get("privacy")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let get_bool = |key: &str| privacy.get(key).and_then(Value::as_bool).unwrap_or(false);
    let memory = seattrellis_io::export_defaults::ExportDefaults {
        template: object
            .get("template")
            .and_then(Value::as_str)
            .unwrap_or("teacher")
            .to_string(),
        hide_scores: get_bool("hide_scores"),
        hide_notes: get_bool("hide_notes"),
        hide_special_needs: get_bool("hide_special_needs"),
        anonymize: get_bool("anonymize"),
        show_height: get_bool("show_height"),
        show_vision: get_bool("show_vision"),
        orientation: object
            .get("orientation")
            .and_then(Value::as_str)
            .map(str::to_string),
        paper_size: object
            .get("paper_size")
            .and_then(Value::as_str)
            .unwrap_or("a4")
            .to_string(),
        page_scale: object
            .get("page_scale")
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
        margin_mm: object
            .get("margin_mm")
            .and_then(Value::as_f64)
            .unwrap_or(12.0),
        locale: object
            .get("locale")
            .and_then(Value::as_str)
            .unwrap_or("zh")
            .to_string(),
        show_student_ids: object
            .get("show_student_ids")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    memory.save_global()
}

#[cfg(test)]
mod tests {
    use super::*;
    use seattrellis_domain::editing::{EditorSeatState, EditorState, EditorStudentState};

    fn editor_state(first_seat: &str, second_seat: &str) -> EditorState {
        EditorState {
            kind: "seattrellis_editor_state".to_string(),
            protocol_version: "1.0".to_string(),
            draft_id: "draft-1".to_string(),
            revision: 1,
            candidate_id: Some("candidate-1".to_string()),
            undo_depth: 0,
            redo_depth: 0,
            students: vec![
                EditorStudentState {
                    student_key: "S1".to_string(),
                    display_name: "S1".to_string(),
                    seat_id: Some(first_seat.to_string()),
                    locked: false,
                },
                EditorStudentState {
                    student_key: "S2".to_string(),
                    display_name: "S2".to_string(),
                    seat_id: Some(second_seat.to_string()),
                    locked: false,
                },
            ],
            seats: vec![
                EditorSeatState {
                    seat_id: "A1".to_string(),
                    row: 1,
                    col: 1,
                    enabled: true,
                    student_key: Some("S1".to_string()),
                    locked: false,
                },
                EditorSeatState {
                    seat_id: "A2".to_string(),
                    row: 1,
                    col: 2,
                    enabled: true,
                    student_key: Some("S2".to_string()),
                    locked: false,
                },
            ],
        }
    }

    fn fixed_request() -> Value {
        json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0]],
            "students": [{"key": "S1"}, {"key": "S2"}],
            "layout": {
                "name": "Room",
                "seats": [
                    {"seat_id": "A1", "row": 1, "col": 1, "enabled": true},
                    {"seat_id": "A2", "row": 1, "col": 2, "enabled": true}
                ]
            }
        })
    }

    #[test]
    fn export_response_marks_only_a_validated_assignment_solved() {
        let response = export_response_value(&fixed_request(), &editor_state("A1", "A2"))
            .expect("fixed seat is preserved");
        assert_eq!(response["status"], "Solved");
        assert_eq!(response["feasible"], true);
        assert_eq!(response["hard_constraints_satisfied"], true);
    }

    #[test]
    fn export_response_rejects_manual_hard_rule_violation() {
        let error = export_response_value(&fixed_request(), &editor_state("A2", "A1"))
            .expect_err("moving a fixed student must block export");
        assert_eq!(error.status, 422);
        assert_eq!(error.code, "invalid_export_assignment");
        assert!(error.message.contains("hard rule"), "{}", error.message);
    }

    #[test]
    fn export_response_rejects_incomplete_editor_state() {
        let mut state = editor_state("A1", "A2");
        state.students[1].seat_id = None;
        let error = export_response_value(&fixed_request(), &state)
            .expect_err("an unseated student must block export");
        assert_eq!(error.status, 422);
        assert!(error.message.contains("assignment"), "{}", error.message);
    }

    #[test]
    fn editor_validation_report_marks_hard_rule_violations_without_hiding_state() {
        let requests =
            SolveRequestStore::new(HashMap::from([("draft-1".to_string(), fixed_request())]));
        let report = editor_validation_report("draft-1", &editor_state("A2", "A1"), &requests)
            .expect("validation report should be produced");
        assert_eq!(report["valid"], false);
        assert_eq!(report["hard_constraints_satisfied"], false);
        assert_eq!(report["violations"][0]["rule_id"], "hard_constraints");
        assert_eq!(
            report["violations"][0]["message_key"],
            "validation.editor_assignment_invalid"
        );
    }
}
