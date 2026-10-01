//! Recent pair counts use the last four global snapshot periods.
//! Periods without a relationship age out earlier occurrences.

use seattrellis_core::pair_report_json;
use serde_json::json;

fn request_doc() -> serde_json::Value {
    json!({
        "api_version": 2,
        "student_count": 3,
        "students": [
            {"key": "STU001", "display_name": "Alpha"},
            {"key": "STU002", "display_name": "Beta"},
            {"key": "STU003", "display_name": "Gamma"},
        ],
        "seat_positions": [[0, 0], [0, 1], [0, 2]],
        "layout": {
            "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
                {"seat_id": "R1C2", "row": 1, "col": 2, "enabled": true},
                {"seat_id": "R1C3", "row": 1, "col": 3, "enabled": true},
            ]
        },
        "options": {}
    })
}

fn snapshot(a: &str, b: &str, c: &str) -> serde_json::Value {
    json!({
        "assignments": [
            {"student_key": a, "seat_id": "R1C1"},
            {"student_key": b, "seat_id": "R1C2"},
            {"student_key": c, "seat_id": "R1C3"},
        ]
    })
}

fn legacy_recent_occurrences(report: &serde_json::Value, total: u64) -> Option<u64> {
    report["top_pairs"]
        .as_array()?
        .iter()
        .find(|pair| pair["total_occurrences"].as_u64() == Some(total))
        .and_then(|pair| pair["recent_occurrences"].as_u64())
}

/// Six snapshots; `STU001`/`STU002` sit within distance 1 only in snapshot 1
/// (the other five put them at the two ends of the row, distance 2). With
/// the global lookback the old occurrence has expired because snapshot 1
/// falls outside the last-four-period window.
#[test]
fn recent_occurrences_expire_after_global_snapshot_window() {
    let request = request_doc();
    let separated = snapshot("STU001", "STU003", "STU002");
    let snapshots = json!([
        snapshot("STU001", "STU002", "STU003"),
        separated.clone(),
        separated.clone(),
        separated.clone(),
        separated.clone(),
        separated,
    ]);

    let report: serde_json::Value = serde_json::from_str(
        &pair_report_json(&request.to_string(), &snapshots.to_string(), 10, 1).unwrap(),
    )
    .unwrap();
    let pairs = report["pairs"].as_array().unwrap();
    let alpha_beta = pairs
        .iter()
        .find(|pair| pair["pair_key"] == "STU001|STU002")
        .expect("STU001|STU002 pair present");
    assert_eq!(
        alpha_beta["total_occurrences"], 1,
        "the pair is within distance 1 exactly once"
    );
    assert_eq!(
        legacy_recent_occurrences(&report, 1),
        Some(0),
        "an occurrence in period 1 expires after five non-neighbor periods"
    );
}

/// A pair with more than `PAIR_REPORT_RECENT_LOOKBACK` records is capped at
/// the lookback, never inflated by the full history.
#[test]
fn recent_occurrences_cap_at_the_lookback() {
    let request = request_doc();
    let snapshots = json!([
        snapshot("STU001", "STU002", "STU003"),
        snapshot("STU001", "STU002", "STU003"),
        snapshot("STU001", "STU002", "STU003"),
        snapshot("STU001", "STU002", "STU003"),
        snapshot("STU001", "STU002", "STU003"),
        snapshot("STU001", "STU002", "STU003"),
    ]);

    let report: serde_json::Value = serde_json::from_str(
        &pair_report_json(&request.to_string(), &snapshots.to_string(), 10, 1).unwrap(),
    )
    .unwrap();
    let pairs = report["pairs"].as_array().unwrap();
    let alpha_beta = pairs
        .iter()
        .find(|pair| pair["pair_key"] == "STU001|STU002")
        .expect("STU001|STU002 pair present");
    assert_eq!(alpha_beta["total_occurrences"], 6);
    assert_eq!(
        legacy_recent_occurrences(&report, 6),
        Some(4),
        "recent occurrences are capped at the lookback, not the total"
    );
}
