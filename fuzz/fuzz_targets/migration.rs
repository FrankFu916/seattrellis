//! Fuzz target: v1→v2 migration + artifact detection (plan §11.4).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let _ = seattrellis_schema::migration::migrate_v1_to_v2(
        seattrellis_schema::ArtifactKind::StudentRoster,
        &value,
    );
    let _ = seattrellis_schema::migration::migrate_v1_to_v2(
        seattrellis_schema::ArtifactKind::ClassroomLayout,
        &value,
    );
});
