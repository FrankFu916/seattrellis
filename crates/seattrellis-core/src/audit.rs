// ---------------------------------------------------------------------------
// audit.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Audit report: hard-rule summary, soft breakdown, missing data, suggested actions.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::engine::{
    assignment_by_key, build_cost_context, full_solution_total_cost,
    solve_partial_assignment_valid, validate_solve_request,
};
use crate::evaluation::{
    assigned_students_are_adjacent, assigned_students_meet_distance, build_graph_distance_matrix,
    build_index_adjacency,
};
use crate::solver::{resolve_group_rules, CoreSolveRequest};
/// The UI consumes this to explain a candidate: which hard rules were
/// checked and satisfied, each soft objective's raw loss / weighted cost,
/// and warnings for rules that could not participate (missing data).
use crate::NATIVE_API_VERSION;

pub fn audit_report_json(request_json: &str, assignment: &[[usize; 2]]) -> Result<String, String> {
    // ---------------------------------------------------------------------------

    use serde_json::{json, Value};

    use crate::cost::{avoid_recent_neighbors_cost, individual_cost};
    use crate::models::effective_neighbor_rule;
    use crate::objectives::evaluate_soft_objectives;
    use crate::rng::SplitMix64;
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);

    // Rebuild the student->seat probe and validate independently (M3-05),
    // so the audit never blesses an illegal assignment. The assignment must
    // be complete and conflict-free: a missing or duplicated student would
    // otherwise be silently masked here.
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    let mut occupied = vec![false; request.seat_positions.len()];
    for [student, seat] in assignment {
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
    if !solve_partial_assignment_valid(&request, &resolved, &probe, &adjacency, &graph_distances) {
        return Err("assignment violates a hard rule".to_string());
    }

    // Hard-rule audit: how many rules of each kind were checked, and how
    // many hold. A full assignment makes every rule checkable.
    let fixed_ok = request
        .fixed_seats
        .iter()
        .filter(|[student, seat]| probe[*student] == Some(*seat))
        .count();
    let must_ok = resolved
        .must_be_adjacent
        .iter()
        .filter(|[a, b]| assigned_students_are_adjacent(&probe, &adjacency, *a, *b))
        .count();
    let cannot_ok = resolved
        .cannot_be_adjacent
        .iter()
        .filter(|[a, b]| !assigned_students_are_adjacent(&probe, &adjacency, *a, *b))
        .count();
    let distance_ok = request
        .min_distance
        .iter()
        .filter(|rule| {
            assigned_students_meet_distance(&request.seat_positions, &probe, &graph_distances, rule)
        })
        .count();

    let hard_rules = json!({
        "fixed_seats": { "checked": request.fixed_seats.len(), "satisfied": fixed_ok },
        "must_be_adjacent": { "checked": resolved.must_be_adjacent.len(), "satisfied": must_ok },
        "cannot_be_adjacent": { "checked": resolved.cannot_be_adjacent.len(), "satisfied": cannot_ok },
        "min_distance": { "checked": request.min_distance.len(), "satisfied": distance_ok },
    });

    // Soft-objective breakdown (raw loss / weight / weighted cost per rule).
    let ctx = build_cost_context(&request);
    let assignment_vec: Vec<usize> = probe.iter().map(|seat| seat.unwrap()).collect();
    let mut evaluation = evaluate_soft_objectives(
        &assignment_by_key(&probe, &ctx),
        &ctx.objective_context,
        &ctx.rules,
    );
    // score_balance is folded into full_solution_total_cost directly (not
    // through evaluate_soft_objectives); surface its breakdown here so the
    // audit covers every soft objective (plan §6.5).
    if ctx.rules.soft.score_balance.enabled && ctx.rules.soft.score_balance.weight != 0 {
        let mut loss = 0.0;
        for first in 0..assignment_vec.len() {
            let Some(first_score) = ctx.students[first].score else {
                continue;
            };
            for second in (first + 1)..assignment_vec.len() {
                let Some(second_score) = ctx.students[second].score else {
                    continue;
                };
                if adjacency[assignment_vec[first]].contains(&assignment_vec[second]) {
                    loss += (first_score - second_score).abs();
                }
            }
        }
        let weight = ctx.rules.soft.score_balance.weight as f64;
        evaluation
            .losses
            .insert("score_balance".to_string(), Some(loss));
        evaluation
            .weighted_costs
            .insert("score_balance".to_string(), -loss * weight);
    }

    // Top soft-cost contributors (plan §6.5 "最大贡献者"): each student's
    // soft contribution = individual cost + half of every pair cost
    // involving the student (score_balance + avoid_recent_neighbors). The
    // randomize term uses a fresh fixed-seed rng so the ranking is
    // deterministic.
    let neighbor_rule = effective_neighbor_rule(&ctx.rules);
    let mut contributions: Vec<(String, f64)> = Vec::new();
    for student in 0..assignment_vec.len() {
        let mut contribution = individual_cost(
            &ctx.students[student],
            &ctx.layout.seats[assignment_vec[student]],
            &ctx.layout,
            &ctx.rules,
            ctx.history.as_ref(),
            &mut SplitMix64::new(0),
            ctx.min_row,
            ctx.max_row,
        ) as f64;
        for other in 0..assignment_vec.len() {
            if other == student {
                continue;
            }
            let first_seat = assignment_vec[student];
            let second_seat = assignment_vec[other];
            if ctx.rules.soft.score_balance.enabled && ctx.rules.soft.score_balance.weight != 0 {
                if let (Some(first_score), Some(second_score)) =
                    (ctx.students[student].score, ctx.students[other].score)
                {
                    if adjacency[first_seat].contains(&second_seat) {
                        contribution -= 0.5
                            * ctx.rules.soft.score_balance.weight as f64
                            * (first_score - second_score).abs();
                    }
                }
            }
            if neighbor_rule.enabled && neighbor_rule.weight != 0 {
                contribution += 0.5
                    * avoid_recent_neighbors_cost(
                        &ctx.students[student].key,
                        &ctx.students[other].key,
                        &ctx.layout.seats[first_seat],
                        &ctx.layout.seats[second_seat],
                        &ctx.layout,
                        &neighbor_rule,
                        ctx.pair_history.as_ref(),
                        Some(&ctx.adjacency_edges),
                    ) as f64;
            }
        }
        contributions.push((ctx.students[student].key.clone(), contribution));
    }
    contributions.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let top_contributors: Vec<Value> = contributions
        .iter()
        .take(3)
        .map(|(student_key, contribution)| {
            json!({ "student_key": student_key, "contribution": contribution })
        })
        .collect();

    // --- UI-consumable explanation fields (plan §6.5) -----------------------
    // `hard_constraint_summary`: a single total view plus violation
    // witnesses. A full valid assignment has no violations, so the witness
    // list is empty by construction (the audit entry point rejects illegal
    // assignments); the field exists so the UI can render violations the
    // moment a partial/invalid plan is ever audited.
    let checked_rule_count = request.fixed_seats.len()
        + resolved.must_be_adjacent.len()
        + resolved.cannot_be_adjacent.len()
        + request.min_distance.len();
    let violation_count = checked_rule_count - (fixed_ok + must_ok + cannot_ok + distance_ok);
    let hard_constraint_summary = json!({
        "all_satisfied": violation_count == 0,
        "checked_rule_count": checked_rule_count,
        "violation_count": violation_count,
        "witnesses": [],
    });

    // `missing_data`: how many students lack each soft-input field, so the
    // UI can explain why a dimension is degraded (plan §6.5 "缺失数据").
    let mut missing_score = 0usize;
    let mut missing_height = 0usize;
    let mut missing_vision = 0usize;
    let mut missing_needs = 0usize;
    for student in &ctx.students {
        if student.score.is_none() {
            missing_score += 1;
        }
        if student.height_cm.is_none() {
            missing_height += 1;
        }
        if student.vision.is_none() {
            missing_vision += 1;
        }
        if student.needs.is_empty() {
            missing_needs += 1;
        }
    }
    let missing_data = json!({
        "students_missing_score": missing_score,
        "students_missing_height": missing_height,
        "students_missing_vision": missing_vision,
        "students_missing_needs": missing_needs,
    });

    // `history`: how much seat history the solve used (plan §6.5 "历史影响").
    let snapshot_count = ctx
        .history
        .as_ref()
        .map(|history| history.history_count.max(0) as usize)
        .unwrap_or(0);
    let history = json!({
        "snapshot_count": snapshot_count,
        "has_history": snapshot_count > 0,
    });

    // `suggested_actions`: localized message keys plus arguments the UI can
    // render without re-deriving the rules (plan §6.5 "可操作建议").
    let mut suggested_actions: Vec<Value> = Vec::new();
    let fair_rotation = &ctx.rules.soft.fair_rotation;
    if fair_rotation.enabled && fair_rotation.weight != 0 && snapshot_count == 0 {
        suggested_actions.push(json!({
            "message_key": "audit.history_recommended",
            "suggested_action": "add_history",
            "args": {},
        }));
    }
    if missing_height > 0 && ctx.rules.soft.height_back.enabled {
        suggested_actions.push(json!({
            "message_key": "audit.missing_height",
            "suggested_action": "add_student_field",
            "args": { "field": "height_cm", "count": missing_height },
        }));
    }
    if missing_vision > 0 && ctx.rules.soft.vision_front.enabled {
        suggested_actions.push(json!({
            "message_key": "audit.missing_vision",
            "suggested_action": "add_student_field",
            "args": { "field": "vision", "count": missing_vision },
        }));
    }
    if missing_score > 0
        && (ctx.rules.soft.score_balance.enabled || ctx.rules.soft.score_position.enabled)
    {
        suggested_actions.push(json!({
            "message_key": "audit.missing_score",
            "suggested_action": "add_student_field",
            "args": { "field": "score", "count": missing_score },
        }));
    }
    if suggested_actions.is_empty() {
        suggested_actions.push(json!({
            "message_key": "audit.ready",
            "suggested_action": "none",
            "args": {},
        }));
    }

    let report = json!({
        "api_version": NATIVE_API_VERSION,
        "hard_rules": hard_rules,
        "hard_constraint_summary": hard_constraint_summary,
        "soft_objectives": {
            "losses": evaluation.losses,
            "weighted_costs": evaluation.weighted_costs,
            "warnings": evaluation.warnings,
            "top_contributors": top_contributors,
        },
        "missing_data": missing_data,
        "history": history,
        "suggested_actions": suggested_actions,
        "total_cost": full_solution_total_cost(&assignment_vec, &adjacency, &ctx),
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize audit report: {error}"))
}

// ---------------------------------------------------------------------------

/// Diagnostics report (M5 B6 / D6): like [`audit_report_json`] but built for
/// *reporting* — the assignment must still be structurally complete and
/// conflict-free, but hard-rule violations are evaluated and itemized into
/// `witnesses` (with a suggested fix seat when one exists) instead of being
/// rejected. The audit stays the strict blessing validator; diagnostics is
/// what the UI renders as the issue list (D6).
pub fn diagnostics_report_json(
    request_json: &str,
    assignment: &[[usize; 2]],
) -> Result<String, String> {
    use serde_json::{json, Value};

    use crate::objectives::evaluate_soft_objectives;
    let request: CoreSolveRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid native solve request: {error}"))?;
    validate_solve_request(&request)?;
    let resolved = resolve_group_rules(&request)?;
    let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
    let graph_distances = build_graph_distance_matrix(&adjacency);
    let seat_ids: Vec<String> = (0..request.seat_positions.len())
        .map(|index| seat_id_for_index(&request, index))
        .collect();

    // Structural gate only (M3-05): a missing or duplicated student cannot
    // be itemized meaningfully, so diagnostics rejects exactly those shapes.
    let mut probe: Vec<Option<usize>> = vec![None; request.student_count];
    let mut occupied = vec![false; request.seat_positions.len()];
    for [student, seat] in assignment {
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

    // Hard-rule evaluation with per-violation witnesses.
    let mut fixed_witnesses: Vec<Value> = Vec::new();
    let mut must_witnesses: Vec<Value> = Vec::new();
    let mut cannot_witnesses: Vec<Value> = Vec::new();
    let mut distance_witnesses: Vec<Value> = Vec::new();

    for (index, [student, seat]) in request.fixed_seats.iter().enumerate() {
        if probe[*student] == Some(*seat) {
            continue;
        }
        let actual = probe[*student].unwrap_or(usize::MAX);
        let student_key = student_key_for_index(&request, *student);
        let actual_seat_id = if actual < seat_ids.len() {
            seat_ids[actual].clone()
        } else {
            "unseated".to_string()
        };
        fixed_witnesses.push(json!({
            "kind": "fixed_seats",
            "index": index,
            "seat_ids": [actual_seat_id.clone()],
            "args": {
                "student": student_key,
                "expected_seat": seat_ids[*seat],
                "actual_seat": actual_seat_id,
            },
            "suggested_fix": {
                "student": student_key,
                "seat_id": seat_ids[*seat],
            },
        }));
    }

    for (index, [a, b]) in resolved.must_be_adjacent.iter().enumerate() {
        if assigned_students_are_adjacent(&probe, &adjacency, *a, *b) {
            continue;
        }
        let fix = suggested_fix_seat(
            &probe,
            *a,
            probe[*b].unwrap_or(usize::MAX),
            &adjacency,
            &graph_distances,
            FixKind::MustBeAdjacent,
            0.0,
        );
        must_witnesses.push(witness_pair(
            "must_be_adjacent",
            index,
            &probe,
            &seat_ids,
            &request,
            *a,
            *b,
            fix,
        ));
    }

    for (index, [a, b]) in resolved.cannot_be_adjacent.iter().enumerate() {
        if !assigned_students_are_adjacent(&probe, &adjacency, *a, *b) {
            continue;
        }
        let fix = suggested_fix_seat(
            &probe,
            *a,
            probe[*b].unwrap_or(usize::MAX),
            &adjacency,
            &graph_distances,
            FixKind::CannotBeAdjacent,
            0.0,
        );
        cannot_witnesses.push(witness_pair(
            "cannot_be_adjacent",
            index,
            &probe,
            &seat_ids,
            &request,
            *a,
            *b,
            fix,
        ));
    }

    for (index, rule) in request.min_distance.iter().enumerate() {
        if assigned_students_meet_distance(&request.seat_positions, &probe, &graph_distances, rule)
        {
            continue;
        }
        let fix = suggested_fix_seat(
            &probe,
            rule.students[0],
            probe[rule.students[1]].unwrap_or(usize::MAX),
            &adjacency,
            &graph_distances,
            FixKind::MinDistance,
            rule.distance,
        );
        distance_witnesses.push(witness_pair(
            "min_distance",
            index,
            &probe,
            &seat_ids,
            &request,
            rule.students[0],
            rule.students[1],
            fix,
        ));
    }

    let checked_rule_count = request.fixed_seats.len()
        + resolved.must_be_adjacent.len()
        + resolved.cannot_be_adjacent.len()
        + request.min_distance.len();
    let violation_count = fixed_witnesses.len()
        + must_witnesses.len()
        + cannot_witnesses.len()
        + distance_witnesses.len();
    let witnesses: Vec<Value> = fixed_witnesses
        .iter()
        .cloned()
        .chain(must_witnesses.iter().cloned())
        .chain(cannot_witnesses.iter().cloned())
        .chain(distance_witnesses.iter().cloned())
        .collect();
    let hard_constraint_summary = json!({
        "all_satisfied": violation_count == 0,
        "checked_rule_count": checked_rule_count,
        "violation_count": violation_count,
        "witnesses": witnesses,
    });
    let hard_rules = json!({
        "fixed_seats": {
            "checked": request.fixed_seats.len(),
            "satisfied": request.fixed_seats.len() - fixed_witnesses.len(),
        },
        "must_be_adjacent": {
            "checked": resolved.must_be_adjacent.len(),
            "satisfied": resolved.must_be_adjacent.len() - must_witnesses.len(),
        },
        "cannot_be_adjacent": {
            "checked": resolved.cannot_be_adjacent.len(),
            "satisfied": resolved.cannot_be_adjacent.len() - cannot_witnesses.len(),
        },
        "min_distance": {
            "checked": request.min_distance.len(),
            "satisfied": request.min_distance.len() - distance_witnesses.len(),
        },
    });

    // Soft-objective breakdown (same shape as the audit).
    let ctx = build_cost_context(&request);
    let assignment_vec: Vec<usize> = probe.iter().map(|seat| seat.unwrap()).collect();
    let mut evaluation = evaluate_soft_objectives(
        &assignment_by_key(&probe, &ctx),
        &ctx.objective_context,
        &ctx.rules,
    );
    if ctx.rules.soft.score_balance.enabled && ctx.rules.soft.score_balance.weight != 0 {
        let mut loss = 0.0;
        for first in 0..assignment_vec.len() {
            let Some(first_score) = ctx.students[first].score else {
                continue;
            };
            for second in (first + 1)..assignment_vec.len() {
                let Some(second_score) = ctx.students[second].score else {
                    continue;
                };
                if adjacency[assignment_vec[first]].contains(&assignment_vec[second]) {
                    loss += (first_score - second_score).abs();
                }
            }
        }
        let weight = ctx.rules.soft.score_balance.weight as f64;
        evaluation
            .losses
            .insert("score_balance".to_string(), Some(loss));
        evaluation
            .weighted_costs
            .insert("score_balance".to_string(), -loss * weight);
    }
    let top_contributors: Vec<Value> = evaluation
        .weighted_costs
        .iter()
        .filter(|(_, cost)| cost.is_finite() && **cost > 0.0)
        .map(|(name, cost)| json!({ "rule": name, "cost": cost }))
        .collect();
    let mut missing_score = 0usize;
    let mut missing_height = 0usize;
    let mut missing_vision = 0usize;
    let mut missing_needs = 0usize;
    for student in &ctx.students {
        if student.score.is_none() {
            missing_score += 1;
        }
        if student.height_cm.is_none() {
            missing_height += 1;
        }
        if student.vision.is_none() {
            missing_vision += 1;
        }
        if student.needs.is_empty() {
            missing_needs += 1;
        }
    }
    let missing_data = json!({
        "students_missing_score": missing_score,
        "students_missing_height": missing_height,
        "students_missing_vision": missing_vision,
        "students_missing_needs": missing_needs,
    });
    let snapshot_count = ctx
        .history
        .as_ref()
        .map(|history| history.history_count.max(0) as usize)
        .unwrap_or(0);
    let history = json!({
        "snapshot_count": snapshot_count,
        "has_history": snapshot_count > 0,
    });

    let mut suggested_actions: Vec<Value> = Vec::new();
    for witness in &hard_constraint_summary["witnesses"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let kind = witness["kind"].as_str().unwrap_or("rule");
        suggested_actions.push(json!({
            "message_key": format!("audit.witness_{kind}"),
            "suggested_action": "fix_violation",
            "args": witness["args"].clone(),
        }));
    }
    let fair_rotation = &ctx.rules.soft.fair_rotation;
    if fair_rotation.enabled && fair_rotation.weight != 0 && snapshot_count == 0 {
        suggested_actions.push(json!({
            "message_key": "audit.history_recommended",
            "suggested_action": "add_history",
            "args": {},
        }));
    }
    if missing_height > 0 && ctx.rules.soft.height_back.enabled {
        suggested_actions.push(json!({
            "message_key": "audit.missing_height",
            "suggested_action": "add_student_field",
            "args": { "field": "height_cm", "count": missing_height },
        }));
    }
    if missing_vision > 0 && ctx.rules.soft.vision_front.enabled {
        suggested_actions.push(json!({
            "message_key": "audit.missing_vision",
            "suggested_action": "add_student_field",
            "args": { "field": "vision", "count": missing_vision },
        }));
    }
    if missing_score > 0
        && (ctx.rules.soft.score_balance.enabled || ctx.rules.soft.score_position.enabled)
    {
        suggested_actions.push(json!({
            "message_key": "audit.missing_score",
            "suggested_action": "add_student_field",
            "args": { "field": "score", "count": missing_score },
        }));
    }
    if suggested_actions.is_empty() {
        suggested_actions.push(json!({
            "message_key": "audit.ready",
            "suggested_action": "none",
            "args": {},
        }));
    }

    let report = json!({
        "api_version": NATIVE_API_VERSION,
        "hard_rules": hard_rules,
        "hard_constraint_summary": hard_constraint_summary,
        "soft_objectives": {
            "losses": evaluation.losses,
            "weighted_costs": evaluation.weighted_costs,
            "warnings": evaluation.warnings,
            "top_contributors": top_contributors,
        },
        "missing_data": missing_data,
        "history": history,
        "suggested_actions": suggested_actions,
        "total_cost": full_solution_total_cost(&assignment_vec, &adjacency, &ctx),
    });
    serde_json::to_string(&report)
        .map_err(|error| format!("could not serialize diagnostics report: {error}"))
}

/// Which hard rule a suggested-fix seat must satisfy.
#[derive(Debug, Clone, Copy)]
enum FixKind {
    MustBeAdjacent,
    CannotBeAdjacent,
    MinDistance,
}

/// First seat that resolves the violated rule for `student` given `other`'s
/// current seat, preferring unoccupied seats. `None` when nothing resolves
/// (the UI then offers no one-click fix for that witness).
#[allow(clippy::too_many_arguments)]
fn suggested_fix_seat(
    probe: &[Option<usize>],
    student: usize,
    other_seat: usize,
    adjacency: &[Vec<usize>],
    graph_distances: &[Vec<Option<u32>>],
    kind: FixKind,
    min_distance: f64,
) -> Option<usize> {
    let current = probe.get(student).copied().flatten();
    let occupied: HashSet<usize> = probe.iter().flatten().copied().collect();
    let satisfies = |seat: usize| -> bool {
        let adjacent =
            adjacency[seat].contains(&other_seat) || adjacency[other_seat].contains(&seat);
        match kind {
            FixKind::MustBeAdjacent => adjacent,
            FixKind::CannotBeAdjacent => !adjacent,
            FixKind::MinDistance => graph_distances[seat][other_seat]
                .map(|distance| (distance as f64) >= min_distance)
                .unwrap_or(true),
        }
    };
    for seat in 0..probe.len() {
        if Some(seat) == current || seat == other_seat || !satisfies(seat) {
            continue;
        }
        if !occupied.contains(&seat) {
            return Some(seat);
        }
    }
    // Fall back to any satisfying seat (the editor unseats the occupant).
    for seat in 0..probe.len() {
        if Some(seat) == current || seat == other_seat || !satisfies(seat) {
            continue;
        }
        return Some(seat);
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn witness_pair(
    kind: &str,
    index: usize,
    probe: &[Option<usize>],
    seat_ids: &[String],
    request: &CoreSolveRequest,
    a: usize,
    b: usize,
    fix: Option<usize>,
) -> Value {
    let seat_of = |student: usize| -> String {
        probe
            .get(student)
            .copied()
            .flatten()
            .map(|seat| seat_ids[seat].clone())
            .unwrap_or_else(|| "unseated".to_string())
    };
    let mut witness = json!({
        "kind": kind,
        "index": index,
        "seat_ids": [seat_of(a), seat_of(b)],
        "args": {
            "student_a": student_key_for_index(request, a),
            "student_b": student_key_for_index(request, b),
        },
    });
    if kind == "min_distance" {
        if let Some(rule) = request.min_distance.get(index) {
            witness["args"]["distance"] = json!(rule.distance);
        }
    }
    if let Some(fix_seat) = fix {
        witness["suggested_fix"] = json!({
            "student": student_key_for_index(request, a),
            "seat_id": seat_ids[fix_seat],
        });
    }
    witness
}

/// Student key for an index, using the same fallback as the draft builder.
fn student_key_for_index(request: &CoreSolveRequest, index: usize) -> String {
    request
        .students
        .get(index)
        .map(|student| student.key.clone())
        .unwrap_or_else(|| format!("student-{}", index + 1))
}

/// Seat id for an index (same convention as the draft builder).
fn seat_id_for_index(request: &CoreSolveRequest, index: usize) -> String {
    request
        .layout
        .as_ref()
        .and_then(|layout| layout.seats.get(index))
        .map(|seat| seat.seat_id.clone())
        .unwrap_or_else(|| format!("seat-{}", index + 1))
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod diagnostics_tests {
    use super::*;
    use serde_json::json;

    fn problem() -> String {
        json!({
            "api_version": 2,
            "student_count": 6,
            "seat_positions": [
                [1.0,1.0],[2.0,1.0],[3.0,1.0],
                [1.0,2.0],[2.0,2.0],[3.0,2.0],
                [1.0,3.0],[2.0,3.0],[3.0,3.0]
            ],
            "edges": [[0,1],[1,2],[0,3],[1,4],[2,5],[3,4],[4,5],[3,6],[4,7],[5,8],[6,7],[7,8]],
            "fixed_seats": [[0, 0]],
            "min_distance": [{
                "students": [1, 2],
                "distance": 2.0,
                "metric": "graph"
            }],
            "students": [
                {"key": "S01"}, {"key": "S02"}, {"key": "S03"},
                {"key": "S04"}, {"key": "S05"}, {"key": "S06"}
            ]
        })
        .to_string()
    }

    #[test]
    fn clean_assignment_reports_no_witnesses() {
        // S01 at seat 0 (fixed), S02 at seat 8, S03 at seat 2:
        // graph distance between 8 and 2 is 2 >= 2.
        let assignment = [[0, 0], [1, 8], [2, 2], [3, 4], [4, 5], [5, 6]];
        let report: Value = serde_json::from_str(
            &diagnostics_report_json(&problem(), &assignment).expect("reports"),
        )
        .unwrap();
        let summary = &report["hard_constraint_summary"];
        assert_eq!(summary["all_satisfied"], true);
        assert_eq!(summary["violation_count"], 0);
        assert_eq!(summary["witnesses"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn fixed_seat_violation_has_a_witness_and_fix() {
        // S01 sits at seat 1 instead of the fixed seat 0.
        let assignment = [[0, 1], [1, 8], [2, 2], [3, 4], [4, 5], [5, 6]];
        let report: Value = serde_json::from_str(
            &diagnostics_report_json(&problem(), &assignment).expect("reports"),
        )
        .unwrap();
        let summary = &report["hard_constraint_summary"];
        assert_eq!(summary["all_satisfied"], false);
        assert_eq!(summary["violation_count"], 1);
        let witness = &summary["witnesses"][0];
        assert_eq!(witness["kind"], "fixed_seats");
        assert_eq!(witness["args"]["student"], "S01");
        assert_eq!(witness["args"]["expected_seat"], "seat-1");
        assert_eq!(witness["suggested_fix"]["seat_id"], "seat-1");
    }

    #[test]
    fn min_distance_violation_suggests_a_resolving_seat() {
        // S02 at seat 3, S03 at seat 4: graph distance 1 < 2.
        let assignment = [[0, 0], [1, 3], [2, 4], [3, 6], [4, 7], [5, 8]];
        let report: Value = serde_json::from_str(
            &diagnostics_report_json(&problem(), &assignment).expect("reports"),
        )
        .unwrap();
        let summary = &report["hard_constraint_summary"];
        assert_eq!(summary["all_satisfied"], false);
        let witness = &summary["witnesses"][0];
        assert_eq!(witness["kind"], "min_distance");
        assert_eq!(witness["args"]["student_a"], "S02");
        let fix = witness["suggested_fix"].as_object();
        assert!(fix.is_some(), "min-distance witness should offer a fix");
        // The suggested seat must actually resolve the rule against S03's seat.
        let fix_seat = fix.unwrap()["seat_id"].as_str().unwrap();
        let index = fix_seat
            .strip_prefix("seat-")
            .unwrap()
            .parse::<usize>()
            .unwrap()
            - 1;
        let adjacency = build_index_adjacency(
            9,
            &[
                [0, 1],
                [1, 2],
                [0, 3],
                [1, 4],
                [2, 5],
                [3, 4],
                [4, 5],
                [3, 6],
                [4, 7],
                [5, 8],
                [6, 7],
                [7, 8],
            ],
        );
        let distances = build_graph_distance_matrix(&adjacency);
        let distance = distances[index][3].unwrap();
        assert!(
            distance >= 2,
            "suggested seat {fix_seat} must satisfy distance >= 2"
        );
    }

    #[test]
    fn incomplete_assignment_is_rejected() {
        let assignment = [[0, 0], [1, 8]];
        assert!(diagnostics_report_json(&problem(), &assignment).is_err());
    }
}
