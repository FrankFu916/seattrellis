//! Fuzz target: export request / option parser (plan §11.4).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = seattrellis_export::export::export_plan(&text);
});
