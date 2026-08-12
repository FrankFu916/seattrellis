//! Repair empty-seat lock gate (ledger §16 D.11 closure): locking an *empty*
//! seat must reserve it (stay empty after re-solve), mirroring the Python
//! `reserved_empty_seats` semantics, while occupied locked seats stay fixed.

use seattrellis_core::repair_json;
use serde_json::{json, Value};

fn request_with_spare_seats() -> Value {
    // 4 students, 6 seats (3x2 grid); seats R3C1/R3C2 are spare.
    json!({
        "api_version": 2,
        "student_count": 4,
        "seat_positions": [[1.0,1.0],[2.0,1.0],[1.0,2.0],[2.0,2.0],[1.0,3.0],[2.0,3.0]],
        "edges": [[0,1],[0,2],[1,3],[2,3],[2,4],[3,5],[4,5]],
        "fixed_seats": [],
        "must_be_adjacent": [],
        "cannot_be_adjacent": [],
        "min_distance": [],
        "seed": 7,
        "students": [
            {"key": "s0", "display_name": "学生0", "height_cm": 150.0, "score": 70.0, "vision": null, "tags": [], "needs": []},
            {"key": "s1", "display_name": "学生1", "height_cm": 160.0, "score": 75.0, "vision": null, "tags": [], "needs": []},
            {"key": "s2", "display_name": "学生2", "height_cm": 140.0, "score": 65.0, "vision": null, "tags": [], "needs": []},
            {"key": "s3", "display_name": "学生3", "height_cm": 170.0, "score": 80.0, "vision": null, "tags": [], "needs": []}
        ],
        "student_scores": [70.0, 75.0, 65.0, 80.0],
        "rules": {"schema_version": 0, "seed": 7, "hard": {}, "soft": {}, "groups": []},
        "layout": {
            "layout_id": "spare-layout",
            "name": "spare",
            "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "enabled": true, "zone": "front", "near_platform": true, "near_window": true, "near_door": false, "near_ac": false, "tags": [], "attributes": {}},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 2.0, "y": 1.0, "enabled": true, "zone": "front", "near_platform": true, "near_window": false, "near_door": false, "near_ac": false, "tags": [], "attributes": {}},
                {"seat_id": "R2C1", "row": 2, "col": 1, "x": 1.0, "y": 2.0, "enabled": true, "zone": "middle", "near_platform": false, "near_window": true, "near_door": false, "near_ac": false, "tags": [], "attributes": {}},
                {"seat_id": "R2C2", "row": 2, "col": 2, "x": 2.0, "y": 2.0, "enabled": true, "zone": "middle", "near_platform": false, "near_window": false, "near_door": false, "near_ac": false, "tags": [], "attributes": {}},
                {"seat_id": "R3C1", "row": 3, "col": 1, "x": 1.0, "y": 3.0, "enabled": true, "zone": "back", "near_platform": false, "near_window": true, "near_door": false, "near_ac": false, "tags": [], "attributes": {}},
                {"seat_id": "R3C2", "row": 3, "col": 2, "x": 2.0, "y": 3.0, "enabled": true, "zone": "back", "near_platform": false, "near_window": false, "near_door": false, "near_ac": false, "tags": [], "attributes": {}}
            ],
            "adjacency": {"include_horizontal": true, "include_vertical": true}
        },
        "history": null,
        "pair_history": null,
        "time_limit_seconds": null
    })
}

fn snapshot_doc() -> Value {
    json!({
        "schema_version": "0.2.2",
        "created_at": "2026-03-17T10:00:00Z",
        "seed": 7,
        "metadata": {},
        "students": [],
        "layout": {},
        "rules": {},
        "assignments": [
            {"student_key": "s0", "seat_id": "R1C1"},
            {"student_key": "s1", "seat_id": "R1C2"},
            {"student_key": "s2", "seat_id": "R2C1"},
            {"student_key": "s3", "seat_id": "R2C2"}
        ],
        "solver_status": "Solved"
    })
}

fn seats_after(repair: &Value) -> Vec<String> {
    repair["assignments"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row["seat_id"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn locked_empty_seat_stays_empty_after_repair() {
    let request = request_with_spare_seats();
    let repair = repair_json(
        &request.to_string(),
        &snapshot_doc().to_string(),
        &["s1".to_string()],   // affected student moves
        &[],                   // no locked students
        &["R3C1".to_string()], // locked *empty* seat must stay empty
    )
    .expect("repair succeeds with an empty-seat lock");
    let repaired: Value = serde_json::from_str(&repair).expect("repair output is JSON");
    let seats = seats_after(&repaired);
    assert!(
        !seats.contains(&"R3C1".to_string()),
        "reserved empty seat R3C1 must stay empty, got {seats:?}"
    );
    assert_eq!(seats.len(), 4, "all four students must be seated");
}

#[test]
fn occupied_locked_seat_stays_anchored() {
    let request = request_with_spare_seats();
    let repair = repair_json(
        &request.to_string(),
        &snapshot_doc().to_string(),
        &["s1".to_string()],
        &["s0".to_string()],
        &[],
    )
    .expect("repair succeeds");
    let repaired: Value = serde_json::from_str(&repair).expect("repair output is JSON");
    let assignments = repaired["assignments"].as_array().unwrap();
    let s0_seat = assignments
        .iter()
        .find(|row| row["student_key"] == "s0")
        .and_then(|row| row["seat_id"].as_str())
        .expect("s0 stays seated");
    assert_eq!(s0_seat, "R1C1", "locked student must stay on R1C1");
}

#[test]
fn reserved_empty_seat_conflicting_with_fixed_rule_is_rejected() {
    let mut request = request_with_spare_seats();
    // Hard-fix s0 to the seat we want to reserve empty.
    request["fixed_seats"] = json!([[0, 4]]); // s0 -> R3C1
    let error = repair_json(
        &request.to_string(),
        &snapshot_doc().to_string(),
        &[],
        &[],
        &["R3C1".to_string()],
    )
    .expect_err("reserving a seat required by a hard rule must fail");
    assert!(
        error.contains("Cannot reserve") && error.contains("R3C1"),
        "unexpected error: {error}"
    );
}

#[test]
fn saved_locks_from_snapshot_metadata_are_reused() {
    // Python `reuse_saved_locks` semantics: locks persisted in the snapshot
    // metadata (lock_state) are merged into the repair anchors even when the
    // explicit anchor lists are empty.
    let request = request_with_spare_seats();
    let mut snapshot = snapshot_doc();
    snapshot["metadata"] = json!({
        "lock_state": {
            "locked_students": ["s0"],
            "locked_seats": ["R3C2"]
        }
    });
    let repair = repair_json(
        &request.to_string(),
        &snapshot.to_string(),
        &["s1".to_string()],
        &[],
        &[],
    )
    .expect("repair succeeds with saved locks");
    let repaired: Value = serde_json::from_str(&repair).expect("repair output is JSON");
    let assignments = repaired["assignments"].as_array().unwrap();
    // s0 (saved locked student) stays on R1C1.
    let s0_seat = assignments
        .iter()
        .find(|row| row["student_key"] == "s0")
        .and_then(|row| row["seat_id"].as_str())
        .expect("s0 seated");
    assert_eq!(s0_seat, "R1C1", "saved locked student must stay anchored");
    // R3C2 (saved locked empty seat) stays empty.
    let seats: Vec<String> = assignments
        .iter()
        .map(|row| row["seat_id"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        !seats.contains(&"R3C2".to_string()),
        "saved locked empty seat must stay empty, got {seats:?}"
    );
}

#[test]
fn saved_lock_conflicting_with_affected_student_is_rejected() {
    let request = request_with_spare_seats();
    let mut snapshot = snapshot_doc();
    snapshot["metadata"] = json!({
        "lock_state": {"locked_students": ["s0"], "locked_seats": []}
    });
    // Affecting the saved-locked student must fail (affected ∩ locked).
    let error = repair_json(
        &request.to_string(),
        &snapshot.to_string(),
        &["s0".to_string()],
        &[],
        &[],
    )
    .expect_err("affected student cannot be saved-locked");
    assert!(
        error.contains("cannot also be locked"),
        "unexpected error: {error}"
    );
}

#[test]
fn ignore_saved_locks_skips_persisted_metadata_locks() {
    // Python `--ignore-saved-locks` (`reuse_saved_locks=False`): the locks
    // persisted in the snapshot metadata are NOT merged into the anchors.
    // Deterministic consequence: a saved-locked student may be listed as
    // affected (Python rejects affected ∩ saved-locked), and the reserved
    // empty seat is not forced to stay empty.
    use seattrellis_core::repair_json_with_options;
    let request = request_with_spare_seats();
    let mut snapshot = snapshot_doc();
    snapshot["metadata"] = json!({
        "lock_state": {
            "locked_students": ["s0"],
            "locked_seats": ["R3C2"]
        }
    });
    // With saved locks ignored, s0 is not locked: affecting s0 succeeds.
    let repair = repair_json_with_options(
        &request.to_string(),
        &snapshot.to_string(),
        &["s0".to_string()],
        &[],
        &[],
        false, // reuse_saved_locks = false (--ignore-saved-locks)
    )
    .expect("affected student may be the saved-locked one when locks are ignored");
    let repaired: Value = serde_json::from_str(&repair).expect("repair output is JSON");
    let seats = seats_after(&repaired);
    assert_eq!(seats.len(), 4, "all four students must be seated");
    // The same request WITHOUT the ignore flag must reject the conflict,
    // mirroring Python's affected ∩ saved-locked rejection.
    let error = repair_json(
        &request.to_string(),
        &snapshot.to_string(),
        &["s0".to_string()],
        &[],
        &[],
    )
    .expect_err("affected ∩ saved-locked is rejected when saved locks are reused");
    assert!(
        error.contains("cannot also be locked"),
        "unexpected error: {error}"
    );
}

#[test]
fn ignore_saved_locks_still_honors_explicit_locks() {
    use seattrellis_core::repair_json_with_options;
    let request = request_with_spare_seats();
    let mut snapshot = snapshot_doc();
    snapshot["metadata"] = json!({
        "lock_state": {"locked_students": ["s0"], "locked_seats": []}
    });
    let repair = repair_json_with_options(
        &request.to_string(),
        &snapshot.to_string(),
        &["s1".to_string()],
        &["s0".to_string()], // explicit lock still applies
        &[],
        false,
    )
    .expect("repair succeeds with explicit locks");
    let repaired: Value = serde_json::from_str(&repair).expect("repair output is JSON");
    let assignments = repaired["assignments"].as_array().unwrap();
    let s0_seat = assignments
        .iter()
        .find(|row| row["student_key"] == "s0")
        .and_then(|row| row["seat_id"].as_str())
        .expect("s0 seated");
    assert_eq!(
        s0_seat, "R1C1",
        "explicit locked student must stay anchored"
    );
}
