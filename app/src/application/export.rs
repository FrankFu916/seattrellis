//! Export orchestration (M1-02): rebuild the current plan from the editor
//! state and render the requested format. Business logic only.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::application::{AppError, SolveRequestStore};
use crate::editing::{self, EditorDraftStore};

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
        return Err(AppError::bad_request("export request is missing a 'draft_id'"));
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
    let response_value = export_response_value(&request_value, &state);

    let mut export_json = value.clone();
    if let Some(object) = export_json.as_object_mut() {
        // `print-html` renders the same native HTML sheet as `html`.
        let format = object.get("format").and_then(Value::as_str).unwrap_or("");
        if format.eq_ignore_ascii_case("print-html") {
            object.insert("format".to_string(), json!("html"));
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

    let filename = format!("seat-plan.{}", format.extension());
    Ok(ExportOutcome {
        content_type: format.mime(),
        content_disposition: format!("attachment; filename=\"{filename}\""),
        body: bytes,
    })
}

/// Reconstruct the `CoreSolveResponse`-shaped JSON for export from the current
/// editor state, so exports reflect manual adjustments, not the original solve.
pub(crate) fn export_response_value(request_value: &Value, state: &editing::EditorState) -> Value {
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
            if let (Some(&student_idx), Some(&seat_idx)) =
                (student_index.get(&student.student_key), seat_index.get(seat_id))
            {
                assignment.push([student_idx, seat_idx]);
            }
        }
    }

    json!({
        "api_version": 2,
        "feasible": true,
        "assignment": assignment,
        "attempts_used": 1,
        "hard_constraints_satisfied": true,
        "total_cost": null,
    })
}