//! Rotation plan generation (M2 parity, ledger A.1): sequential per-period
//! solves where every later period's history includes all earlier periods,
//! mirroring Python's `compute_rotation_plan` (service.py:178). Business
//! logic only — no HTTP types.

use std::collections::HashMap;

use serde_json::{json, Value};

use seattrellis_domain::room_templates::grid_from_layout;

use crate::class_generation::{
    build_history_json, frontend_class_request_to_core, new_draft_id, seat_id_for_index,
    seat_specs, student_keys, DEFAULT_SEED,
};
use crate::{store_solve_request, AppError, SolveRequestStore};
use seattrellis_domain::editing::{self, EditorDraftStore};

/// The result of a rotation-plan request: the plan document plus an editable
/// draft per period (the transport formats the response).
pub struct GenerateRotationOutcome {
    pub feasible: bool,
    pub status: seattrellis_core::SolveStatus,
    pub class_name: String,
    pub warnings: Vec<String>,
    pub plan: Option<Value>,
    pub editor: Option<Value>,
    pub period_editors: Option<Vec<Value>>,
    pub failed_period: Option<usize>,
}

/// Generate a rotation plan: solve each period with the accumulated history
/// of all previous periods (fair rotation + recent-neighbor costs), then
/// summarize fairness and pair repetition across the plan.
pub fn generate_rotation_plan(
    raw_request: &Value,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Result<GenerateRotationOutcome, AppError> {
    // Expand the workbench request exactly like class generation.
    let core_request = frontend_class_request_to_core(raw_request)?;
    let period_count = match raw_request.get("period_count") {
        None => 4,
        Some(value) => value
            .as_u64()
            .filter(|count| (1..=20).contains(count))
            .map(|count| count as usize)
            .ok_or_else(|| {
                AppError::unprocessable(
                    "invalid_rotation",
                    "period_count must be an integer between 1 and 20",
                )
            })?,
    };
    let labels: Vec<String> = match raw_request.get("period_labels") {
        None => Vec::new(),
        Some(Value::Array(values)) => {
            let labels = values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::trim)
                        .filter(|label| !label.is_empty())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            AppError::unprocessable(
                                "invalid_rotation",
                                "period_labels must contain non-empty strings",
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !labels.is_empty() && labels.len() != period_count {
                return Err(AppError::unprocessable(
                    "invalid_rotation",
                    "period_labels must be empty or match period_count",
                ));
            }
            labels
        }
        Some(_) => {
            return Err(AppError::unprocessable(
                "invalid_rotation",
                "period_labels must be an array",
            ))
        }
    };
    let base_seed = raw_request
        .pointer("/options/seed")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SEED);
    let name = raw_request
        .pointer("/draft/name")
        .and_then(Value::as_str)
        .unwrap_or("SeatTrellis Rotation Plan")
        .to_string();
    let base_snapshots: Vec<Value> = raw_request
        .pointer("/draft/history_snapshots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    generate_rotation_plan_from_core(
        &core_request,
        RotationOptions {
            period_count,
            labels,
            base_seed,
            plan_name: name,
            base_snapshots,
        },
        editor_store,
        solve_requests,
    )
}

/// Per-run rotation options shared by the frontend-shaped and core-shaped
/// entry points.
pub struct RotationOptions {
    pub period_count: usize,
    pub labels: Vec<String>,
    pub base_seed: u64,
    pub plan_name: String,
    /// Base history snapshots fed into the first period (frontend drafts
    /// only; the CLI's project-rotate passes none).
    pub base_snapshots: Vec<Value>,
}

/// Rotation generation from an already-compiled `CoreSolveRequest` JSON
/// document (the CLI's project-rotate entry point). The frontend-shaped
/// request and the core-shaped request share this path, so every rotation
/// product — wherever it is generated — goes through the same solver +
/// independent-validation loop.
pub fn generate_rotation_plan_from_core(
    core_request: &Value,
    options: RotationOptions,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Result<GenerateRotationOutcome, AppError> {
    let RotationOptions {
        period_count,
        labels,
        base_seed,
        plan_name,
        base_snapshots,
    } = options;
    let request: seattrellis_core::CoreSolveRequest = serde_json::from_value(core_request.clone())
        .map_err(|_| AppError::bad_request("request body is not a valid solve problem"))?;

    // Rebuild the grid and student records for history accumulation; the
    // base snapshots come from the draft exactly as class generation sees them.
    let grid = grid_from_layout(
        core_request
            .get("layout")
            .ok_or_else(|| AppError::bad_request("request body has no layout"))?,
    )
    .map_err(AppError::bad_request)?;
    let students: Vec<Value> = core_request
        .get("students")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut snapshots = base_snapshots.clone();
    let mut periods = Vec::with_capacity(period_count);
    let warnings: Vec<String> = Vec::new();
    let mut period_assignments: Vec<(usize, Vec<[usize; 2]>)> = Vec::with_capacity(period_count);
    let class_name = request
        .layout
        .as_ref()
        .map(|layout| layout.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Classroom".to_string());

    for period in 1..=period_count {
        let label = labels
            .get(period - 1)
            .cloned()
            .unwrap_or_else(|| format!("Period {period}"));
        let mut period_request = core_request.clone();
        period_request["seed"] = json!(base_seed + period as u64 - 1);
        if !snapshots.is_empty() {
            if let Some((history, pair_history)) = build_history_json(&students, &grid, &snapshots)
            {
                period_request["history"] = history;
                period_request["pair_history"] = pair_history;
            }
        }

        let typed_period_request: seattrellis_core::CoreSolveRequest =
            serde_json::from_value(period_request).map_err(|_| {
                AppError::internal("rotation produced a malformed period solve request")
            })?;
        // The shared solve use case (solver + independent validation) keeps
        // every rotation period on the same path as /api/v2/solve.
        let response = crate::class_generation::solve_core(&typed_period_request)?;
        if !response.feasible {
            return Ok(GenerateRotationOutcome {
                feasible: false,
                status: response.status,
                class_name,
                warnings,
                plan: None,
                editor: None,
                period_editors: None,
                failed_period: Some(period),
            });
        }

        let snapshot = build_period_snapshot(&typed_period_request, &response, period, &label);
        snapshots.push(snapshot.clone());
        periods.push(json!({ "period": period, "label": label, "snapshot": snapshot }));
        period_assignments.push((period, response.assignment.clone()));
    }

    // Build the full-plan history (base + every generated period) once, so
    // the fairness and pair-repeat summaries cover the whole plan exactly
    // like Python's post-generation `build_seat_history` report.
    let (final_history, final_pair_history) =
        build_history_json(&students, &grid, &snapshots).unwrap_or((Value::Null, Value::Null));
    // The fairness spread is computed over the whole roster: a student who
    // never reached a category must count as 0 there (oracle parity), so
    // the roster keys are needed at summary time.
    let keys: Vec<String> = student_keys(&request);
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let fairness_summary = fairness_summary_from_history(&final_history, snapshots.len(), &keys);
    let pair_summary = pair_repeat_summary_from_history(&final_pair_history, snapshots.len());

    let plan = json!({
        // Mirror the oracle artifact contract: Python writes
        // ROTATION_PLAN_SCHEMA_VERSION = "1.0" (schema.py:14); the v1
        // rotation-plan.schema.json declares the same default. The old
        // "0.2.2" string was copied from the candidate-set contract and
        // made Rust rotation plans invalid against the oracle schema
        // (ledger §19.33).
        "schema_version": "1.0",
        "kind": "rotation_plan",
        "name": plan_name,
        "periods": periods,
        "base_history_count": base_snapshots.len(),
        "fairness_summary": fairness_summary,
        "pair_repeat_summary": pair_summary,
        "warnings": warnings,
        "metadata": {
            "period_count": period_count,
            "backend": "native",
            "seed": base_seed,
        },
    });

    // Open an editable draft per period from the already validated solver
    // assignments. Do not reconstruct a synthetic `Solved` response from the
    // presentation snapshot: that would discard validation evidence. The
    // first period's draft is also the response's `editor`; the workbench
    // switches periods by matching `candidate_id == "period-N"`.
    let draft_id = new_draft_id();
    let seat_ids: Vec<String> = (0..request.seat_positions.len())
        .map(|index| seat_id_for_index(&request, index))
        .collect();
    let seats = seat_specs(&request);
    let display_names = rotation_display_names(&request);

    let mut period_editors: Vec<Value> = Vec::with_capacity(period_count);
    let mut first_editor: Option<Value> = None;
    for (period, assignment) in &period_assignments {
        let period_draft_id = if *period == 1 {
            draft_id.clone()
        } else {
            new_draft_id()
        };
        let period_assignment: Vec<(&str, &str)> = assignment
            .iter()
            .filter(|[student, seat]| *student < key_refs.len() && *seat < seat_ids.len())
            .map(|[student, seat]| (key_refs[*student], seat_ids[*seat].as_str()))
            .collect();
        let editor = match editing::create_draft(
            editor_store,
            period_draft_id.clone(),
            Some(format!("period-{period}")),
            &key_refs,
            seats.clone(),
            &period_assignment,
            Some(&display_names),
        ) {
            Ok(state) => state,
            Err(message) => return Err(AppError::internal(&message)),
        };
        // Remember the (core-shaped) request that produced this draft so
        // export can rebuild the full plan after edits. Route through the
        // capped FIFO store (`store_solve_request`): a direct insert here
        // would let long rotation runs accumulate PII-bearing requests
        // without bound.
        store_solve_request(solve_requests, period_draft_id, core_request.clone())
            .map_err(AppError::internal)?;
        let editor_value =
            serde_json::to_value(editor).map_err(|error| AppError::internal(error.to_string()))?;
        if first_editor.is_none() {
            first_editor = Some(editor_value.clone());
        }
        period_editors.push(editor_value);
    }
    let editor =
        first_editor.ok_or_else(|| AppError::internal("rotation plan has no validated periods"))?;

    Ok(GenerateRotationOutcome {
        feasible: true,
        status: seattrellis_core::SolveStatus::Solved,
        class_name,
        warnings,
        plan: Some(plan),
        editor: Some(editor),
        period_editors: Some(period_editors),
        failed_period: None,
    })
}

/// Roster display names (`key -> display_name`) from the solve request, so
/// the editable draft for period 1 renders the roster names (Python parity).
fn rotation_display_names(request: &seattrellis_core::CoreSolveRequest) -> HashMap<String, String> {
    request
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
        .collect()
}

/// Build one period snapshot in the frontend-consumable shape:
/// `{assignments: [{student_key, student_name, seat_id}], solver_status,
/// seed, metadata: {rotation_period, rotation_label}}`.
fn build_period_snapshot(
    request: &seattrellis_core::CoreSolveRequest,
    response: &seattrellis_core::CoreSolveResponse,
    period: usize,
    label: &str,
) -> Value {
    let keys = student_keys(request);
    let assignments: Vec<Value> = response
        .assignment
        .iter()
        .filter(|[student, seat]| *student < keys.len() && *seat < request.seat_positions.len())
        .map(|[student, seat]| {
            let student_key = keys[*student].clone();
            let student_name = request
                .students
                .get(*student)
                .and_then(|student| student.display_name.clone())
                .unwrap_or_else(|| student_key.clone());
            json!({
                "student_key": student_key,
                "student_name": student_name,
                "seat_id": seat_id_for_index(request, *seat),
            })
        })
        .collect();
    json!({
        "assignments": assignments,
        "solver_status": response.status.as_str(),
        "seed": request.seed,
        "metadata": {
            "rotation_period": period,
            "rotation_label": label,
        },
    })
}

/// Canonical position-report categories for the fairness summary, mirroring
/// the Python oracle's `POSITION_REPORT_CATEGORIES` (core's
/// `REPORT_POSITION_CATEGORIES` is crate-private there, so the list is
/// repeated at this call boundary).
const ROTATION_POSITION_CATEGORIES: [&str; 10] = [
    "front",
    "back",
    "middle",
    "side",
    "corner",
    "near_window",
    "near_door",
    "near_platform",
    "near_ac",
    "unknown",
];

/// Fairness summary from the final history document (mirrors
/// `history.build_fairness_report`): per-category totals plus per-category
/// min/max/spread across the **whole roster** — every (student, category)
/// cell absent from the history document counts as 0 participation (the
/// oracle's `category_counts.get(key, 0)`), so students who never reached a
/// category pull its minimum down instead of being silently skipped.
fn fairness_summary_from_history(
    history: &Value,
    snapshot_count: usize,
    roster_keys: &[String],
) -> Value {
    if history.is_null() {
        return json!({
            "history_count": snapshot_count,
            "student_count": 0,
            "category_totals": {},
            "summary": { "warning_count": 0 },
        });
    }
    let students = history.get("students").and_then(Value::as_object);
    let student_count = students.map(serde_json::Map::len).unwrap_or(0);

    // Per-category totals: sum every student's recorded category_counts.
    let mut totals: std::collections::BTreeMap<String, u64> = Default::default();
    for student in students.into_iter().flat_map(|students| students.values()) {
        let Some(counts) = student.get("category_counts").and_then(Value::as_object) else {
            continue;
        };
        for (category, count) in counts {
            *totals.entry(category.clone()).or_default() += count.as_u64().unwrap_or(0);
        }
    }

    // Per-category spread over the whole roster; missing cells are 0.
    let count_of = |student_key: &str, category: &str| -> u64 {
        students
            .and_then(|students| students.get(student_key))
            .and_then(|student| student.get("category_counts"))
            .and_then(|counts| counts.get(category))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let mut category_spread: serde_json::Map<String, Value> = serde_json::Map::new();
    for category in ROTATION_POSITION_CATEGORIES {
        let counts: Vec<u64> = roster_keys
            .iter()
            .map(|key| count_of(key, category))
            .collect();
        // An empty roster mirrors the oracle's empty-counts branch.
        let min = counts.iter().copied().min().unwrap_or(0);
        let max = counts.iter().copied().max().unwrap_or(0);
        category_spread.insert(
            category.to_string(),
            json!({ "min": min, "max": max, "spread": max - min }),
        );
    }

    json!({
        "history_count": history
            .get("history_count")
            .and_then(Value::as_u64)
            .unwrap_or(snapshot_count as u64),
        "student_count": student_count,
        "category_totals": totals,
        "summary": { "category_spread": category_spread, "warning_count": 0 },
    })
}

/// Pair-repeat summary from the final pair-history document (mirrors
/// `history.build_pair_history_report`): total pairs, repeated pairs, max
/// occurrences and per-relation totals.
fn pair_repeat_summary_from_history(pair_history: &Value, snapshot_count: usize) -> Value {
    if pair_history.is_null() {
        return json!({
            "history_count": snapshot_count,
            "pair_count": 0,
            "repeated_pair_count": 0,
            "max_occurrences": 0,
            "relation_totals": {},
        });
    }
    let pairs = pair_history
        .get("pairs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut relation_totals: std::collections::BTreeMap<String, u64> = Default::default();
    let mut repeated = 0;
    let mut max_occurrences = 0;
    for pair in pairs.values() {
        let Some(records) = pair.get("records").and_then(Value::as_array) else {
            continue;
        };
        let occurrences = records.len() as u64;
        max_occurrences = max_occurrences.max(occurrences);
        if occurrences > 1 {
            repeated += 1;
        }
        for record in records {
            let Some(relations) = record.get("relations").and_then(Value::as_array) else {
                continue;
            };
            for relation in relations {
                if let Some(relation) = relation.as_str() {
                    *relation_totals.entry(relation.to_string()).or_default() += 1;
                }
            }
        }
    }
    json!({
        "history_count": pair_history
            .get("history_count")
            .and_then(Value::as_u64)
            .unwrap_or(snapshot_count as u64),
        "pair_count": pairs.len(),
        "repeated_pair_count": repeated,
        "max_occurrences": max_occurrences,
        "relation_totals": relation_totals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O regression: a student absent from a category must count as 0
    /// participation (oracle `category_counts.get(key, 0)`), so the spread
    /// reflects them instead of only the students who appear in the history.
    #[test]
    fn fairness_summary_counts_absent_students_as_zero_participants() {
        let history = json!({
            "history_count": 2,
            "students": {
                "S1": { "category_counts": { "front": 1 } },
                "S2": { "category_counts": {} },
                "S3": { "category_counts": {} },
            },
        });
        let roster: Vec<String> = ["S1", "S2", "S3"]
            .iter()
            .map(|key| key.to_string())
            .collect();
        let summary = fairness_summary_from_history(&history, 2, &roster);
        let front = &summary["summary"]["category_spread"]["front"];
        assert_eq!(front["min"], 0, "absent students pull the minimum to 0");
        assert_eq!(front["max"], 1);
        assert_eq!(front["spread"], 1);
        // Totals keep their recorded-count semantics.
        assert_eq!(summary["category_totals"]["front"], 1);
    }

    #[test]
    fn fairness_summary_spreads_every_canonical_category() {
        let history = json!({
            "history_count": 1,
            "students": {
                "S1": { "category_counts": { "front": 3, "side": 1 } },
            },
        });
        let roster: Vec<String> = vec!["S1".to_string()];
        let summary = fairness_summary_from_history(&history, 1, &roster);
        let spread = &summary["summary"]["category_spread"];
        for category in ROTATION_POSITION_CATEGORIES {
            assert!(
                spread.get(category).is_some(),
                "canonical category {category} must always be reported"
            );
        }
        assert_eq!(spread["back"]["min"], 0, "unvisited category counts as 0");
    }

    fn small_rotation_request(periods: usize) -> Value {
        json!({
            "draft": {
                "name": "Rotation Bound",
                "students": (0..4)
                    .map(|index| {
                        json!({
                            "student_id": format!("S{}", index + 1),
                            "name": format!("Student {}", index + 1),
                            "score": 100 - (index as i64),
                        })
                    })
                    .collect::<Vec<_>>(),
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "daily-rotation"}
            },
            "period_count": periods,
            "options": {"seed": 42}
        })
    }

    /// N regression: rotation drafts remember their originating request via
    /// the same FIFO-capped store as single solves; per-period inserts must
    /// never grow it past [`crate::MAX_SOLVE_REQUESTS`].
    #[test]
    fn rotation_period_requests_stay_bounded_in_the_fifo_store() {
        let editor_store = editing::new_draft_store();
        let solve_requests = SolveRequestStore::default();
        // 5 runs x 14 periods = 70 per-period inserts > the cap of 64.
        for _ in 0..5 {
            let outcome =
                generate_rotation_plan(&small_rotation_request(14), &editor_store, &solve_requests)
                    .expect("rotation terminates with a domain result");
            assert!(outcome.feasible, "small-classroom rotation must solve");
        }
        let guard = solve_requests.lock().unwrap();
        assert_eq!(
            guard.len(),
            crate::MAX_SOLVE_REQUESTS,
            "store stays at the cap despite 70 rotation inserts"
        );
    }
}
