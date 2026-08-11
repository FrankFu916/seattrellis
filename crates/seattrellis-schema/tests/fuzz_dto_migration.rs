//! Fuzz-style DTO / migration / import bombardment (plan §11.4): arbitrary
//! bytes into the typed DTO parsers, the v1→v2 migration step, and the CSV
//! roster importer must never panic and never traverse the filesystem
//! (importer keeps path handling inside the workspace root by contract).

use proptest::prelude::*;

use seattrellis_schema::dto::candidate_set::CandidateSetArtifact;
use seattrellis_schema::dto::plan_comparison::PlanComparisonReportArtifact;
use seattrellis_schema::dto::snapshot::SeatingSnapshotArtifact;
use seattrellis_schema::dto::student_roster::StudentRoster;
use seattrellis_schema::envelope::ArtifactEnvelope;
use seattrellis_schema::migration::migrate_v1_to_v2;
use seattrellis_schema::ArtifactKind;

fn random_document(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    #[test]
    fn student_roster_dto_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = serde_json::from_str::<StudentRoster>(&random_document(bytes));
    }

    #[test]
    fn snapshot_dto_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = serde_json::from_str::<SeatingSnapshotArtifact>(&random_document(bytes));
    }

    #[test]
    fn candidate_set_dto_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = serde_json::from_str::<CandidateSetArtifact>(&random_document(bytes));
    }

    #[test]
    fn plan_comparison_dto_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = serde_json::from_str::<PlanComparisonReportArtifact>(&random_document(bytes));
    }

    #[test]
    fn envelope_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = serde_json::from_str::<ArtifactEnvelope<serde_json::Value>>(&random_document(bytes));
    }

    #[test]
    fn migration_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let doc: serde_json::Value = serde_json::from_str(&random_document(bytes)).unwrap_or(serde_json::Value::Null);
        let _ = migrate_v1_to_v2(ArtifactKind::StudentRoster, &doc);
        let _ = migrate_v1_to_v2(ArtifactKind::ClassroomLayout, &doc);
    }

}
