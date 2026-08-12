//! Draft audit endpoint support (M5 B5 / D5): recompute the PlanScore
//! seven-dimension breakdown and the hard-constraint audit for any stored
//! editor draft (a fresh candidate or a hand-edited plan) from its
//! originating solve request and current assignment. The candidates UI and
//! the diagnostics panel consume the same report (D5/D6 share the data).

use serde_json::{json, Value};

use seattrellis_core::audit_report_json;
use seattrellis_core::score_assignment_json;
use seattrellis_domain::editing::{self, EditorDraftStore};

use crate::export::editor_solve_response;
use crate::{AppError, SolveRequestStore};

/// Re-audit a stored draft and return the combined score + audit report.
///
/// The request that produced the draft comes from the solve-request store
/// (the same source the exporter uses to rebuild plans after edits), so the
/// report always reflects the draft's *current* assignment. An incomplete
/// assignment (e.g. a student unseated by editing) is a structured 422 —
/// diagnostics can never bless an illegal plan (M3-06).
pub fn audit_draft(
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
    draft_id: &str,
) -> Result<Value, AppError> {
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
    let (_request, response) = editor_solve_response(&request_value, &state)?;

    // The stored value is already the core-shaped solve request; re-serialize
    // it as-is so the score and audit see exactly the problem that produced
    // (and constrains) this draft.
    let request_json = request_value.to_string();
    let assignment = &response.assignment;

    let score = score_assignment_json(&request_json, assignment, "[]", None).map_err(|message| {
        AppError::unprocessable("invalid_assignment", format!("plan cannot be scored: {message}"))
    })?;
    let audit = audit_report_json(&request_json, assignment)
        .map_err(|message| AppError::internal(format!("audit failed: {message}")))?;

    let score_value: Value = serde_json::from_str(&score)
        .map_err(|error| AppError::internal(format!("could not encode plan score: {error}")))?;
    let audit_value: Value = serde_json::from_str(&audit)
        .map_err(|error| AppError::internal(format!("could not encode audit report: {error}")))?;

    Ok(json!({
        "api_version": "1",
        "draft_id": draft_id,
        "feasible": true,
        "score": score_value,
        "audit": audit_value,
    }))
}
