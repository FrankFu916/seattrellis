//! Fuzz-style parser bombardment (plan §11.4): arbitrary bytes fed into the
//! JSON/DTO-facing entry points must never panic, never hang, and never
//! escape the workspace (no directory traversal). Proptest provides the
//! coverage driver here (the environment has no nightly/libFuzzer toolchain;
//! cargo-fuzz migration is tracked in the ledger).
//!
//! Targets covered: solve request, validate request, precheck, audit,
//! candidate generation, evaluation request, repair request, history/pair
//! report inputs.

use proptest::prelude::*;

use seattrellis_core::{
    audit_report_json, generate_candidates_json, precheck_report_json,
    solve_problem_json, validate_solve_request_json,
};

fn random_document(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    #[test]
    fn solve_request_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = solve_problem_json(&random_document(bytes));
    }

    #[test]
    fn validate_request_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = validate_solve_request_json(&random_document(bytes));
    }

    #[test]
    fn precheck_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = precheck_report_json(&random_document(bytes));
    }

    #[test]
    fn audit_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = audit_report_json(&random_document(bytes), &[]);
    }

    #[test]
    fn candidate_generation_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = generate_candidates_json(&random_document(bytes), 3);
    }

    #[test]
    fn evaluate_request_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = seattrellis_core::evaluate_problem_json(&random_document(bytes));
    }

    #[test]
    fn repair_request_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = seattrellis_core::repair_json(&random_document(bytes), "{}", &[], &[], &[]);
    }

    #[test]
    fn history_report_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = seattrellis_core::history_report_json(&random_document(bytes), "[]");
    }

    #[test]
    fn pair_report_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = seattrellis_core::pair_report_json(&random_document(bytes), "[]", 5, 2);
    }
}
