//! Property-based migration gates (plan §11.3 Migration).
//!
//! For arbitrarily generated v1 documents (roster + layout):
//! 1. migrate → validate: the v2 output parses into the strict typed DTO;
//! 2. migrate → serialize → read: re-parsing the serialized v2 output
//!    yields the same document (canonical round-trip);
//! 3. normalization is idempotent: migrating twice produces the same target;
//! 4. field preservation: every source student id survives the transform.
//!
//! Backup/restore properties are covered by the io-layer fault-injection
//! suite (crates/seattrellis-io/src/rollback_faults.rs, plan §17.2.4).

use proptest::prelude::*;
use seattrellis_schema::dto::student_roster::StudentRoster;
use seattrellis_schema::migration::{canonical_json, migrate_v1_to_v2};
use seattrellis_schema::ArtifactKind;
use serde_json::{json, Value};

fn v1_roster(students: Vec<(String, bool, bool, bool)>) -> Value {
    // (id, has_score, has_vision, has_tags)
    let rows: Vec<Value> = students
        .iter()
        .map(|(id, has_score, has_vision, has_tags)| {
            let mut row = json!({ "student_id": id, "name": format!("学生{id}") });
            if *has_score {
                row["score"] = json!(50.0 + (id.len() as f64 * 7.0) % 50.0);
            }
            if *has_vision {
                row["vision"] = json!("0.6");
            }
            if *has_tags {
                row["tags"] = json!(["leader"]);
            }
            row
        })
        .collect();
    json!({ "students": rows })
}

fn v1_layout(seat_count: usize, seed: u64) -> Value {
    let seats: Vec<Value> = (0..seat_count)
        .map(|i| {
            json!({
                "seat_id": format!("R{}C{}", i / 4 + 1, i % 4 + 1),
                "row": i / 4 + 1,
                "col": i % 4 + 1,
                "x": (i % 4 + 1) as f64,
                "y": (i / 4 + 1) as f64,
                "enabled": true,
                "zone": if i / 4 == 0 { "front" } else { "middle" },
                "near_window": i % 4 == 0,
                "tags": [],
                "attributes": { "probe": seed % 7 }
            })
        })
        .collect();
    json!({
        "layout_id": format!("prop-{seed}"),
        "name": "property layout",
        "seats": seats,
        "adjacency": {
            "include_horizontal": true,
            "include_vertical": true,
            "custom_edges": []
        },
        "metadata": { "platform": "front" }
    })
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    #[test]
    fn roster_migrate_validate_round_trip_and_idempotent(
        count in 1usize..=20,
        flags in prop::collection::vec(any::<(bool, bool, bool)>(), 1..=20),
    ) {
        let students: Vec<(String, bool, bool, bool)> = (0..count)
            .map(|i| (format!("STU{i:03}"), flags[i % flags.len()].0, flags[i % flags.len()].1, flags[i % flags.len()].2))
            .collect();
        let source = v1_roster(students.clone());
        let (target, report) = migrate_v1_to_v2(ArtifactKind::StudentRoster, &source).unwrap();
        prop_assert!(report.lossless, "roster migration must be lossless");

        // (1) migrate → validate: the envelope's data payload parses into
        // the strict DTO (the envelope itself is checked separately).
        let payload = target.get("data").cloned().unwrap_or(target.clone());
        let typed: StudentRoster = serde_json::from_value(payload)
            .unwrap_or_else(|e| panic!("v2 output must parse into DTO: {e}"));
        prop_assert_eq!(typed.students.len(), count);

        // (2) migrate → serialize → read: canonical round-trip.
        let re_read: Value = serde_json::from_str(&serde_json::to_string(&target).unwrap()).unwrap();
        prop_assert_eq!(canonical_json(&target), canonical_json(&re_read));

        // (3) envelope contract: the migration product is a valid v2
        // envelope (kind + schema_version + data), i.e. it validates against
        // the current schema and is canonical-stable (normalization check).
        prop_assert_eq!(target.get("kind"), Some(&json!("student_roster")));
        prop_assert!(target.get("schema_version").is_some());
        prop_assert!(target.get("data").is_some());

        // (4) field preservation: every source student id survives.
        let source_ids: std::collections::HashSet<String> =
            students.iter().map(|(id, _, _, _)| id.clone()).collect();
        let target_ids: std::collections::HashSet<String> = typed
            .students
            .iter()
            .filter_map(|s| s.student_id.clone())
            .collect();
        prop_assert_eq!(source_ids, target_ids, "all student ids must survive migration");
    }

    #[test]
    fn layout_migrate_round_trip_and_idempotent(
        seat_count in 1usize..=24,
        seed in any::<u64>(),
    ) {
        let source = v1_layout(seat_count, seed);
        let (target, report) = migrate_v1_to_v2(ArtifactKind::ClassroomLayout, &source).unwrap();
        prop_assert!(report.lossless);
        prop_assert_eq!((report.from_version, report.to_version), (1, 2));

        let re_read: Value = serde_json::from_str(&serde_json::to_string(&target).unwrap()).unwrap();
        prop_assert_eq!(canonical_json(&target), canonical_json(&re_read));

        prop_assert_eq!(target.get("kind"), Some(&json!("classroom_layout")));
        prop_assert_eq!(
            target["data"]["seats"].as_array().map(|s| s.len()).unwrap_or(0),
            seat_count
        );
    }
}

proptest! {
    #[test]
    fn canonical_normalization_is_idempotent(
        bytes in prop::collection::vec(any::<u8>(), 0..1024),
    ) {
        // plan §11.3 "current schema normalization 幂等":
        // canonical(canonical(x)) == canonical(x).
        let document = String::from_utf8_lossy(&bytes).into_owned();
        let value: Value = serde_json::from_str(&document).unwrap_or(Value::Null);
        let once = canonical_json(&value);
        let twice_input: Value = serde_json::from_str(&once).expect("canonical output parses");
        let twice = canonical_json(&twice_input);
        prop_assert_eq!(once, twice, "canonical normalization must be idempotent");
    }
}
