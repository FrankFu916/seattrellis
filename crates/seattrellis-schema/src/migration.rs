//! v1→v2 artifact migration graph (M2-03).
//!
//! Every step records its version pair, whether it is lossless, any static
//! warnings, and the canonical (sorted-key) SHA-256 of the source and target
//! documents so reruns and idempotency can be verified. A v1 field that
//! cannot be interpreted is an *error* — old fields are never silently
//! dropped (plan §八: v1 reader 与 v2 writer 分离；v2 final 不允许静默
//! lossy migration).
//!
//! The v1 readers (`V1StudentRoster`, `V1ClassroomLayout`) parse strictly:
//! unknown v1 fields block the migration instead of vanishing.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::classroom_layout::{AdjacencyConfig, ClassroomLayout, SeatNode};
use crate::dto::project::{ExportFormat, SeatTrellisProjectArtifact};
use crate::dto::student_roster::{RosterStudent, StudentRoster};
use crate::{ArtifactEnvelope, ArtifactKind};

/// The record attached to every applied migration step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MigrationReport {
    pub from_version: u32,
    pub to_version: u32,
    /// False when the step drops or reshapes information.
    pub lossless: bool,
    /// Static warnings for the step (e.g. approximate semantics).
    pub warnings: Vec<String>,
    /// SHA-256 of the canonical (sorted-key) source document.
    pub source_hash: String,
    /// SHA-256 of the canonical (sorted-key) target document.
    pub target_hash: String,
}

/// Canonical serialization used for the source/target hashes: sorted keys,
/// no whitespace. Byte-stable across runs and platforms.
pub fn canonical_json(document: &Value) -> String {
    serde_json::to_string(&document).expect("document serializes")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = <sha2::Sha256 as sha2::Digest>::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn build_report(
    from_version: u32,
    to_version: u32,
    lossless: bool,
    warnings: Vec<String>,
    source: &Value,
    target: &Value,
) -> MigrationReport {
    MigrationReport {
        from_version,
        to_version,
        lossless,
        warnings,
        source_hash: sha256_hex(canonical_json(source).as_bytes()),
        target_hash: sha256_hex(canonical_json(target).as_bytes()),
    }
}

// ---------------------------------------------------------------------------
// v1 readers (strict: unknown v1 fields block the migration)
// ---------------------------------------------------------------------------

/// v1 student roster: the ordered student list (schemas/student.schema.json,
/// implicit version 1 — the v1 schema has no version field).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V1StudentRoster {
    pub students: Vec<V1Student>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V1Student {
    #[serde(default)]
    pub student_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub gender: Option<String>,
    #[serde(default)]
    pub height_cm: Option<f64>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub vision: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub attributes: HashMap<String, Value>,
}

impl From<V1Student> for RosterStudent {
    fn from(v1: V1Student) -> Self {
        RosterStudent {
            student_id: v1.student_id,
            name: v1.name,
            gender: v1.gender,
            height_cm: v1.height_cm,
            score: v1.score,
            vision: v1.vision,
            notes: v1.notes,
            tags: v1.tags,
            needs: v1.needs,
            attributes: v1.attributes,
        }
    }
}

/// v1 classroom layout (schemas/classroom-layout.schema.json, implicit v1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V1ClassroomLayout {
    #[serde(default)]
    pub layout_id: String,
    #[serde(default)]
    pub name: String,
    pub seats: Vec<V1SeatNode>,
    #[serde(default)]
    pub adjacency: V1AdjacencyConfig,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V1SeatNode {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub near_window: bool,
    #[serde(default)]
    pub near_door: bool,
    #[serde(default)]
    pub near_platform: bool,
    #[serde(default)]
    pub near_ac: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub attributes: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V1AdjacencyConfig {
    #[serde(default = "default_true")]
    pub include_horizontal: bool,
    #[serde(default)]
    pub include_vertical: bool,
    #[serde(default)]
    pub include_diagonal: bool,
    #[serde(default = "default_one")]
    pub max_row_delta: i32,
    #[serde(default = "default_one")]
    pub max_col_delta: i32,
    #[serde(default)]
    pub max_distance: Option<f64>,
    #[serde(default = "default_true")]
    pub use_xy_distance: bool,
    #[serde(default)]
    pub custom_edges: Vec<(String, String)>,
}

impl Default for V1AdjacencyConfig {
    /// v1 defaults: horizontal adjacency only, delta 1, xy distances.
    fn default() -> Self {
        V1AdjacencyConfig {
            include_horizontal: true,
            include_vertical: false,
            include_diagonal: false,
            max_row_delta: 1,
            max_col_delta: 1,
            max_distance: None,
            use_xy_distance: true,
            custom_edges: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_one() -> i32 {
    1
}

fn default_outputs_dir() -> String {
    "outputs".to_string()
}

fn default_candidates() -> u32 {
    5
}

fn default_candidate() -> String {
    "recommended".to_string()
}

fn default_export_format() -> ExportFormat {
    ExportFormat::Html
}

/// v1 project workspace document (schemas/project.schema.json, version 1).
/// Strict: unknown v1 fields block the migration. Field-level validation
/// mirrors the Python `SeatTrellisProject` model (models/project.py): the
/// kind literal, the supported schema version and the `default_candidates`
/// range are checked after parsing; relative-path checks remain the job of
/// the workspace loader (mirroring the DTO contract).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V1Project {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub name: String,
    pub students: String,
    pub layout: String,
    pub rules: String,
    #[serde(default)]
    pub history_dir: Option<String>,
    #[serde(default = "default_outputs_dir")]
    pub outputs_dir: String,
    #[serde(default = "default_candidates")]
    pub default_candidates: u32,
    #[serde(default = "default_candidate")]
    pub default_candidate: String,
    #[serde(default = "default_export_format")]
    pub default_export_format: ExportFormat,
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

/// v1 StudentRoster → v2 `ArtifactEnvelope<StudentRoster>`. Field-preserving
/// (lossless); the v2 envelope adds kind/version/extensions.
pub fn migrate_student_roster_v1_to_v2(source: &Value) -> Result<(Value, MigrationReport), String> {
    let v1: V1StudentRoster = serde_json::from_value(source.clone())
        .map_err(|error| format!("invalid v1 student roster: {error}"))?;
    let data = StudentRoster {
        students: v1.students.into_iter().map(RosterStudent::from).collect(),
    };
    let envelope = ArtifactEnvelope::new(ArtifactKind::StudentRoster, data);
    let target = serde_json::to_value(&envelope)
        .map_err(|error| format!("cannot serialize v2 roster: {error}"))?;
    let report = build_report(1, 2, true, Vec::new(), source, &target);
    Ok((target, report))
}

/// v1 ClassroomLayout → v2 `ArtifactEnvelope<ClassroomLayout>`. Lossless.
pub fn migrate_classroom_layout_v1_to_v2(
    source: &Value,
) -> Result<(Value, MigrationReport), String> {
    let v1: V1ClassroomLayout = serde_json::from_value(source.clone())
        .map_err(|error| format!("invalid v1 classroom layout: {error}"))?;
    let data = ClassroomLayout {
        layout_id: v1.layout_id,
        name: v1.name,
        seats: v1
            .seats
            .into_iter()
            .map(|seat| SeatNode {
                seat_id: seat.seat_id,
                row: seat.row,
                col: seat.col,
                x: seat.x,
                y: seat.y,
                enabled: seat.enabled,
                zone: seat.zone,
                group_id: seat.group_id,
                near_window: seat.near_window,
                near_door: seat.near_door,
                near_platform: seat.near_platform,
                near_ac: seat.near_ac,
                tags: seat.tags,
                attributes: seat.attributes,
            })
            .collect(),
        adjacency: AdjacencyConfig {
            include_horizontal: v1.adjacency.include_horizontal,
            include_vertical: v1.adjacency.include_vertical,
            include_diagonal: v1.adjacency.include_diagonal,
            max_row_delta: v1.adjacency.max_row_delta,
            max_col_delta: v1.adjacency.max_col_delta,
            max_distance: v1.adjacency.max_distance,
            use_xy_distance: v1.adjacency.use_xy_distance,
            custom_edges: v1.adjacency.custom_edges,
        },
        metadata: v1.metadata,
    };
    let envelope = ArtifactEnvelope::new(ArtifactKind::ClassroomLayout, data);
    let target = serde_json::to_value(&envelope)
        .map_err(|error| format!("cannot serialize v2 layout: {error}"))?;
    let report = build_report(1, 2, true, Vec::new(), source, &target);
    Ok((target, report))
}

/// v1 project document → v2 `ArtifactEnvelope<SeatTrellisProjectArtifact>`.
/// Field-preserving (lossless): the envelope adds kind/version/extensions
/// while every v1 field — including the generation/export defaults — maps
/// onto the typed v2 DTO. The v2 DTO parses strictly, so a lossy or
/// malformed migration can never pass as a valid target.
pub fn migrate_project_v1_to_v2(source: &Value) -> Result<(Value, MigrationReport), String> {
    let v1: V1Project = serde_json::from_value(source.clone())
        .map_err(|error| format!("invalid v1 project: {error}"))?;
    if v1.kind != "seattrellis_project" {
        return Err(format!(
            "invalid v1 project: kind must be \"seattrellis_project\", got {:?}",
            v1.kind
        ));
    }
    if v1.schema_version != 1 {
        return Err(format!(
            "invalid v1 project: unsupported schema_version {} (expected 1)",
            v1.schema_version
        ));
    }
    if !(1..=20).contains(&v1.default_candidates) {
        return Err("invalid v1 project: default_candidates must be between 1 and 20".to_string());
    }
    let data = SeatTrellisProjectArtifact {
        kind: v1.kind,
        schema_version: v1.schema_version,
        name: v1.name,
        students: v1.students,
        layout: v1.layout,
        rules: v1.rules,
        history_dir: v1.history_dir,
        outputs_dir: v1.outputs_dir,
        default_candidates: v1.default_candidates,
        default_candidate: v1.default_candidate,
        default_export_format: v1.default_export_format,
    };
    let envelope = ArtifactEnvelope::new(ArtifactKind::Project, data);
    let target = serde_json::to_value(&envelope)
        .map_err(|error| format!("cannot serialize v2 project: {error}"))?;
    let report = build_report(1, 2, true, Vec::new(), source, &target);
    Ok((target, report))
}

/// The v1→v2 dispatch for the kinds this crate currently migrates. Unknown
/// kinds are an error: migration coverage is explicit, never inferred.
pub fn migrate_v1_to_v2(
    kind: ArtifactKind,
    source: &Value,
) -> Result<(Value, MigrationReport), String> {
    match kind {
        ArtifactKind::StudentRoster => migrate_student_roster_v1_to_v2(source),
        ArtifactKind::ClassroomLayout => migrate_classroom_layout_v1_to_v2(source),
        ArtifactKind::Project => migrate_project_v1_to_v2(source),
        other => Err(format!(
            "no v1→v2 migration step registered for {other:?} (M2-03 coverage is explicit)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1_ROSTER: &str = r#"{
        "students": [
            { "student_id": "STU001", "name": "学生一", "gender": "F",
              "height_cm": 165.0, "score": 88.5, "vision": "0.8",
              "notes": null, "tags": ["leader"], "needs": ["vision_front"],
              "attributes": { "class": "7A" } },
            { "student_id": "STU002", "name": "学生二" }
        ]
    }"#;

    const V1_LAYOUT: &str = r#"{
        "layout_id": "room-1", "name": "教室 A",
        "seats": [ { "seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0,
                     "enabled": true, "zone": "front", "near_window": true,
                     "tags": [], "attributes": {} } ],
        "adjacency": { "include_horizontal": true, "include_vertical": true,
                       "custom_edges": [["R1C1", "R2C2"]] },
        "metadata": { "platform": "front" }
    }"#;

    const V1_PROJECT: &str = r#"{
        "kind": "seattrellis_project",
        "schema_version": 1,
        "name": "Demo 班级",
        "students": "students.csv",
        "layout": "classroom.json",
        "rules": "rules.json",
        "history_dir": "history",
        "outputs_dir": "outputs",
        "default_candidates": 7,
        "default_candidate": "balanced",
        "default_export_format": "png"
    }"#;

    #[test]
    fn student_roster_v1_migrates_losslessly() {
        let source: Value = serde_json::from_str(V1_ROSTER).unwrap();
        let (target, report) = migrate_v1_to_v2(ArtifactKind::StudentRoster, &source).unwrap();
        assert!(report.lossless);
        assert_eq!((report.from_version, report.to_version), (1, 2));
        assert!(!report.source_hash.is_empty());
        assert!(!report.target_hash.is_empty());

        // The target must parse as the typed v2 envelope.
        let parsed: ArtifactEnvelope<StudentRoster> = serde_json::from_value(target).unwrap();
        assert_eq!(parsed.kind, ArtifactKind::StudentRoster);
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.data.students.len(), 2);
        assert_eq!(parsed.data.students[0].name.as_deref(), Some("学生一"));
        assert_eq!(parsed.data.students[0].attributes["class"], "7A");
        assert_eq!(parsed.data.students[1].gender, None);
    }

    #[test]
    fn classroom_layout_v1_migrates_losslessly() {
        let source: Value = serde_json::from_str(V1_LAYOUT).unwrap();
        let (target, report) = migrate_v1_to_v2(ArtifactKind::ClassroomLayout, &source).unwrap();
        assert!(report.lossless);
        let parsed: ArtifactEnvelope<ClassroomLayout> = serde_json::from_value(target).unwrap();
        assert_eq!(parsed.data.seats[0].zone.as_deref(), Some("front"));
        assert_eq!(
            parsed.data.adjacency.custom_edges,
            vec![("R1C1".to_string(), "R2C2".to_string())]
        );
        assert_eq!(parsed.data.metadata["platform"], "front");
    }

    #[test]
    fn unknown_v1_fields_block_the_migration() {
        let mut source: Value = serde_json::from_str(V1_ROSTER).unwrap();
        source["students"][0]["mystery_field"] = Value::Bool(true);
        let error = migrate_v1_to_v2(ArtifactKind::StudentRoster, &source).unwrap_err();
        assert!(
            error.contains("invalid v1 student roster"),
            "error: {error}"
        );
    }

    #[test]
    fn unregistered_kinds_are_explicit_errors() {
        let error = migrate_v1_to_v2(ArtifactKind::RuleSet, &Value::Null).unwrap_err();
        assert!(error.contains("no v1→v2 migration step registered"));
    }

    #[test]
    fn migration_hashes_are_deterministic() {
        let source: Value = serde_json::from_str(V1_ROSTER).unwrap();
        let (_, first) = migrate_v1_to_v2(ArtifactKind::StudentRoster, &source).unwrap();
        let (_, second) = migrate_v1_to_v2(ArtifactKind::StudentRoster, &source).unwrap();
        assert_eq!(first.source_hash, second.source_hash);
        assert_eq!(first.target_hash, second.target_hash);
    }

    #[test]
    fn migrated_target_round_trips_through_strict_parse() {
        let source: Value = serde_json::from_str(V1_ROSTER).unwrap();
        let (target, _) = migrate_v1_to_v2(ArtifactKind::StudentRoster, &source).unwrap();
        let json = serde_json::to_string(&target).unwrap();
        // Strict: a tampered target must fail to parse.
        let mut tampered: Value = serde_json::from_str(&json).unwrap();
        tampered["data"]["students"][0]["mystery"] = Value::Null;
        assert!(serde_json::from_value::<ArtifactEnvelope<StudentRoster>>(tampered).is_err());
    }

    #[test]
    fn project_v1_migrates_losslessly_with_defaults_preserved() {
        let source: Value = serde_json::from_str(V1_PROJECT).unwrap();
        let (target, report) = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap();
        assert!(report.lossless);
        assert_eq!((report.from_version, report.to_version), (1, 2));
        assert!(!report.source_hash.is_empty());
        assert!(!report.target_hash.is_empty());

        // The target must parse as the typed v2 envelope (kind/schema/data).
        let parsed: ArtifactEnvelope<SeatTrellisProjectArtifact> =
            serde_json::from_value(target).unwrap();
        assert_eq!(parsed.kind, ArtifactKind::Project);
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.data.name, "Demo 班级");
        assert_eq!(parsed.data.students, "students.csv");
        assert_eq!(parsed.data.layout, "classroom.json");
        assert_eq!(parsed.data.rules, "rules.json");
        assert_eq!(parsed.data.history_dir.as_deref(), Some("history"));
        assert_eq!(parsed.data.outputs_dir, "outputs");
        // The v1 defaults must survive the migration untouched.
        assert_eq!(parsed.data.default_candidates, 7);
        assert_eq!(parsed.data.default_candidate, "balanced");
        assert_eq!(parsed.data.default_export_format, ExportFormat::Png);
    }

    #[test]
    fn project_v1_omitted_defaults_fall_back_to_oracle_values() {
        // A minimal v1 project without the Defaults: fields migrates with the
        // Python model defaults (5 / recommended / html).
        let source: Value = serde_json::from_str(
            r#"{
                "kind": "seattrellis_project",
                "schema_version": 1,
                "name": "Minimal",
                "students": "students.csv",
                "layout": "layout.json",
                "rules": "rules.json"
            }"#,
        )
        .unwrap();
        let (target, report) = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap();
        assert!(report.lossless);
        let parsed: ArtifactEnvelope<SeatTrellisProjectArtifact> =
            serde_json::from_value(target).unwrap();
        assert_eq!(parsed.data.name, "Minimal");
        assert_eq!(parsed.data.history_dir, None);
        assert_eq!(parsed.data.outputs_dir, "outputs");
        assert_eq!(parsed.data.default_candidates, 5);
        assert_eq!(parsed.data.default_candidate, "recommended");
        assert_eq!(parsed.data.default_export_format, ExportFormat::Html);
    }

    #[test]
    fn project_v1_unknown_fields_block_the_migration() {
        let mut source: Value = serde_json::from_str(V1_PROJECT).unwrap();
        source["mystery_field"] = Value::Bool(true);
        let error = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap_err();
        assert!(error.contains("invalid v1 project"), "error: {error}");
    }

    #[test]
    fn project_v1_missing_required_fields_block_the_migration() {
        let mut source: Value = serde_json::from_str(V1_PROJECT).unwrap();
        source.as_object_mut().unwrap().remove("students");
        let error = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap_err();
        assert!(
            error.contains("invalid v1 project") && error.contains("students"),
            "error: {error}"
        );
    }

    #[test]
    fn project_v1_wrong_kind_or_schema_version_blocks_the_migration() {
        let mut wrong_kind: Value = serde_json::from_str(V1_PROJECT).unwrap();
        wrong_kind["kind"] = Value::String("seattrellis_bundle".to_string());
        let error = migrate_v1_to_v2(ArtifactKind::Project, &wrong_kind).unwrap_err();
        assert!(
            error.contains("kind must be \"seattrellis_project\""),
            "error: {error}"
        );

        let mut wrong_version: Value = serde_json::from_str(V1_PROJECT).unwrap();
        wrong_version["schema_version"] = Value::from(2);
        let error = migrate_v1_to_v2(ArtifactKind::Project, &wrong_version).unwrap_err();
        assert!(
            error.contains("unsupported schema_version 2"),
            "error: {error}"
        );
    }

    #[test]
    fn project_v1_candidate_count_range_is_validated() {
        for bad in [0, 21] {
            let mut source: Value = serde_json::from_str(V1_PROJECT).unwrap();
            source["default_candidates"] = Value::from(bad);
            let error = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap_err();
            assert!(
                error.contains("default_candidates must be between 1 and 20"),
                "candidates {bad}: error: {error}"
            );
        }
        let mut valid: Value = serde_json::from_str(V1_PROJECT).unwrap();
        valid["default_candidates"] = Value::from(1);
        assert!(migrate_v1_to_v2(ArtifactKind::Project, &valid).is_ok());
        valid["default_candidates"] = Value::from(20);
        assert!(migrate_v1_to_v2(ArtifactKind::Project, &valid).is_ok());
    }

    #[test]
    fn project_v1_unknown_export_format_blocks_the_migration() {
        let mut source: Value = serde_json::from_str(V1_PROJECT).unwrap();
        source["default_export_format"] = Value::String("pdf".to_string());
        let error = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap_err();
        assert!(error.contains("invalid v1 project"), "error: {error}");
    }

    #[test]
    fn project_migration_target_round_trips_through_strict_parse() {
        let source: Value = serde_json::from_str(V1_PROJECT).unwrap();
        let (target, _) = migrate_v1_to_v2(ArtifactKind::Project, &source).unwrap();
        let json = serde_json::to_string(&target).unwrap();
        let mut tampered: Value = serde_json::from_str(&json).unwrap();
        tampered["data"]["mystery"] = Value::Null;
        assert!(
            serde_json::from_value::<ArtifactEnvelope<SeatTrellisProjectArtifact>>(tampered)
                .is_err()
        );
    }
}
