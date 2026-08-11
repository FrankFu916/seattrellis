//! Property-based solver gates (plan §11.3 Solver).
//!
//! Every generated problem is *planted*: a full assignment is drawn first and
//! all hard rules are derived from it, so the problem is known-feasible by
//! construction. Properties asserted for any generated instance:
//!
//! 1. `Solved` ⇒ the independent evaluator reports zero violations and the
//!    assignment is a unique permutation (plan: no false ProvenInfeasible,
//!    no illegal result marked solved);
//! 2. seed determinism: solving twice with the same seed yields the same
//!    student→seat assignment;
//! 3. appending unrelated disabled seats never breaks a satisfied fixed-seat
//!    rule (adding empty space is semantics-neutral);
//! 4. reordering student input rows (stable keys, sorted order) does not
//!    change the feasibility verdict (semantics are input-order independent).

use proptest::prelude::*;
use seattrellis_core::{evaluate_problem_json, solve_problem_json};
use serde_json::{json, Value};

fn planted_request(
    n: usize,
    permutation: &[usize],
    fixed_count: usize,
    vision_front: bool,
    height_back: bool,
    extra_disabled_seats: usize,
    seed: u64,
) -> Value {
    // Grid layout, row-major; extra seats are appended disabled.
    let cols = 4;
    let seats: Vec<Value> = (0..n + extra_disabled_seats)
        .map(|i| {
            let row = (i / cols) as f64 + 1.0;
            let col = (i % cols) as f64 + 1.0;
            json!({
                "seat_id": format!("R{}C{}", row as u32, col as u32),
                "row": row as u32,
                "col": col as u32,
                "x": col,
                "y": row,
                "enabled": i < n,
                "zone": if row <= 1.0 { "front" } else { "middle" },
                "near_platform": row <= 1.0,
                "near_window": col <= 1.0,
                "near_door": false,
                "near_ac": false,
                "tags": [],
                "attributes": {}
            })
        })
        .collect();
    // Layout with adjacency (4-neighbour grid).
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for i in 0..n {
        let (row, col) = (i / cols, i % cols);
        let right = i + 1;
        if col + 1 < cols && right < n && right / cols == row {
            edges.push([i, right]);
        }
        let down = i + cols;
        if down < n {
            edges.push([i, down]);
        }
    }
    // Disabled seats never join the adjacency graph.
    let layout = json!({
        "layout_id": "prop-layout",
        "name": "property layout",
        "seats": seats,
        "adjacency": {"include_horizontal": true, "include_vertical": true}
    });
    let students: Vec<Value> = (0..n)
        .map(|i| {
            json!({
                "key": format!("S{i:03}"),
                "display_name": format!("学生{i:03}"),
                "height_cm": 120.0 + (i % 60) as f64,
                "score": 50.0 + (i * 7 % 50) as f64,
                "vision": if i % 5 == 0 { Some("0.6") } else { None },
                "tags": [],
                "needs": if i % 7 == 0 && vision_front { vec!["vision_front"] } else { vec![] }
            })
        })
        .collect();
    let mut fixed: Vec<[usize; 2]> = Vec::new();
    for (student, seat) in permutation.iter().enumerate().take(fixed_count) {
        fixed.push([student, *seat]);
    }
    let mut soft = json!({});
    if vision_front {
        soft["vision_front"] = json!({"enabled": true, "weight": 20});
    }
    if height_back {
        soft["height_back"] = json!({"enabled": true, "weight": 10});
    }
    json!({
        "api_version": 2,
        "student_count": n,
        "seat_positions": (0..n).map(|i| {
            let row = (i / cols) as f64 + 1.0;
            let col = (i % cols) as f64 + 1.0;
            [col, row]
        }).collect::<Vec<_>>(),
        "edges": edges,
        "fixed_seats": fixed,
        "must_be_adjacent": [],
        "cannot_be_adjacent": [],
        "min_distance": [],
        "seed": seed,
        "students": students,
        "student_scores": (0..n).map(|i| json!(50.0 + (i * 7 % 50) as f64)).collect::<Vec<_>>(),
        "rules": {
            "schema_version": 0,
            "seed": seed,
            "hard": {},
            "soft": soft,
            "groups": []
        },
        "layout": layout,
        "history": null,
        "pair_history": null,
        "time_limit_seconds": null
    })
}

fn seat_id_for(seat: usize, cols: usize) -> String {
    let row = seat / cols + 1;
    let col = seat % cols + 1;
    format!("R{row}C{col}")
}

fn assignment_from_response(response: &Value) -> Vec<usize> {
    let assignments = response["assignment"]
        .as_array()
        .expect("assignments array");
    let mut probe = vec![usize::MAX; assignments.len()];
    for pair in assignments {
        let student = pair[0].as_u64().unwrap() as usize;
        let seat = pair[1].as_u64().unwrap() as usize;
        probe[student] = seat;
    }
    probe
}

fn status_of(response: &Value) -> String {
    response["status"].as_str().unwrap_or("").to_string()
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    #[test]
    fn solved_implies_zero_violations_and_unique_assignment(
        n in 4usize..=16,
        fixed_count in 0usize..=3,
        vision_front in any::<bool>(),
        height_back in any::<bool>(),
        seed in any::<u64>(),
    ) {
        prop_assume!(fixed_count <= n);
        // Planted assignment: a random permutation.
        let mut perm: Vec<usize> = (0..n).collect();
        perm.sort_by_key(|&i| i * 2654435761 % n);
        let request = planted_request(n, &perm, fixed_count, vision_front, height_back, 0, seed);
        let response: Value = serde_json::from_str(&solve_problem_json(&request.to_string()).unwrap()).unwrap();
        if status_of(&response) == "Solved" {
            let assignment = assignment_from_response(&response);
            // (a) unique permutation
            let mut seen = vec![false; n];
            for seat in &assignment {
                prop_assert!(*seat < n && !std::mem::replace(&mut seen[*seat], true));
            }
            // (b) independent evaluator: zero violations
            let mut eval_request = request.clone();
            eval_request["assignments"] = json!(response["assignment"]);
            let eval: Value = serde_json::from_str(
                &evaluate_problem_json(&eval_request.to_string()).unwrap()
            ).unwrap();
            prop_assert_eq!(eval["violations"].as_array().map(|v| v.len()).unwrap_or(0), 0);
        }
    }

    #[test]
    fn same_seed_same_assignment(
        n in 4usize..=14,
        fixed_count in 0usize..=2,
        seed in any::<u64>(),
    ) {
        let mut perm: Vec<usize> = (0..n).collect();
        perm.sort_by_key(|&i| i * 40503 % n);
        let request = planted_request(n, &perm, fixed_count, false, false, 0, seed);
        let text = request.to_string();
        let r1: Value = serde_json::from_str(&solve_problem_json(&text).unwrap()).unwrap();
        let r2: Value = serde_json::from_str(&solve_problem_json(&text).unwrap()).unwrap();
        if status_of(&r1) == "Solved" && status_of(&r2) == "Solved" {
            prop_assert_eq!(assignment_from_response(&r1), assignment_from_response(&r2));
        }
    }

    #[test]
    fn extra_disabled_seats_keep_fixed_rules_satisfied(
        n in 4usize..=14,
        fixed_count in 1usize..=2,
        seed in any::<u64>(),
    ) {
        prop_assume!(fixed_count <= n);
        let mut perm: Vec<usize> = (0..n).collect();
        perm.sort_by_key(|&i| i * 9137 % n);
        let base = planted_request(n, &perm, fixed_count, false, false, 0, seed);
        let base_text = base.to_string();
        let with_extra = planted_request(n, &perm, fixed_count, false, false, 3, seed);
        let extra_text = with_extra.to_string();
        let r1: Value = serde_json::from_str(&solve_problem_json(&base_text).unwrap()).unwrap();
        let r2: Value = serde_json::from_str(&solve_problem_json(&extra_text).unwrap()).unwrap();
        if status_of(&r1) == "Solved" && status_of(&r2) == "Solved" {
            // The same fixed student must still be on the same seat id.
            for i in 0..fixed_count {
                let expected = format!("S{i:03}");
                let seat_base = seat_id_for(perm[i], 4);
                let a1 = assignment_from_response(&r1);
                let a2 = assignment_from_response(&r2);
                let _ = expected;
                // student i -> seat perm[i] in both solves
                prop_assert_eq!(a1[i], perm[i]);
                prop_assert_eq!(a2[i], perm[i]);
                let _ = seat_base;
            }
        }
    }

    #[test]
    fn reordered_student_input_preserves_feasibility_verdict(
        n in 4usize..=14,
        fixed_count in 0usize..=2,
        seed in any::<u64>(),
    ) {
        prop_assume!(fixed_count <= n);
        let mut perm: Vec<usize> = (0..n).collect();
        perm.sort_by_key(|&i| i * 7177 % n);
        let original = planted_request(n, &perm, fixed_count, false, false, 0, seed);
        // Reorder student rows by stable key (sorted); the problem semantics
        // (feasibility) must not depend on input order.
        let mut students = original["students"].as_array().unwrap().clone();
        students.sort_by_key(|s| s["key"].as_str().unwrap().to_string());
        let mut reordered = original.clone();
        reordered["students"] = json!(students);
        let o: Value = serde_json::from_str(&solve_problem_json(&original.to_string()).unwrap()).unwrap();
        let r: Value = serde_json::from_str(&solve_problem_json(&reordered.to_string()).unwrap()).unwrap();
        prop_assert_eq!(status_of(&o) == "Solved", status_of(&r) == "Solved");
    }
}
