// ---------------------------------------------------------------------------
// reports.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// History and pair reports.
// ---------------------------------------------------------------------------

use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::cost::{classify_seat_position, detect_neighbor_relation_types};
use crate::models::Seat;

use crate::engine::{effective_layout, effective_students, validate_solve_request};
use crate::repair::{
    parse_snapshot_assignments, HistoryStudentAccumulator, PAIR_REPORT_RECENT_LOOKBACK,
    REPORT_PAIR_RELATIONS, REPORT_POSITION_CATEGORIES,
};
use crate::solver::parse_core_solve_request;

pub fn history_report_json(request_json: &str, snapshots_json: &str) -> Result<String, String> {
    let request = parse_core_solve_request(request_json)?;
    validate_solve_request(&request)?;
    let students = effective_students(&request);
    let layout = effective_layout(&request);
    let snapshots: Vec<Value> = serde_json::from_str(snapshots_json)
        .map_err(|error| format!("invalid snapshots document: {error}"))?;
    let seat_by_id: HashMap<&str, &Seat> = layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();
    let known_students: HashSet<&str> = students
        .iter()
        .map(|student| student.key.as_str())
        .collect();
    let mut per_student: BTreeMap<String, HistoryStudentAccumulator> = students
        .iter()
        .map(|student| {
            (
                student.key.clone(),
                HistoryStudentAccumulator {
                    student_name: student.display_name.clone(),
                    total_assignments: 0,
                    seat_counts: BTreeMap::new(),
                    category_counts: BTreeMap::new(),
                    records: Vec::new(),
                },
            )
        })
        .collect();
    let mut totals: BTreeMap<String, u64> = REPORT_POSITION_CATEGORIES
        .iter()
        .map(|category| ((*category).to_string(), 0))
        .collect();
    let mut warnings: Vec<String> = Vec::new();

    for (snapshot_offset, snapshot) in snapshots.iter().enumerate() {
        let snapshot_index = snapshot_offset + 1;
        let parsed =
            parse_snapshot_assignments(snapshot, &format!("history snapshot {snapshot_index}"))?;
        let mut assignments: BTreeMap<String, String> = BTreeMap::new();
        let mut seat_owner: HashMap<String, String> = HashMap::new();
        for assignment in parsed {
            if !known_students.contains(assignment.student_key.as_str()) {
                warnings.push(format!(
                    "history snapshot {snapshot_index} references unknown student {:?}; skipped",
                    assignment.student_key
                ));
                continue;
            }
            if assignments
                .insert(assignment.student_key.clone(), assignment.seat_id.clone())
                .is_some()
            {
                warnings.push(format!(
                    "history snapshot {snapshot_index} contains duplicate assignments for student {:?}; the last one was used",
                    assignment.student_key
                ));
            }
            if let Some(previous) =
                seat_owner.insert(assignment.seat_id.clone(), assignment.student_key.clone())
            {
                warnings.push(format!(
                    "history snapshot {snapshot_index} assigns seat {:?} to both {:?} and {:?}",
                    assignment.seat_id, previous, assignment.student_key
                ));
            }
        }
        let missing: Vec<&str> = known_students
            .iter()
            .copied()
            .filter(|student| !assignments.contains_key(*student))
            .collect();
        if !missing.is_empty() {
            warnings.push(format!(
                "history snapshot {snapshot_index} is missing {} current student(s)",
                missing.len()
            ));
        }

        for (student_key, seat_id) in assignments {
            let student = per_student
                .get_mut(&student_key)
                .expect("known student accumulators are pre-initialized");
            let (categories, unknown_seat, disabled_seat) = match seat_by_id.get(seat_id.as_str()) {
                None => {
                    warnings.push(format!(
                        "history snapshot {snapshot_index} references unknown seat_id {seat_id:?} for student {student_key:?}; marked as unknown"
                    ));
                    (vec!["unknown".to_string()], true, false)
                }
                Some(seat) if !seat.enabled => {
                    warnings.push(format!(
                        "history snapshot {snapshot_index} references disabled seat_id {seat_id:?} for student {student_key:?}; categories skipped"
                    ));
                    (Vec::new(), false, true)
                }
                Some(seat) => {
                    let mut categories: Vec<String> =
                        classify_seat_position(seat, &layout).into_iter().collect();
                    categories.sort();
                    (categories, false, false)
                }
            };
            student.total_assignments += 1;
            *student.seat_counts.entry(seat_id.clone()).or_default() += 1;
            for category in &categories {
                *student.category_counts.entry(category.clone()).or_default() += 1;
                *totals.entry(category.clone()).or_default() += 1;
            }
            student.records.push(json!({
                "snapshot_index": snapshot_index,
                "seat_id": seat_id,
                "categories": categories,
                "unknown_seat": unknown_seat,
                "disabled_seat": disabled_seat,
            }));
        }
    }

    let student_values: Vec<Value> = per_student
        .iter()
        .map(|(student_key, student)| {
            json!({
                "student_key": student_key,
                "student_name": student.student_name,
                "total_assignments": student.total_assignments,
                "seat_counts": student.seat_counts,
                "category_counts": student.category_counts,
                "records": student.records,
            })
        })
        .collect();
    let mut category_spread: BTreeMap<String, Value> = BTreeMap::new();
    for category in REPORT_POSITION_CATEGORIES {
        let counts: Vec<u64> = per_student
            .values()
            .map(|student| student.category_counts.get(category).copied().unwrap_or(0))
            .collect();
        let min = counts.iter().copied().min().unwrap_or(0);
        let max = counts.iter().copied().max().unwrap_or(0);
        category_spread.insert(
            category.to_string(),
            json!({ "min": min, "max": max, "spread": max - min }),
        );
    }

    let report = json!({
        "history_count": snapshots.len(),
        "student_count": request.student_count,
        "category_totals": totals,
        "students": student_values,
        "summary": {
            "category_spread": category_spread,
            "warning_count": warnings.len(),
        },
        "warnings": warnings,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize history report: {error}"))
}

struct PairReportAccumulator {
    pair_key: String,
    first_student_key: String,
    second_student_key: String,
    first_student_name: Option<String>,
    second_student_name: Option<String>,
    total_occurrences: u64,
    recent_occurrences: u64,
    relation_counts: BTreeMap<String, u64>,
    records: Vec<Value>,
}

fn pair_report_value(pair: &PairReportAccumulator) -> Value {
    json!({
        "pair_key": pair.pair_key,
        "first_student_key": pair.first_student_key,
        "second_student_key": pair.second_student_key,
        "first_student_name": pair.first_student_name,
        "second_student_name": pair.second_student_name,
        "total_occurrences": pair.total_occurrences,
        "relation_counts": pair.relation_counts,
        "records": pair.records,
    })
}

fn rank_pairs_for_relation<'a>(
    pairs: impl Iterator<Item = &'a PairReportAccumulator>,
    relation: &str,
    top: usize,
) -> Vec<&'a PairReportAccumulator> {
    let mut ranked: Vec<&PairReportAccumulator> = pairs
        .filter(|pair| pair.relation_counts.get(relation).copied().unwrap_or(0) > 0)
        .collect();
    ranked.sort_by(|left, right| {
        right
            .relation_counts
            .get(relation)
            .copied()
            .unwrap_or(0)
            .cmp(&left.relation_counts.get(relation).copied().unwrap_or(0))
            .then_with(|| right.total_occurrences.cmp(&left.total_occurrences))
            .then_with(|| left.pair_key.cmp(&right.pair_key))
    });
    ranked.truncate(top);
    ranked
}

/// Pair-history report with Python-compatible pair records and relation
/// rankings. `top` and `within_distance` are contract inputs, so invalid
/// values are rejected rather than silently coerced.
/// Fairness report over historical snapshots, retaining all current students
/// even when no snapshot contains an assignment for them. Malformed snapshot
/// entries are rejected instead of silently disappearing; semantic history
/// gaps are reported as warnings, matching the Python report contract.
pub fn pair_report_json(
    request_json: &str,
    snapshots_json: &str,
    top: usize,
    within_distance: i32,
) -> Result<String, String> {
    if top == 0 {
        return Err("invalid top: expected a positive value".to_string());
    }
    if within_distance <= 0 {
        return Err("invalid within_distance: expected a positive value".to_string());
    }
    let request = parse_core_solve_request(request_json)?;
    validate_solve_request(&request)?;
    let students = effective_students(&request);
    let student_names: HashMap<&str, Option<String>> = students
        .iter()
        .map(|student| (student.key.as_str(), student.display_name.clone()))
        .collect();
    let known_students: HashSet<&str> = student_names.keys().copied().collect();
    let layout = effective_layout(&request);
    let snapshots: Vec<Value> = serde_json::from_str(snapshots_json)
        .map_err(|error| format!("invalid snapshots document: {error}"))?;
    let seat_by_id: HashMap<&str, &Seat> = layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();
    let mut pairs: BTreeMap<String, PairReportAccumulator> = BTreeMap::new();
    let mut relation_totals: BTreeMap<String, u64> = REPORT_PAIR_RELATIONS
        .iter()
        .map(|relation| ((*relation).to_string(), 0))
        .collect();
    let mut warnings: Vec<String> = Vec::new();
    for (snapshot_offset, snapshot) in snapshots.iter().enumerate() {
        let snapshot_index = snapshot_offset + 1;
        let parsed = parse_snapshot_assignments(
            snapshot,
            &format!("pair-history snapshot {snapshot_index}"),
        )?;
        let mut by_student: BTreeMap<String, String> = BTreeMap::new();
        for assignment in parsed {
            if !known_students.contains(assignment.student_key.as_str()) {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} references unknown student {:?}; skipped",
                    assignment.student_key
                ));
                continue;
            }
            if by_student
                .insert(assignment.student_key.clone(), assignment.seat_id)
                .is_some()
            {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} contains duplicate assignments for student {:?}; the last one was used",
                    assignment.student_key
                ));
            }
        }
        let missing = known_students
            .iter()
            .filter(|student| !by_student.contains_key(**student))
            .count();
        if missing > 0 {
            warnings.push(format!(
                "pair-history snapshot {snapshot_index} is missing {missing} current student(s)"
            ));
        }

        let mut known: Vec<(&str, &str, &Seat)> = Vec::new();
        let mut occupied: HashMap<&str, &str> = HashMap::new();
        for (student_key, seat_id) in &by_student {
            let Some(seat) = seat_by_id.get(seat_id.as_str()).copied() else {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} references unknown seat_id {seat_id:?} for student {student_key:?}; pair relations skipped"
                ));
                continue;
            };
            if let Some(previous) = occupied.insert(seat_id.as_str(), student_key.as_str()) {
                warnings.push(format!(
                    "pair-history snapshot {snapshot_index} assigns seat {seat_id:?} to both {previous:?} and {student_key:?}; later assignment skipped"
                ));
                continue;
            }
            known.push((student_key.as_str(), seat_id.as_str(), seat));
        }
        for first in 0..known.len() {
            for second in (first + 1)..known.len() {
                let (first_key, first_seat_id, first_seat) = known[first];
                let (second_key, second_seat_id, second_seat) = known[second];
                let mut relations: Vec<String> = detect_neighbor_relation_types(
                    first_seat,
                    second_seat,
                    &layout,
                    None,
                    within_distance,
                )
                .into_iter()
                .collect();
                if relations.is_empty() {
                    continue;
                }
                relations.sort();
                let pair_key = format!("{first_key}|{second_key}");
                let pair = pairs
                    .entry(pair_key.clone())
                    .or_insert_with(|| PairReportAccumulator {
                        pair_key,
                        first_student_key: first_key.to_string(),
                        second_student_key: second_key.to_string(),
                        first_student_name: student_names.get(first_key).cloned().flatten(),
                        second_student_name: student_names.get(second_key).cloned().flatten(),
                        total_occurrences: 0,
                        recent_occurrences: 0,
                        relation_counts: BTreeMap::new(),
                        records: Vec::new(),
                    });
                pair.total_occurrences += 1;
                for relation in &relations {
                    *pair.relation_counts.entry(relation.clone()).or_default() += 1;
                    *relation_totals.entry(relation.clone()).or_default() += 1;
                }
                // Deltas in i64: the i32 subtraction of saturated extreme
                // coordinates would overflow (debug panic).
                let row_delta =
                    (i64::from(first_seat.row) - i64::from(second_seat.row)).unsigned_abs();
                let col_delta =
                    (i64::from(first_seat.col) - i64::from(second_seat.col)).unsigned_abs();
                pair.records.push(json!({
                    "snapshot_index": snapshot_index,
                    "first_seat_id": first_seat_id,
                    "second_seat_id": second_seat_id,
                    "relations": relations,
                    "row_delta": row_delta,
                    "col_delta": col_delta,
                    "chebyshev_distance": row_delta.max(col_delta),
                    "manhattan_distance": row_delta + col_delta,
                    "first_seat_disabled": !first_seat.enabled,
                    "second_seat_disabled": !second_seat.enabled,
                }));
            }
        }
    }

    // Python's StudentPairHistory.recent_occurrence_count applies lookback to
    // the pair's own records, not to the global snapshot window. A pair that
    // occurred once long ago therefore still has one recent occurrence when
    // it has fewer than four records in total. Keep the Rust compatibility
    // field aligned with that frozen oracle behavior.
    for pair in pairs.values_mut() {
        pair.recent_occurrences = pair.records.len().min(PAIR_REPORT_RECENT_LOOKBACK) as u64;
    }

    let pair_values: Vec<Value> = pairs.values().map(pair_report_value).collect();
    let top_desk_mates: Vec<Value> = rank_pairs_for_relation(pairs.values(), "desk_mate", top)
        .into_iter()
        .map(pair_report_value)
        .collect();
    let top_adjacent_pairs: Vec<Value> =
        rank_pairs_for_relation(pairs.values(), "adjacent_any", top)
            .into_iter()
            .map(pair_report_value)
            .collect();
    let mut ranked: Vec<&PairReportAccumulator> = pairs.values().collect();
    ranked.sort_by(|left, right| {
        right
            .total_occurrences
            .cmp(&left.total_occurrences)
            .then_with(|| left.pair_key.cmp(&right.pair_key))
    });
    let repeated = ranked
        .iter()
        .filter(|pair| pair.total_occurrences > 1)
        .count();
    let max_occurrences = ranked
        .first()
        .map(|pair| pair.total_occurrences)
        .unwrap_or(0);

    // Retain the compact anonymized legacy view for current Rust consumers,
    // but compute "recent" from the Python-default four-snapshot lookback.
    let mut anonymized_students: BTreeMap<String, usize> = BTreeMap::new();
    let mut legacy_top_pairs: Vec<Value> = Vec::new();
    for pair in ranked.iter().take(top) {
        let next_first = anonymized_students.len() + 1;
        let first = *anonymized_students
            .entry(pair.first_student_key.clone())
            .or_insert(next_first);
        let next_second = anonymized_students.len() + 1;
        let second = *anonymized_students
            .entry(pair.second_student_key.clone())
            .or_insert(next_second);
        legacy_top_pairs.push(json!({
            "student_a": format!("student-{first}"),
            "student_b": format!("student-{second}"),
            "total_occurrences": pair.total_occurrences,
            "recent_occurrences": pair.recent_occurrences,
        }));
    }

    let report = json!({
        "history_count": snapshots.len(),
        "student_count": request.student_count,
        "pair_count": pairs.len(),
        "within_distance_metric": "chebyshev",
        "within_distance": within_distance,
        "relation_totals": relation_totals,
        "top_desk_mates": top_desk_mates,
        "top_adjacent_pairs": top_adjacent_pairs,
        "pairs": pair_values,
        "repeated_pair_count": repeated,
        "max_occurrences": max_occurrences,
        "top_pairs": legacy_top_pairs,
        "summary": {
            "warning_count": warnings.len(),
            "within_distance_metric": "chebyshev",
            "within_distance": within_distance,
            "recent_lookback": PAIR_REPORT_RECENT_LOOKBACK,
        },
        "warnings": warnings,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize pair report: {error}"))
}
