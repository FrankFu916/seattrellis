//! Regression contracts from the October 2026 core audit. Assertions describe
//! the requested semantics rather than freezing solver-specific assignments.
use seattrellis_core::{
    diagnostics_report_json, generate_candidates_json,
    generate_candidates_json_with_latest_snapshot_and_control, pair_report_json, repair_json,
    repair_json_with_control, score_assignment_json, solve_problem, solve_problem_json,
    solve_problem_with_control, validate_solve_request_json, validate_solve_response,
    CoreSolveRequest, CoreSolveResponse, SolveControl, SolveStatus,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn disabled_defaults() -> Value {
    json!({"vision_front":{"enabled":false}, "height_back":{"enabled":false}, "randomize":{"enabled":false}})
}

fn custom_adjacency() -> Value {
    let mut soft = disabled_defaults();
    soft["score_balance"] = json!({"enabled":true,"weight":1});
    json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[10,0],[20,0]],
        "edges":[[0,1]],"fixed_seats":[[0,0]],"student_scores":[100,0],"seed":0,"rules":{"soft":soft}})
}

#[test]
fn layout_is_one_canonical_enabled_index_domain() {
    let base = json!({"api_version":2,"student_count":1,"seat_positions":[[0,0],[1,0]],
        "layout":{"seats":[{"seat_id":"a","row":0,"col":0},{"seat_id":"b","row":0,"col":1}]}});
    validate_solve_request_json(&base.to_string()).unwrap();
    let mut extra = base.clone();
    extra["layout"]["seats"]
        .as_array_mut()
        .unwrap()
        .push(json!({"seat_id":"c","row":0,"col":2}));
    assert!(validate_solve_request_json(&extra.to_string())
        .unwrap_err()
        .contains("one-to-one"));
    let mut disabled = base.clone();
    disabled["layout"]["seats"][1]["enabled"] = json!(false);
    assert!(validate_solve_request_json(&disabled.to_string())
        .unwrap_err()
        .contains("must be enabled"));
    let mut duplicate = base.clone();
    duplicate["layout"]["seats"][1]["seat_id"] = json!("a");
    assert!(validate_solve_request_json(&duplicate.to_string())
        .unwrap_err()
        .contains("unique"));
    let mut mismatch = base;
    mismatch["layout"]["seats"][1]["x"] = json!(99.0);
    assert!(validate_solve_request_json(&mismatch.to_string())
        .unwrap_err()
        .contains("coordinates must match"));
}

#[test]
fn scorer_and_recommendation_honor_requested_topology() {
    let request = custom_adjacency().to_string();
    let strong: Value = serde_json::from_str(
        &score_assignment_json(&request, &[[0, 0], [1, 1]], "", None).unwrap(),
    )
    .unwrap();
    let weak: Value = serde_json::from_str(
        &score_assignment_json(&request, &[[0, 0], [1, 2]], "", None).unwrap(),
    )
    .unwrap();
    assert_eq!(strong["breakdown"]["score_balance_score"]["score"], 100.0);
    assert_eq!(
        strong["breakdown"]["score_balance_score"]["details"]["adjacent_pair_count"],
        1
    );
    assert_eq!(
        weak["breakdown"]["score_balance_score"]["status"],
        "not_available"
    );
    let candidates: Value =
        serde_json::from_str(&generate_candidates_json(&request, 2).unwrap()).unwrap();
    let recommended = candidates["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["candidate_id"] == candidates["recommended_candidate_id"])
        .unwrap();
    assert_eq!(recommended["assignment"], json!([[0, 0], [1, 1]]));
    assert_eq!(recommended["total_cost"], -100.0);
}

#[test]
fn candidate_diversity_uses_percent_and_needs_an_actual_peer() {
    let request = custom_adjacency().to_string();
    let candidates: Value =
        serde_json::from_str(&generate_candidates_json(&request, 2).unwrap()).unwrap();
    for candidate in candidates["candidates"].as_array().unwrap() {
        let diversity = &candidate["plan_score"]["breakdown"]["diversity_score"];
        assert_eq!(
            diversity["score"], 50.0,
            "one of two students changes seats"
        );
        assert_eq!(diversity["rating"], "medium");
    }
    let single: Value =
        serde_json::from_str(&generate_candidates_json(&request, 1).unwrap()).unwrap();
    assert_eq!(
        single["candidates"][0]["plan_score"]["breakdown"]["diversity_score"]["status"],
        "not_available"
    );
    // Asking for two candidates still yields only one when every seat is fixed.
    let only =
        json!({"api_version":2,"student_count":1,"seat_positions":[[0,0]],"fixed_seats":[[0,0]]});
    let exhausted: Value =
        serde_json::from_str(&generate_candidates_json(&only.to_string(), 2).unwrap()).unwrap();
    assert_eq!(exhausted["candidate_count"], 1);
    assert_eq!(
        exhausted["candidates"][0]["plan_score"]["breakdown"]["diversity_score"]["status"],
        "not_available"
    );
}

#[test]
fn equal_candidate_scores_recommend_the_first_identifier() {
    let request = json!({"api_version":2,"student_count":1,"seat_positions":[[0,0],[1,0]],"rules":{"soft":disabled_defaults()}});
    let candidates: Value =
        serde_json::from_str(&generate_candidates_json(&request.to_string(), 2).unwrap()).unwrap();
    assert_eq!(
        candidates["candidates"][0]["plan_score"]["total"],
        candidates["candidates"][1]["plan_score"]["total"]
    );
    assert_eq!(candidates["recommended_candidate_id"], "candidate_01");
}

#[test]
fn local_repair_expands_distance_and_group_hard_dependencies() {
    let distance = json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0],[2,0]],
        "students":[{"key":"a"},{"key":"b"}], "min_distance":[{"students":[0,1],"distance":2,"metric":"euclidean"}]});
    let snapshot = json!({"assignments":[{"student_key":"a","seat_id":"seat-1"},{"student_key":"b","seat_id":"seat-2"}]});
    let repaired: Value = serde_json::from_str(
        &repair_json(
            &distance.to_string(),
            &snapshot.to_string(),
            &["a".into()],
            &[],
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    let seats: Vec<_> = repaired["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|assignment| assignment["seat_id"].as_str().unwrap())
        .collect();
    assert!(
        seats.contains(&"seat-1") && seats.contains(&"seat-3"),
        "distance 2 requires endpoints"
    );
    assert_eq!(repaired["summary"]["affected_students"], json!(["a", "b"]));
    let group = json!({"api_version":2,"student_count":3,"seat_positions":[[0,0],[1,0],[2,0],[3,0]],
        "edges":[[0,1]],"students":[{"key":"a"},{"key":"b"},{"key":"c"}],
        "rules":{"groups":[{"name":"partners","students":["a","b"],"together":true}]}});
    let snapshot = json!({"assignments":[{"student_key":"a","seat_id":"seat-1"},{"student_key":"b","seat_id":"seat-3"},{"student_key":"c","seat_id":"seat-4"}]});
    let repaired: Value = serde_json::from_str(
        &repair_json(
            &group.to_string(),
            &snapshot.to_string(),
            &["a".into()],
            &[],
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(repaired["summary"]["affected_students"], json!(["a", "b"]));
    let assignments = repaired["assignments"].as_array().unwrap();
    assert_eq!(
        assignments
            .iter()
            .find(|assignment| assignment["student_key"] == "c")
            .unwrap()["seat_id"],
        "seat-4"
    );
    assert!(assignments
        .iter()
        .filter(|assignment| assignment["student_key"] != "c")
        .all(|assignment| assignment["seat_id"] == "seat-1" || assignment["seat_id"] == "seat-2"));
}

#[test]
fn extreme_scores_are_invalid_and_solved_products_require_finite_costs() {
    let mut request = custom_adjacency();
    request["student_scores"] = json!([1e308, -1e308]);
    assert!(solve_problem_json(&request.to_string())
        .unwrap_err()
        .contains("between"));
    let request: CoreSolveRequest = serde_json::from_value(custom_adjacency()).unwrap();
    let mut response = solve_problem(&request).unwrap();
    for cost in [Some(f64::NAN), Some(f64::INFINITY), Some(f64::NEG_INFINITY)] {
        response.total_cost = cost;
        assert!(validate_solve_response(&request, &response)
            .unwrap_err()
            .contains("total_cost must be finite"));
    }
    response.total_cost = None;
    validate_solve_response(&request, &response).expect("legacy responses may omit optional cost");
}

#[test]
fn preparation_obeys_deadlines_without_unused_quadratic_topology_work() {
    let request = json!({"api_version":2,"student_count":1,
        "seat_positions":(0..4000).map(|index| [index as f64,0.0]).collect::<Vec<_>>(),
        "time_limit_seconds":0.01});
    let started = Instant::now();
    let response: CoreSolveResponse =
        serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
    assert!(matches!(
        response.status,
        SolveStatus::Solved | SolveStatus::Timeout
    ));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "10ms budget should not require constructing a 4000x4000 matrix"
    );
    let request: CoreSolveRequest = serde_json::from_value(request).unwrap();
    let control = SolveControl::new();
    control.cancel();
    let response = solve_problem_with_control(&request, &control).unwrap();
    assert_eq!(response.status, SolveStatus::Cancelled);
    assert_eq!(response.attempts_used, 0);
    assert!(response.assignment.is_empty());
}

#[test]
fn candidate_generation_and_repair_reuse_the_callers_cancellation() {
    let control = SolveControl::new();
    control.cancel();
    let request = custom_adjacency().to_string();
    assert!(
        generate_candidates_json_with_latest_snapshot_and_control(&request, 2, "", &control)
            .unwrap_err()
            .contains("Cancelled")
    );
    let snapshot = json!({"assignments":[{"student_key":"STU001","seat_id":"seat-1"},{"student_key":"STU002","seat_id":"seat-2"}]}).to_string();
    assert!(
        repair_json_with_control(&request, &snapshot, &[], &[], &[], true, &control)
            .unwrap_err()
            .contains("Cancelled")
    );
}

#[test]
fn cooling_expires_by_global_period_while_legacy_occurrence_windows_remain_readable() {
    let mut soft = disabled_defaults();
    soft["cooling"] = json!({"enabled":true,"weight":1,"cooling_period":3,"relation_types":["adjacent_any"],"within_distance":2});
    let mut request = json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0],[6,0]],
        "edges":[[0,1]],"students":[{"key":"a"},{"key":"b"}],"rules":{"soft":soft},
        "pair_history":{"history_count":6,"pairs":{"a|b":{"records":[{"period_index":1,"relations":["adjacent_any"]}]}}}});
    let expired: Value = serde_json::from_str(
        &score_assignment_json(&request.to_string(), &[[0, 0], [1, 1]], "", None).unwrap(),
    )
    .unwrap();
    assert_eq!(
        expired["breakdown"]["avoid_recent_neighbors_score"]["details"]["penalty_cost"],
        0
    );
    request["pair_history"]["pairs"]["a|b"]["records"][0]["period_index"] = json!(4);
    let recent: Value = serde_json::from_str(
        &score_assignment_json(&request.to_string(), &[[0, 0], [1, 1]], "", None).unwrap(),
    )
    .unwrap();
    assert_eq!(
        recent["breakdown"]["avoid_recent_neighbors_score"]["details"]["penalty_cost"],
        100
    );
    request["pair_history"]["pairs"]["a|b"]["records"][0]
        .as_object_mut()
        .unwrap()
        .remove("period_index");
    let legacy: Value = serde_json::from_str(
        &score_assignment_json(&request.to_string(), &[[0, 0], [1, 1]], "", None).unwrap(),
    )
    .unwrap();
    assert_eq!(
        legacy["breakdown"]["avoid_recent_neighbors_score"]["details"]["penalty_cost"],
        100
    );
    let snapshots: Vec<Value> = (0..6).map(|index| json!({"assignments":[{"student_key":"a","seat_id":"seat-1"},{"student_key":"b","seat_id":if index==0 {"seat-2"} else {"seat-3"}}]})).collect();
    let report: Value = serde_json::from_str(
        &pair_report_json(
            &request.to_string(),
            &serde_json::to_string(&snapshots).unwrap(),
            10,
            2,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["top_pairs"][0]["total_occurrences"], 1);
    assert_eq!(report["top_pairs"][0]["recent_occurrences"], 0);
}

#[test]
fn compiled_neighbor_rule_preserves_cooling_cost_and_candidate_ranking() {
    for (neighbors, cooling, expected_cost) in [
        (false, false, 0.0),
        (true, false, 0.0),
        (false, true, 300.0),
        (true, true, 500.0),
    ] {
        let mut soft = disabled_defaults();
        soft["avoid_recent_neighbors"] = json!({"enabled":neighbors,"weight":2,
            "relation_types":["adjacent_any"],"lookback":1,"max_recent_count":1});
        soft["cooling"] = json!({"enabled":cooling,"weight":3,
            "relation_types":["adjacent_any"],"cooling_period":2});
        let mut request = json!({"api_version":2,"student_count":2,
            "seat_positions":[[0,0],[1,0],[6,0]],"edges":[[0,1]],
            "students":[{"key":"a"},{"key":"b"}],"fixed_seats":[[0,0],[1,1]],
            "rules":{"soft":soft},"pair_history":{"history_count":2,
                "pairs":{"a|b":{"records":[{"period_index":2,"relations":["adjacent_any"]}]}}}});
        let fixed: CoreSolveResponse =
            serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
        assert_eq!(fixed.total_cost, Some(expected_cost));

        if expected_cost > 0.0 {
            request["fixed_seats"] = json!([[0, 0]]);
            let free: CoreSolveResponse =
                serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
            assert_eq!(free.total_cost, Some(0.0));
            let second_seat = free.assignment.iter().find(|pair| pair[0] == 1).unwrap()[1];
            assert_ne!(second_seat, 1, "ranking must avoid the penalized neighbor");
        }
    }
}

#[test]
fn score_objective_allocation_guard_preserves_enabled_position_costs() {
    for (enabled, weight, expected_cost) in [(false, 2, 0.0), (true, 0, 0.0), (true, 2, 200.0)] {
        let mut soft = disabled_defaults();
        soft["score_position"] =
            json!({"enabled":enabled,"weight":weight,"direction":"high_front"});
        let request = json!({"api_version":2,"student_count":2,
            "seat_positions":[[0,0],[0,1]],"student_scores":[100,0],
            "fixed_seats":[[0,1],[1,0]],"rules":{"soft":soft}});
        let response: CoreSolveResponse =
            serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
        assert_eq!(response.total_cost, Some(expected_cost));
    }
}

#[test]
fn diagnostics_use_the_requested_distance_metric_and_all_available_seats() {
    let request = json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0],[5,0]],
        "edges":[],"min_distance":[{"students":[0,1],"distance":3,"metric":"euclidean"}]});
    let report: Value = serde_json::from_str(
        &diagnostics_report_json(&request.to_string(), &[[0, 0], [1, 1]]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["hard_constraint_summary"]["witnesses"][0]["suggested_fix"]["seat_id"], "seat-3",
        "graph disconnection alone cannot fix a Euclidean violation"
    );
}

#[test]
fn partial_rule_documents_use_declared_defaults_and_validate_parameters() {
    use seattrellis_core::models::{CoolingRule, MentorPairingRule, RuleSet};
    let cooling: CoolingRule = serde_json::from_value(json!({"enabled":true})).unwrap();
    assert_eq!(cooling.weight, 5);
    assert_eq!(cooling.cooling_period, 3);
    assert_eq!(cooling.relation_types, vec!["desk_mate", "adjacent_any"]);
    let mentor: MentorPairingRule = serde_json::from_value(json!({"enabled":true})).unwrap();
    assert_eq!(mentor.mentor_percentile, 0.75);
    assert_eq!(mentor.learner_percentile, 0.25);
    assert!(mentor.avoid_recent_repeats);
    let rules: RuleSet = serde_json::from_value(json!({"soft":{}})).unwrap();
    assert_eq!(rules.soft, RuleSet::default().soft);
    let mut request = custom_adjacency();
    request["rules"]["soft"]["mentor_pairing"] = json!({"enabled":true,"mentor_percentile":2.0});
    assert!(validate_solve_request_json(&request.to_string())
        .unwrap_err()
        .contains("between 0 and 1"));
    request["rules"]["soft"]["mentor_pairing"] = json!({"enabled":false});
    request["rules"]["soft"]["cooling"] = json!({"enabled":true,"cooling_period":-1});
    assert!(validate_solve_request_json(&request.to_string())
        .unwrap_err()
        .contains("cooling_period"));
}

#[test]
fn disabled_seats_can_be_interspersed_without_changing_index_meaning() {
    let request = json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0]],
        "students":[{"key":"a"},{"key":"b"}],"fixed_seats":[[0,0],[1,1]],
        "layout":{"seats":[{"seat_id":"a0","row":0,"col":0},{"seat_id":"disabled","row":0,"col":99,"enabled":false},{"seat_id":"b1","row":0,"col":1}]}});
    let snapshot = json!({"assignments":[{"student_key":"a","seat_id":"a0"},{"student_key":"b","seat_id":"b1"}]});
    let repaired: Value = serde_json::from_str(
        &repair_json(&request.to_string(), &snapshot.to_string(), &[], &[], &[]).unwrap(),
    )
    .unwrap();
    assert_eq!(repaired["assignments"][0]["seat_id"], "a0");
    assert_eq!(repaired["assignments"][1]["seat_id"], "b1");
    let response: CoreSolveResponse =
        serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
    assert_eq!(response.assignment, vec![[0, 0], [1, 1]]);
}

#[test]
fn malformed_indexed_histories_and_nonfinite_diversity_are_rejected() {
    let mut request = custom_adjacency();
    request["pair_history"] = json!({"history_count":6,"pairs":{"a|b":{"records":[{"period_index":1,"relations":["adjacent_any"]},{"relations":["adjacent_any"]}]}}});
    assert!(validate_solve_request_json(&request.to_string())
        .unwrap_err()
        .contains("cannot mix"));
    request["pair_history"]["pairs"]["a|b"]["records"][1]["period_index"] = json!(7);
    assert!(validate_solve_request_json(&request.to_string())
        .unwrap_err()
        .contains("within 1..=history_count"));
    for score in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
        assert!(score_assignment_json(
            &custom_adjacency().to_string(),
            &[[0, 0], [1, 1]],
            "",
            Some(score)
        )
        .unwrap_err()
        .contains("invalid diversity_score"));
    }
}

#[test]
fn fairness_windows_count_periods_where_the_student_is_absent() {
    use seattrellis_core::models::{SeatHistory, StudentSeatHistory};
    let student: StudentSeatHistory = serde_json::from_value(json!({"category_counts":{"front":1},"records":[{"period_index":1,"categories":["front"]}]})).unwrap();
    let history: SeatHistory = serde_json::from_value(json!({"history_count":6})).unwrap();
    assert!(history.recent_category_counts(&student, Some(3)).is_empty());
    assert_eq!(history.recent_category_counts(&student, None)["front"], 1);
    let legacy: StudentSeatHistory = serde_json::from_value(
        json!({"category_counts":{"front":1},"records":[{"categories":["front"]}]}),
    )
    .unwrap();
    assert_eq!(history.recent_category_counts(&legacy, Some(3))["front"], 1);
}

#[test]
fn input_validation_and_solving_agree_on_representable_budgets() {
    let mut request = json!({"api_version":2,"student_count":1,"seat_positions":[[0,0]],"time_limit_seconds":1e300});
    let validation_error = validate_solve_request_json(&request.to_string()).unwrap_err();
    let solve_error = solve_problem_json(&request.to_string()).unwrap_err();
    assert_eq!(validation_error, solve_error);
    assert!(validation_error.contains("duration is out of range"));

    // This duration fits Duration while exceeding Instant on normal targets.
    // Keep the assertion portable to targets with a wider Instant range.
    let seconds = 1e19;
    let duration = Duration::try_from_secs_f64(seconds).unwrap();
    request["time_limit_seconds"] = json!(seconds);
    if Instant::now().checked_add(duration).is_none() {
        let validation_error = validate_solve_request_json(&request.to_string()).unwrap_err();
        assert_eq!(
            validation_error,
            solve_problem_json(&request.to_string()).unwrap_err()
        );
        assert!(validation_error.contains("deadline is out of range"));
    } else {
        validate_solve_request_json(&request.to_string()).unwrap();
        assert_eq!(
            serde_json::from_str::<CoreSolveResponse>(
                &solve_problem_json(&request.to_string()).unwrap()
            )
            .unwrap()
            .status,
            SolveStatus::Solved
        );
    }

    request["time_limit_seconds"] = json!(1.0);
    validate_solve_request_json(&request.to_string()).unwrap();
    let solved: CoreSolveResponse =
        serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
    assert_eq!(solved.status, SolveStatus::Solved);
}
