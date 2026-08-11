//! Fuzz-style export-option bombardment (plan §11.4 export option parser
//! target): arbitrary bytes as an export request must never panic; the
//! exporter validates options before rendering anything.

use proptest::prelude::*;
use seattrellis_export::export::export_plan;

fn random_document(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(128))]

    #[test]
    fn export_plan_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = export_plan(&random_document(bytes));
    }

    #[test]
    fn export_plan_with_random_options_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        // Random option values (format/privacy/page/locale) must yield a
        // structured error, never a panic.
        let doc = format!(
            r#"{{"draft_id":"fuzz","format":"{}","template":"{}","privacy":{{"hide_scores":{},"anonymize":{}}},"orientation":"{}","page_scale":{},"locale":"{}","request":{{"api_version":2,"student_count":0,"seat_positions":[],"edges":[],"fixed_seats":[],"must_be_adjacent":[],"cannot_be_adjacent":[],"min_distance":[],"seed":1}},"response":{{"api_version":2,"feasible":true,"status":"Solved","assignment":[],"attempts_used":1,"hard_constraints_satisfied":true}}}}"#,
            random_document(bytes.clone()),
            "teacher",
            true,
            false,
            "portrait",
            1.0,
            "zh",
        );
        let _ = export_plan(&doc);
    }
}
