//! Fuzz target: roster CSV importer (plan §11.4). No panics, no traversal.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = seattrellis_io::roster::parse_roster_csv(data);
});
