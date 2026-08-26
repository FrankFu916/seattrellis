//! Reserved-empty-seat domain gate (F·HIGH): a locked *empty* seat must leave
//! the solver's search domain entirely. The repair used to only flip the
//! layout's `enabled` flag, but the solve domain comes from `seat_positions`
//! and several cost paths never read `enabled` — notably the `score_balance`
//! adjacency reward in `full_solution_total_cost`, which walks the index
//! graph built from `request.edges`. This fixture makes R2C2 strictly
//! required by the optimum: the only score-balance edge is (seat 0, seat 3)
//! and seating the extreme scorers there earns `-weight * |Δscore|`, which
//! dwarfs every other term. Before the fix the repair solve reliably seats a
//! student on the locked-empty R2C2; after the fix the seat cannot be taken.

use seattrellis_core::repair_json;
use serde_json::{json, Value};

fn request() -> Value {
    // Seat order: 0=R1C1, 1=R1C2, 2=R2C1, 3=R2C2 (the reserved empty seat).
    json!({
        "api_version": 2,
        "student_count": 3,
        "seat_positions": [[1.0, 1.0], [2.0, 1.0], [1.0, 2.0], [2.0, 2.0]],
        "edges": [[0, 3]],
        "fixed_seats": [],
        "must_be_adjacent": [],
        "cannot_be_adjacent": [],
        "min_distance": [],
        "seed": 7,
        "students": [
            {"key": "hi", "display_name": "高分", "score": 100.0},
            {"key": "mid", "display_name": "中分", "score": 60.0},
            {"key": "lo", "display_name": "低分", "score": 20.0}
        ],
        "rules": {
            "seed": 7,
            "soft": {
                "vision_front": {"enabled": false, "weight": 20},
                "height_back": {"enabled": false, "weight": 1},
                "randomize": {"enabled": false, "weight": 0},
                "score_balance": {"enabled": true, "weight": 100},
                "score_position": {"enabled": false, "weight": 1, "direction": "high_front"},
                "score_distribution": {"enabled": false, "weight": 1, "scope": "row"}
            }
        },
        "layout": {
            "layout_id": "reserved-domain",
            "name": "reserved domain",
            "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "enabled": true},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 2.0, "y": 1.0, "enabled": true},
                {"seat_id": "R2C1", "row": 2, "col": 1, "x": 1.0, "y": 2.0, "enabled": true},
                {"seat_id": "R2C2", "row": 2, "col": 2, "x": 2.0, "y": 2.0, "enabled": true}
            ],
            "adjacency": {"include_horizontal": true, "include_vertical": false}
        }
    })
}

fn snapshot_doc() -> Value {
    json!({
        "schema_version": "0.2.2",
        "created_at": "2026-08-24T10:00:00Z",
        "seed": 7,
        "metadata": {},
        "students": [],
        "layout": {},
        "rules": {},
        "assignments": [
            {"student_key": "hi", "seat_id": "R1C1"},
            {"student_key": "mid", "seat_id": "R1C2"},
            {"student_key": "lo", "seat_id": "R2C1"}
        ],
        "solver_status": "Solved"
    })
}

#[test]
fn reserved_most_attractive_seat_is_never_taken_after_repair() {
    let repair = repair_json(
        &request().to_string(),
        &snapshot_doc().to_string(),
        &[],
        &[],
        &["R2C2".to_string()],
    )
    .expect("repair succeeds with the empty-seat lock");
    let repaired: Value = serde_json::from_str(&repair).expect("repair output is JSON");

    let rows = repaired["assignments"].as_array().expect("assignments");
    assert_eq!(rows.len(), 3, "all three students must stay seated");
    let seats: Vec<&str> = rows
        .iter()
        .map(|row| row["seat_id"].as_str().expect("seat_id"))
        .collect();
    assert!(
        !seats.contains(&"R2C2"),
        "reserved empty seat R2C2 must stay empty, got {seats:?}"
    );
    for student in ["hi", "mid", "lo"] {
        assert!(
            rows.iter().any(|row| row["student_key"] == student),
            "{student} must remain seated, got {rows:?}"
        );
    }
}
