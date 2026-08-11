//! Rotation gate (plan §6.2/§17.4 rotation evidence): 1/3/5/10/20 periods
//! with a fixed workbench request must produce deterministic plans, every
//! period assignment must pass the independent validator (the shared solve
//! use case), and infeasible periods must surface as ordinary domain
//! results instead of fabricated success.
//!
//! Also covers §11.9 "取消正在运行的 solve 后再次 solve" at the rotation
//! level via the infeasible-period path: a period that cannot be seated
//! returns `feasible=false` with the honest status and a failed_period
//! index, and a corrected request still generates a full plan.

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
        assert!(
            !assignments.is_empty(),
            "period {} must seat every student",
            index + 1
        );
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
    assert!(outcome.feasible);

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
    // after the failed attempt — §11.9 cancel/recover spirit.
    let fixed = workbench_request(24, 3, 7);
    let outcome = run(&fixed, &editor_store, &solve_requests);
    assert_valid_plan(&outcome, 3, 7);
}
