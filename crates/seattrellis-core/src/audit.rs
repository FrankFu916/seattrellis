// ---------------------------------------------------------------------------
// audit.rs — split from the former lib.rs monolith (plan 1.2: separate
// solver / evaluator / validation responsibilities into independent
// modules, each unit-testable).
//
// Audit report: hard-rule summary, soft breakdown, missing data, suggested actions.

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
