//! Class-generation orchestration (M1-02): workbench request expansion,
//! hard-rule resolution, history forwarding, solve and draft creation.
//!
//! Business logic only: no HTTP types. Errors are [`AppError`] values the
//! transport layer maps onto HTTP.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use serde_json::{json, Value};

use crate::{AppError, SolveRequestStore};
use seattrellis_core::cost::{
    classify_seat_position, detect_neighbor_relation_types, student_pair_key,
};
use seattrellis_core::{CoreSolveRequest, CoreSolveResponse};
use seattrellis_domain::editing::{self, EditorDraftStore, EditorSeatSpec};

/// One editable candidate plan inside a class-generation outcome.
pub struct CandidateOutcome {
    pub draft_id: String,
    pub total_score: f64,
    pub recommended: bool,
}

/// The result of a class-generation request: everything the transport layer
/// needs to format one normal domain response. Solver outcomes such as
/// `ProvenInfeasible`, `Timeout` and `Unknown` are not transport errors.
pub struct GenerateClassOutcome {
    pub feasible: bool,
    pub status: seattrellis_core::SolveStatus,
    pub class_name: String,
    pub goal_id: String,
    pub total_score: Option<f64>,
    pub draft_id: Option<String>,
    /// Every candidate's editable draft (one entry for a single solve).
    pub candidates: Vec<CandidateOutcome>,
    pub recommended_candidate_id: Option<String>,
    pub editor: Option<Value>,
}

/// Orchestrate class generation: expand the workbench request (or pass a raw
/// CoreSolveRequest through), solve, open the editable draft and record the
/// request for later export. Business logic only - no HTTP types (M1-02).
pub fn generate_class(
    raw_request: &Value,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Result<GenerateClassOutcome, AppError> {
    // Expand the workbench's GenerateClassRequest into the core solve shape;
    // anything without a `draft.room.template_id` is already a CoreSolveRequest.
    let (core_request, goal_id) = if is_frontend_class_request(raw_request) {
        let goal_id = raw_request
            .pointer("/draft/goal/goal_id")
            .and_then(Value::as_str)
            .unwrap_or("daily-rotation")
            .to_string();
        (frontend_class_request_to_core(raw_request)?, goal_id)
    } else {
        (raw_request.clone(), "daily-rotation".to_string())
    };

    let request: CoreSolveRequest = match serde_json::from_value(core_request.clone()) {
        Ok(request) => request,
        Err(_) => {
            return Err(AppError::bad_request(
                "request body is not a valid solve problem",
            ))
        }
    };

    let class_name = request
        .layout
        .as_ref()
        .map(|layout| layout.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Classroom".to_string());

    let response = solve_core(&request)?;
    if !response.feasible {
        // Heuristic exhaustion, timeout and sound infeasibility are normal
        // domain results. They carry no draft/candidate payload.
        return Ok(GenerateClassOutcome {
            feasible: false,
            status: response.status,
            class_name,
            goal_id,
            total_score: None,
            draft_id: None,
            candidates: Vec::new(),
            recommended_candidate_id: None,
            editor: None,
        });
    }

    // Multi-candidate generation (M5 B5 / D5, plan §6.3): when the request
    // asks for several candidates, produce a distinct-feasible set via the
    // candidates engine (seeded repeated solve + exact-assignment exclusion)
    // instead of a single plan. The single-candidate path below keeps the
    // frozen seven-state semantics intact.
    let candidate_count = raw_request
        .pointer("/options/candidate_count")
        .and_then(Value::as_u64)
        .map(|count| count.clamp(1, 20) as usize)
        .unwrap_or(1);

    if candidate_count > 1 {
        return generate_candidate_set(
            &core_request,
            candidate_count,
            editor_store,
            solve_requests,
            class_name,
            goal_id,
        );
    }

    // Open an editable draft mirroring the recommended plan.
    let keys: Vec<String> = student_keys(&request);
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let seats = seat_specs(&request);
    let seat_ids: Vec<String> = (0..request.seat_positions.len())
        .map(|index| seat_id_for_index(&request, index))
        .collect();
    let assignment: Vec<(&str, &str)> = response
        .assignment
        .iter()
        .filter(|[student, seat]| *student < key_refs.len() && *seat < seat_ids.len())
        .map(|[student, seat]| (key_refs[*student], seat_ids[*seat].as_str()))
        .collect();

    let draft_id = new_draft_id();
    // Mirror the roster names into the editable draft (the Python oracle's
    // editor state carries `display_name`, and the workbench canvas renders
    // it), falling back to the student key when a name is absent.
    let display_names: HashMap<String, String> = request
        .students
        .iter()
        .map(|student| {
            (
                student.key.clone(),
                student
                    .display_name
                    .clone()
                    .unwrap_or_else(|| student.key.clone()),
            )
        })
        .collect();
    let editor = match editing::create_draft(
        editor_store,
        draft_id.clone(),
        Some(draft_id.clone()),
        &key_refs,
        seats,
        &assignment,
        Some(&display_names),
    ) {
        Ok(state) => state,
        Err(message) => return Err(AppError::internal(&message)),
    };

    // Remember the (core-shaped) request that produced this draft so export
    // can rebuild the full plan (request + current assignment) after edits.
    match solve_requests.lock() {
        Ok(mut guard) => {
            guard.insert(draft_id.clone(), core_request);
        }
        Err(_) => return Err(AppError::internal("solve request store is poisoned")),
    }

    Ok(GenerateClassOutcome {
        feasible: true,
        status: response.status,
        class_name,
        goal_id,
        total_score: response.total_cost,
        draft_id: Some(draft_id.clone()),
        candidates: vec![CandidateOutcome {
            draft_id: draft_id.clone(),
            total_score: response.total_cost.unwrap_or(0.0),
            recommended: true,
        }],
        recommended_candidate_id: Some(draft_id.clone()),
        editor: Some(
            serde_json::to_value(editor).map_err(|error| AppError::internal(error.to_string()))?,
        ),
    })
}

/// Execute the core solver as an application use case and independently
/// validate every successful assignment before any transport/UI consumer can
/// observe it. Non-solved statuses remain ordinary return values. Solver
/// rejections are classified onto the frozen vocabulary: input validation
/// failures surface as `InvalidInput` (400), anything else as an internal
/// error (500) — a solver bug must never masquerade as a bad request.
pub fn solve_core(request: &CoreSolveRequest) -> Result<CoreSolveResponse, AppError> {
    let response = seattrellis_core::solve_problem(request).map_err(|message| {
        match seattrellis_core::classify_solve_error(&message) {
            seattrellis_core::SolveStatus::InvalidInput => AppError::solve_invalid_input(message),
            _ => AppError::internal(format!("solver failed: {message}")),
        }
    })?;
    if response.feasible {
        seattrellis_core::validate_solve_response(request, &response).map_err(|message| {
            AppError::internal(format!(
                "solver output failed independent validation: {message}"
            ))
        })?;
    }
    Ok(response)
}

/// `true` when the body is a React workbench `GenerateClassRequest`, i.e. it
/// carries a `draft` object whose `room` selects a room template (`template_id`)
/// or an explicit custom layout (`layout`).
pub(crate) fn is_frontend_class_request(value: &Value) -> bool {
    if value
        .pointer("/draft/room/template_id")
        .and_then(Value::as_str)
        .map(|template_id| !template_id.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    value
        .pointer("/draft/room/layout")
        .is_some_and(Value::is_object)
}

/// Adapt a React `GenerateClassRequest` (`draft.students` + `draft.room`
/// template or custom layout + `draft.goal`) into the core `CoreSolveRequest`
/// JSON document, expanding the room grid and the goal rule-set, mapping each
/// student record onto the core `Student` shape (`key`/`display_name`/
/// `score`/`height_cm`/`vision`/`tags`/`needs`), deep-merging the
/// `rules_overlay`, and resolving `hard_rules` (fixed seats, adjacency pairs,
/// min-distance) from student keys and seat ids into index pairs.
///
/// Returns a `422` response naming the missing piece when the draft is
/// malformed (`invalid_class_draft`), the room template is unknown
/// (`room_not_found`) or the goal is unknown (`unknown_goal`).
pub(crate) fn frontend_class_request_to_core(value: &Value) -> Result<Value, AppError> {
    let draft = value
        .get("draft")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::unprocessable("invalid_class_draft", "missing 'draft' object"))?;
    let room = draft
        .get("room")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::unprocessable("invalid_class_draft", "missing 'draft.room' object")
        })?;
    let goal = draft
        .get("goal")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::unprocessable("invalid_class_draft", "missing 'draft.goal' object")
        })?;

    // Room source: a built-in template id or an explicit custom layout.
    let grid = if let Some(layout) = room.get("layout") {
        match seattrellis_domain::room_templates::grid_from_layout(layout) {
            Ok(grid) => grid,
            Err(message) => return Err(AppError::unprocessable("invalid_class_draft", &message)),
        }
    } else {
        let template_id = room
            .get("template_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AppError::unprocessable("invalid_class_draft", "missing draft.room.template_id")
            })?;
        match seattrellis_domain::room_templates::room_template_grid(template_id) {
            Ok(grid) => grid,
            Err(message) => return Err(AppError::unprocessable("room_not_found", &message)),
        }
    };

    let goal_id = goal
        .get("goal_id")
        .and_then(Value::as_str)
        .map(|id| id.trim().to_ascii_lowercase().replace('_', "-"))
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| "daily-rotation".to_string());

    // Base rules: the custom goal's full document, or the goal preset.
    let mut rules: Value = if goal_id == "custom" {
        match goal
            .get("custom_rules")
            .filter(|value| !value.is_null() && value.is_object())
        {
            Some(custom_rules) => custom_rules.clone(),
            None => {
                return Err(AppError::unprocessable(
                    "invalid_class_draft",
                    "the custom goal requires draft.goal.custom_rules",
                ))
            }
        }
    } else {
        match seattrellis_domain::goal_rules::goal_rules(&goal_id) {
            Ok(rules) => rules,
            Err(message) => return Err(AppError::unprocessable("unknown_goal", &message)),
        }
    };

    // Deep-merge the partial rules_overlay (soft weights + groups) on top of
    // the base rules, mirroring `presets._deep_merge`.
    if let Some(overlay) = goal.get("rules_overlay") {
        if !overlay.is_object() {
            return Err(AppError::unprocessable(
                "invalid_class_draft",
                "rules_overlay must be an object",
            ));
        }
        deep_merge_value(&mut rules, overlay);
    }

    let students: Vec<Value> = draft
        .get("students")
        .and_then(Value::as_array)
        .map(|students| students.iter().map(core_student_value).collect())
        .unwrap_or_default();

    let options = value.get("options").and_then(Value::as_object);
    let seed = options
        .and_then(|options| options.get("seed"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SEED);

    // The solver, the editor-draft builder and the renderer all assume
    // `layout.seats[index]` aligns one-to-one with `seat_positions[index]`.
    // The grid's full layout also carries disabled cells, so hand the request
    // the *enabled* seats in layout order (which `room_templates` guarantees
    // are exactly `seat_positions`, in order).
    let layout = seattrellis_domain::room_templates::Layout {
        layout_id: grid.layout.layout_id.clone(),
        name: grid.layout.name.clone(),
        seats: grid
            .layout
            .enabled_seats()
            .iter()
            .map(|seat| (*seat).clone())
            .collect(),
        adjacency: grid.layout.adjacency.clone(),
    };

    // Resolve teacher-entered hard rules (student keys + seat ids) into the
    // core request's index pairs, mirroring `_append_hard_rules`.
    let ResolvedHardRules {
        fixed_seats,
        must_be_adjacent,
        cannot_be_adjacent,
        min_distance,
    } = resolve_hard_rules(goal, &students, &grid)?;

    // Forward history snapshots so fair_rotation / recent-neighbor costs see
    // past placements (mirrors `history.build_seat_history` +
    // `history.build_pair_history`).
    let history_snapshots: Vec<Value> = draft
        .get("history_snapshots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (history, pair_history) = build_history_json(&students, &grid, &history_snapshots)
        .unwrap_or((Value::Null, Value::Null));

    let mut request = json!({
        "api_version": 2,
        "student_count": students.len(),
        "seat_positions": grid.seat_positions.clone(),
        "edges": grid.edges.clone(),
        "layout": layout,
        "rules": rules,
        "students": students,
        "seed": seed,
        "fixed_seats": fixed_seats,
        "must_be_adjacent": must_be_adjacent,
        "cannot_be_adjacent": cannot_be_adjacent,
        "min_distance": min_distance,
    });
    if !history.is_null() {
        request["history"] = history;
        request["pair_history"] = pair_history;
    }
    Ok(request)
}

/// Hard rules resolved to core index pairs.
struct ResolvedHardRules {
    fixed_seats: Vec<[usize; 2]>,
    must_be_adjacent: Vec<[usize; 2]>,
    cannot_be_adjacent: Vec<[usize; 2]>,
    min_distance: Vec<Value>,
}

/// Resolve `draft.goal.hard_rules` (student-key + seat-id references) into
/// core index pairs. Missing or unresolvable references are a 422
/// `invalid_class_draft`, mirroring strict Python rule compilation.
fn resolve_hard_rules(
    goal: &serde_json::Map<String, Value>,
    students: &[Value],
    grid: &seattrellis_domain::room_templates::RoomGrid,
) -> Result<ResolvedHardRules, AppError> {
    let Some(hard) = goal.get("hard_rules").filter(|value| !value.is_null()) else {
        return Ok(ResolvedHardRules {
            fixed_seats: Vec::new(),
            must_be_adjacent: Vec::new(),
            cannot_be_adjacent: Vec::new(),
            min_distance: Vec::new(),
        });
    };
    if !hard.is_object() {
        return Err(AppError::unprocessable(
            "invalid_class_draft",
            "hard_rules must be an object",
        ));
    }

    let student_index: HashMap<&str, usize> = students
        .iter()
        .enumerate()
        .filter_map(|(index, student)| core_student_key(student).map(|key| (key, index)))
        .collect();
    let seat_index: HashMap<&str, usize> = grid
        .layout
        .enabled_seats()
        .iter()
        .enumerate()
        .map(|(index, seat)| (seat.seat_id.as_str(), index))
        .collect();

    let mut fixed_seats: Vec<[usize; 2]> = Vec::new();
    for entry in hard
        .get("fixed_seats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let student = entry.get("student").and_then(Value::as_str).unwrap_or("");
        let seat_id = entry.get("seat_id").and_then(Value::as_str).unwrap_or("");
        let student_index = *student_index.get(student).ok_or_else(|| {
            AppError::unprocessable(
                "invalid_class_draft",
                format!("hard rule references unknown student {student:?}"),
            )
        })?;
        let seat_index = *seat_index.get(seat_id).ok_or_else(|| {
            AppError::unprocessable(
                "invalid_class_draft",
                format!("hard rule references unknown seat {seat_id:?}"),
            )
        })?;
        fixed_seats.push([student_index, seat_index]);
    }

    let mut must_be_adjacent: Vec<[usize; 2]> = Vec::new();
    let mut cannot_be_adjacent: Vec<[usize; 2]> = Vec::new();
    for (field, out) in [
        ("must_be_adjacent", &mut must_be_adjacent),
        ("cannot_be_adjacent", &mut cannot_be_adjacent),
    ] {
        for entry in hard
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let pair = resolve_student_pair(entry, &student_index)?;
            out.push(pair);
        }
    }

    let mut min_distance: Vec<Value> = Vec::new();
    for entry in hard
        .get("min_distance")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let pair = resolve_student_pair(entry, &student_index)?;
        let distance = entry
            .get("distance")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                AppError::unprocessable(
                    "invalid_class_draft",
                    "min_distance needs a positive distance",
                )
            })?;
        let metric = match entry.get("metric").and_then(Value::as_str) {
            Some("euclidean") => "euclidean",
            _ => "graph",
        };
        min_distance.push(json!({
            "students": pair,
            "distance": distance,
            "metric": metric,
        }));
    }

    Ok(ResolvedHardRules {
        fixed_seats,
        must_be_adjacent,
        cannot_be_adjacent,
        min_distance,
    })
}

/// Resolve one `{ "students": [keyA, keyB] }` hard-rule entry into a
/// normalized student-index pair.
fn resolve_student_pair(
    entry: &Value,
    student_index: &HashMap<&str, usize>,
) -> Result<[usize; 2], AppError> {
    let names = entry
        .get("students")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::unprocessable("invalid_class_draft", "pair rule needs a 'students' array")
        })?;
    let first = names.first().and_then(Value::as_str).unwrap_or("");
    let second = names.get(1).and_then(Value::as_str).unwrap_or("");
    let first_index = *student_index.get(first).ok_or_else(|| {
        AppError::unprocessable(
            "invalid_class_draft",
            format!("pair rule references unknown student {first:?}"),
        )
    })?;
    let second_index = *student_index.get(second).ok_or_else(|| {
        AppError::unprocessable(
            "invalid_class_draft",
            format!("pair rule references unknown student {second:?}"),
        )
    })?;
    if first_index == second_index {
        return Err(AppError::unprocessable(
            "invalid_class_draft",
            "a pair rule must reference two different students",
        ));
    }
    Ok([first_index.min(second_index), first_index.max(second_index)])
}

/// The core student key for a `draft.students` entry (mirrors
/// [`core_student_value`]: `student_id` if present, else `name`, falling back
/// to the already-mapped `key` when the record is core-shaped).
pub(crate) fn core_student_key(student: &Value) -> Option<&str> {
    let student_id = student
        .get("student_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = student.get("name").and_then(Value::as_str).unwrap_or("");
    if !student_id.is_empty() {
        Some(student_id)
    } else if !name.is_empty() {
        Some(name)
    } else {
        student
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
    }
}

/// Recursive deep merge: object values merge key-by-key, any other value
/// replaces the target (mirrors `presets._deep_merge`).
fn deep_merge_value(target: &mut Value, patch: &Value) {
    let (Some(target_object), Some(patch_object)) = (target.as_object_mut(), patch.as_object())
    else {
        *target = patch.clone();
        return;
    };
    for (key, patch_value) in patch_object {
        match target_object.get_mut(key) {
            Some(existing) => deep_merge_value(existing, patch_value),
            None => {
                target_object.insert(key.clone(), patch_value.clone());
            }
        }
    }
}

/// Build the core `history` and `pair_history` JSON documents from the
/// frontend's `draft.history_snapshots`, mirroring
/// `history.build_seat_history` and `history.build_pair_history`: per-student
/// seat-category counts and records for fair rotation, plus per-pair relation
/// records for recent-neighbor avoidance. Returns `None` when there are no
/// snapshots.
///
/// Snapshot assignments that reference a student outside the current roster or
/// an unknown seat are skipped, exactly like Python's missing-student handling.
pub fn build_history_json(
    students: &[Value],
    grid: &seattrellis_domain::room_templates::RoomGrid,
    snapshots: &[Value],
) -> Option<(Value, Value)> {
    if snapshots.is_empty() {
        return None;
    }
    let core_layout: seattrellis_core::models::Layout =
        serde_json::from_value(serde_json::to_value(&grid.layout).ok()?).ok()?;
    let current_keys: std::collections::HashSet<&str> =
        students.iter().filter_map(core_student_key).collect();
    let seat_by_id: HashMap<&str, &seattrellis_core::models::Seat> = core_layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();

    let mut student_histories: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut pair_histories: serde_json::Map<String, Value> = serde_json::Map::new();

    for snapshot in snapshots {
        let Some(assignments) = snapshot.get("assignments").and_then(Value::as_array) else {
            continue;
        };
        // One shared view of the known (current-student -> seat) assignments so
        // the per-student and per-pair records describe the same snapshot.
        let mut known: Vec<(&str, &seattrellis_core::models::Seat)> = Vec::new();
        for assignment in assignments {
            let Some(student_key) = assignment.get("student_key").and_then(Value::as_str) else {
                continue;
            };
            if !current_keys.contains(student_key) {
                continue;
            }
            let Some(seat_id) = assignment.get("seat_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(seat) = seat_by_id.get(seat_id) else {
                continue;
            };
            known.push((student_key, seat));
        }

        for (student_key, seat) in &known {
            let categories = classify_seat_position(seat, &core_layout);
            let entry = student_histories
                .entry((*student_key).to_string())
                .or_insert_with(|| json!({ "category_counts": {}, "records": [] }));
            if let Some(counts) = entry
                .get_mut("category_counts")
                .and_then(Value::as_object_mut)
            {
                for category in &categories {
                    let current = counts.get(category).and_then(Value::as_u64).unwrap_or(0);
                    counts.insert(category.clone(), json!(current + 1));
                }
            }
            if let Some(records) = entry.get_mut("records").and_then(Value::as_array_mut) {
                let sorted: Vec<String> = {
                    let mut list: Vec<String> = categories.into_iter().collect();
                    list.sort();
                    list
                };
                records.push(json!({ "categories": sorted }));
            }
        }

        for first in 0..known.len() {
            for second in (first + 1)..known.len() {
                let (first_key, first_seat) = known[first];
                let (second_key, second_seat) = known[second];
                let relations =
                    detect_neighbor_relation_types(first_seat, second_seat, &core_layout, None, 2);
                if relations.is_empty() {
                    continue;
                }
                let pair_key = student_pair_key(first_key, second_key);
                let entry = pair_histories
                    .entry(pair_key)
                    .or_insert_with(|| json!({ "records": [] }));
                if let Some(records) = entry.get_mut("records").and_then(Value::as_array_mut) {
                    let sorted: Vec<String> = {
                        let mut list: Vec<String> = relations.into_iter().collect();
                        list.sort();
                        list
                    };
                    records.push(json!({ "relations": sorted }));
                }
            }
        }
    }

    let history = json!({
        "history_count": snapshots.len(),
        "students": student_histories,
    });
    let pair_history = json!({
        "history_count": snapshots.len(),
        "within_distance_metric": "graph",
        "within_distance": 2,
        "pairs": pair_histories,
    });
    Some((history, pair_history))
}

/// Default solve seed when the frontend sends no `options.seed` (matches the
/// rule-set default in `goal_rules.rs` / the core `RuleSet` model).
pub(crate) const DEFAULT_SEED: u64 = 42;

/// Map one React `draft.students` entry onto the core `Student` JSON shape.
/// Absent or `null` fields are omitted so they deserialize to the core
/// defaults; `vision` follows the core convention of storing its string
/// rendering (`0.8` -> `"0.8"`, `"poor"` -> `"poor"`).
fn core_student_value(student: &Value) -> Value {
    let mut result = serde_json::Map::new();
    // The core `key` mirrors Python's `student_id or name or ""`.
    let student_id = student
        .get("student_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = student.get("name").and_then(Value::as_str).unwrap_or("");
    let key = if !student_id.is_empty() {
        student_id
    } else {
        name
    };
    if !key.is_empty() {
        result.insert("key".to_string(), json!(key));
    }
    if !name.is_empty() {
        result.insert("display_name".to_string(), json!(name));
    }
    if let Some(score) = student.get("score").and_then(Value::as_f64) {
        result.insert("score".to_string(), json!(score));
    }
    if let Some(height_cm) = student.get("height_cm").and_then(Value::as_f64) {
        result.insert("height_cm".to_string(), json!(height_cm));
    }
    if let Some(vision) = student.get("vision") {
        match vision {
            Value::String(text) if !text.is_empty() => {
                result.insert("vision".to_string(), json!(text));
            }
            Value::Number(number) => {
                result.insert("vision".to_string(), json!(number.to_string()));
            }
            _ => {}
        }
    }
    if let Some(tags) = student.get("tags").and_then(Value::as_array) {
        result.insert("tags".to_string(), json!(tags));
    }
    if let Some(needs) = student.get("needs").and_then(Value::as_array) {
        result.insert("needs".to_string(), json!(needs));
    }
    Value::Object(result)
}

/// Student keys for an editor draft: the solve request's `students` `key`,
/// falling back to `student-N` for placeholder/padded students.
pub fn student_keys(request: &CoreSolveRequest) -> Vec<String> {
    (0..request.student_count)
        .map(|index| {
            request
                .students
                .get(index)
                .map(|student| student.key.trim())
                .filter(|key| !key.is_empty())
                .map(|key| key.to_string())
                .unwrap_or_else(|| format!("student-{}", index + 1))
        })
        .collect()
}

/// Seat specs for an editor draft: prefer the layout's authoritative
/// row/col/enabled per seat; otherwise derive grid coordinates from the raw
/// `seat_positions` (mirrors `render::seat_row_col`).
pub fn seat_specs(request: &CoreSolveRequest) -> Vec<EditorSeatSpec> {
    request
        .seat_positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let (row, col, enabled) = match request.layout.as_ref() {
                Some(layout) => match layout.seats.get(index) {
                    Some(seat) => (seat.row, seat.col, seat.enabled),
                    None => fallback_coordinates(position),
                },
                None => fallback_coordinates(position),
            };
            EditorSeatSpec {
                seat_id: seat_id_for_index(request, index),
                row,
                col,
                enabled,
            }
        })
        .collect()
}

/// The seat id the editor draft uses for a seat index: the layout's `seat_id`
/// when present, else `seat-N`.
pub fn seat_id_for_index(request: &CoreSolveRequest, index: usize) -> String {
    request
        .layout
        .as_ref()
        .and_then(|layout| layout.seats.get(index))
        .map(|seat| seat.seat_id.clone())
        .unwrap_or_else(|| format!("seat-{}", index + 1))
}

fn fallback_coordinates(position: &[f64; 2]) -> (i32, i32, bool) {
    (position[1].round() as i32, position[0].round() as i32, true)
}

/// `POST /api/v1/rosters/drafts`: parse a multipart `file` field and store the
static DRAFT_SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) fn new_draft_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = DRAFT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("draft-{nanos:x}{seq:x}")
}

/// Produce `candidate_count` distinct feasible plans (plan §6.3): each
/// candidate becomes an editable draft; every draft remembers its originating
/// request so export and audit can rebuild it after edits. The recommended
/// candidate matches the engine's PlanScore recommendation (D5).
fn generate_candidate_set(
    core_request: &Value,
    candidate_count: usize,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
    class_name: String,
    goal_id: String,
) -> Result<GenerateClassOutcome, AppError> {
    let request: CoreSolveRequest = match serde_json::from_value(core_request.clone()) {
        Ok(request) => request,
        Err(_) => {
            return Err(AppError::bad_request(
                "request body is not a valid solve problem",
            ))
        }
    };
    // The stored value is already the core-shaped request; serialize it as-is
    // (CoreSolveRequest is Deserialize-only by contract).
    let request_json = core_request.to_string();
    let report_json =
        match seattrellis_core::generate_candidates_json(&request_json, candidate_count) {
            Ok(report) => report,
            Err(message) if message.contains("did not produce any feasible plan") => {
                // Heuristic exhaustion across the requested set is a normal
                // domain result, never a transport error (M0-03).
                return Ok(GenerateClassOutcome {
                    feasible: false,
                    status: seattrellis_core::SolveStatus::Unknown,
                    class_name,
                    goal_id,
                    total_score: None,
                    draft_id: None,
                    candidates: Vec::new(),
                    recommended_candidate_id: None,
                    editor: None,
                });
            }
            Err(message) => {
                return Err(AppError::unprocessable(
                    "invalid_class_draft",
                    format!("candidate generation failed: {message}"),
                ))
            }
        };
    let report: Value = match serde_json::from_str(&report_json) {
        Ok(report) => report,
        Err(error) => {
            return Err(AppError::internal(format!(
                "candidate report is malformed: {error}"
            )))
        }
    };
    let Some(candidates) = report.get("candidates").and_then(Value::as_array) else {
        return Err(AppError::internal("candidate report has no candidates"));
    };
    let recommended_id = report
        .get("recommended_candidate_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let keys: Vec<String> = student_keys(&request);
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let seats = seat_specs(&request);
    let seat_ids: Vec<String> = (0..request.seat_positions.len())
        .map(|index| seat_id_for_index(&request, index))
        .collect();
    let display_names: HashMap<String, String> = request
        .students
        .iter()
        .map(|student| {
            (
                student.key.clone(),
                student
                    .display_name
                    .clone()
                    .unwrap_or_else(|| student.key.clone()),
            )
        })
        .collect();

    let mut outcomes: Vec<CandidateOutcome> = Vec::with_capacity(candidates.len());
    let mut recommended_state: Option<Value> = None;
    let mut recommended_draft: Option<String> = None;
    for candidate in candidates {
        let engine_id = candidate
            .get("candidate_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let pairs: Vec<[usize; 2]> = candidate
            .get("assignment")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|pair| pair.as_array())
                    .filter_map(|pair| {
                        Some([
                            pair.first()?.as_u64()? as usize,
                            pair.get(1)?.as_u64()? as usize,
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        let assignment: Vec<(&str, &str)> = pairs
            .iter()
            .filter(|[student, seat]| *student < key_refs.len() && *seat < seat_ids.len())
            .map(|[student, seat]| (key_refs[*student], seat_ids[*seat].as_str()))
            .collect();

        let draft_id = new_draft_id();
        let state = match editing::create_draft(
            editor_store,
            draft_id.clone(),
            Some(engine_id.clone()),
            &key_refs,
            seats.clone(),
            &assignment,
            Some(&display_names),
        ) {
            Ok(state) => state,
            Err(message) => return Err(AppError::internal(&message)),
        };
        match solve_requests.lock() {
            Ok(mut guard) => {
                guard.insert(draft_id.clone(), core_request.clone());
            }
            Err(_) => return Err(AppError::internal("solve request store is poisoned")),
        }
        let total_score = candidate
            .get("plan_score")
            .and_then(|score| score.get("total"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let recommended = engine_id == recommended_id;
        if recommended {
            recommended_draft = Some(draft_id.clone());
            recommended_state = Some(
                serde_json::to_value(state)
                    .map_err(|error| AppError::internal(error.to_string()))?,
            );
        }
        outcomes.push(CandidateOutcome {
            draft_id,
            total_score,
            recommended,
        });
    }
    let Some(editor) = recommended_state else {
        return Err(AppError::internal("candidate set has no recommended plan"));
    };

    Ok(GenerateClassOutcome {
        feasible: true,
        status: seattrellis_core::SolveStatus::Solved,
        class_name,
        goal_id,
        total_score: None,
        draft_id: None,
        candidates: outcomes,
        recommended_candidate_id: recommended_draft,
        editor: Some(editor),
    })
}
