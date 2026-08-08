//! Artifact registry (M2-01): every long-lived artifact kind, its current
//! v2 schema version and its migration policy. Readers must consult this
//! table before parsing `data`; future versions are rejected, never guessed.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stable artifact identifiers (plan §四.3). Serialized as snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    StudentRoster,
    ClassroomLayout,
    RuleSet,
    SeatingSnapshot,
    CandidateSet,
    PlanComparison,
    HistoryArchive,
    RotationPlan,
    EditingOperationLog,
    Project,
    ProjectBundleManifest,
    ExportPreset,
}

/// The current v2 artifact version. Independent of the product SemVer.
pub const V2_ARTIFACT_VERSION: u32 = 2;

/// Per-kind registry entry: the version this crate reads/writes today and
/// the migration policy for older versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactEntry {
    pub kind: ArtifactKind,
    /// The schema version this crate natively reads/writes.
    pub current_version: u32,
    /// Whether an older version can be migrated (typed transform). Until
    /// M2-03 lands the migration graph this is always false: old artifacts
    /// are rejected, never silently reshaped.
    pub migratable_from_older: bool,
}

pub const REGISTRY: &[ArtifactEntry] = &[
    ArtifactEntry { kind: ArtifactKind::StudentRoster, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::ClassroomLayout, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::RuleSet, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::SeatingSnapshot, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::CandidateSet, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::PlanComparison, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::HistoryArchive, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::RotationPlan, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::EditingOperationLog, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::Project, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::ProjectBundleManifest, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
    ArtifactEntry { kind: ArtifactKind::ExportPreset, current_version: V2_ARTIFACT_VERSION, migratable_from_older: false },
];

/// Look up a kind in the registry.
pub fn entry_for(kind: ArtifactKind) -> Option<&'static ArtifactEntry> {
    REGISTRY.iter().find(|entry| entry.kind == kind)
}

/// Validate an envelope's kind/version pair before parsing `data`.
///
/// Returns the registry entry on success. A `schema_version` that is not the
/// current one is rejected: without a typed migration step (M2-03) an old
/// artifact must never be silently reshaped.
pub fn check_version(kind: ArtifactKind, schema_version: u32) -> Result<&'static ArtifactEntry, String> {
    let entry = entry_for(kind)
        .ok_or_else(|| format!("unknown artifact kind: {kind:?}"))?;
    if schema_version != entry.current_version {
        return Err(format!(
            "unsupported schema_version {schema_version} for {kind:?}; \
             current version is {} and no migration step exists yet (M2-03)",
            entry.current_version
        ));
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_registry_entry() {
        for kind in [
            ArtifactKind::StudentRoster,
            ArtifactKind::ClassroomLayout,
            ArtifactKind::RuleSet,
            ArtifactKind::SeatingSnapshot,
            ArtifactKind::CandidateSet,
            ArtifactKind::PlanComparison,
            ArtifactKind::HistoryArchive,
            ArtifactKind::RotationPlan,
            ArtifactKind::EditingOperationLog,
            ArtifactKind::Project,
            ArtifactKind::ProjectBundleManifest,
            ArtifactKind::ExportPreset,
        ] {
            assert!(entry_for(kind).is_some(), "registry entry missing for {kind:?}");
        }
    }

    #[test]
    fn current_version_is_accepted() {
        assert!(check_version(ArtifactKind::StudentRoster, 2).is_ok());
    }

    #[test]
    fn future_versions_are_rejected() {
        let error = check_version(ArtifactKind::StudentRoster, 3).unwrap_err();
        assert!(error.contains("unsupported schema_version 3"));
    }

    #[test]
    fn kind_wire_spelling_is_frozen() {
        assert_eq!(
            serde_json::to_string(&ArtifactKind::StudentRoster).unwrap(),
            "\"student_roster\""
        );
        assert_eq!(
            serde_json::to_string(&ArtifactKind::ProjectBundleManifest).unwrap(),
            "\"project_bundle_manifest\""
        );
    }
}
