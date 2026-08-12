pub mod cost;
pub mod models;
pub mod objectives;
pub mod rng;

pub const NATIVE_API_VERSION: u32 = 2;

// Module split (plan 1.2): the former single-file monolith is decomposed so
// solver, evaluation, validation, scoring, audit, repair, reports and
// candidate generation are independently testable modules. Public items are
// re-exported at the crate root to keep the external API unchanged.
mod audit;
mod candidates;
mod engine;
mod evaluation;
mod precheck;
mod repair;
mod reports;
mod scoring;
mod solver;

pub use audit::*;
pub use candidates::*;
pub use engine::*;
pub use evaluation::*;
pub use precheck::*;
pub use repair::*;
pub use reports::*;
pub use scoring::*;
pub use solver::*;

// Tests were historically collected at the bottom of lib.rs; they now
// reference the modules through the crate root re-exports above.
#[cfg(test)]
mod tests {
    use crate::candidates::generate_candidates_json;
    use crate::engine::{
        build_candidate_domains, build_cost_context, full_solution_total_cost, greedy_attempt,
        hard_search_with_budget, local_search, maximum_candidate_matching, validate_assignment,
        validate_solve_request, validate_solve_request_json, SearchOutcome,
        HARD_SEARCH_NODE_BUDGET,
    };
    use crate::evaluation::{
        assigned_students_meet_distance, assignment_is_unique, build_graph_distance_matrix,
        build_index_adjacency, evaluate_problem_json, seat_distance, CoreEvaluationResponse,
    };
    use crate::repair::repair_json;
    use crate::reports::{history_report_json, pair_report_json};
    use crate::rng::SplitMix64;
    use crate::scoring::score_assignment_json;
    use crate::solver::{
        classify_solve_error, resolve_group_rules, solve_problem_json, solve_problem_with_control,
        validate_solve_response, CoreSolveRequest, CoreSolveResponse, SolveControl, SolveStatus,
    };
    use crate::{audit_report_json, precheck_report_json, NATIVE_API_VERSION};
    use serde_json::{json, Value};

    #[test]
    fn exposes_expected_native_api_version() {
        assert_eq!(NATIVE_API_VERSION, 2);
    }

    #[test]
    fn accepts_complete_unique_assignment() {
        let assignments = vec![(0, 1), (1, 0), (2, 2)];
        assert!(assignment_is_unique(3, 3, &assignments));
    }

    #[test]
    fn rejects_duplicate_student_or_seat() {
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (0, 1)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (1, 0)]));
    }

    #[test]
    fn rejects_missing_or_out_of_bounds_assignment() {
        assert!(!assignment_is_unique(2, 2, &[(0, 0)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (2, 1)]));
        assert!(!assignment_is_unique(2, 2, &[(0, 0), (1, 2)]));
    }

    #[test]
    fn computes_euclidean_distance() {
        assert_eq!(seat_distance(1.0, 1.0, 4.0, 5.0), Some(5.0));
        assert_eq!(seat_distance(f64::NAN, 1.0, 4.0, 5.0), None);
    }

    #[test]
    fn validates_solve_request_without_running_search() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0]]
        }"#;
        assert!(validate_solve_request_json(request).is_ok());

        // Regression: a fixed seat whose seat index is >= student_count must
        // validate (seat indexes are independent of student_count). The merged
        // hard-rule loop used to mistake the seat slot for a student index and
        // reject this valid request.
        let fixed_high_seat = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "fixed_seats": [[0, 0], [1, 2]]
        }"#;
        assert!(
            validate_solve_request_json(fixed_high_seat).is_ok(),
            "fixed seat index 2 with 2 students must be accepted"
        );
        let fixed_out_of_range = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0], [1, 5]]
        }"#;
        let error = validate_solve_request_json(fixed_out_of_range).unwrap_err();
        assert!(error.contains("unknown student or seat"), "{error}");

        let invalid = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]]
        }"#;
        let error = validate_solve_request_json(invalid).unwrap_err();
        assert!(error.contains("more students than available seats"));
    }

    #[test]
    fn validate_rejects_empty_class_and_invalid_student_keys() {
        let empty_class = r#"{
            "api_version": 2,
            "student_count": 0,
            "seat_positions": [[0.0, 0.0]]
        }"#;
        let error = validate_solve_request_json(empty_class).unwrap_err();
        assert!(error.contains("at least one student"), "{error}");

        let empty_key = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "students": [{"key": "   "}]
        }"#;
        let error = validate_solve_request_json(empty_key).unwrap_err();
        assert!(error.contains("non-empty keys"), "{error}");

        let duplicate_key = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "students": [{"key": "same"}, {"key": "same"}]
        }"#;
        let error = validate_solve_request_json(duplicate_key).unwrap_err();
        assert!(error.contains("duplicate student key"), "{error}");
    }

    #[test]
    fn validate_rejects_non_positive_or_non_finite_time_limit() {
        let negative = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "time_limit_seconds": -1.0
        }"#;
        let error = validate_solve_request_json(negative).unwrap_err();
        assert!(error.contains("time_limit_seconds"), "{error}");

        let mut request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 1,
                "seat_positions": [[0.0, 0.0]]
            }"#,
        )
        .unwrap();
        request.time_limit_seconds = Some(f64::NAN);
        let error = validate_solve_request(&request).unwrap_err();
        assert!(error.contains("finite"), "{error}");

        request.time_limit_seconds = Some(0.0);
        let error = validate_solve_request(&request).unwrap_err();
        assert!(error.contains("greater than zero"), "{error}");
    }

    #[test]
    fn validate_rejects_self_referential_pair_and_distance_rules() {
        let pair = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "must_be_adjacent": [[0, 0]]
        }"#;
        let error = validate_solve_request_json(pair).unwrap_err();
        assert!(error.contains("two different students"), "{error}");

        let distance = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "min_distance": [
                {"students": [0, 0], "distance": 1.0, "metric": "graph"}
            ]
        }"#;
        let error = validate_solve_request_json(distance).unwrap_err();
        assert!(error.contains("two different students"), "{error}");
    }

    #[test]
    fn evaluates_versioned_problem_dto() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "assignments": [[0, 0], [1, 1], [2, 2]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]],
            "cannot_be_adjacent": [[0, 2]],
            "min_distance": [
                {"students": [0, 2], "distance": 2.0, "metric": "graph"}
            ],
            "student_scores": [90.0, 60.0, 30.0]
        }"#;

        let response_json = evaluate_problem_json(request).expect("request should be valid");
        let response: CoreEvaluationResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.assignment_unique);
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.checked_rule_count, 7);
        assert_eq!(response.violation_count, 0);
        assert_eq!(response.graph_distance_matrix[0][2], Some(2));
        assert_eq!(response.peer_mixing_gap_sum, 60.0);
        assert_eq!(response.peer_mixing_pair_count, 2);
        assert_eq!(response.peer_mixing_mean_gap, Some(30.0));
    }

    #[test]
    fn reports_hard_rule_violations_without_identity_data() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "assignments": [[0, 0], [1, 1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;

        let response_json = evaluate_problem_json(request).expect("request should be valid");
        let response: CoreEvaluationResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.hard_constraints_satisfied);
        assert_eq!(response.violation_codes, vec!["cannot_be_adjacent"]);
    }

    #[test]
    fn rejects_incompatible_problem_dto_versions() {
        let request = r#"{
            "api_version": 3,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "assignments": [[0, 0]]
        }"#;

        let error = evaluate_problem_json(request).expect_err("version should be rejected");
        assert!(error.contains("expected 2"));
    }

    #[test]
    fn solves_a_simple_feasible_class() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "seed": 7
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.assignment.len(), 3);
        let mut seats = response
            .assignment
            .iter()
            .map(|pair| pair[1])
            .collect::<Vec<_>>();
        seats.sort_unstable();
        assert_eq!(seats, vec![0, 1, 2]);
    }

    #[test]
    fn solves_single_student_single_seat_without_local_search_panic() {
        let request = r#"{
            "api_version": 2,
            "student_count": 1,
            "seat_positions": [[0.0, 0.0]],
            "students": [{"key": "only"}],
            "seed": 1
        }"#;

        let response_json = solve_problem_json(request).expect("single-student solve succeeds");
        let response: CoreSolveResponse = serde_json::from_str(&response_json).unwrap();
        assert_eq!(response.status, SolveStatus::Solved);
        assert_eq!(response.assignment, vec![[0, 0]]);
    }

    #[test]
    fn respects_fixed_seats() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[1, 2]],
            "seed": 3
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        let seat_of_one = response
            .assignment
            .iter()
            .find(|pair| pair[0] == 1)
            .map(|pair| pair[1]);
        assert_eq!(seat_of_one, Some(2));
    }

    #[test]
    fn places_must_be_adjacent_students_near_each_other() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "must_be_adjacent": [[0, 1]],
            "seed": 5
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        let seat_of = |student: usize| {
            response
                .assignment
                .iter()
                .find(|pair| pair[0] == student)
                .map(|pair| pair[1])
                .unwrap()
        };
        let (first, second) = (seat_of(0), seat_of(1));
        assert!((first as isize - second as isize).unsigned_abs() == 1);
    }

    #[test]
    fn reports_infeasible_when_no_placement_satisfies_hard_rules() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]],
            "seed": 1
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert!(response.assignment.is_empty());
        assert!(!response.hard_constraints_satisfied);
    }

    #[test]
    fn rejects_too_many_students_for_the_room() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]]
        }"#;

        let error = solve_problem_json(request).expect_err("capacity should be rejected");
        assert!(error.contains("cannot seat more students"));
    }

    // -----------------------------------------------------------------------
    // Cost-ranked greedy: the ranking must prefer the cheaper seat, and the
    // response must carry the new total_cost field.
    // -----------------------------------------------------------------------

    /// Two students, two seats in different rows. Student 0 has poor vision
    /// (needs the front), student 1 is short (no height penalty). With
    /// vision_front enabled and randomize disabled, cost ranking must seat
    /// student 0 in the front row regardless of greedy placement order.
    #[test]
    fn cost_ranking_prefers_cheaper_front_seat_for_vision_student() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 6.0]],
            "seed": 0,
            "students": [
                {"key": "STU001", "vision": "poor", "height_cm": null, "tags": [], "needs": []},
                {"key": "STU002", "vision": null, "height_cm": 150.0, "tags": [], "needs": []}
            ],
            "layout": {
                "layout_id": "t",
                "name": "T",
                "seats": [
                    {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
                    {"seat_id": "R6C1", "row": 6, "col": 1, "enabled": true}
                ]
            },
            "rules": {
                "seed": 0,
                "soft": {
                    "vision_front": {"enabled": true, "weight": 20},
                    "height_back": {"enabled": true, "weight": 1},
                    "randomize": {"enabled": false, "weight": 1},
                    "score_balance": {"enabled": false, "weight": 1},
                    "fair_rotation": {"enabled": false, "weight": 10},
                    "avoid_recent_neighbors": {"enabled": false, "weight": 10}
                }
            }
        }"#;

        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        assert!(response.hard_constraints_satisfied);
        let seat_of_student0 = response
            .assignment
            .iter()
            .find(|pair| pair[0] == 0)
            .map(|pair| pair[1])
            .expect("student 0 is assigned");
        // Student 0 (vision "poor") must be seated in the front row, seat 0.
        assert_eq!(seat_of_student0, 0);
        assert!(response.total_cost.is_some());
    }

    /// The new `total_cost` field must serialize: present and finite for a
    /// feasible solve, `null` for an infeasible one.
    #[test]
    fn solve_response_serializes_total_cost() {
        let feasible_request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]],
            "seed": 0
        }"#;
        let response_json = solve_problem_json(feasible_request).expect("request should be valid");
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        let total_cost = value.get("total_cost").expect("total_cost is serialized");
        assert!(
            total_cost.as_f64().is_some(),
            "feasible solve reports a number"
        );

        let infeasible_request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]],
            "seed": 1
        }"#;
        let response_json =
            solve_problem_json(infeasible_request).expect("request should be valid");
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert!(value
            .get("total_cost")
            .unwrap_or(&serde_json::Value::Null)
            .is_null());
    }

    /// Cross-check against the frozen 40-student parity reference: the native
    /// solver must report feasible=true and the returned assignment must pass
    /// the native hard-constraint evaluator. Python's reference cost is
    /// recorded (59975.0) for comparison; exact agreement is not required.
    ///
    /// Ignored by default because it runs the full 480-attempt cost-ranked
    /// solve in debug mode (~9s). Run explicitly with
    /// `cargo test -p seattrellis_core -- --ignored`.
    #[test]
    #[ignore = "runs the full 480-attempt cost-ranked solve (~9s); opt-in"]
    fn solves_forty_parity_reference_feasibly() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/reference/40-parity.json");
        let payload_text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("cannot read 40-parity.json at {}: {error}", path.display())
        });
        let payload: serde_json::Value =
            serde_json::from_str(&payload_text).expect("reference payload should be valid JSON");
        let problem = payload
            .get("problem")
            .expect("reference has a problem block");
        let problem_json = serde_json::to_string(problem).expect("problem block serializes");

        let response_json = solve_problem_json(&problem_json).expect("native solve should run");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(
            response.feasible,
            "40-person parity problem must be feasible"
        );
        assert!(response.hard_constraints_satisfied);
        assert_eq!(response.assignment.len(), 40);
        let total_cost = response
            .total_cost
            .expect("feasible solve reports total_cost");
        assert!(total_cost.is_finite());

        // Feed the same problem plus the solved assignment to the native
        // hard-constraint evaluator for an independent verification.
        let mut eval_request: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&problem_json).expect("problem is an object");
        eval_request.insert(
            "assignments".to_string(),
            serde_json::Value::Array(
                response
                    .assignment
                    .iter()
                    .map(|pair| {
                        serde_json::Value::Array(vec![
                            serde_json::Value::from(pair[0]),
                            serde_json::Value::from(pair[1]),
                        ])
                    })
                    .collect(),
            ),
        );
        let eval_json = serde_json::Value::Object(eval_request).to_string();
        let eval_response_json = evaluate_problem_json(&eval_json).expect("evaluation should run");
        let eval_response: CoreEvaluationResponse =
            serde_json::from_str(&eval_response_json).expect("evaluation response JSON");

        assert!(eval_response.assignment_unique);
        assert!(
            eval_response.hard_constraints_satisfied,
            "native assignment must satisfy all hard rules, violations: {:?}",
            eval_response.violation_codes
        );

        let python_cost = payload
            .pointer("/python_reference/total_cost")
            .and_then(serde_json::Value::as_f64);
        assert!(python_cost.is_some(), "reference records a python cost");
        eprintln!(
            "40-parity: native feasible=true total_cost={total_cost:.1} python_reference_cost={:.1}",
            python_cost.unwrap()
        );
    }

    // -----------------------------------------------------------------------
    // Group rules (RuleSet.groups): expanded into pairwise must/cannot-be-
    // adjacent constraints exactly like `rule_compiler._expand_group_rules`.
    // -----------------------------------------------------------------------

    #[test]
    fn expands_group_rules_into_pairwise_hard_rules() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 4,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                "must_be_adjacent": [[0, 1]],
                "students": [
                    {"key": "A"}, {"key": "B"}, {"key": "C"}, {"key": "D"}
                ],
                "rules": {
                    "seed": 1,
                    "groups": [
                        {"name": "buddies", "students": ["A", "B", "C"], "together": true},
                        {"name": "rivals", "students": ["C", "D"], "separate": true},
                        {"name": "solo", "students": ["D"], "together": true},
                        {"name": "dupe", "students": ["A", "A", "B"], "separate": true}
                    ]
                }
            }"#,
        )
        .expect("request parses");

        let resolved = resolve_group_rules(&request).expect("groups resolve");
        // Explicit pairs first, then group-derived pairs in member order:
        // buddies(A,B,C) together → (A,B),(A,C),(B,C).
        assert_eq!(
            resolved.must_be_adjacent,
            vec![[0, 1], [0, 1], [0, 2], [1, 2]]
        );
        // rivals(C,D) separate → (C,D); dupe dedupes to (A,B) → (A,B).
        assert_eq!(resolved.cannot_be_adjacent, vec![[2, 3], [0, 1]]);
    }

    #[test]
    fn group_member_references_resolve_by_student_key() {
        // Members may appear in any order and are paired by index, not by the
        // order the student records appear in the request.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 3,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
                "students": [
                    {"key": "first", "display_name": "Alpha"},
                    {"key": "second", "display_name": "Beta"},
                    {"key": "third", "display_name": "Gamma"}
                ],
                "rules": {
                    "groups": [
                        {"name": "trio", "students": ["third", "first"], "together": true}
                    ]
                }
            }"#,
        )
        .expect("request parses");
        let resolved = resolve_group_rules(&request).expect("groups resolve");
        assert_eq!(resolved.must_be_adjacent, vec![[0, 2]]);
        assert!(resolved.cannot_be_adjacent.is_empty());
    }

    #[test]
    fn rejects_group_member_that_is_not_a_student() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
                "students": [{"key": "A"}, {"key": "B"}],
                "rules": {
                    "groups": [{"name": "g", "students": ["A", "GHOST"], "together": true}]
                }
            }"#,
        )
        .expect("request parses");
        let error = resolve_group_rules(&request).unwrap_err();
        assert!(error.contains("Unknown student reference"), "{error}");
        assert!(error.contains("GHOST"), "{error}");
    }

    #[test]
    fn validate_rejects_unknown_group_member() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "students": [{"key": "A"}, {"key": "B"}],
            "rules": {"groups": [{"name": "g", "students": ["A", "GHOST"], "together": true}]}
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(error.contains("Unknown student reference"), "{error}");
    }

    /// The solver must honor group rules end-to-end: a `together` group is
    /// seated adjacently and a `separate` group is kept apart, using only the
    /// top-level `rules.groups` (no explicit pairwise lists).
    #[test]
    fn solver_enforces_group_together_and_separate() {
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
            "edges": [[0, 1], [1, 2], [2, 3]],
            "students": [
                {"key": "A", "score": 90.0},
                {"key": "B", "score": 80.0},
                {"key": "C", "score": 70.0},
                {"key": "D", "score": 60.0}
            ],
            "rules": {
                "seed": 7,
                "soft": {"randomize": {"enabled": false, "weight": 1}},
                "groups": [
                    {"name": "buddy", "students": ["A", "B"], "together": true},
                    {"name": "rival", "students": ["C", "D"], "separate": true}
                ]
            }
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(
            response.feasible,
            "groups A/B together and C/D apart must be feasible"
        );
        assert!(response.hard_constraints_satisfied);

        let seat_of = |student: usize| -> usize {
            response
                .assignment
                .iter()
                .find(|pair| pair[0] == student)
                .map(|pair| pair[1])
                .expect("student is assigned")
        };
        let adjacent = |first: usize, second: usize| {
            (seat_of(first) as i64 - seat_of(second) as i64).abs() == 1
        };
        assert!(adjacent(0, 1), "A and B must sit together");
        assert!(!adjacent(2, 3), "C and D must sit apart");
    }

    /// An infeasible group combination must be reported as infeasible rather
    /// than silently ignored.
    #[test]
    fn solver_reports_infeasible_group_as_not_found() {
        // Three seats in a line, A fixed to seat 0 and B to seat 2; the
        // `together` group demands adjacency, which the fixed seats rule out.
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0], [1, 2]],
            "students": [{"key": "A"}, {"key": "B"}],
            "rules": {
                "seed": 3,
                "groups": [{"name": "g", "students": ["A", "B"], "together": true}]
            }
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(
            !response.feasible,
            "A and B are pinned apart but must sit together"
        );
        assert!(response.assignment.is_empty());
    }

    // ------------------------------------------------------------------
    // M1-03: frozen SolveStatus contract (plan §四.1)
    // ------------------------------------------------------------------

    #[test]
    fn solved_reports_solved_status() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]],
            "seed": 0
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert_eq!(response.status, SolveStatus::Solved);
        assert!(response.feasible);

        // The wire value must be the frozen PascalCase spelling.
        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert_eq!(value["status"], "Solved");
    }

    /// Exhaustive search proves a fully-constrained 2x2 grid infeasible
    /// (M3-04: the status upgrades from Unknown to ProvenInfeasible once the
    /// whole state space is swept; see
    /// `hard_search_budget_exhaustion_stays_unknown` for the honest-Unknown
    /// case).
    #[test]
    fn greedy_exhaustion_reports_unknown_status() {
        // 2x2 grid, every seat pair forbidden from adjacency: the request
        // passes static validation but no complete assignment exists.
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
            "cannot_be_adjacent": [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]],
            "seed": 7
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::ProvenInfeasible);

        let value: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert_eq!(value["status"], "ProvenInfeasible");
    }

    #[test]
    fn validation_errors_are_err_and_classify_as_invalid_input() {
        let request = r#"{
            "api_version": 99,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]]
        }"#;
        let err = solve_problem_json(request).expect_err("unsupported api_version must fail");
        assert_eq!(classify_solve_error(&err), SolveStatus::InvalidInput);
    }

    #[test]
    fn classify_solve_error_distinguishes_input_from_internal() {
        for message in [
            "unsupported api_version 99",
            "native solve requires at least one seat",
            "native solve cannot seat more students than available seats",
            "Duplicate student identifiers: STU001",
            "unknown rule kind",
        ] {
            assert_eq!(
                classify_solve_error(message),
                SolveStatus::InvalidInput,
                "message {message:?} should be InvalidInput",
            );
        }
        for message in [
            "solver panicked while ranking candidates",
            "could not serialize the response",
            "internal store is poisoned",
        ] {
            assert_eq!(
                classify_solve_error(message),
                SolveStatus::InternalError,
                "message {message:?} should be InternalError",
            );
        }
    }

    /// The status vocabulary is frozen: every variant serializes to exactly
    /// the plan's spelling, and deserialization round-trips.
    #[test]
    fn solve_status_vocabulary_is_frozen_on_the_wire() {
        let cases = [
            (SolveStatus::Solved, "Solved"),
            (SolveStatus::ProvenInfeasible, "ProvenInfeasible"),
            (SolveStatus::Timeout, "Timeout"),
            (SolveStatus::Unknown, "Unknown"),
            (SolveStatus::InvalidInput, "InvalidInput"),
            (SolveStatus::Cancelled, "Cancelled"),
            (SolveStatus::InternalError, "InternalError"),
        ];
        for (status, wire) in cases {
            let encoded = serde_json::to_string(&status).unwrap();
            assert_eq!(encoded, format!("\"{wire}\""));
            let decoded: SolveStatus = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, status);
        }
    }

    /// Responses without a `status` field (pre-M1-03 wire data) must
    /// deserialize with the honest default `Unknown`.
    #[test]
    fn legacy_response_without_status_defaults_to_unknown() {
        let legacy = r#"{
            "api_version": 2,
            "feasible": true,
            "assignment": [[0, 0]],
            "attempts_used": 1,
            "hard_constraints_satisfied": true
        }"#;
        let response: CoreSolveResponse = serde_json::from_str(legacy).unwrap();
        assert_eq!(response.status, SolveStatus::Unknown);
    }

    fn response_validation_request() -> CoreSolveRequest {
        serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
                "edges": [[0, 1]],
                "students": [{"key": "A"}, {"key": "B"}]
            }"#,
        )
        .unwrap()
    }

    fn structurally_valid_solved_response() -> CoreSolveResponse {
        CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: true,
            status: SolveStatus::Solved,
            assignment: vec![[0, 0], [1, 1]],
            attempts_used: 1,
            hard_constraints_satisfied: true,
            total_cost: Some(0.0),
        }
    }

    #[test]
    fn solve_response_validation_rejects_forged_success_flags() {
        let request = response_validation_request();
        let mut response = structurally_valid_solved_response();
        assert!(validate_solve_response(&request, &response).is_ok());

        response.api_version = 1;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("api_version"), "{error}");

        response = structurally_valid_solved_response();
        response.status = SolveStatus::Unknown;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("status must be Solved"), "{error}");

        response = structurally_valid_solved_response();
        response.feasible = false;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("feasible=true"), "{error}");

        response = structurally_valid_solved_response();
        response.hard_constraints_satisfied = false;
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("hard_constraints_satisfied=true"), "{error}");
    }

    #[test]
    fn solve_response_validation_rejects_duplicate_and_out_of_range_indices() {
        let request = response_validation_request();

        let mut response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [0, 1]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("student 0 more than once"), "{error}");

        response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [1, 0]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("seat 0 more than once"), "{error}");

        response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [2, 1]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("out-of-range student 2"), "{error}");

        response = structurally_valid_solved_response();
        response.assignment = vec![[0, 0], [1, 2]];
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("out-of-range seat 2"), "{error}");
    }

    #[test]
    fn solve_response_validation_rechecks_group_derived_hard_rules() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
                "edges": [[0, 1], [1, 2]],
                "students": [{"key": "A"}, {"key": "B"}],
                "rules": {
                    "groups": [
                        {"name": "together", "students": ["A", "B"], "together": true}
                    ]
                }
            }"#,
        )
        .unwrap();
        let response = CoreSolveResponse {
            api_version: NATIVE_API_VERSION,
            feasible: true,
            status: SolveStatus::Solved,
            assignment: vec![[0, 0], [1, 2]],
            attempts_used: 1,
            hard_constraints_satisfied: true,
            total_cost: Some(0.0),
        };
        let error = validate_solve_response(&request, &response).unwrap_err();
        assert!(error.contains("violates a hard rule"), "{error}");
    }

    // ---- M3-02: static conflict layer (plan §6.1 first layer) ----

    #[test]
    fn static_conflict_student_fixed_to_two_seats_is_invalid() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "fixed_seats": [[0, 0], [0, 2]]
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(
            error.contains("fixed to more than one seat"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn static_conflict_seat_fixed_to_two_students_is_invalid() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "fixed_seats": [[0, 0], [1, 0]]
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(
            error.contains("fixed to more than one student"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn static_conflict_same_pair_in_must_and_cannot_is_invalid() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "must_be_adjacent": [[0, 1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;
        let error = validate_solve_request_json(request).unwrap_err();
        assert!(
            error.contains("appears in both must_be_adjacent and cannot_be_adjacent"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn static_conflict_fixed_seats_contradict_pair_rules() {
        // Fixed seats 0 and 2 are not adjacent, but must_be_adjacent demands
        // adjacency: unsolvable before any search.
        let must_violated = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0], [1, 2]],
            "must_be_adjacent": [[0, 1]]
        }"#;
        let error = validate_solve_request_json(must_violated).unwrap_err();
        assert!(
            error.contains("do not satisfy a must_be_adjacent rule"),
            "{error}"
        );

        // Fixed seats 0 and 1 are adjacent, but cannot_be_adjacent forbids it.
        let cannot_violated = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "fixed_seats": [[0, 0], [1, 1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;
        let error = validate_solve_request_json(cannot_violated).unwrap_err();
        assert!(
            error.contains("violate a cannot_be_adjacent rule"),
            "{error}"
        );

        // Fixed seats 0 and 1 violate a graph min_distance of 2 (they are 1 hop).
        let distance_violated = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0]],
            "edges": [[0, 1]],
            "fixed_seats": [[0, 0], [1, 1]],
            "min_distance": [{"students": [0, 1], "distance": 2.0, "metric": "graph"}]
        }"#;
        let error = validate_solve_request_json(distance_violated).unwrap_err();
        assert!(error.contains("violate a min_distance rule"), "{error}");
    }

    #[test]
    fn conflicting_errors_classify_as_invalid_input() {
        assert_eq!(
            classify_solve_error("conflicting hard rules: fixed seats violate a min_distance rule"),
            SolveStatus::InvalidInput
        );
    }

    // ---- M3-02: candidate domains (plan §6.1 second layer) ----

    #[test]
    fn candidate_domains_respect_fixed_and_pair_rules() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 3,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
                "edges": [[0, 1], [1, 2], [2, 3]],
                "fixed_seats": [[0, 0]],
                "must_be_adjacent": [[0, 1]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);
        let domains = build_candidate_domains(&request, &resolved, &adjacency, &graph_distances);
        // Student 0 is fixed to seat 0: domain is exactly {0}.
        assert_eq!(domains[0].seats, vec![0]);
        assert!(domains[0].excluded.iter().all(|(seat, _)| *seat != 0));

        // Student 1 must sit adjacent to student 0: only seat 1 is legal.
        assert_eq!(domains[1].seats, vec![1]);
        assert!(domains[1]
            .excluded
            .iter()
            .any(|(seat, reason)| *seat == 3 && reason.contains("adjacent")));

        // Student 2 is unconstrained: every seat is legal.
        assert_eq!(domains[2].seats.len(), 4);
    }

    #[test]
    fn empty_candidate_domain_is_proven_infeasible() {
        // Student 1 must sit at graph distance >= 3 from the fixed student 0
        // (seat 0), but every seat is closer than 3 hops on this line graph:
        // no legal seat exists for student 1, a sound infeasibility proof.
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0]],
            "min_distance": [{"students": [0, 1], "distance": 3.0, "metric": "graph"}]
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::ProvenInfeasible);
        assert_eq!(response.attempts_used, 0);
        assert!(response.assignment.is_empty());
    }

    // ---- M3-03: global matching precheck (plan §6.1 third layer) ----

    #[test]
    fn maximum_matching_counts_jointly_seatable_students() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 3,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
                "edges": [[0, 1], [1, 2], [2, 3]],
                "fixed_seats": [[0, 0]],
                "must_be_adjacent": [[0, 1]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);
        let domains = build_candidate_domains(&request, &resolved, &adjacency, &graph_distances);
        // domains: student0={0}, student1={1}, student2={0,1,2,3} — a full
        // matching of size 3 exists (0->0, 1->1, 2->2).
        assert_eq!(maximum_candidate_matching(&domains), 3);
    }

    #[test]
    fn matching_precheck_proves_infeasibility_when_seats_are_overbooked() {
        // Three students, three seats. Students 0/1 are fixed to seats 0/1;
        // student 2 must not sit adjacent to student 0, which rules out seat
        // 2 (its only neighbor) but not seats 0/1 (no edges). Every domain is
        // non-empty, yet seats 0/1 are both taken: maximum matching = 2 < 3.
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 2]],
            "fixed_seats": [[0, 0], [1, 1]],
            "cannot_be_adjacent": [[0, 2]]
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::ProvenInfeasible);
        assert_eq!(response.attempts_used, 0);
    }
    // ---- M3-04: exhaustive hard search (plan §6.1 fourth layer) ----

    #[test]
    fn hard_search_finds_legal_assignment_when_greedy_fails() {
        // A 4-cycle of must_be_adjacent pairs: students 0-1, 1-2, 2-3, 3-0
        // must all sit adjacent, but seat 2 is disabled... no: use a layout
        // where the only legal seating is a specific rotation the random
        // greedy misses. Here a 2x3 grid with a min_distance pair between
        // students 0 and 1 (>= 2 graph hops): greedy attempt 0 pins 0 and 1
        // on adjacent cheap seats and every randomized attempt fails to
        // escape; the search finds the far-apart placement.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [0.0, 1.0], [1.0, 1.0], [2.0, 1.0]],
                "edges": [[0, 1], [1, 2], [3, 4], [4, 5], [0, 3], [1, 4], [2, 5]],
                "min_distance": [{"students": [0, 1], "distance": 3.0, "metric": "graph"}],
                "seed": 1
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);

        let outcome = hard_search_with_budget(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            200_000,
            None,
        );
        let SearchOutcome::Found(assignment) = outcome else {
            panic!("hard search should find the far-apart placement, got {outcome:?}");
        };
        // Student 0 and 1 must be >= 3 hops apart: only opposite corners work
        // in this 2x3 ladder (e.g. 0->seat 0 and 1->seat 5 is 3 hops).
        let probe: Vec<Option<usize>> = assignment.iter().map(|seat| Some(*seat)).collect();
        assert!(assigned_students_meet_distance(
            &request.seat_positions,
            &probe,
            &graph_distances,
            &request.min_distance[0],
        ));
        assert_eq!(assignment.len(), 2);
        assert!(assignment[0] != assignment[1]);
    }

    #[test]
    fn hard_search_budget_exhaustion_stays_unknown() {
        // The 2x2 fully-forbidden grid is proven infeasible in a few nodes;
        // with a tiny budget the sweep cannot complete and the honest status
        // must stay Unknown (never ProvenInfeasible).
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 4,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
                "cannot_be_adjacent": [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);

        // A budget of 1 node cannot sweep anything.
        let outcome =
            hard_search_with_budget(&request, &resolved, &adjacency, &graph_distances, 1, None);
        assert_eq!(outcome, SearchOutcome::BudgetExceeded);

        // The full budget proves it (and solve_problem reports that).
        let outcome = hard_search_with_budget(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            HARD_SEARCH_NODE_BUDGET,
            None,
        );
        assert_eq!(outcome, SearchOutcome::ProvenInfeasible);
    }
    #[test]
    fn independent_validator_rejects_violating_assignments() {
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 2,
                "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
                "edges": [[0, 1], [1, 2]],
                "cannot_be_adjacent": [[0, 1]]
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);

        // Adjacent seats 0/1 violate the pair rule: must be rejected.
        let error = validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0, 1])
            .expect_err("adjacent placement must violate cannot_be_adjacent");
        assert!(error.contains("violates a hard rule"), "{error}");

        // Duplicate seat: must be rejected.
        let error = validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0, 0])
            .expect_err("duplicate seat must be rejected");
        assert!(error.contains("duplicate seat"), "{error}");

        // Missing students: must be rejected.
        let error = validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0])
            .expect_err("short assignment must be rejected");
        assert!(error.contains("students"), "{error}");

        // Seats 0 and 2 are not adjacent: a legal pairing passes.
        validate_assignment(&request, &resolved, &adjacency, &graph_distances, &[0, 2])
            .expect("non-adjacent pairing must pass");
    }
    #[test]
    fn precheck_report_lists_domains_and_reasons() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
            "edges": [[0, 1], [1, 2], [2, 3]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]]
        }"#;
        let report: serde_json::Value =
            serde_json::from_str(&precheck_report_json(request).unwrap()).unwrap();
        assert_eq!(report["precheck"], "clean");
        assert!(report["infeasible_reason"].is_null());
        assert_eq!(report["matching_size"], 3);
        assert_eq!(report["students"][0]["candidate_count"], 1);
        assert_eq!(report["students"][0]["seats"][0], 0);
        assert_eq!(report["students"][1]["candidate_count"], 1);
        assert_eq!(report["students"][1]["seats"][0], 1);
        // The exclusion reason names the pair rule.
        let excluded = &report["students"][1]["excluded"];
        assert!(
            excluded
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["reason"].as_str().unwrap().contains("adjacent")),
            "excluded reasons: {excluded}"
        );
    }

    #[test]
    fn precheck_report_flags_empty_domain_with_reason() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "fixed_seats": [[0, 0]],
            "min_distance": [{"students": [0, 1], "distance": 3.0, "metric": "graph"}]
        }"#;
        let report: serde_json::Value =
            serde_json::from_str(&precheck_report_json(request).unwrap()).unwrap();
        assert_eq!(report["precheck"], "infeasible");
        let reason = report["infeasible_reason"].as_str().unwrap();
        assert!(reason.contains("student 1 has no legal seat"), "{reason}");
    }
    #[test]
    fn time_limit_reports_timeout_when_budget_is_spent() {
        // The fully-forbidden 2x2 grid is provably infeasible, but with a
        // sub-millisecond budget the search cannot sweep it: the honest
        // status is Timeout (a time budget was given), never ProvenInfeasible.
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            "edges": [[0, 1], [2, 3], [0, 2], [1, 3]],
            "cannot_be_adjacent": [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]],
            "time_limit_seconds": 0.000001
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::Timeout);
    }

    #[test]
    fn time_limit_with_incumbent_still_reports_solved() {
        // A trivial problem solved by greedy attempt 0 within the budget:
        // the incumbent wins even though the budget is tiny. The budget
        // stays small (not the old 1ms - scheduler latency on loaded CI
        // runners could exhaust it before the first greedy attempt, making
        // the test flaky) while still exercising the budget check.
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "time_limit_seconds": 0.05
        }"#;
        let response_json = solve_problem_json(request).expect("request should validate");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");

        assert!(response.feasible);
        assert_eq!(response.status, SolveStatus::Solved);
    }

    #[test]
    fn cancelled_control_reports_cancelled_before_any_incumbent() {
        // Cooperative cancellation (plan §6.1): a control cancelled before
        // the solve starts must terminate with the Cancelled status and
        // never produce an incumbent.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 8,
                "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
                "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
                "seed": 7
            }"#,
        )
        .expect("request should parse");
        let control = SolveControl::new();
        control.cancel();
        let response =
            solve_problem_with_control(&request, &control).expect("solve should terminate");
        assert!(!response.feasible);
        assert_eq!(response.status, SolveStatus::Cancelled);
        assert!(response.assignment.is_empty());
        // A fresh control on the same request still solves normally.
        let response =
            solve_problem_with_control(&request, &SolveControl::new()).expect("solve should run");
        assert_eq!(response.status, SolveStatus::Solved);
    }
    // ---- M3 6.2: soft optimization (local search) ----

    #[test]
    fn local_search_never_worsens_cost_and_keeps_legality() {
        // Skewed scores + enabled score_balance give the hill climber room
        // to improve on the raw greedy output.
        let request: CoreSolveRequest = serde_json::from_str(
            r#"{
                "api_version": 2,
                "student_count": 8,
                "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
                "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
                "students": [
                    {"key":"s0","score":100.0},{"key":"s1","score":10.0},
                    {"key":"s2","score":95.0},{"key":"s3","score":15.0},
                    {"key":"s4","score":90.0},{"key":"s5","score":20.0},
                    {"key":"s6","score":85.0},{"key":"s7","score":25.0}
                ],
                "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}},
                "seed": 42
            }"#,
        )
        .unwrap();
        let resolved = resolve_group_rules(&request).unwrap();
        let adjacency = build_index_adjacency(request.seat_positions.len(), &request.edges);
        let graph_distances = build_graph_distance_matrix(&adjacency);
        let ctx = build_cost_context(&request);
        let mut rng = SplitMix64::new(42);

        let initial = greedy_attempt(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &mut rng,
            &ctx,
            0,
        )
        .expect("greedy should seat everyone");
        let before = full_solution_total_cost(&initial, &adjacency, &ctx);

        let improved = local_search(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &initial,
            &ctx,
            &mut rng,
        );
        let after = full_solution_total_cost(&improved, &adjacency, &ctx);

        assert!(after <= before + 1e-9, "cost worsened: {before} -> {after}");
        validate_assignment(&request, &resolved, &adjacency, &graph_distances, &improved)
            .expect("local search must keep the assignment legal");

        // Determinism: same seed, same input -> identical output. Replay the
        // same RNG consumption (greedy first, then local search).
        let mut rng2 = SplitMix64::new(42);
        let _ = greedy_attempt(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &mut rng2,
            &ctx,
            0,
        )
        .expect("greedy should seat everyone");
        let rerun = local_search(
            &request,
            &resolved,
            &adjacency,
            &graph_distances,
            &initial,
            &ctx,
            &mut rng2,
        );
        assert_eq!(improved, rerun, "local search must be deterministic");
    }

    #[test]
    fn solve_applies_local_search_without_breaking_parity_status() {
        // End-to-end: the solver still reports Solved with a legal assignment
        // (the local search path runs inside solve_problem).
        let request = r#"{
            "api_version": 2,
            "student_count": 8,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
            "students": [
                {"key":"s0","score":100.0},{"key":"s1","score":10.0},
                {"key":"s2","score":95.0},{"key":"s3","score":15.0},
                {"key":"s4","score":90.0},{"key":"s5","score":20.0},
                {"key":"s6","score":85.0},{"key":"s7","score":25.0}
            ],
            "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}},
            "seed": 42
        }"#;
        let response_json = solve_problem_json(request).expect("request should be valid");
        let response: CoreSolveResponse =
            serde_json::from_str(&response_json).expect("response should be valid JSON");
        assert!(response.feasible);
        assert_eq!(response.status, SolveStatus::Solved);
        assert!(response.total_cost.unwrap().is_finite());
    }
    #[test]
    fn audit_report_breaks_down_hard_and_soft_rules() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]],
            "students": [
                {"key":"s0","score":100.0},{"key":"s1","score":10.0},{"key":"s2","score":90.0}
            ],
            "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}}
        }"#;
        // Legal assignment: s0->0 (fixed), s1->1 (adjacent to s0), s2->2.
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2]];
        let report: serde_json::Value =
            serde_json::from_str(&audit_report_json(request, &assignment).unwrap()).unwrap();

        assert_eq!(report["hard_rules"]["fixed_seats"]["satisfied"], 1);
        assert_eq!(report["hard_rules"]["must_be_adjacent"]["satisfied"], 1);
        assert_eq!(report["hard_rules"]["cannot_be_adjacent"]["satisfied"], 0);
        // The soft breakdown must carry the score_balance weighted cost.
        let weighted = &report["soft_objectives"]["weighted_costs"];
        assert!(
            weighted.as_object().unwrap().contains_key("score_balance"),
            "weighted_costs: {weighted}"
        );
        assert!(report["total_cost"].is_number());
    }

    #[test]
    fn audit_report_carries_ui_consumption_fields() {
        // plan §6.5: a UI must be able to render the audit without
        // re-deriving the rules — summary, witnesses, missing data, history
        // impact and localized suggested actions.
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0, 0]],
            "must_be_adjacent": [[0, 1]],
            "students": [
                {"key":"s0","score":100.0,"height_cm":150.0,"vision":"poor","needs":["vision_front"]},
                {"key":"s1","score":10.0},
                {"key":"s2","score":90.0}
            ],
            "rules": {"seed": 42, "soft": {
                "score_balance": {"enabled": true, "weight": 5},
                "vision_front": {"enabled": true, "weight": 20},
                "height_back": {"enabled": true, "weight": 1}
            }}
        }"#;
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2]];
        let report: serde_json::Value =
            serde_json::from_str(&audit_report_json(request, &assignment).unwrap()).unwrap();

        // hard_constraint_summary with a total view and empty witnesses.
        assert_eq!(report["hard_constraint_summary"]["all_satisfied"], true);
        assert_eq!(report["hard_constraint_summary"]["checked_rule_count"], 2);
        assert_eq!(report["hard_constraint_summary"]["violation_count"], 0);
        assert_eq!(
            report["hard_constraint_summary"]["witnesses"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        // missing_data: two students lack height/vision/needs.
        assert_eq!(report["missing_data"]["students_missing_height"], 2);
        assert_eq!(report["missing_data"]["students_missing_vision"], 2);
        assert_eq!(report["missing_data"]["students_missing_needs"], 2);
        assert_eq!(report["missing_data"]["students_missing_score"], 0);

        // history impact: no history was supplied.
        assert_eq!(report["history"]["has_history"], false);
        assert_eq!(report["history"]["snapshot_count"], 0);

        // suggested_actions: vision_front is enabled and vision data is
        // missing, so the vision suggestion must be present.
        let actions = report["suggested_actions"].as_array().unwrap();
        assert!(
            actions
                .iter()
                .any(|action| action["message_key"] == "audit.missing_vision"),
            "actions: {actions:?}"
        );
        assert!(
            actions
                .iter()
                .any(|action| action["message_key"] == "audit.missing_height"),
            "actions: {actions:?}"
        );
    }

    #[test]
    fn audit_rejects_illegal_assignments() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]],
            "cannot_be_adjacent": [[0, 1]]
        }"#;
        // Adjacent seats violate the pair rule: the audit must refuse.
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1]];
        let error = audit_report_json(request, &assignment).unwrap_err();
        assert!(error.contains("violates a hard rule"), "{error}");
    }
    #[test]
    fn candidate_set_is_diverse_and_fully_validated() {
        let request = r#"{
            "api_version": 2,
            "student_count": 10,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0],[0.0,2.0],[1.0,2.0],[2.0,2.0],[3.0,2.0]],
            "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[8,9],[9,10],[10,11],[0,4],[1,5],[2,6],[3,7],[4,8],[5,9],[6,10],[7,11]],
            "students": [
                {"key":"s0","score":100.0},{"key":"s1","score":10.0},{"key":"s2","score":95.0},{"key":"s3","score":15.0},
                {"key":"s4","score":90.0},{"key":"s5","score":20.0},{"key":"s6","score":85.0},{"key":"s7","score":25.0},
                {"key":"s8","score":80.0},{"key":"s9","score":30.0}
            ],
            "rules": {"seed": 42, "soft": {"score_balance": {"enabled": true, "weight": 5}}},
            "seed": 42
        }"#;
        let report: serde_json::Value =
            serde_json::from_str(&generate_candidates_json(request, 3).unwrap()).unwrap();

        let candidates = report["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 3, "requested 3 candidates");
        assert_eq!(report["requested_candidate_count"], 3);
        assert!(report["recommended_candidate_id"].is_string());
        assert_eq!(report["base_seed"], 42);
        assert_eq!(
            report["generation_method"],
            "seeded repeated solve with exact-assignment exclusion"
        );

        // Every candidate is distinct, hard-validated, and carries the
        // reproducibility + diversity metadata.
        let mut assignments: Vec<Vec<[usize; 2]>> = Vec::new();
        for candidate in candidates {
            assert_eq!(candidate["hard_constraints_satisfied"], true);
            assert!(candidate["seed"].is_u64());
            assert!(candidate["total_cost"].is_number());
            assert!(candidate["distance_to_best"].is_number());
            let assignment: Vec<[usize; 2]> =
                serde_json::from_value(candidate["assignment"].clone()).unwrap();
            assert!(
                !assignments.contains(&assignment),
                "candidates must be distinct"
            );
            assignments.push(assignment);
        }
    }
    #[test]
    fn history_report_counts_categories_with_identifiers() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]],
            "students": [{"key":"S1","display_name":"Alice"},{"key":"S2","display_name":"Bob"}],
            "layout": {"layout_id": "l", "name": "l", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 0.0, "y": 0.0, "zone": "front", "enabled": true},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 1.0, "y": 0.0, "zone": "front", "enabled": true}
            ], "adjacency": {"edges": [["R1C1","R1C2"]]}}
        }"#;
        let snapshots = r#"[
            {"assignments": [{"student_key":"S1","seat_id":"R1C1"},{"student_key":"S2","seat_id":"R1C2"}]},
            {"assignments": [{"student_key":"S1","seat_id":"R1C2"},{"student_key":"S2","seat_id":"R1C1"}]}
        ]"#;
        let report: Value =
            serde_json::from_str(&history_report_json(request, snapshots).unwrap()).unwrap();
        assert_eq!(report["history_count"], 2);
        assert_eq!(report["student_count"], 2);
        // Both students sat in the front zone in both periods.
        assert!(report["category_totals"]["front"].as_u64().unwrap() >= 4);
        // Teacher-side report: identifiers are present (oracle contract,
        // mirroring Python's StudentSeatHistory). Anonymization happens at
        // the export/display boundary (teacher vs public templates), not in
        // the core report.
        let students = report["students"].as_array().unwrap();
        assert_eq!(students.len(), 2);
        assert!(report["students"][0]["student_key"].as_str().is_some());
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(serialized.contains("Alice") && serialized.contains("S1"));
    }

    #[test]
    fn plan_score_matches_python_semantics_for_a_fixed_assignment() {
        // The breakdown mirrors Python's `score_snapshot`: three enabled soft
        // rules produce available dimensions with Python's formulas, disabled
        // rules report not_available with the exact reasons, the weighted
        // total matches, and the hard summary counts every rule.
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "students": [
                {"key":"S1","display_name":"A","score":100.0,"height_cm":150.0,"vision":"poor"},
                {"key":"S2","display_name":"B","score":90.0,"height_cm":160.0,"vision":"poor"},
                {"key":"S3","display_name":"C","score":80.0,"height_cm":170.0,"vision":"normal"},
                {"key":"S4","display_name":"D","score":70.0,"height_cm":180.0,"vision":"normal"}
            ],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}},
            "rules": {"seed": 42, "soft": {
                "score_balance": {"enabled": true, "weight": 1},
                "height_back": {"enabled": true, "weight": 1},
                "vision_front": {"enabled": true, "weight": 20}
            }}
        }"#;
        let assignment: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2], [3, 3]];
        let report: Value =
            serde_json::from_str(&score_assignment_json(request, &assignment, "", None).unwrap())
                .unwrap();
        let breakdown = &report["breakdown"];
        // Enabled dimensions are available with a 0..100 score.
        for key in [
            "score_balance_score",
            "height_preference_score",
            "vision_preference_score",
        ] {
            assert_eq!(breakdown[key]["status"], "available", "{key}");
            let score = breakdown[key]["score"].as_f64().unwrap();
            assert!((0.0..=100.0).contains(&score), "{key}: {score}");
        }
        // Disabled / missing-input dimensions report the exact Python reason.
        assert_eq!(
            breakdown["fair_rotation_score"]["details"]["reason"],
            "fair_rotation is disabled."
        );
        assert_eq!(
            breakdown["stability_score"]["details"]["reason"],
            "No previous snapshot was supplied."
        );
        assert_eq!(
            breakdown["rule_scores"]["mentor_pairing_score"]["details"]["reason"],
            "mentor_pairing is disabled."
        );
        // The weighted total is a plain weighted average of available dims.
        let total = report["total"].as_f64().unwrap();
        let mut weighted = 0.0;
        let mut total_weight = 0.0;
        for key in [
            "score_balance_score",
            "height_preference_score",
            "vision_preference_score",
        ] {
            weighted += breakdown[key]["score"].as_f64().unwrap()
                * breakdown[key]["weight"].as_f64().unwrap();
            total_weight += breakdown[key]["weight"].as_f64().unwrap();
        }
        assert!(
            (total - weighted / total_weight).abs() < 0.01,
            "total {total} vs weighted average {}",
            weighted / total_weight
        );
        // The hard summary is satisfied with the base three integrity checks.
        assert_eq!(breakdown["hard_constraint_summary"]["satisfied"], true);
        assert_eq!(
            breakdown["hard_constraint_summary"]["checked_rule_count"],
            3
        );
        assert_eq!(breakdown["hard_constraint_summary"]["violation_count"], 0);

        // A rule-violating assignment is flagged: total 0 + a counted
        // violation (integrity failures like duplicate seats are rejected
        // outright by the completeness checks, so a fixed-seat violation is
        // the honest path here).
        let fixed_request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0, 1]],
            "students": [
                {"key":"S1","display_name":"A","score":100.0,"height_cm":150.0,"vision":"poor"},
                {"key":"S2","display_name":"B","score":90.0,"height_cm":160.0,"vision":"poor"},
                {"key":"S3","display_name":"C","score":80.0,"height_cm":170.0,"vision":"normal"},
                {"key":"S4","display_name":"D","score":70.0,"height_cm":180.0,"vision":"normal"}
            ],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}},
            "rules": {"seed": 42, "soft": {
                "score_balance": {"enabled": true, "weight": 1},
                "height_back": {"enabled": true, "weight": 1},
                "vision_front": {"enabled": true, "weight": 20}
            }}
        }"#;
        let violating: Vec<[usize; 2]> = vec![[0, 0], [1, 1], [2, 2], [3, 3]];
        let report: Value = serde_json::from_str(
            &score_assignment_json(fixed_request, &violating, "", None).unwrap(),
        )
        .unwrap();
        assert_eq!(report["total"], 0.0);
        assert_eq!(
            report["breakdown"]["hard_constraint_summary"]["violation_count"],
            1
        );
        assert_eq!(
            report["breakdown"]["hard_constraint_summary"]["checked_rule_count"],
            4
        );
    }

    #[test]
    fn candidate_report_carries_plan_score_and_recommends_max_total() {
        let request = json!({
            "api_version": 2,
            "student_count": 8,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0],[0.0,1.0],[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "edges": [[0,1],[1,2],[2,3],[4,5],[5,6],[6,7],[0,4],[1,5],[2,6],[3,7]],
            "students": (0..8).map(|index| json!({"key": format!("S{index}")})).collect::<Vec<_>>(),
            "rules": {"seed": 42, "soft": {}},
            "seed": 42
        });
        let report: Value =
            serde_json::from_str(&generate_candidates_json(&request.to_string(), 3).unwrap())
                .unwrap();
        let candidates = report["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 3);
        // Every candidate carries a plan_score; diversity is available
        // (multiple candidates), stability is not (no history in the core
        // request), and the recommended candidate has the max total.
        let totals: Vec<f64> = candidates
            .iter()
            .map(|candidate| candidate["plan_score"]["total"].as_f64().unwrap())
            .collect();
        for candidate in candidates {
            assert_eq!(
                candidate["plan_score"]["breakdown"]["diversity_score"]["status"],
                "available"
            );
            assert_eq!(
                candidate["plan_score"]["breakdown"]["stability_score"]["status"],
                "not_available"
            );
        }
        let recommended = report["recommended_candidate_id"].as_str().unwrap();
        let recommended_index = candidates
            .iter()
            .position(|candidate| candidate["candidate_id"].as_str() == Some(recommended))
            .unwrap();
        assert_eq!(
            totals[recommended_index],
            totals.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        );
    }

    #[test]
    fn pair_report_counts_repeated_pairs_and_relations() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0]],
            "edges": [[0,1],[1,2]],
            "students": [{"key":"S1"},{"key":"S2"},{"key":"S3"}],
            "layout": {"layout_id": "l", "name": "l", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 0.0, "y": 0.0, "zone": "front", "enabled": true},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 1.0, "y": 0.0, "zone": "front", "enabled": true},
                {"seat_id": "R1C3", "row": 1, "col": 3, "x": 2.0, "y": 0.0, "zone": "front", "enabled": true}
            ], "adjacency": {"edges": [["R1C1","R1C2"],["R1C2","R1C3"]]}}
        }"#;
        // S1-S2 sit adjacent in both periods: repeated pair with occurrences 2.
        let snapshots = r#"[
            {"assignments": [{"student_key":"S1","seat_id":"R1C1"},{"student_key":"S2","seat_id":"R1C2"},{"student_key":"S3","seat_id":"R1C3"}]},
            {"assignments": [{"student_key":"S1","seat_id":"R1C1"},{"student_key":"S2","seat_id":"R1C2"},{"student_key":"S3","seat_id":"R1C3"}]}
        ]"#;
        let report: Value =
            serde_json::from_str(&pair_report_json(request, snapshots, 10, 2).unwrap()).unwrap();
        assert_eq!(report["history_count"], 2);
        assert!(report["pair_count"].as_u64().unwrap() >= 1);
        assert!(report["repeated_pair_count"].as_u64().unwrap() >= 1);
        assert_eq!(report["max_occurrences"], 2);
        // Top pair is anonymized.
        let top = report["top_pairs"][0].clone();
        assert!(top["student_a"].as_str().unwrap().starts_with("student-"));
        assert_eq!(top["total_occurrences"], 2);
        // Desk-mate relations were counted.
        assert!(report["relation_totals"]["desk_mate"].as_u64().unwrap() >= 2);
    }

    #[test]
    fn repair_rejects_solve_response_pairs_with_extra_indices() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]]
        }"#;
        let snapshot = r#"{
            "status": "Solved",
            "assignment": [[0,0,99],[1,1]]
        }"#;
        let error = repair_json(request, snapshot, &[], &[], &[])
            .expect_err("malformed CoreSolveResponse pairs must be rejected");
        assert!(error.contains("exactly two indices"), "{error}");
    }

    #[test]
    fn repair_keeps_locked_student_seated() {
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "students": [
                {"key":"S1","display_name":"Alice"},{"key":"S2"},
                {"key":"S3"},{"key":"S4"}
            ],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C4","row":1,"col":4,"x":3.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"],["R1C3","R1C4"]]}}
        }"#;
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","student_name":"Alice","seat_id":"R1C1"},
                {"student_key":"S2","student_name":"Bob","seat_id":"R1C2"},
                {"student_key":"S3","student_name":"Carol","seat_id":"R1C3"},
                {"student_key":"S4","student_name":"Dan","seat_id":"R1C4"}
            ],
            "solver_status": "FEASIBLE"
        }"#;
        // Lock S1 in place; everything else may re-arrange.
        let repaired = repair_json(request, snapshot, &[], &["S1".to_string()], &[])
            .expect("repair should succeed");
        let value: Value = serde_json::from_str(&repaired).unwrap();
        let assignments = value["assignments"].as_array().unwrap();
        assert_eq!(assignments.len(), 4);
        let s1 = assignments
            .iter()
            .find(|a| a["student_key"] == "S1")
            .unwrap();
        assert_eq!(s1["seat_id"], "R1C1", "locked student must keep its seat");
        assert!(value["summary"]["moved_students"].as_u64().is_some());
    }

    #[test]
    fn repair_preserves_original_fixed_seat_for_affected_student_non_identity() {
        let request = r#"{
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0],[3.0,0.0]],
            "edges": [[0,1],[1,2],[2,3]],
            "fixed_seats": [[0,2]],
            "seed": 2,
            "students": [{"key":"S1"},{"key":"S2"},{"key":"S3"},{"key":"S4"}],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C4","row":1,"col":4,"x":3.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"],["R1C3","R1C4"]]}}
        }"#;
        // Deliberately non-identity: S1 is student index 0 but occupies seat
        // index 2. S1 and S2 are both movable, so dropping the original fixed
        // rule would let the deterministic seed move S1 to R1C1.
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","seat_id":"R1C3"},
                {"student_key":"S2","seat_id":"R1C1"},
                {"student_key":"S3","seat_id":"R1C4"},
                {"student_key":"S4","seat_id":"R1C2"}
            ]
        }"#;

        let repaired = repair_json(
            request,
            snapshot,
            &["S1".to_string(), "S2".to_string()],
            &[],
            &[],
        )
        .expect("repair should preserve the original fixed-seat rule");
        let value: Value = serde_json::from_str(&repaired).unwrap();
        let s1 = value["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|assignment| assignment["student_key"] == "S1")
            .unwrap();
        assert_eq!(s1["seat_id"], "R1C3");
    }

    #[test]
    fn repair_rejects_anchors_conflicting_with_original_fixed_seat() {
        let request = r#"{
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0,0.0],[1.0,0.0],[2.0,0.0]],
            "edges": [[0,1],[1,2]],
            "fixed_seats": [[0,2]],
            "students": [{"key":"S1"},{"key":"S2"},{"key":"S3"}],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C3","row":1,"col":3,"x":2.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R1C2","R1C3"]]}}
        }"#;
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","seat_id":"R1C1"},
                {"student_key":"S2","seat_id":"R1C3"},
                {"student_key":"S3","seat_id":"R1C2"}
            ]
        }"#;

        // Same student, different seat.
        let error = repair_json(request, snapshot, &[], &["S1".to_string()], &[]).unwrap_err();
        assert!(error.contains("original fixed-seat rule"), "{error}");
        assert!(error.contains("student S1"), "{error}");

        // Different anchored student attempts to occupy the original fixed
        // student's seat when S1 is the local affected student.
        let error = repair_json(request, snapshot, &["S1".to_string()], &[], &[]).unwrap_err();
        assert!(error.contains("original fixed-seat rule"), "{error}");
        assert!(error.contains("student S2"), "{error}");
    }

    #[test]
    fn repair_rejects_invalid_anchor_combinations() {
        let request = r#"{
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[0.0,0.0],[1.0,0.0]],
            "edges": [[0,1]],
            "students": [{"key":"S1"},{"key":"S2"}],
            "layout": {"layout_id":"l","name":"l","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"zone":"front","enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"zone":"front","enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"]]}}
        }"#;
        let snapshot = r#"{
            "assignments": [
                {"student_key":"S1","seat_id":"R1C1"},
                {"student_key":"S2","seat_id":"R1C2"}
            ]
        }"#;

        // Locking an unknown student is an error.
        let err = repair_json(request, snapshot, &[], &["S9".to_string()], &[]).unwrap_err();
        assert!(err.contains("unknown"), "{err}");

        // Affected and locked cannot overlap.
        let err = repair_json(
            request,
            snapshot,
            &["S1".to_string()],
            &["S1".to_string()],
            &[],
        )
        .unwrap_err();
        assert!(err.contains("cannot also be locked"), "{err}");

        // A locked seat must be known.
        let err = repair_json(request, snapshot, &[], &[], &["R1C9".to_string()]).unwrap_err();
        assert!(err.contains("unknown"), "{err}");
    }
}

#[cfg(test)]
mod latest_snapshot_tests {
    use super::*;
    use serde_json::{json, Value};

    /// The per-candidate stability dimension activates when a latest
    /// snapshot is supplied and stays `not_available` without one.
    #[test]
    fn candidates_stability_activates_with_latest_snapshot() {
        let request = json!({
            "api_version": 2,
            "student_count": 3,
            "seat_positions": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            "edges": [[0, 1], [1, 2]],
            "seed": 7,
            "students": [
                {"key": "A", "display_name": "Alpha"},
                {"key": "B", "display_name": "Beta"},
                {"key": "C", "display_name": "Gamma"}
            ]
        })
        .to_string();
        let latest = json!({
            "kind": "snapshot",
            "assignments": [
                {"student_key": "A", "seat_id": "R1C1"},
                {"student_key": "B", "seat_id": "R1C2"},
                {"student_key": "C", "seat_id": "R1C3"}
            ]
        })
        .to_string();

        let without = generate_candidates_json(&request, 2).expect("candidates generate");
        let without_value: Value = serde_json::from_str(&without).unwrap();
        let stability_without =
            &without_value["candidates"][0]["plan_score"]["breakdown"]["stability_score"]["status"];
        assert_eq!(stability_without, "not_available");

        let with = generate_candidates_json_with_latest_snapshot(&request, 2, &latest)
            .expect("candidates generate with latest snapshot");
        let with_value: Value = serde_json::from_str(&with).unwrap();
        let stability_with =
            &with_value["candidates"][0]["plan_score"]["breakdown"]["stability_score"]["status"];
        assert_eq!(stability_with, "available", "stability must activate");
    }
}
