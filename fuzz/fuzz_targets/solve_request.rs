//! Fuzz target: solve request parser + solver entry (plan §11.4).
//! Must never panic / hang / OOM on arbitrary input.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = seattrellis_core::solve_problem_json(&text);
    let _ = seattrellis_core::validate_solve_request_json(&text);
    let _ = seattrellis_core::precheck_report_json(&text);
});
