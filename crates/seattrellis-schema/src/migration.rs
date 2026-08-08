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

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

/// v1 StudentRoster → v2 `ArtifactEnvelope<StudentRoster>`. Field-preserving
/// (lossless); the v2 envelope adds kind/version/extensions.
pub fn migrate_student_roster_v1_to_v2(
    source: &Value,
) -> Result<(Value, MigrationReport), String> {
    let v1: V1StudentRoster =
        serde_json::from_value(source.clone()).map_err(|error| format!("invalid v1 student roster: {error}"))?;
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

/// The v1→v2 dispatch for the kinds this crate currently migrates. Unknown
/// kinds are an error: migration coverage is explicit, never inferred.
pub fn migrate_v1_to_v2(kind: ArtifactKind, source: &Value) -> Result<(Value, MigrationReport), String> {
    match kind {
        ArtifactKind::StudentRoster => migrate_student_roster_v1_to_v2(source),
        ArtifactKind::ClassroomLayout => migrate_classroom_layout_v1_to_v2(source),
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
        assert!(error.contains("invalid v1 student roster"), "error: {error}");
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
}
