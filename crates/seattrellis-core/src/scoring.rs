// ---------------------------------------------------------------------------
// scoring.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// PlanScore per-assignment scoring (Python-parity breakdown).
// ---------------------------------------------------------------------------

use serde_json::Value;
use std::collections::HashMap;

use crate::cost::{
    avoid_recent_neighbors_cost, build_adjacency_edges, detect_neighbor_relation_types,
    fair_rotation_cost, student_needs_front,
};
use crate::models::{effective_neighbor_rule, Seat};
use crate::objectives::evaluate_soft_objectives;

// ---------------------------------------------------------------------------
// PlanScore (plan §6.2/§6.6): Python-parity per-assignment score breakdown
// ---------------------------------------------------------------------------

/// `_rating` from `scoring.py`: a coarse qualitative band over the 0..100
/// score.
use crate::engine::{assignment_by_key, build_cost_context, validate_solve_request};
use crate::evaluation::{
    assigned_students_are_adjacent, assigned_students_meet_distance, build_graph_distance_matrix,
    build_index_adjacency,
};
use crate::solver::{resolve_group_rules, CoreSolveRequest};

fn score_rating(score: f64) -> &'static str {
    if score >= 75.0 {
        "high"
    } else if score >= 50.0 {
        "medium"
    } else {
        "low"
    }
}

/// `_available_dimension` from `scoring.py`: a finite score clamped to
/// 0..100 and rounded to two decimals, with the rating band.
fn score_dimension(
    score: f64,
    raw_value: Option<f64>,
    weight: f64,
    details: serde_json::Value,
) -> serde_json::Value {
    let score = (score.clamp(0.0, 100.0) * 100.0).round() / 100.0;
    serde_json::json!({
        "status": "available",
        "score": score,
        "raw_value": raw_value,
        "weight": weight,
        "rating": score_rating(score),
        "details": details,
    })
}

/// `_not_available` from `scoring.py`.
fn not_available_dimension(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "not_available",
        "score": null,
        "raw_value": null,
        "weight": 0,
        "rating": "not_available",
        "details": { "reason": reason },
    })
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Score a fixed assignment exactly like Python's `score_snapshot`
/// (plan §6.6 item 4: Rust/Python objective breakdown parity on the same
/// assignment). The breakdown mirrors `ScoreBreakdown` field-for-field:
/// seven named dimensions plus `rule_scores` (score_position /
/// score_distribution / mentor_pairing) and the hard-constraint summary.
///
/// `latest_snapshot_json` is a snapshot document (or `[]`) used by the
/// stability dimension; `diversity_score` is the caller-provided mean
/// assignment distance for candidate sets.
pub fn score_assignment_json(
    request_json: &str,
    assignment_pairs: &[[usize; 2]],
    latest_snapshot_json: &str,
    diversity_score: Option<f64>,
) -> Result<String, String> {
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;

    // Rebuild the student->seat probe (same completeness checks as the
    // audit): a partial or duplicated assignment cannot be scored.
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    let mut occupied = vec![false; request.seat_positions.len()];
    for [student, seat] in assignment_pairs {
        if *student >= request.student_count || *seat >= request.seat_positions.len() {
            return Err(format!(
                "assignment references unknown student {student} or seat {seat}"
            ));
        }
        if probe[*student].replace(*seat).is_some() {
            return Err(format!(
                "assignment assigns student {student} more than once"
            ));
        }
        if occupied[*seat] {
            return Err(format!(
                "assignment assigns seat {seat} to more than one student"
            ));
        }
        occupied[*seat] = true;
    }
    if let Some(missing) = probe.iter().position(Option::is_none) {
        return Err(format!(
            "assignment is incomplete: student index {} has no seat",
            missing + 1
        ));
    }

    let ctx = build_cost_context(&request);
    let assignment_vec: Vec<usize> = probe.iter().map(|seat| seat.unwrap()).collect();
    let by_key = assignment_by_key(&probe, &ctx);
    let adjacency_edges = build_adjacency_edges(&ctx.layout);
    let seat_by_id: HashMap<&str, &Seat> = ctx
        .layout
        .seats
        .iter()
        .map(|seat| (seat.seat_id.as_str(), seat))
        .collect();
    let student_count = assignment_vec.len();

    // --- fair_rotation_score -------------------------------------------------
    let fair_rule = &ctx.rules.soft.fair_rotation;
    let fair_rotation_score = if !fair_rule.enabled || fair_rule.weight == 0 {
        not_available_dimension("fair_rotation is disabled.")
    } else if ctx
        .history
        .as_ref()
        .is_none_or(|history| history.history_count == 0)
    {
        not_available_dimension("No history snapshots were supplied.")
    } else {
        let penalty: i64 = assignment_vec
            .iter()
            .enumerate()
            .map(|(student, seat)| {
                fair_rotation_cost(
                    &ctx.students[student],
                    &ctx.layout.seats[*seat],
                    &ctx.layout,
                    fair_rule,
                    ctx.history.as_ref(),
                )
            })
            .sum();
        let penalty_units = penalty as f64 / (fair_rule.weight.max(1) * 100) as f64;
        let score = 100.0 / (1.0 + penalty_units / student_count.max(1) as f64);
        score_dimension(
            score,
            Some(penalty as f64),
            fair_rule.weight as f64,
            serde_json::json!({
                "penalty_cost": penalty,
                "history_count": ctx.history.as_ref().map(|h| h.history_count).unwrap_or(0),
                "lookback": fair_rule.lookback,
                "lower_penalty_is_better": true,
            }),
        )
    };

    // --- avoid_recent_neighbors_score ---------------------------------------
    let neighbor_rule = effective_neighbor_rule(&ctx.rules);
    let avoid_recent_neighbors_score = if !neighbor_rule.enabled || neighbor_rule.weight == 0 {
        not_available_dimension("avoid_recent_neighbors is disabled.")
    } else if ctx
        .pair_history
        .as_ref()
        .is_none_or(|pair_history| pair_history.history_count == 0)
    {
        not_available_dimension("No pair history was supplied.")
    } else {
        let selected_relations: std::collections::HashSet<&str> = neighbor_rule
            .relation_types
            .iter()
            .map(String::as_str)
            .collect();
        let mut penalty: i64 = 0;
        let mut relevant_pairs = 0usize;
        for first in 0..student_count {
            for second in (first + 1)..student_count {
                let first_seat = &ctx.layout.seats[assignment_vec[first]];
                let second_seat = &ctx.layout.seats[assignment_vec[second]];
                let current_relations = detect_neighbor_relation_types(
                    first_seat,
                    second_seat,
                    &ctx.layout,
                    Some(&adjacency_edges),
                    neighbor_rule.within_distance,
                );
                if current_relations
                    .iter()
                    .any(|relation| selected_relations.contains(relation.as_str()))
                {
                    relevant_pairs += 1;
                }
                penalty += avoid_recent_neighbors_cost(
                    &ctx.students[first].key,
                    &ctx.students[second].key,
                    first_seat,
                    second_seat,
                    &ctx.layout,
                    &neighbor_rule,
                    ctx.pair_history.as_ref(),
                    Some(&adjacency_edges),
                );
            }
        }
        let excess_units = penalty as f64 / (neighbor_rule.weight.max(1) * 100) as f64;
        let score = 100.0 / (1.0 + excess_units / relevant_pairs.max(1) as f64);
        score_dimension(
            score,
            Some(penalty as f64),
            neighbor_rule.weight as f64,
            serde_json::json!({
                "penalty_cost": penalty,
                "relevant_current_pairs": relevant_pairs,
                "history_count": ctx.pair_history.as_ref().map(|h| h.history_count).unwrap_or(0),
                "lookback": neighbor_rule.lookback,
                "lower_penalty_is_better": true,
            }),
        )
    };

    // --- score_balance_score -------------------------------------------------
    let balance_rule = &ctx.rules.soft.score_balance;
    let score_balance_score = if !balance_rule.enabled || balance_rule.weight == 0 {
        not_available_dimension("score_balance is disabled.")
    } else {
        let scores: Vec<f64> = ctx
            .students
            .iter()
            .filter_map(|student| student.score)
            .collect();
        let distinct: std::collections::HashSet<u64> =
            scores.iter().map(|score| score.to_bits()).collect();
        if scores.len() < 2 || distinct.len() < 2 {
            not_available_dimension("At least two different student scores are required.")
        } else {
            let min_score = scores.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_score = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let score_by_key: HashMap<&str, f64> = ctx
                .students
                .iter()
                .filter_map(|student| student.score.map(|score| (student.key.as_str(), score)))
                .collect();
            let mut gaps: Vec<f64> = Vec::new();
            for (first_seat, second_seat) in &adjacency_edges {
                let first_key = seat_by_id
                    .get(first_seat.as_str())
                    .and_then(|seat| by_key.iter().find(|(_, id)| *id == &seat.seat_id));
                let second_key = seat_by_id
                    .get(second_seat.as_str())
                    .and_then(|seat| by_key.iter().find(|(_, id)| *id == &seat.seat_id));
                if let (Some((first_student, _)), Some((second_student, _))) =
                    (first_key, second_key)
                {
                    if let (Some(first_score), Some(second_score)) = (
                        score_by_key.get(first_student.as_str()),
                        score_by_key.get(second_student.as_str()),
                    ) {
                        gaps.push((first_score - second_score).abs());
                    }
                }
            }
            if gaps.is_empty() {
                not_available_dimension("No adjacent assigned pairs have score data.")
            } else {
                let gap_mean = mean(&gaps);
                let score_range = max_score - min_score;
                let normalized = (gap_mean / score_range * 100.0).min(100.0);
                score_dimension(
                    normalized,
                    Some(gap_mean),
                    balance_rule.weight as f64,
                    serde_json::json!({
                        "mean_adjacent_score_gap": gap_mean,
                        "score_range": score_range,
                        "adjacent_pair_count": gaps.len(),
                        "meaning": "Higher values indicate stronger mixing of different score levels across adjacent seats.",
                    }),
                )
            }
        }
    };

    // --- height_preference_score ---------------------------------------------
    let height_rule = &ctx.rules.soft.height_back;
    let height_preference_score = if !height_rule.enabled || height_rule.weight == 0 {
        not_available_dimension("height_back is disabled.")
    } else {
        let heights: Vec<f64> = ctx
            .students
            .iter()
            .filter_map(|student| student.height_cm)
            .collect();
        let distinct_heights: std::collections::HashSet<u64> =
            heights.iter().map(|height| height.to_bits()).collect();
        let min_row = ctx.min_row;
        let max_row = ctx.max_row;
        if heights.len() < 2 || distinct_heights.len() < 2 || min_row == max_row {
            not_available_dimension("Different heights and more than one seat row are required.")
        } else {
            let height_by_key: HashMap<&str, f64> = ctx
                .students
                .iter()
                .filter_map(|student| {
                    student
                        .height_cm
                        .map(|height| (student.key.as_str(), height))
                })
                .collect();
            let min_height = heights.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_height = heights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mut errors: Vec<f64> = Vec::new();
            for (student, seat) in assignment_vec.iter().enumerate() {
                let Some(height) = height_by_key.get(ctx.students[student].key.as_str()) else {
                    continue;
                };
                let seat = &ctx.layout.seats[*seat];
                let height_position = (height - min_height) / (max_height - min_height);
                let row_position = (seat.row - min_row) as f64 / (max_row - min_row) as f64;
                errors.push((height_position - row_position).abs());
            }
            if errors.is_empty() {
                not_available_dimension("No assignments have height data.")
            } else {
                let error_mean = mean(&errors);
                let score = (100.0 * (1.0 - error_mean)).max(0.0);
                score_dimension(
                    score,
                    Some(error_mean),
                    height_rule.weight as f64,
                    serde_json::json!({
                        "mean_normalized_position_error": error_mean,
                        "lower_error_is_better": true,
                    }),
                )
            }
        }
    };

    // --- vision_preference_score ---------------------------------------------
    let vision_rule = &ctx.rules.soft.vision_front;
    let vision_preference_score = if !vision_rule.enabled || vision_rule.weight == 0 {
        not_available_dimension("vision_front is disabled.")
    } else {
        let needing_front: Vec<&str> = ctx
            .students
            .iter()
            .filter(|student| student_needs_front(student))
            .map(|student| student.key.as_str())
            .collect();
        if needing_front.is_empty() {
            not_available_dimension("No students are marked as needing a front seat.")
        } else {
            let min_row = ctx.min_row;
            let max_row = ctx.max_row;
            let mut positions: Vec<f64> = Vec::new();
            for (student, seat) in assignment_vec.iter().enumerate() {
                if !needing_front.contains(&ctx.students[student].key.as_str()) {
                    continue;
                }
                let seat = &ctx.layout.seats[*seat];
                let normalized = if min_row == max_row {
                    0.0
                } else {
                    (seat.row - min_row) as f64 / (max_row - min_row) as f64
                };
                positions.push(normalized);
            }
            if positions.is_empty() {
                not_available_dimension("No front-seat preference assignments could be evaluated.")
            } else {
                let position_mean = mean(&positions);
                let score = (100.0 * (1.0 - position_mean)).max(0.0);
                score_dimension(
                    score,
                    Some(position_mean),
                    vision_rule.weight as f64,
                    serde_json::json!({
                        "students_needing_front": needing_front.len(),
                        "mean_normalized_row": position_mean,
                        "lower_row_value_is_better": true,
                    }),
                )
            }
        }
    };

    // --- diversity_score ------------------------------------------------------
    let diversity_score = match diversity_score {
        Some(score) => score_dimension(
            score,
            Some(score),
            ctx.rules.soft.randomize.weight.max(1) as f64,
            serde_json::json!({
                "meaning": "Mean percentage of students seated differently from the other candidates.",
            }),
        ),
        None => not_available_dimension("Diversity requires at least two generated candidates."),
    };

    // --- stability_score ------------------------------------------------------
    let stability_score = if latest_snapshot_json.trim().is_empty() {
        not_available_dimension("No previous snapshot was supplied.")
    } else {
        let snapshot: Value = serde_json::from_str(latest_snapshot_json)
            .map_err(|error| format!("invalid latest snapshot document: {error}"))?;
        let mut previous: HashMap<String, String> = HashMap::new();
        if let Some(assignments) = snapshot.get("assignments").and_then(Value::as_array) {
            for assignment in assignments {
                if let (Some(student), Some(seat)) = (
                    assignment.get("student_key").and_then(Value::as_str),
                    assignment.get("seat_id").and_then(Value::as_str),
                ) {
                    previous.insert(student.to_string(), seat.to_string());
                }
            }
        }
        if previous.is_empty() {
            not_available_dimension("The previous snapshot has no comparable students.")
        } else {
            let mut comparable = 0usize;
            let mut unchanged = 0usize;
            for (student, seat) in assignment_vec.iter().enumerate() {
                if let Some(previous_seat) = previous.get(ctx.students[student].key.as_str()) {
                    comparable += 1;
                    if previous_seat == &ctx.layout.seats[*seat].seat_id {
                        unchanged += 1;
                    }
                }
            }
            if comparable == 0 {
                not_available_dimension("The previous snapshot has no comparable students.")
            } else {
                let score = unchanged as f64 / comparable as f64 * 100.0;
                score_dimension(
                    score,
                    Some(unchanged as f64),
                    1.0,
                    serde_json::json!({
                        "unchanged_students": unchanged,
                        "changed_students": comparable - unchanged,
                        "comparable_students": comparable,
                        "meaning": "Higher values preserve more seats from the latest historical snapshot.",
                    }),
                )
            }
        }
    };

    // --- rule_scores ----------------------------------------------------------
    let evaluation = evaluate_soft_objectives(&by_key, &ctx.objective_context, &ctx.rules);
    let mut rule_scores = serde_json::Map::new();
    for (name, unavailable_reason) in [
        (
            "score_position",
            "At least two students with different scores are required.",
        ),
        (
            "score_distribution",
            "At least two populated rows or groups with score data are required.",
        ),
        (
            "mentor_pairing",
            "No deterministic mentor/learner pairs could be formed from the score data.",
        ),
    ] {
        let (enabled, weight) = match name {
            "score_position" => (
                ctx.rules.soft.score_position.enabled,
                ctx.rules.soft.score_position.weight,
            ),
            "score_distribution" => (
                ctx.rules.soft.score_distribution.enabled,
                ctx.rules.soft.score_distribution.weight,
            ),
            _ => (
                ctx.rules.soft.mentor_pairing.enabled,
                ctx.rules.soft.mentor_pairing.weight,
            ),
        };
        let dimension = if !enabled || weight == 0 {
            not_available_dimension(&format!("{name} is disabled."))
        } else {
            match evaluation.losses.get(name).copied().flatten() {
                Some(loss) => {
                    let mut details = evaluation
                        .details
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    if !evaluation.warnings.is_empty() {
                        details["warnings"] = serde_json::Value::Array(
                            evaluation
                                .warnings
                                .iter()
                                .map(|warning| serde_json::Value::String(warning.clone()))
                                .collect(),
                        );
                    }
                    score_dimension((1.0 - loss) * 100.0, Some(loss), weight as f64, details)
                }
                None => {
                    let reason = if name == "score_distribution" && !evaluation.warnings.is_empty()
                    {
                        evaluation.warnings[0].clone()
                    } else {
                        unavailable_reason.to_string()
                    };
                    not_available_dimension(&reason)
                }
            }
        };
        rule_scores.insert(format!("{name}_score"), dimension);
    }

    // --- hard_constraint_summary ---------------------------------------------
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    let mut violations: Vec<String> = Vec::new();
    let mut checked = 3usize;
    let mut student_seats: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut seat_owners: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for [student, seat] in assignment_pairs {
        student_seats.insert(*student);
        seat_owners.insert(*seat);
    }
    if student_seats.len() != assignment_pairs.len() {
        violations.push("A student is assigned more than once.".to_string());
    }
    if seat_owners.len() != assignment_pairs.len() {
        violations.push("A seat is assigned more than once.".to_string());
    }
    if student_seats.len() != request.student_count {
        violations
            .push("Assignments do not contain every current student exactly once.".to_string());
    }
    for [student, seat] in &request.fixed_seats {
        checked += 1;
        if probe[*student] != Some(*seat) {
            violations.push(format!(
                "fixed_seats is not satisfied for student {student}."
            ));
        }
    }
    for [first, second] in &resolved.must_be_adjacent {
        checked += 1;
        if !assigned_students_are_adjacent(&probe, &adjacency, *first, *second) {
            violations.push(format!(
                "must_be_adjacent is not satisfied for students {first}, {second}."
            ));
        }
    }
    for [first, second] in &resolved.cannot_be_adjacent {
        checked += 1;
        if assigned_students_are_adjacent(&probe, &adjacency, *first, *second) {
            violations.push(format!(
                "cannot_be_adjacent is not satisfied for students {first}, {second}."
            ));
        }
    }
    for rule in &request.min_distance {
        checked += 1;
        if !assigned_students_meet_distance(&request.seat_positions, &probe, &graph_distances, rule)
        {
            violations.push(format!(
                "min_distance is not satisfied for students {:?}.",
                rule.students
            ));
        }
    }
    let hard_constraint_summary = serde_json::json!({
        "satisfied": violations.is_empty(),
        "checked_rule_count": checked,
        "violation_count": violations.len(),
        "violations": violations,
        "details": {},
    });

    // --- total ---------------------------------------------------------------
    let mut available: Vec<(f64, f64)> = Vec::new();
    for dimension in [
        &fair_rotation_score,
        &avoid_recent_neighbors_score,
        &score_balance_score,
        &height_preference_score,
        &vision_preference_score,
        &diversity_score,
        &stability_score,
    ] {
        if dimension.get("status").and_then(Value::as_str) == Some("available")
            && dimension.get("score").and_then(Value::as_f64).is_some()
            && dimension
                .get("weight")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                > 0.0
        {
            available.push((
                dimension["score"].as_f64().unwrap(),
                dimension["weight"].as_f64().unwrap(),
            ));
        }
    }
    for dimension in rule_scores.values() {
        if dimension.get("status").and_then(Value::as_str) == Some("available")
            && dimension.get("score").and_then(Value::as_f64).is_some()
            && dimension
                .get("weight")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                > 0.0
        {
            available.push((
                dimension["score"].as_f64().unwrap(),
                dimension["weight"].as_f64().unwrap(),
            ));
        }
    }
    let total = if !violations.is_empty() {
        0.0
    } else if available.is_empty() {
        100.0
    } else {
        let total_weight: f64 = available.iter().map(|(_, weight)| weight).sum();
        let weighted: f64 = available.iter().map(|(score, weight)| score * weight).sum();
        (weighted / total_weight * 100.0).round() / 100.0
    };

    let breakdown = serde_json::json!({
        "fair_rotation_score": fair_rotation_score,
        "avoid_recent_neighbors_score": avoid_recent_neighbors_score,
        "score_balance_score": score_balance_score,
        "height_preference_score": height_preference_score,
        "vision_preference_score": vision_preference_score,
        "diversity_score": diversity_score,
        "stability_score": stability_score,
        "rule_scores": serde_json::Value::Object(rule_scores),
        "hard_constraint_summary": hard_constraint_summary,
    });
    let report = serde_json::json!({
        "total": total,
        "breakdown": breakdown,
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize plan score: {error}"))
}
