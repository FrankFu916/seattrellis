// ---------------------------------------------------------------------------
// repair.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Constrained re-solve repair.
// ---------------------------------------------------------------------------

use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

///   everyone connected by a hard pair rule are movable, everyone else is
///   fixed; without `affected_students` only the locks are fixed and the
///   rest may re-arrange globally.
///
/// Returns the repaired snapshot document (`assignments` + `solver_status`)
/// plus a short summary of moved/unseated students.
use crate::engine::{effective_students, validate_solve_request};
use crate::solver::{solve_problem, validate_solve_response, CoreSolveRequest};

#[derive(Debug, Clone)]

pub(crate) struct ParsedSnapshotAssignment {
    pub(crate) student_key: String,
    pub(crate) seat_id: String,
}

pub(crate) fn parse_snapshot_assignments(
    snapshot: &Value,
    context: &str,
) -> Result<Vec<ParsedSnapshotAssignment>, String> {
    let object = snapshot
        .as_object()
        .ok_or_else(|| format!("invalid {context}: expected a JSON object"))?;
    let assignments = object
        .get("assignments")
        .ok_or_else(|| format!("invalid {context}: missing assignments"))?
        .as_array()
        .ok_or_else(|| format!("invalid {context}: assignments must be an array"))?;
    assignments
        .iter()
        .enumerate()
        .map(|(index, assignment)| {
            let assignment = assignment.as_object().ok_or_else(|| {
                format!("invalid {context}: assignments[{index}] must be an object")
            })?;
            let student_key = assignment
                .get("student_key")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "invalid {context}: assignments[{index}].student_key must be a non-empty string"
                    )
                })?;
            let seat_id = assignment
                .get("seat_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "invalid {context}: assignments[{index}].seat_id must be a non-empty string"
                    )
                })?;
            Ok(ParsedSnapshotAssignment {
                student_key: student_key.to_string(),
                seat_id: seat_id.to_string(),
            })
        })
        .collect()
}

fn request_seat_ids(request: &CoreSolveRequest) -> Vec<String> {
    (0..request.seat_positions.len())
        .map(|index| {
            request
                .layout
                .as_ref()
                .and_then(|layout| layout.seats.get(index))
                .map(|seat| seat.seat_id.clone())
                .unwrap_or_else(|| format!("seat-{}", index + 1))
        })
        .collect()
}

pub fn repair_json(
    request_json: &str,
    snapshot_json: &str,
    affected_students: &[String],
    locked_students: &[String],
    locked_seats: &[String],
) -> Result<String, String> {
    let mut request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let snapshot: Value = serde_json::from_str(snapshot_json)
        .map_err(|error| format!("invalid snapshot document: {error}"))?;
    let snapshot_assignments = parse_snapshot_assignments(&snapshot, "repair snapshot")?;

    // Student keys -> indices from the request.
    let students = effective_students(&request);
    let index_by_key: HashMap<&str, usize> = students
        .iter()
        .enumerate()
        .map(|(index, student)| (student.key.as_str(), index))
        .collect();
    let seat_ids = request_seat_ids(&request);
    let seat_index_by_id: HashMap<&str, usize> = seat_ids
        .iter()
        .enumerate()
        .map(|(index, seat_id)| (seat_id.as_str(), index))
        .collect();

    // Current assignment: student key -> seat id (and reverse). Repair uses
    // strict semantics: malformed, unknown, or duplicate references must not
    // silently change which anchors are preserved.
    let mut seat_by_student: HashMap<String, String> = HashMap::new();
    let mut student_by_seat: HashMap<String, String> = HashMap::new();
    for assignment in snapshot_assignments {
        if !index_by_key.contains_key(assignment.student_key.as_str()) {
            return Err(format!(
                "Repair snapshot references unknown student: {}.",
                assignment.student_key
            ));
        }
        if !seat_index_by_id.contains_key(assignment.seat_id.as_str()) {
            return Err(format!(
                "Repair snapshot references unknown seat: {}.",
                assignment.seat_id
            ));
        }
        if seat_by_student
            .insert(assignment.student_key.clone(), assignment.seat_id.clone())
            .is_some()
        {
            return Err(format!(
                "Repair snapshot contains duplicate assignments for student: {}.",
                assignment.student_key
            ));
        }
        if student_by_seat
            .insert(assignment.seat_id.clone(), assignment.student_key.clone())
            .is_some()
        {
            return Err(format!(
                "Repair snapshot assigns seat {} more than once.",
                assignment.seat_id
            ));
        }
    }

    // Validate the anchor sets.
    let unknown_affected: Vec<&str> = affected_students
        .iter()
        .map(String::as_str)
        .filter(|key| !index_by_key.contains_key(key))
        .collect();
    if !unknown_affected.is_empty() {
        return Err(format!(
            "Affected students are unknown: {}.",
            unknown_affected.join(", ")
        ));
    }
    for student in locked_students {
        if !index_by_key.contains_key(student.as_str()) {
            return Err(format!("Locked student is unknown: {student}."));
        }
        if !seat_by_student.contains_key(student) {
            return Err(format!(
                "Locked students must have a current seat before re-solving: {student}."
            ));
        }
    }
    // Locked seats: occupied seats become fixed anchors; *empty* locked
    // seats become reserved seats that stay empty (mirroring the Python
    // `reserved_empty_seats` repair semantics).
    let mut reserved_empty_seats: Vec<usize> = Vec::new();
    for seat in locked_seats {
        if !seat_index_by_id.contains_key(seat.as_str()) {
            return Err(format!("Locked seat is unknown: {seat}."));
        }
        if !student_by_seat.contains_key(seat) {
            let seat_index = seat_index_by_id[seat.as_str()];
            if !reserved_empty_seats.contains(&seat_index) {
                reserved_empty_seats.push(seat_index);
            }
        }
    }
    for student in affected_students {
        if locked_students.contains(student) {
            return Err(format!(
                "Affected students cannot also be locked: {student}."
            ));
        }
        if let Some(seat) = seat_by_student.get(student) {
            if locked_seats.contains(seat) {
                return Err(format!("Affected students occupy locked seats: {student}."));
            }
        }
    }

    // Fixed set: locked students + locked-seat occupants + (when a local
    // scope is requested) every student outside the affected closure.
    let mut fixed_students: Vec<usize> = Vec::new();
    for student in locked_students {
        let index = index_by_key[student.as_str()];
        if !fixed_students.contains(&index) {
            fixed_students.push(index);
        }
    }
    for seat in locked_seats {
        let Some(occupant) = student_by_seat.get(seat.as_str()) else {
            continue; // reserved empty seat, handled separately
        };
        let index = index_by_key[occupant.as_str()];
        if !fixed_students.contains(&index) {
            fixed_students.push(index);
        }
    }
    if !affected_students.is_empty() {
        let mut affected_indices: Vec<usize> = affected_students
            .iter()
            .map(|student| index_by_key[student.as_str()])
            .collect();
        // One-hop closure via hard pair rules.
        let pair_rules: Vec<[usize; 2]> = request
            .must_be_adjacent
            .iter()
            .chain(request.cannot_be_adjacent.iter())
            .copied()
            .collect();
        let mut grew = true;
        while grew {
            grew = false;
            for pair in &pair_rules {
                if affected_indices.contains(&pair[0]) && !affected_indices.contains(&pair[1]) {
                    affected_indices.push(pair[1]);
                    grew = true;
                }
                if affected_indices.contains(&pair[1]) && !affected_indices.contains(&pair[0]) {
                    affected_indices.push(pair[0]);
                    grew = true;
                }
            }
        }
        for index in 0..request.student_count {
            if !affected_indices.contains(&index) && !fixed_students.contains(&index) {
                fixed_students.push(index);
            }
        }
    }

    // Express repair anchors as additional fixed seats. The request's original
    // fixed-seat rules remain authoritative even when the fixed student is in
    // the affected (movable) set.
    let original_fixed_seats = request.fixed_seats.clone();
    let original_fixed_by_student: HashMap<usize, usize> = original_fixed_seats
        .iter()
        .map(|[student, seat]| (*student, *seat))
        .collect();
    let original_fixed_by_seat: HashMap<usize, usize> = original_fixed_seats
        .iter()
        .map(|[student, seat]| (*seat, *student))
        .collect();
    let mut repair_anchors: Vec<[usize; 2]> = Vec::new();
    for index in fixed_students {
        let student_key = students[index].key.clone();
        let seat_id = seat_by_student
            .get(&student_key)
            .ok_or_else(|| format!("Student has no current seat: {student_key}."))?;
        let seat_index = seat_index_by_id
            .get(seat_id.as_str())
            .ok_or_else(|| format!("Current seat is unknown: {seat_id}."))?;

        if let Some(original_seat) = original_fixed_by_student.get(&index) {
            if original_seat != seat_index {
                return Err(format!(
                    "Repair anchor conflicts with the original fixed-seat rule: student \
                     {student_key} is fixed to seat index {original_seat}, but the repair \
                     anchor requires {seat_id} (index {seat_index})."
                ));
            }
            // The identical pair already exists in `original_fixed_seats`.
            continue;
        }
        if let Some(original_student) = original_fixed_by_seat.get(seat_index) {
            let original_student_key = &students[*original_student].key;
            return Err(format!(
                "Repair anchor conflicts with the original fixed-seat rule: seat {seat_id} \
                 (index {seat_index}) is fixed to student {original_student_key}, but the \
                 repair anchor requires student {student_key}."
            ));
        }
        repair_anchors.push([index, *seat_index]);
    }
    // Reserved empty seats must not be required by the original fixed-seat
    // rules (mirroring the Python `reserved_fixed_conflicts` rejection).
    for reserved in &reserved_empty_seats {
        if let Some(student) = original_fixed_by_seat.get(reserved) {
            let student_key = &students[*student].key;
            let seat_id = &seat_ids[*reserved];
            return Err(format!(
                "Cannot reserve an empty locked seat required by existing hard rules: \
                 {student_key}->{seat_id}."
            ));
        }
    }

    // Disable reserved empty seats in the solver layout so they stay empty.
    if !reserved_empty_seats.is_empty() {
        if let Some(layout) = request.layout.as_mut() {
            for reserved in &reserved_empty_seats {
                if let Some(seat) = layout.seats.get_mut(*reserved) {
                    seat.enabled = false;
                }
            }
        } else {
            // No typed layout in the request: synthesize one from the
            // seat positions so disabled seats are representable.
            let positions = request.seat_positions.clone();
            let seats: Vec<Value> = positions
                .iter()
                .enumerate()
                .map(|(index, [x, y])| {
                    json!({
                        "seat_id": seat_ids[index],
                        "row": 1 + index as u32,
                        "col": 1,
                        "x": x,
                        "y": y,
                        "enabled": !reserved_empty_seats.contains(&index),
                        "zone": "middle",
                        "near_platform": false,
                        "near_window": false,
                        "near_door": false,
                        "near_ac": false,
                        "tags": [],
                        "attributes": {}
                    })
                })
                .collect();
            request.layout = Some(
                serde_json::from_value(json!({
                    "layout_id": "repair-reserved",
                    "name": "repair reserved layout",
                    "seats": seats,
                    "adjacency": {"include_horizontal": true, "include_vertical": true}
                }))
                .map_err(|error: serde_json::Error| {
                    format!("could not build reserved layout: {error}")
                })?,
            );
        }
    }

    request.fixed_seats = original_fixed_seats;
    request.fixed_seats.extend(repair_anchors);
    // Re-run static conflict detection now that repair anchors have been
    // merged with the original hard rules (also catches anchor/anchor clashes).
    validate_solve_request(&request)
        .map_err(|error| format!("Repair constraints are invalid: {error}"))?;

    let response = solve_problem(&request)?;
    if !response.feasible {
        return Err(format!(
            "Repair solve did not find a legal seating (status {}).",
            response.status.as_str()
        ));
    }
    // Boundary validation is intentionally repeated here: repair must not
    // publish a snapshot unless the response satisfies both the original hard
    // rules and every repair anchor in the merged request.
    validate_solve_response(&request, &response)
        .map_err(|error| format!("Repair solve returned an invalid result: {error}"))?;

    // Build the repaired snapshot (frontend shape) + summary.
    let mut assignments: Vec<Value> = Vec::new();
    let mut moved = 0;
    let mut unseated = 0;
    for [student, seat] in &response.assignment {
        let student_key = students[*student].key.clone();
        let seat_id = seat_ids[*seat].clone();
        let display_name = students[*student]
            .display_name
            .clone()
            .unwrap_or_else(|| student_key.clone());
        if let Some(previous) = seat_by_student.get(&student_key) {
            if previous != &seat_id {
                moved += 1;
            }
        } else {
            unseated += 1;
        }
        assignments.push(json!({
            "student_key": student_key,
            "student_name": display_name,
            "seat_id": seat_id,
        }));
    }

    let repaired = json!({
        "assignments": assignments,
        "solver_status": response.status.as_str(),
        "seed": request.seed,
        "summary": {
            "moved_students": moved,
            "unseated_students": unseated,
            "locked_students": locked_students.len(),
            "locked_seats": locked_seats.len(),
        },
    });
    serde_json::to_string(&repaired)
        .map_err(|error| format!("could not serialize repair result: {error}"))
}

pub(crate) const REPORT_POSITION_CATEGORIES: [&str; 10] = [
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
pub(crate) const REPORT_PAIR_RELATIONS: [&str; 6] = [
    "desk_mate",
    "horizontal",
    "vertical",
    "diagonal",
    "adjacent_any",
    "within_distance",
];
pub(crate) const PAIR_REPORT_RECENT_LOOKBACK: usize = 4;

pub(crate) struct HistoryStudentAccumulator {
    pub(crate) student_name: Option<String>,
    pub(crate) total_assignments: u64,
    pub(crate) seat_counts: BTreeMap<String, u64>,
    pub(crate) category_counts: BTreeMap<String, u64>,
    pub(crate) records: Vec<Value>,
}
