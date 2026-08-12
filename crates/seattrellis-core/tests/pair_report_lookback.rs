//! Boundary regression for the pair-report `recent_occurrences` lookback
//! (ledger §19.3.3/§19.33): the Python oracle computes the recent count on
//! the *pair's own records* (`records[-lookback:]`, models/history.py:138),
//! not on a global snapshot window. A pair whose only occurrence is in an
//! old snapshot still reports one recent occurrence when it has fewer than
//! `PAIR_REPORT_RECENT_LOOKBACK` records total. The old Rust code used a
//! global window (`snapshot_index >= len - lookback + 1`), which undercounted
//! exactly that case; the fixture corpus (<= 4 snapshots) could not tell the
//! two apart. `recent_occurrences` is emitted in the anonymous `top_pairs`
//! compatibility view, mirroring the oracle report.

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
/// the per-pair lookback the pair keeps `min(1, 4) = 1` recent occurrence;
/// the old global-window code reported 0 (snapshot 1 falls outside the
/// last-4 window).
#[test]
fn recent_occurrences_use_the_pair_own_record_window() {
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
        Some(1),
        "per-pair lookback: one record in total -> one recent occurrence \
         (old global-window code reported 0)"
    );
}

/// A pair with more than `PAIR_REPORT_RECENT_LOOKBACK` records is capped at
/// the lookback, never inflated by the full history (Python
/// `records[-lookback:]`).
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
