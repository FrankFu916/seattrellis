//! Determinism gate (B·CRITICAL): the same request JSON string solved twice
//! inside one process must produce byte-identical output. The evaluator used
//! to accumulate f64 means/RMS/totals while iterating `HashMap` instances,
//! whose order differs between constructions, so two runs could diverge in
//! the low bits of `total_cost` and even pick different assignments.

use seattrellis_core::solve_problem_json;

fn request_json() -> String {
    // Decimal scores exercise the float-sensitive score_position /
    // score_distribution / score_balance objectives; the grid spans four
    // rows so the row-bucket RMS and the row percentiles are non-trivial.
    let scores = [
        88.5, 72.25, 91.125, 63.75, 77.5, 84.25, 59.375, 95.5, 66.75, 81.5, 73.25, 90.75,
    ];
    let students: Vec<serde_json::Value> = scores
        .iter()
        .enumerate()
        .map(|(index, score)| {
            serde_json::json!({
                "key": format!("s{index:02}"),
                "display_name": format!("学生{index}"),
                "score": score,
            })
        })
        .collect();

    let mut seat_positions: Vec<[f64; 2]> = Vec::new();
    for row in 1..=3 {
        for col in 1..=4 {
            seat_positions.push([col as f64, row as f64]);
        }
    }
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for row in 0..3usize {
        for col in 0..4usize {
            if col + 1 < 4 {
                edges.push([row * 4 + col, row * 4 + col + 1]);
            }
            if row + 1 < 3 {
                edges.push([row * 4 + col, (row + 1) * 4 + col]);
            }
        }
    }

    serde_json::json!({
        "api_version": 2,
        "student_count": 12,
        "seat_positions": seat_positions,
        "edges": edges,
        "seed": 20260824,
        "students": students,
        "rules": {
            "seed": 20260824,
            "soft": {
                "vision_front": {"enabled": false, "weight": 20},
                "height_back": {"enabled": false, "weight": 1},
                "randomize": {"enabled": false, "weight": 1},
                "score_balance": {"enabled": true, "weight": 2},
                "score_position": {"enabled": true, "weight": 3, "direction": "high_back"},
                "score_distribution": {"enabled": true, "weight": 3, "scope": "row"}
            }
        }
    })
    .to_string()
}

#[test]
fn repeated_solves_of_one_request_json_are_byte_identical() {
    let request = request_json();
    let first = solve_problem_json(&request).expect("first solve succeeds");
    let second = solve_problem_json(&request).expect("second solve succeeds");
    assert_eq!(
        first, second,
        "two solves of the same request JSON in one process must emit identical bytes"
    );
    let parsed: serde_json::Value = serde_json::from_str(&first).expect("output is JSON");
    assert_eq!(parsed["status"], "Solved", "fixture must stay solvable");
    assert_eq!(parsed["assignment"].as_array().map(Vec::len), Some(12));
}
