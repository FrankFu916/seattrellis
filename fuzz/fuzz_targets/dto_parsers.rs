//! Fuzz target: typed DTO + envelope parsers (plan §11.4).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = serde_json::from_str::<seattrellis_schema::dto::student_roster::StudentRoster>(&text);
    let _ = serde_json::from_str::<seattrellis_schema::dto::snapshot::SeatingSnapshotArtifact>(&text);
    let _ = serde_json::from_str::<seattrellis_schema::dto::candidate_set::CandidateSetArtifact>(&text);
    let _ = serde_json::from_str::<seattrellis_schema::dto::plan_comparison::PlanComparisonReportArtifact>(&text);
    let _ = serde_json::from_str::<seattrellis_schema::ArtifactEnvelope<serde_json::Value>>(&text);
});
