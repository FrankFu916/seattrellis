//! Rotation gate (plan §6.2/§17.4 rotation evidence): 1/3/5/10/20 periods
//! with a fixed workbench request must produce deterministic plans, every
//! period assignment must pass the independent validator (the shared solve
//! use case), and infeasible periods must surface as ordinary domain
//! results instead of fabricated success.
//!
//! Cancellation evidence uses a live cooperative control, then starts a fresh
//! solve and verifies that cancelled work left no editor/source contexts.

use std::collections::HashMap;
use std::sync::Mutex;

use seattrellis_application::rotation::{generate_rotation_plan, GenerateRotationOutcome};
use seattrellis_application::SolveRequestStore;
use seattrellis_domain::editing::{new_draft_store, EditorDraftStore};
use serde_json::{json, Value};

fn workbench_request(students: usize, periods: usize, seed: u64) -> Value {
    json!({
        "draft": {
            "name": "Rotation Gate",
            "students": (0..students)
                .map(|index| json!({
                    "student_id": format!("S{}", index + 1),
                    "name": format!("Student {}", index + 1),
                    "score": 100 - (index as i64),
                }))
                .collect::<Vec<_>>(),
            "room": {"template_id": "standard-30"},
            "goal": {"goal_id": "daily-rotation"}
        },
        "period_count": periods,
        "options": {"seed": seed}
    })
}

fn run(
    request: &Value,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> GenerateRotationOutcome {
    generate_rotation_plan(request, editor_store, solve_requests)
        .expect("rotation terminates with a domain result")
}

fn assert_valid_plan(outcome: &GenerateRotationOutcome, periods: usize, seed: u64) {
    assert!(outcome.feasible, "plan must be feasible (seed {seed})");
    assert_eq!(outcome.status, seattrellis_core::SolveStatus::Solved);
    assert!(outcome.failed_period.is_none());
    let plan = outcome
        .plan
        .as_ref()
        .expect("feasible plan carries a document");
    let periods_doc = plan["periods"].as_array().expect("periods array");
    assert_eq!(periods_doc.len(), periods);
    for (index, period) in periods_doc.iter().enumerate() {
        let assignments = period["snapshot"]["assignments"]
            .as_array()
            .unwrap_or_else(|| panic!("period {} has no assignments", index + 1));
        let source = &period["snapshot"]["original_request"];
        let request = seattrellis_core::parse_core_solve_request(&source.to_string())
            .expect("persisted per-period solve source");
        let keys = seattrellis_application::class_generation::student_keys(&request);
        let seats = seattrellis_application::class_generation::seat_specs(&request);
        assert_eq!(
            assignments.len(),
            request.student_count,
            "period must seat the entire roster"
        );
        let pairs = assignments
            .iter()
            .map(|entry| {
                [
                    keys.iter()
                        .position(|key| Some(key.as_str()) == entry["student_key"].as_str())
                        .expect("known student"),
                    seats
                        .iter()
                        .position(|seat| Some(seat.seat_id.as_str()) == entry["seat_id"].as_str())
                        .expect("known seat"),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(
            pairs
                .iter()
                .map(|pair| pair[0])
                .collect::<std::collections::HashSet<_>>()
                .len(),
            request.student_count,
            "unique complete roster coverage"
        );
        assert_eq!(period["snapshot"]["solver_status"], "Solved");
        let response = seattrellis_core::CoreSolveResponse {
            api_version: 2,
            feasible: true,
            status: seattrellis_core::SolveStatus::Solved,
            assignment: pairs,
            attempts_used: 0,
            hard_constraints_satisfied: true,
            total_cost: None,
        };
        seattrellis_core::validate_solve_response(&request, &response)
            .expect("independent assignment audit must validate all original hard constraints");
        let editor = &outcome.period_editors.as_ref().expect("all period editors")[index];
        for entry in assignments {
            let student = editor["students"]
                .as_array()
                .unwrap()
                .iter()
                .find(|student| student["student_key"] == entry["student_key"])
                .expect("editor roster coverage");
            assert_eq!(
                student["seat_id"], entry["seat_id"],
                "editor and persisted snapshot agree"
            );
        }
        if let Some(locks) = period["snapshot"].pointer("/metadata/lock_state") {
            for key in locks["locked_students"].as_array().into_iter().flatten() {
                assert!(editor["students"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|student| student["student_key"] == *key && student["locked"] == true));
            }
            for id in locks["locked_seats"].as_array().into_iter().flatten() {
                assert!(editor["seats"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|seat| seat["seat_id"] == *id && seat["locked"] == true));
            }
        }
    }
    // The first-period editor draft must be present.
    assert!(outcome.editor.is_some());
}

/// The gate is release-only: the 20-period combination costs minutes in
/// debug builds. CI runs it explicitly with `cargo test --release -p
/// seattrellis_application --test rotation_gate -- --ignored` (rust.yml
/// long-run-gates job).
#[test]
fn period_editors_carry_one_draft_per_period_with_roster_names() {
    let editor_store = new_draft_store();
    let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());
    let outcome = run(&workbench_request(4, 2, 42), &editor_store, &solve_requests);
    assert_valid_plan(&outcome, 2, 42);
    assert!(outcome.feasible);
    let plan = outcome.plan.as_ref().expect("feasible plan document");
    assert_eq!(plan["kind"], "rotation_plan");
    assert_eq!(
        plan["schema_version"], "1.0",
        "rotation artifacts must match the frozen oracle schema version"
    );

    let period_editors = outcome
        .period_editors
        .as_ref()
        .expect("feasible rotation carries per-period editors");
    assert_eq!(period_editors.len(), 2, "one editor per period");
    for (index, editor) in period_editors.iter().enumerate() {
        assert_eq!(
            editor["candidate_id"],
            format!("period-{}", index + 1),
            "workbench matches periods by candidate_id == period-N"
        );
        let names: Vec<&str> = editor["students"]
            .as_array()
            .expect("editor students")
            .iter()
            .map(|student| student["display_name"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(
            names,
            vec!["Student 1", "Student 2", "Student 3", "Student 4"],
            "editor drafts must mirror the roster display names"
        );
    }
    // The first period's draft doubles as the response `editor`.
    assert_eq!(
        outcome.editor.as_ref().expect("editor").get("candidate_id"),
        period_editors[0].get("candidate_id")
    );
}

#[test]
#[ignore = "expensive: run in release mode via the CI long-run-gates job"]
fn rotation_period_counts_are_deterministic_and_validated() {
    let editor_store = new_draft_store();
    let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());
    for periods in [1, 3, 5, 10, 20] {
        let request = workbench_request(24, periods, 42);
        let first = run(&request, &editor_store, &solve_requests);
        assert_valid_plan(&first, periods, 42);
        // Determinism: the same request + seed reproduces the plan exactly.
        // Editor drafts carry a fresh draft_id per generation, so compare
        // the editor shape with the id stripped.
        let second = run(&request, &editor_store, &solve_requests);
        assert_eq!(
            first.plan, second.plan,
            "plan must be reproducible (periods={periods})"
        );
        let mut first_editor = first.editor.clone().unwrap();
        let mut second_editor = second.editor.clone().unwrap();
        // Fresh ids are generated per plan; strip them and compare the rest.
        first_editor["draft_id"] = json!("<draft>");
        first_editor["candidate_id"] = json!("<candidate>");
        second_editor["draft_id"] = json!("<draft>");
        second_editor["candidate_id"] = json!("<candidate>");
        assert_eq!(
            first_editor, second_editor,
            "editor must be reproducible (periods={periods})"
        );
    }
}

#[test]
#[ignore = "expensive: run in release mode via the CI long-run-gates job"]
fn infeasible_period_is_an_honest_domain_result_and_a_fixed_request_recovers() {
    // A valid request (students fit the template) whose hard rules make the
    // first period impossible: the outcome must report feasible=false with
    // the honest status and failed_period=1 — never a fabricated Solved
    // plan.
    let editor_store = new_draft_store();
    let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());
    let mut request = workbench_request(24, 3, 7);
    // Every pair cannot be adjacent: search-provable infeasibility.
    let pairs: Vec<Value> = (0..24)
        .flat_map(|first| {
            ((first + 1)..24).map(move |second| {
                json!({ "students": [format!("S{}", first + 1), format!("S{}", second + 1)] })
            })
        })
        .collect();
    request["draft"]["goal"]["hard_rules"] = json!({ "cannot_be_adjacent": pairs });
    let outcome = run(&request, &editor_store, &solve_requests);
    assert!(!outcome.feasible);
    assert!(outcome.plan.is_none());
    assert!(outcome.editor.is_none());
    assert_eq!(outcome.failed_period, Some(1));
    assert!(
        matches!(
            outcome.status,
            seattrellis_core::SolveStatus::ProvenInfeasible
                | seattrellis_core::SolveStatus::Unknown
                | seattrellis_core::SolveStatus::Timeout
        ),
        "honest non-solved status, got {:?}",
        outcome.status
    );

    // A corrected request (no hard rules) generates a full plan immediately
    // after the failed attempt. Cancellation has a separate live-control gate.
    let fixed = workbench_request(24, 3, 7);
    let outcome = run(&fixed, &editor_store, &solve_requests);
    assert_valid_plan(&outcome, 3, 7);
}

#[test]
fn running_rotation_cancelled_with_control_leaves_no_contexts_and_fresh_run_recovers() {
    use std::sync::Arc;
    let editors = Arc::new(new_draft_store());
    let sources = Arc::new(SolveRequestStore::default());
    let control = seattrellis_core::SolveControl::new();
    let mut request = workbench_request(60, 3, 42);
    request["draft"]["room"] = json!({"template_id":"standard-60"});
    request["options"]["time_limit_seconds"] = json!(10.0);
    request["draft"]["goal"]["hard_rules"] = json!({"min_distance":(0..60).flat_map(|first|((first+1)..60).filter(move|second|(first+second)%5==0).map(move|second|json!({"students":[format!("S{}",first+1),format!("S{}",second+1)],"distance":2.0,"metric":"euclidean"}))).collect::<Vec<_>>()});
    let worker_editors = Arc::clone(&editors);
    let worker_sources = Arc::clone(&sources);
    let worker_control = control.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = seattrellis_application::with_request_control(worker_control, || {
            generate_rotation_plan(&request, &worker_editors, &worker_sources)
        });
        finished_tx.send(result).unwrap();
    });
    started_rx.recv().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        matches!(
            finished_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ),
        "fixture must still be running when cancelled"
    );
    control.cancel();
    match finished_rx
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("cooperative cancel must terminate promptly")
    {
        Ok(outcome) => {
            assert!(!outcome.feasible);
            assert_eq!(outcome.status, seattrellis_core::SolveStatus::Cancelled);
            assert!(outcome.plan.is_none());
        }
        Err(error) => assert_eq!(error.code, "cancelled"),
    }
    worker.join().unwrap();
    assert!(editors.lock().unwrap().is_empty());
    assert!(sources.lock().unwrap().is_empty());
    let mut fresh = workbench_request(4, 2, 42);
    fresh["draft"]["goal"]["hard_rules"] =
        json!({"fixed_seats":[{"student":"S1","seat_id":"R1C1"}]});
    let recovered = run(&fresh, &editors, &sources);
    assert_valid_plan(&recovered, 2, 42);
    assert_eq!(editors.lock().unwrap().len(), 2);
    assert_eq!(sources.lock().unwrap().len(), 2);
}
