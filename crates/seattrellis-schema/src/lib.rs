//! seattrellis-schema: the v2 durable artifact contract (M2-01/M2-02).
//!
//! - [`envelope`]: the `{kind, schema_version, data, extensions}` artifact
//!   envelope with strict parsing.
//! - [`registry`]: the artifact registry (kind → current version + migration
//!   policy); future/unknown versions are rejected, never guessed.
//! - [`dto`]: typed v2 payloads (StudentRoster, ClassroomLayout, ...),
//!   strict `deny_unknown_fields`, mirroring the v1 Python models so a
//!   v1→v2 migration step is a field-preserving transform (M2-03).
//!
//! The JSON Schema files under `schemas/*.v2.schema.json` are generated from
//! these types (`cargo run -p xtask -- contract schemas`) and drift-checked
//! in CI.

pub mod dto;
pub mod envelope;
pub mod migration;
pub mod privacy;
pub mod registry;

pub use envelope::ArtifactEnvelope;
pub use migration::{migrate_v1_to_v2, MigrationReport};
pub use privacy::{
    aggregate_verdicts, classify_document, classify_findings, classify_scan, classify_unscanned,
    is_sensitive_key, scan_document, PrivacyFinding, PrivacyVerdict,
};
pub use registry::{
    check_version, entry_for, ArtifactEntry, ArtifactKind, REGISTRY, V2_ARTIFACT_VERSION,
};
