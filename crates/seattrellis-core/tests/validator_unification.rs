//! Validator-unification gate: every solve product — including the plain
//! string API used by the CLI — must pass the full independent response
//! validator, and the validator must reject tampered "Solved" responses.

use seattrellis_core::validate_solve_response;
use seattrellis_core::{solve_problem_json, CoreSolveRequest, CoreSolveResponse};

fn solvable_request() -> String {
    serde_json::json!({
        "api_version": 2,
        "student_count": 3,
        "seat_positions": [[1.0, 1.0], [2.0, 1.0], [1.0, 2.0], [2.0, 2.0]],
        "edges": [[0, 1], [0, 2], [1, 3], [2, 3]],
        "cannot_be_adjacent": [[0, 1]],
        "seed": 11,
        "students": [
            {"key": "s0", "score": 90.0},
            {"key": "s1", "score": 80.0},
            {"key": "s2", "score": 70.0}
        ]
    })
    .to_string()
}

#[test]
fn cli_json_product_passes_the_full_response_validator() {
    let request_json = solvable_request();
    let output = solve_problem_json(&request_json).expect("solve succeeds");
    let request: CoreSolveRequest = serde_json::from_str(&request_json).expect("request");
    let response: CoreSolveResponse = serde_json::from_str(&output).expect("response");
    assert_eq!(response.status, seattrellis_core::SolveStatus::Solved);
    validate_solve_response(&request, &response)
        .expect("the CLI string API product must clear independent validation");
}

#[test]
fn tampered_solved_responses_are_rejected() {
    let request_json = solvable_request();
    let request: CoreSolveRequest = serde_json::from_str(&request_json).expect("request");

    // A Solved flag wrapped around an assignment that puts students 0 and 1 on
    // the adjacent seat pair (0, 1) must not pass the cannot_be_adjacent rule.
    let mut response: CoreSolveResponse =
        serde_json::from_str(&solve_problem_json(&request_json).unwrap()).unwrap();
    response.assignment = vec![[0, 1], [1, 0], [2, 2]];
    assert!(validate_solve_response(&request, &response).is_err());

    // Duplicate seats and missing students must fail as well.
    let mut duplicated: CoreSolveResponse =
        serde_json::from_str(&solve_problem_json(&request_json).unwrap()).unwrap();
    duplicated.assignment[1][1] = duplicated.assignment[0][1];
    assert!(validate_solve_response(&request, &duplicated).is_err());

    let mut truncated: CoreSolveResponse =
        serde_json::from_str(&solve_problem_json(&request_json).unwrap()).unwrap();
    truncated.assignment.pop();
    assert!(validate_solve_response(&request, &truncated).is_err());
}
