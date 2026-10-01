//! Portable class documents preserve teacher input and complete editable solve contexts.
use crate::class_generation::{new_draft_id, seat_specs, student_keys};
use crate::{AppError, SolveRequestStore};
use seattrellis_domain::editing::{self, EditorDraft, EditorDraftStore, EditorState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftReference {
    pub draft_id: String,
    pub revision: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub student_key: String,
    pub seat_id: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockState {
    #[serde(default)]
    pub locked_students: Vec<String>,
    #[serde(default)]
    pub locked_seats: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDraft {
    pub candidate_id: Option<String>,
    pub solve_request: Value,
    pub assignments: Vec<Assignment>,
    #[serde(default)]
    pub lock_state: LockState,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassDocument {
    pub kind: String,
    pub schema_version: u32,
    pub class_source: Value,
    pub drafts: Vec<SavedDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_plan: Option<Value>,
}

/// Validate and construct a persisted editor without changing its locks or undo history.
pub fn restored_draft(
    request: &Value,
    draft_id: &str,
    candidate_id: Option<String>,
    assignment: &[(&str, &str)],
    locked_students: &[String],
    locked_seats: &[String],
) -> Result<EditorDraft, AppError> {
    let request_json = request.to_string();
    let typed = seattrellis_core::parse_core_solve_request(&request_json)
        .map_err(AppError::solve_invalid_input)?;
    seattrellis_core::validate_solve_request_json(&request_json)
        .map_err(AppError::solve_invalid_input)?;
    let keys = student_keys(&typed);
    let refs = keys.iter().map(String::as_str).collect::<Vec<_>>();
    let names: HashMap<String, String> = typed
        .students
        .iter()
        .map(|s| {
            (
                s.key.clone(),
                s.display_name.clone().unwrap_or_else(|| s.key.clone()),
            )
        })
        .collect();
    let mut draft = EditorDraft::new(
        draft_id,
        candidate_id,
        &refs,
        seat_specs(&typed),
        assignment,
        Some(&names),
    )
    .map_err(AppError::bad_request)?;
    draft
        .restore_locks(locked_students, locked_seats)
        .map_err(AppError::bad_request)?;
    Ok(draft)
}

/// Restore a source, assignment and persisted locks as a paired application context.
#[allow(clippy::too_many_arguments)]
pub fn restore_draft(
    request: &Value,
    draft_id: &str,
    candidate_id: Option<String>,
    assignment: &[(&str, &str)],
    locked_students: &[String],
    locked_seats: &[String],
    editors: &EditorDraftStore,
    sources: &SolveRequestStore,
) -> Result<EditorState, AppError> {
    let draft = restored_draft(
        request,
        draft_id,
        candidate_id,
        assignment,
        locked_students,
        locked_seats,
    )?;
    crate::store_draft_context(editors, sources, draft, request.clone())
}

pub fn serialize_document(
    input: &Value,
    editors: &EditorDraftStore,
    sources: &SolveRequestStore,
) -> Result<Value, AppError> {
    let class_source = input
        .get("class_source")
        .filter(|source| source.is_object())
        .ok_or_else(|| AppError::bad_request("class_source must be an object"))?
        .clone();
    let refs: Vec<DraftReference> = serde_json::from_value(
        input
            .get("draft_refs")
            .cloned()
            .ok_or_else(|| AppError::bad_request("draft_refs are required"))?,
    )
    .map_err(|_| AppError::bad_request("invalid draft_refs"))?;
    if refs.len() > 20 {
        return Err(AppError::bad_request("at most 20 drafts may be saved"));
    }
    let editors = editors
        .lock()
        .map_err(|_| AppError::internal("editor store is poisoned"))?;
    let sources = sources
        .lock()
        .map_err(|_| AppError::internal("solve store is poisoned"))?;
    let mut saved = Vec::with_capacity(refs.len());
    let mut ids = std::collections::HashSet::new();
    for reference in refs {
        if !ids.insert(reference.draft_id.clone()) {
            return Err(AppError::bad_request("duplicate draft reference"));
        }
        let draft = editors
            .get(&reference.draft_id)
            .ok_or_else(|| AppError::not_found("editor draft was not found"))?;
        let state = editing::build_editor_state(draft);
        if state.revision != reference.revision {
            return Err(revision_conflict());
        }
        let request = sources
            .get(&reference.draft_id)
            .ok_or_else(|| AppError::not_found("draft source was not found"))?
            .clone();
        saved.push(SavedDraft {
            candidate_id: state.candidate_id,
            solve_request: request,
            assignments: state
                .students
                .iter()
                .filter_map(|s| {
                    s.seat_id.as_ref().map(|seat| Assignment {
                        student_key: s.student_key.clone(),
                        seat_id: seat.clone(),
                    })
                })
                .collect(),
            lock_state: LockState {
                locked_students: state
                    .students
                    .iter()
                    .filter(|s| s.locked)
                    .map(|s| s.student_key.clone())
                    .collect(),
                locked_seats: state
                    .seats
                    .iter()
                    .filter(|s| s.locked)
                    .map(|s| s.seat_id.clone())
                    .collect(),
            },
        });
    }
    let rotation_plan = normalized_rotation_plan(
        input.get("rotation_plan").filter(|v| !v.is_null()).cloned(),
        &saved,
    )?;
    serde_json::to_value(ClassDocument {
        kind: "seattrellis_class_document".into(),
        schema_version: 1,
        class_source,
        drafts: saved,
        rotation_plan,
    })
    .map_err(|e| AppError::internal(e.to_string()))
}

pub fn open_document(
    input: &Value,
    editors: &EditorDraftStore,
    sources: &SolveRequestStore,
) -> Result<Value, AppError> {
    let doc: ClassDocument = serde_json::from_value(input.clone())
        .map_err(|e| AppError::bad_request(format!("invalid class document: {e}")))?;
    if doc.kind != "seattrellis_class_document"
        || doc.schema_version != 1
        || !doc.class_source.is_object()
        || doc.drafts.len() > 20
    {
        return Err(AppError::bad_request("unsupported class document"));
    }
    // Validate every candidate before publishing any one of them.
    let rotation_plan = normalized_rotation_plan(doc.rotation_plan, &doc.drafts)?;
    let mut ready = Vec::new();
    for saved in doc.drafts {
        let pairs: Vec<(&str, &str)> = saved
            .assignments
            .iter()
            .map(|a| (a.student_key.as_str(), a.seat_id.as_str()))
            .collect();
        let id = new_draft_id();
        let draft = restored_draft(
            &saved.solve_request,
            &id,
            saved.candidate_id,
            &pairs,
            &saved.lock_state.locked_students,
            &saved.lock_state.locked_seats,
        )?;
        ready.push((draft, saved.solve_request));
    }
    crate::ensure_request_active()?;
    if rotation_plan.is_none() {
        crate::attach_candidate_peers(&mut ready)?;
    }
    let states = crate::store_draft_contexts(editors, sources, ready)?;
    let mut candidates = Vec::new();
    if rotation_plan.is_none() {
        let scores: Vec<(Option<f64>, bool)> = states
            .iter()
            .map(|state| {
                crate::draft_audit::audit_draft(editors, sources, &state.draft_id)
                    .ok()
                    .map(|report| {
                        (
                            report.pointer("/score/total").and_then(Value::as_f64),
                            report["feasible"] == true,
                        )
                    })
                    .unwrap_or((None, false))
            })
            .collect();
        let recommended = scores
            .iter()
            .enumerate()
            .filter_map(|(index, (score, valid))| {
                score.filter(|_| *valid).map(|score| (index, score))
            })
            .max_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| {
                        states[right.0]
                            .candidate_id
                            .cmp(&states[left.0].candidate_id)
                    })
                    .then_with(|| right.0.cmp(&left.0))
            })
            .map(|(index, _)| index);
        candidates = states.iter().enumerate().map(|(index,state)| json!({"candidate_id":state.draft_id,"recommended":recommended==Some(index),"total_score":scores[index].0})).collect();
    }
    let period_editors = if rotation_plan.is_some() {
        states.clone()
    } else {
        Vec::new()
    };
    Ok(
        json!({"class_source":doc.class_source,"editor":states.first(),"candidates":candidates,"period_editors":period_editors,"rotation_plan":rotation_plan}),
    )
}

fn revision_conflict() -> AppError {
    AppError {
        status: 409,
        code: "revision_conflict",
        message: "The seating plan changed. Refresh before retrying.".into(),
    }
}

pub fn repair_draft(
    draft_id: &str,
    input: &Value,
    editors: &EditorDraftStore,
    sources: &SolveRequestStore,
) -> Result<Value, AppError> {
    let expected = input
        .get("base_revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::bad_request("base_revision is required"))?;
    let affected: Vec<String> = serde_json::from_value(
        input
            .get("affected_students")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(|_| AppError::bad_request("affected_students must contain student ids"))?;
    let state = editing::fetch_state(editors, draft_id).map_err(AppError::not_found)?;
    if state.revision != expected {
        return Err(revision_conflict());
    }
    let source = crate::export::stored_solve_request(sources, draft_id)?;
    let snapshot = json!({"assignments":state.students.iter().filter_map(|s| s.seat_id.as_ref().map(|seat| json!({"student_key":s.student_key,"seat_id":seat}))).collect::<Vec<_>>()});
    let locked_students = state
        .students
        .iter()
        .filter(|s| s.locked)
        .map(|s| s.student_key.clone())
        .collect::<Vec<_>>();
    let locked_seats = state
        .seats
        .iter()
        .filter(|s| s.locked)
        .map(|s| s.seat_id.clone())
        .collect::<Vec<_>>();
    let report = seattrellis_core::repair_json_with_control(
        &source.to_string(),
        &snapshot.to_string(),
        &affected,
        &locked_students,
        &locked_seats,
        true,
        &crate::request_control(),
    )
    .map_err(|message| {
        if message.contains("status Cancelled") {
            AppError {
                status: 408,
                code: "cancelled",
                message,
            }
        } else if message.contains("status Timeout") {
            AppError {
                status: 504,
                code: "repair_timeout",
                message,
            }
        } else {
            AppError::unprocessable("repair_failed", message)
        }
    })?;
    let value: Value =
        serde_json::from_str(&report).map_err(|e| AppError::internal(e.to_string()))?;
    let assignments: Vec<Assignment> = value["assignments"]
        .as_array()
        .ok_or_else(|| AppError::internal("repair response has no assignments"))?
        .iter()
        .map(|a| {
            Ok(Assignment {
                student_key: a["student_key"]
                    .as_str()
                    .ok_or_else(|| AppError::internal("repair student is missing"))?
                    .to_string(),
                seat_id: a["seat_id"]
                    .as_str()
                    .ok_or_else(|| AppError::internal("repair seat is missing"))?
                    .to_string(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let pairs: Vec<(&str, &str)> = assignments
        .iter()
        .map(|a| (a.student_key.as_str(), a.seat_id.as_str()))
        .collect();
    crate::ensure_request_active()?;
    let mut guard = editors
        .lock()
        .map_err(|_| AppError::internal("editor store is poisoned"))?;
    let draft = guard
        .get_mut(draft_id)
        .ok_or_else(|| AppError::not_found("editor draft was not found"))?;
    if draft.revision() != expected {
        return Err(revision_conflict());
    }
    let result = draft
        .apply_repair_assignment(expected, &pairs)
        .map_err(AppError::bad_request)?;
    serde_json::to_value(result).map_err(|e| AppError::internal(e.to_string()))
}

fn normalized_rotation_plan(
    plan: Option<Value>,
    drafts: &[SavedDraft],
) -> Result<Option<Value>, AppError> {
    let Some(mut plan) = plan else {
        return Ok(None);
    };
    if plan["kind"] != "rotation_plan" {
        return Err(AppError::bad_request("rotation_plan kind is invalid"));
    }
    let periods = plan
        .get_mut("periods")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::bad_request("rotation_plan periods are missing"))?;
    if periods.is_empty() || periods.len() != drafts.len() {
        return Err(AppError::bad_request(
            "rotation periods must match captured drafts",
        ));
    }
    for (period, saved) in periods.iter_mut().zip(drafts) {
        let snapshot = period
            .get_mut("snapshot")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| AppError::bad_request("rotation period snapshot is missing"))?;
        snapshot.insert(
            "assignments".into(),
            serde_json::to_value(&saved.assignments)
                .map_err(|e| AppError::internal(e.to_string()))?,
        );
        snapshot.insert("original_request".into(), saved.solve_request.clone());
        let metadata = snapshot.entry("metadata").or_insert_with(|| json!({}));
        metadata
            .as_object_mut()
            .ok_or_else(|| AppError::bad_request("snapshot metadata must be an object"))?
            .insert(
                "lock_state".into(),
                serde_json::to_value(&saved.lock_state)
                    .map_err(|e| AppError::internal(e.to_string()))?,
            );
    }
    if let Some(first) = drafts.first() {
        let grid =
            seattrellis_domain::room_templates::grid_from_layout(&first.solve_request["layout"])
                .map_err(AppError::bad_request)?;
        let students = first.solve_request["students"]
            .as_array()
            .ok_or_else(|| AppError::bad_request("rotation source has no roster"))?;
        let mut snapshots = plan
            .get("base_history_snapshots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        snapshots.extend(
            plan["periods"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|period| period["snapshot"].clone()),
        );
        let keys = students
            .iter()
            .filter_map(|student| student["key"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        if let Some((history, pairs)) =
            crate::class_generation::build_history_json(students, &grid, &snapshots)
        {
            plan["fairness_summary"] =
                crate::rotation::fairness_summary_from_history(&history, snapshots.len(), &keys);
            plan["pair_repeat_summary"] =
                crate::rotation::pair_repeat_summary_from_history(&pairs, snapshots.len());
        }
    }
    Ok(Some(plan))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn problem() -> Value {
        json!({"api_version":2,"student_count":2,"students":[{"key":"A","display_name":"Alice","height_cm":152.0,"score":91.0,"needs":["front"]},{"key":"B","display_name":"Bob","vision":"0.6"}],"seat_positions":[[1.0,1.0],[1.0,2.0],[1.0,3.0]],"seed":42,"fixed_seats":[[0,0]],"layout":{"layout_id":"test","name":"Roundtrip","seats":[{"seat_id":"s1","row":1,"col":1},{"seat_id":"s2","row":1,"col":2},{"seat_id":"s3","row":1,"col":3}]}})
    }
    #[test]
    fn roundtrip_restores_full_source_assignment_and_locks_then_audits_and_exports() {
        let editors = editing::new_draft_store();
        let sources = SolveRequestStore::default();
        let source = problem();
        let initial = restore_draft(
            &source,
            "test-original",
            None,
            &[("A", "s1"), ("B", "s2")],
            &["A".into()],
            &["s3".into()],
            &editors,
            &sources,
        )
        .unwrap();
        let saved=serialize_document(&json!({"class_source":{"name":"All fields","settings":{"goal":"custom"}},"draft_refs":[{"draft_id":initial.draft_id,"revision":initial.revision}]}),&editors,&sources).unwrap();
        let fresh_editors = editing::new_draft_store();
        let fresh_sources = SolveRequestStore::default();
        let reopened = open_document(&saved, &fresh_editors, &fresh_sources).unwrap();
        assert_eq!(reopened["class_source"]["settings"]["goal"], "custom");
        assert_eq!(reopened["period_editors"], json!([]));
        let id = reopened["editor"]["draft_id"].as_str().unwrap();
        let state = editing::fetch_state(&fresh_editors, id).unwrap();
        assert!(state.students[0].locked);
        assert!(state.seats[2].locked);
        assert_eq!(state.undo_depth, 0);
        assert_eq!(state.revision, 0);
        assert_eq!(
            fresh_sources.lock().unwrap()[id]["students"][0]["score"],
            91.0
        );
        crate::draft_audit::audit_draft(&fresh_editors, &fresh_sources, id).unwrap();
        let exported = crate::export::export_draft(
            &json!({"draft_id":id,"format":"svg"}),
            &fresh_editors,
            &fresh_sources,
        )
        .unwrap();
        assert!(!exported.body.is_empty());
    }
    #[test]
    fn malformed_last_candidate_is_rejected_without_publishing_first_or_evicting_old_drafts() {
        let editors = editing::new_draft_store();
        let sources = SolveRequestStore::default();
        let source = problem();
        restore_draft(
            &source,
            "existing",
            None,
            &[("A", "s1"), ("B", "s2")],
            &[],
            &[],
            &editors,
            &sources,
        )
        .unwrap();
        let mut saved = serialize_document(
            &json!({"class_source":{},"draft_refs":[{"draft_id":"existing","revision":0}]}),
            &editors,
            &sources,
        )
        .unwrap();
        let mut bad = saved["drafts"][0].clone();
        bad["assignments"][0]["student_key"] = json!("UNKNOWN");
        saved["drafts"].as_array_mut().unwrap().push(bad);
        assert!(open_document(&saved, &editors, &sources).is_err());
        assert_eq!(editors.lock().unwrap().len(), 1);
        assert_eq!(sources.lock().unwrap().len(), 1);
    }
    #[test]
    fn save_rejects_stale_revision_and_source_only_documents_roundtrip() {
        let editors = editing::new_draft_store();
        let sources = SolveRequestStore::default();
        restore_draft(
            &problem(),
            "existing",
            None,
            &[("A", "s1")],
            &[],
            &[],
            &editors,
            &sources,
        )
        .unwrap();
        assert_eq!(
            serialize_document(
                &json!({"class_source":{},"draft_refs":[{"draft_id":"existing","revision":8}]}),
                &editors,
                &sources
            )
            .unwrap_err()
            .status,
            409
        );
        let doc = serialize_document(
            &json!({"class_source":{"students":[{"notes":"teacher notes"}]},"draft_refs":[]}),
            &editors,
            &sources,
        )
        .unwrap();
        let opened = open_document(&doc, &editors, &sources).unwrap();
        assert!(opened["editor"].is_null());
        assert_eq!(opened["candidates"], json!([]));
    }
    #[test]
    fn repair_seats_unseated_students_preserves_locks_and_is_one_undoable_revision() {
        let editors = editing::new_draft_store();
        let sources = SolveRequestStore::default();
        restore_draft(
            &problem(),
            "repair",
            None,
            &[("A", "s1")],
            &["A".into()],
            &["s3".into()],
            &editors,
            &sources,
        )
        .unwrap();
        let state = repair_draft(
            "repair",
            &json!({"base_revision":0,"affected_students":[]}),
            &editors,
            &sources,
        )
        .unwrap();
        assert_eq!(state["revision"], 1);
        assert_eq!(state["undo_depth"], 1);
        assert_eq!(state["students"][0]["seat_id"], "s1");
        assert_eq!(state["students"][1]["seat_id"], "s2");
        assert!(state["seats"][2]["student_key"].is_null());
        assert_eq!(
            repair_draft("repair", &json!({"base_revision":0}), &editors, &sources)
                .unwrap_err()
                .status,
            409
        );
        let command:editing::EditorCommandEnvelope=serde_json::from_value(json!({"kind":"seattrellis_editor_command","protocol_version":"1.0","draft_id":"repair","command_id":"undo-repair","base_revision":1,"action":"undo","operations":[]})).unwrap();
        let undone = editing::apply_command_in_store(&editors, &command).unwrap();
        assert!(undone.students[1].seat_id.is_none());
        assert!(undone.students[0].locked);
        assert!(undone.seats[2].locked);
    }
    #[test]
    fn cancelled_context_batch_leaves_existing_session_unchanged() {
        let editors = editing::new_draft_store();
        let sources = SolveRequestStore::default();
        restore_draft(
            &problem(),
            "existing",
            None,
            &[("A", "s1")],
            &[],
            &[],
            &editors,
            &sources,
        )
        .unwrap();
        let control = seattrellis_core::SolveControl::new();
        control.cancel();
        let result = crate::with_request_control(control, || {
            restore_draft(
                &problem(),
                "cancelled",
                None,
                &[("A", "s1")],
                &[],
                &[],
                &editors,
                &sources,
            )
        });
        assert_eq!(result.unwrap_err().status, 408);
        assert_eq!(editors.lock().unwrap().len(), 1);
        assert!(!sources.lock().unwrap().contains_key("cancelled"));
    }
    #[test]
    fn unchanged_candidate_scores_and_recommendation_survive_portable_roundtrip() {
        let editors = editing::new_draft_store();
        let sources = SolveRequestStore::default();
        let mut source = problem();
        source["options"] = json!({"candidate_count":2});
        let generated =
            crate::class_generation::generate_class(&source, &editors, &sources).unwrap();
        assert_eq!(generated.candidates.len(), 2);
        let before: Vec<f64> = generated
            .candidates
            .iter()
            .map(|candidate| {
                let audit =
                    crate::draft_audit::audit_draft(&editors, &sources, &candidate.draft_id)
                        .unwrap();
                let score = audit["score"]["total"].as_f64().unwrap();
                assert!(
                    (score - candidate.total_score).abs() < 1e-9,
                    "generation and audit must score identical candidate contexts"
                );
                score
            })
            .collect();
        let refs: Vec<Value> = generated
            .candidates
            .iter()
            .map(|candidate| json!({"draft_id":candidate.draft_id,"revision":0}))
            .collect();
        let doc = serialize_document(
            &json!({"class_source":{},"draft_refs":refs}),
            &editors,
            &sources,
        )
        .unwrap();
        let fresh_editors = editing::new_draft_store();
        let fresh_sources = SolveRequestStore::default();
        let opened = open_document(&doc, &fresh_editors, &fresh_sources).unwrap();
        for (index, candidate) in opened["candidates"].as_array().unwrap().iter().enumerate() {
            assert!((candidate["total_score"].as_f64().unwrap() - before[index]).abs() < 1e-9);
            assert_eq!(
                candidate["recommended"], generated.candidates[index].recommended,
                "recommendation preserves tie-breaking order"
            );
        }
        let old_ids: Vec<&str> = generated
            .candidates
            .iter()
            .map(|candidate| candidate.draft_id.as_str())
            .collect();
        for source in fresh_sources.lock().unwrap().values() {
            for peer in source
                .pointer("/metadata/_seattrellis_application/candidate_peer_ids")
                .unwrap()
                .as_array()
                .unwrap()
            {
                assert!(!old_ids.contains(&peer.as_str().unwrap()));
            }
        }
    }
}
