//! Shared, read-only validation used by migration previews and artifact readers.
use std::collections::HashSet;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{check_version, dto, ArtifactEnvelope, ArtifactKind};

fn payload<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value::<ArtifactEnvelope<T>>(value.clone())
        .map(|envelope| envelope.data)
        .map_err(|error| format!("invalid artifact: {error}"))
}

/// Validate the envelope, its concrete payload, and cross-field invariants.
/// This function never writes a file or guesses a missing/future version.
pub fn validate_artifact_document(value: &Value) -> Result<ArtifactKind, String> {
    let kind: ArtifactKind =
        serde_json::from_value(value.get("kind").cloned().ok_or("missing artifact kind")?)
            .map_err(|error| format!("invalid artifact kind: {error}"))?;
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or("artifact schema_version must be an unsigned integer")?;
    check_version(kind, version)?;
    match kind {
        ArtifactKind::StudentRoster => {
            roster(&payload::<dto::student_roster::StudentRoster>(value)?.students)?
        }
        ArtifactKind::ClassroomLayout => layout(&payload(value)?)?,
        ArtifactKind::RuleSet => rules(&payload(value)?)?,
        ArtifactKind::SeatingSnapshot => snapshot(&payload(value)?)?,
        ArtifactKind::CandidateSet => {
            let set = payload::<dto::candidate_set::CandidateSetArtifact>(value)?;
            if set.kind != "candidate_set" {
                return Err("invalid candidate set discriminator".into());
            }
            let mut ids = HashSet::new();
            for candidate in &set.candidates {
                if candidate.candidate_id.trim().is_empty()
                    || !ids.insert(candidate.candidate_id.as_str())
                {
                    return Err("candidate ids must be non-empty and unique".into());
                }
                snapshot(&candidate.snapshot)?;
            }
            if !ids.contains(set.recommended_candidate_id.as_str()) {
                return Err("recommended candidate does not exist".into());
            }
        }
        ArtifactKind::PlanComparison => {
            payload::<dto::plan_comparison::PlanComparisonReportArtifact>(value)?
                .validate_references()?
        }
        ArtifactKind::HistoryArchive => {
            let archive = payload::<dto::history_archive::HistoryArchiveArtifact>(value)?;
            archive.validate()?;
            for entry in archive.snapshots {
                snapshot(&entry.snapshot)?;
            }
        }
        ArtifactKind::RotationPlan => {
            let plan = payload::<dto::rotation_plan::RotationPlanArtifact>(value)?;
            plan.validate()?;
            for period in plan.periods {
                match period.snapshot {
                    dto::rotation_plan::RotationSnapshot::Full(full) => snapshot(&full)?,
                    dto::rotation_plan::RotationSnapshot::Compact(compact) => {
                        assignments(&compact.assignments)?
                    }
                }
            }
        }
        ArtifactKind::EditingOperationLog => {
            payload::<dto::editing_operation_log::EditingOperationLogArtifact>(value)?.validate()?
        }
        ArtifactKind::ExportPreset => {
            payload::<dto::export_preset::ExportPresetArtifact>(value)?.validate()?
        }
        ArtifactKind::Project => {
            let project = payload::<dto::project::SeatTrellisProjectArtifact>(value)?;
            if project.kind != "seattrellis_project" || project.schema_version != 1 {
                return Err("unsupported project payload kind/version".into());
            }
            for path in [
                &project.students,
                &project.layout,
                &project.rules,
                &project.outputs_dir,
            ] {
                safe_path(path)?;
            }
            if let Some(path) = project.history_dir {
                safe_path(&path)?;
            }
            if !(1..=20).contains(&project.default_candidates) {
                return Err("default_candidates must be in 1..=20".into());
            }
        }
        ArtifactKind::ProjectBundleManifest => {
            let manifest = payload::<dto::bundle_manifest::ProjectBundleManifest>(value)?;
            safe_path(&manifest.project_file)?;
            let mut paths = HashSet::new();
            for file in manifest.files {
                safe_path(&file.path)?;
                if !paths.insert(file.path) {
                    return Err("duplicate manifest entry".into());
                }
                if file.sha256.len() != 64
                    || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err("invalid manifest SHA-256".into());
                }
            }
            if !paths.contains(&manifest.project_file) {
                return Err("manifest must include its project file".into());
            }
        }
    }
    Ok(kind)
}

fn safe_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains(['\0', ':'])
        || path.starts_with(['/', '\\'])
        || path
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("unsafe project path: {path}"));
    }
    Ok(())
}

fn roster(students: &[dto::student_roster::RosterStudent]) -> Result<(), String> {
    let mut keys = HashSet::new();
    for student in students {
        let id = student
            .student_id
            .as_deref()
            .filter(|id| !id.trim().is_empty());
        let name = student
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty());
        if id.is_none() && name.is_none() {
            return Err("student requires an id or name".into());
        }
        let key = id
            .or(name)
            .expect("a student has an id or name after validation");
        if !keys.insert(key) {
            return Err(format!("duplicate effective student key: {key}"));
        }
        if student
            .height_cm
            .is_some_and(|n| !n.is_finite() || !(0.0..=300.0).contains(&n))
        {
            return Err("height_cm must be finite and in 0..=300".into());
        }
        if student
            .score
            .is_some_and(|n| !n.is_finite() || n.abs() > 1e9)
        {
            return Err("score must be finite and within +/-1e9".into());
        }
    }
    Ok(())
}

fn layout(layout: &dto::classroom_layout::ClassroomLayout) -> Result<(), String> {
    let mut ids = HashSet::new();
    for seat in &layout.seats {
        if seat.seat_id.trim().is_empty() || !ids.insert(seat.seat_id.as_str()) {
            return Err("seat ids must be non-empty and unique".into());
        }
        if seat.x.into_iter().chain(seat.y).any(|n| !n.is_finite()) {
            return Err("seat coordinates must be finite".into());
        }
    }
    if layout.adjacency.max_row_delta < 0
        || layout.adjacency.max_col_delta < 0
        || layout
            .adjacency
            .max_distance
            .is_some_and(|n| !n.is_finite() || n < 0.0)
    {
        return Err("invalid adjacency distance".into());
    }
    for (a, b) in &layout.adjacency.custom_edges {
        if a == b || !ids.contains(a.as_str()) || !ids.contains(b.as_str()) {
            return Err("custom edge references an unknown seat or itself".into());
        }
    }
    Ok(())
}

fn rules(rules: &dto::rule_set::RuleSetArtifact) -> Result<(), String> {
    for rule in &rules.hard.min_distance {
        if !rule.distance.is_finite() || rule.distance <= 0.0 {
            return Err("minimum distance must be finite and positive".into());
        }
    }
    for group in &rules.groups {
        if group.separate && group.together {
            return Err("group cannot be both separate and together".into());
        }
    }
    let soft = serde_json::to_value(&rules.soft).map_err(|e| e.to_string())?;
    for rule in soft
        .as_object()
        .expect("soft rules serialize as object")
        .values()
    {
        let weight = rule["weight"].as_i64().ok_or("invalid soft weight")?;
        if !(0..=1_000_000).contains(&weight) {
            return Err("soft weight must be in 0..=1000000".into());
        }
    }
    Ok(())
}

fn assignments(rows: &[dto::snapshot::SeatAssignment]) -> Result<(), String> {
    let mut students = HashSet::new();
    let mut seats = HashSet::new();
    for row in rows {
        if row.student_key.trim().is_empty()
            || row.seat_id.trim().is_empty()
            || !students.insert(&row.student_key)
            || !seats.insert(&row.seat_id)
        {
            return Err("assignments must have unique, non-empty students and seats".into());
        }
    }
    Ok(())
}

fn snapshot(snapshot: &dto::snapshot::SeatingSnapshotArtifact) -> Result<(), String> {
    roster(&snapshot.students)?;
    layout(&snapshot.layout)?;
    rules(&snapshot.rules)?;
    assignments(&snapshot.assignments)?;
    let students: HashSet<_> = snapshot
        .students
        .iter()
        .filter_map(|s| {
            s.student_id
                .as_deref()
                .filter(|id| !id.trim().is_empty())
                .or(s.name.as_deref())
        })
        .collect();
    let seats: HashSet<_> = snapshot
        .layout
        .seats
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.seat_id.as_str())
        .collect();
    for row in &snapshot.assignments {
        if !students.contains(row.student_key.as_str()) || !seats.contains(row.seat_id.as_str()) {
            return Err(
                "snapshot assignment references unknown student or disabled/unknown seat".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn validates_payload_instead_of_trusting_version() {
        let valid = json!({"kind":"student_roster","schema_version":2,"data":{"students":[{"student_id":"A"}]}});
        assert_eq!(
            validate_artifact_document(&valid).unwrap(),
            ArtifactKind::StudentRoster
        );
        for bad in [
            json!({"kind":"student_roster","schema_version":2,"data":{"students":"bad"}}),
            json!({"kind":"student_roster","schema_version":3,"data":{"students":[]}}),
            json!({"kind":"student_roster","schema_version":2,"data":{"students":[],"extra":true}}),
            json!({"kind":"student_roster","schema_version":2,"data":{"students":[{"student_id":"A"},{"student_id":"A"}]}}),
        ] {
            assert!(validate_artifact_document(&bad).is_err(), "accepted {bad}");
        }
    }
    #[test]
    fn rejects_colliding_effective_roster_keys_and_zero_distance() {
        for students in [
            json!([{"name":"Alice"},{"name":"Alice"}]),
            json!([{"student_id":"Alice","name":"Bob"},{"name":"Alice"}]),
        ] {
            let doc =
                json!({"kind":"student_roster","schema_version":2,"data":{"students":students}});
            assert!(validate_artifact_document(&doc).is_err());
        }
        let doc = json!({"kind":"rule_set","schema_version":2,"data":{"hard":{"min_distance":[{"students":["A","B"],"distance":0}]}}});
        assert!(validate_artifact_document(&doc).is_err());
        let valid = json!({"kind":"student_roster","schema_version":2,"data":{"students":[{"student_id":"A","name":"Same"},{"student_id":"B","name":"Same"}]}});
        assert!(validate_artifact_document(&valid).is_ok());
    }

    #[test]
    fn rejects_cross_platform_path_escapes() {
        for path in ["../x", "a\\..\\b", "C:\\x", "/x", "a//b"] {
            assert!(safe_path(path).is_err());
        }
        assert!(safe_path("班级/students.json").is_ok());
    }
}
