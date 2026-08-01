//! Schema migration domain for durable SeatTrellis JSON artifacts.
//!
//! This is the self-contained Rust port of the Python migration pipeline
//! (`src/seattrellis/schema_migration.py` plus the migration handlers in
//! `src/seattrellis/api/handlers.py`). It exposes the JSON in/out helpers a
//! loopback HTTP server can wire up with no third-party filesystem or tempfile
//! dependency:
//!
//! * [`migration_preview_json`] — validate a project artifact, normalize it to
//!   the current schema, and return a field-level change list plus backup info
//!   without writing anything.
//! * [`migration_apply_json`] — write the normalized artifact (in-place with a
//!   `.bak` backup, or to a new `.migrated.json` sibling) using an atomic
//!   temp-then-rename write.
//! * [`migration_reference_checks_json`] — report per-field status
//!   (`ok`/`missing`/`wrong_type`) for every file or directory a project file
//!   references, with actionable guidance.
//! * [`migration_batch_preview_json`] / [`migration_batch_apply_json`] — work
//!   across several projects, detect shared references, and roll back earlier
//!   writes if a later project fails mid-batch.
//! * [`migration_restore_json`] — restore an in-place migration backup to its
//!   destination, keeping a reversible `.pre-restore.bak` safety copy.
//!
//! All JSON shapes match `clients/web/src/api/types.ts` (`snake_case`):
//! `ProjectMigrationResponse`, `ProjectMigrationBatchResponse`,
//! `ProjectMigrationReferenceCheck`, and `ProjectMigrationRestoreResponse`.
//! The artifact detection and schema normalization are intentionally
//! simplified versus the pydantic pipeline: known artifact types are
//! recognized, canonical fields are ensured, and unknown extension fields are
//! preserved so a no-op migration stays forward-safe.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Map, Value};

/// Wire `api_version` reported by every migration response.
const API_VERSION: &str = "1";
/// Maximum number of field changes included in a migration response.
const CHANGE_LIMIT: usize = 200;
/// Current schema versions for each recognized durable artifact.
const PROJECT_SCHEMA_VERSION: i64 = 1;
const SNAPSHOT_SCHEMA_VERSION: &str = "1.0";
const CANDIDATE_SCHEMA_VERSION: &str = "0.2.2";
const ROTATION_PLAN_SCHEMA_VERSION: &str = "1.0";
const RULESET_SCHEMA_VERSION: i64 = 1;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One privacy-safe description of a normalized JSON field change.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct MigrationChange {
    path: String,
    change: &'static str, // "added" | "removed" | "changed"
    before_type: Option<&'static str>,
    after_type: Option<&'static str>,
}

/// Status of one file or directory referenced by a project file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ReferenceCheck {
    field: String,
    path: String,
    expected: &'static str, // "file" | "directory"
    status: &'static str,   // "ok" | "missing" | "wrong_type"
}

/// A reference file shared by multiple selected projects.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct SharedReference {
    path: String,
    projects: Vec<String>,
    fields: Vec<String>,
}

/// Preview or result of one schema migration (matches `ProjectMigrationResponse`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct MigrationResponse {
    api_version: &'static str,
    project_path: String,
    source_path: String,
    artifact: String,
    schema_version: Value,
    output_path: Option<String>,
    backup_path: Option<String>,
    dry_run: bool,
    before_valid: bool,
    after_valid: Option<bool>,
    rollback_available: bool,
    change_count: usize,
    changes: Vec<MigrationChange>,
    reference_checks: Vec<ReferenceCheck>,
}

/// Combined migration preview with cross-project reference warnings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct BatchResponse {
    api_version: &'static str,
    projects: Vec<MigrationResponse>,
    shared_references: Vec<SharedReference>,
    ready: bool,
}

/// Standalone reference-check report with actionable guidance.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ReferenceChecksResponse {
    api_version: &'static str,
    project_path: String,
    ready: bool,
    checks: Vec<ReferenceCheck>,
    guidance: Vec<String>,
}

/// Result of a safe migration-backup restoration.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct RestoreResponse {
    api_version: &'static str,
    project_path: String,
    source_path: String,
    backup_path: String,
    safety_backup_path: Option<String>,
    artifact: String,
    schema_version: Value,
    restored_valid: bool,
}

// ---------------------------------------------------------------------------
// Artifact detection and normalization
// ---------------------------------------------------------------------------

/// The recognized durable artifact kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactKind {
    CandidateSet,
    PlanComparisonReport,
    Project,
    RotationPlan,
    Snapshot,
    Ruleset,
}

impl ArtifactKind {
    /// The human-readable artifact label used in responses.
    fn label(self) -> &'static str {
        match self {
            ArtifactKind::CandidateSet => "candidate set",
            ArtifactKind::PlanComparisonReport => "plan comparison report",
            ArtifactKind::Project => "project",
            ArtifactKind::RotationPlan => "rotation plan",
            ArtifactKind::Snapshot => "snapshot",
            ArtifactKind::Ruleset => "ruleset",
        }
    }
}

/// Identify the migratable artifact type from a parsed JSON object.
fn detect_artifact(data: &Value, source: &str) -> Result<ArtifactKind, String> {
    let object = data
        .as_object()
        .ok_or_else(|| format!("Invalid JSON in {source}: top-level value must be an object."))?;
    match object.get("kind").and_then(Value::as_str) {
        Some("candidate_set") => return Ok(ArtifactKind::CandidateSet),
        Some("plan_comparison_report") => return Ok(ArtifactKind::PlanComparisonReport),
        Some("seattrellis_project") => return Ok(ArtifactKind::Project),
        Some("rotation_plan") => return Ok(ArtifactKind::RotationPlan),
        _ => {}
    }
    if object.contains_key("students")
        && object.contains_key("layout")
        && object.contains_key("rules")
        && object.contains_key("assignments")
    {
        return Ok(ArtifactKind::Snapshot);
    }
    if object.contains_key("students") && object.contains_key("layout") && object.contains_key("rules")
    {
        return Ok(ArtifactKind::Project);
    }
    // A ruleset carries only schema metadata plus {seed, hard, soft, groups}.
    let is_ruleset = object
        .keys()
        .all(|key| matches!(key.as_str(), "schema_version" | "seed" | "hard" | "soft" | "groups"));
    let has_ruleset_data = ["seed", "hard", "soft", "groups"]
        .iter()
        .any(|key| object.contains_key(*key));
    if is_ruleset && has_ruleset_data {
        return Ok(ArtifactKind::Ruleset);
    }
    Err(format!(
        "Cannot identify a migratable SeatTrellis artifact: {source}. Expected ruleset, \
         snapshot, candidate set, plan comparison report, rotation plan, or project JSON."
    ))
}

/// The canonical `schema_version` value for an artifact kind.
fn artifact_schema_version(kind: ArtifactKind) -> Value {
    match kind {
        ArtifactKind::Project => json!(PROJECT_SCHEMA_VERSION),
        ArtifactKind::Snapshot => json!(SNAPSHOT_SCHEMA_VERSION),
        ArtifactKind::CandidateSet | ArtifactKind::PlanComparisonReport => {
            json!(CANDIDATE_SCHEMA_VERSION)
        }
        ArtifactKind::RotationPlan => json!(ROTATION_PLAN_SCHEMA_VERSION),
        ArtifactKind::Ruleset => json!(RULESET_SCHEMA_VERSION),
    }
}

/// Build the small overlay of canonical fields to add or fix for an artifact.
///
/// Overlaying (rather than replacing) the whole document keeps unknown
/// extension fields written by a newer producer, matching the Python
/// `merge_normalized_data` forward-safety contract.
fn normalized_overlay(data: &Value, kind: ArtifactKind) -> Value {
    let original = data.as_object().cloned().unwrap_or_default();
    let mut out = Map::new();
    match kind {
        ArtifactKind::Project => {
            out.insert("kind".into(), json!("seattrellis_project"));
            out.insert("schema_version".into(), json!(PROJECT_SCHEMA_VERSION));
            let name_ok = original
                .get("name")
                .and_then(Value::as_str)
                .map(|name| !name.trim().is_empty())
                .unwrap_or(false);
            if !name_ok {
                out.insert("name".into(), json!("SeatTrellis Project"));
            }
            if !original.contains_key("outputs_dir") {
                out.insert("outputs_dir".into(), json!("outputs"));
            }
            if !original.contains_key("default_candidates") {
                out.insert("default_candidates".into(), json!(5));
            }
            if !original.contains_key("default_candidate") {
                out.insert("default_candidate".into(), json!("recommended"));
            }
            if !original.contains_key("default_export_format") {
                out.insert("default_export_format".into(), json!("html"));
            }
        }
        ArtifactKind::Snapshot => {
            out.insert("schema_version".into(), json!(SNAPSHOT_SCHEMA_VERSION));
        }
        ArtifactKind::CandidateSet | ArtifactKind::PlanComparisonReport => {
            out.insert("schema_version".into(), json!(CANDIDATE_SCHEMA_VERSION));
        }
        ArtifactKind::RotationPlan => {
            out.insert("schema_version".into(), json!(ROTATION_PLAN_SCHEMA_VERSION));
        }
        ArtifactKind::Ruleset => {
            out.insert("schema_version".into(), json!(RULESET_SCHEMA_VERSION));
        }
    }
    Value::Object(out)
}

/// Overlay `normalized` values onto `original`, retaining unknown extension
/// fields and recursing into matching dict/list shapes.
fn merge_normalized(original: &Value, normalized: &Value) -> Value {
    match (original, normalized) {
        (Value::Object(original), Value::Object(normalized)) => {
            let mut merged = original.clone();
            for (key, value) in normalized {
                match merged.get(key) {
                    Some(existing) => {
                        let merged_value = merge_normalized(existing, value);
                        merged.insert(key.clone(), merged_value);
                    }
                    None => {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            }
            Value::Object(merged)
        }
        (Value::Array(original), Value::Array(normalized)) if original.len() == normalized.len() => {
            let merged: Vec<Value> = original
                .iter()
                .zip(normalized.iter())
                .map(|(left, right)| merge_normalized(left, right))
                .collect();
            Value::Array(merged)
        }
        (_, normalized) => normalized.clone(),
    }
}

// ---------------------------------------------------------------------------
// Change summary
// ---------------------------------------------------------------------------

/// The JSON type name used for `before_type` / `after_type`.
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Collect a bounded field-level diff between two JSON values.
#[derive(Default)]
struct ChangeCollector {
    count: usize,
    changes: Vec<MigrationChange>,
}

impl ChangeCollector {
    fn record(
        &mut self,
        path: &str,
        change: &'static str,
        before_type: Option<&'static str>,
        after_type: Option<&'static str>,
    ) {
        self.count += 1;
        if self.changes.len() < CHANGE_LIMIT {
            self.changes.push(MigrationChange {
                path: path.to_string(),
                change,
                before_type,
                after_type,
            });
        }
    }

    fn visit(&mut self, before: &Value, after: &Value, path: &str) {
        match (before, after) {
            (Value::Object(left), Value::Object(right)) => {
                let mut keys: Vec<&String> = left.keys().collect();
                keys.extend(right.keys());
                keys.sort();
                keys.dedup();
                for key in keys {
                    let child = if path.is_empty() {
                        (*key).clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    match (left.get(key), right.get(key)) {
                        (None, Some(after_value)) => {
                            self.record(&child, "added", None, Some(json_type(after_value)));
                        }
                        (Some(before_value), None) => {
                            self.record(&child, "removed", Some(json_type(before_value)), None);
                        }
                        (Some(before_value), Some(after_value)) => {
                            self.visit(before_value, after_value, &child);
                        }
                        (None, None) => {}
                    }
                }
            }
            (Value::Array(left), Value::Array(right)) => {
                let shared = left.len().min(right.len());
                for index in 0..shared {
                    let child = format!("{path}[{index}]");
                    self.visit(&left[index], &right[index], &child);
                }
                for (index, value) in left.iter().enumerate().skip(shared) {
                    let child = format!("{path}[{index}]");
                    self.record(&child, "removed", Some(json_type(value)), None);
                }
                for (index, value) in right.iter().enumerate().skip(shared) {
                    let child = format!("{path}[{index}]");
                    self.record(&child, "added", None, Some(json_type(value)));
                }
            }
            (left, right) if left != right => {
                let leaf_path = if path.is_empty() {
                    "$".to_string()
                } else {
                    path.to_string()
                };
                self.record(
                    &leaf_path,
                    "changed",
                    Some(json_type(left)),
                    Some(json_type(right)),
                );
            }
            _ => {}
        }
    }
}

/// Compare normalized JSON without returning any original values.
fn change_summary(before: &Value, after: &Value) -> (usize, Vec<MigrationChange>) {
    let mut collector = ChangeCollector::default();
    collector.visit(before, after, "");
    (collector.count, collector.changes)
}

// ---------------------------------------------------------------------------
// Project reference checks
// ---------------------------------------------------------------------------

/// The project fields that reference files or directories on disk.
const REFERENCE_FIELDS: [(&str, &str); 5] = [
    ("students", "file"),
    ("layout", "file"),
    ("rules", "file"),
    ("history_dir", "directory"),
    ("outputs_dir", "directory"),
];

/// The parent directory of `path`, falling back to the current directory.
fn parent_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Reject reference paths that escape the project workspace.
fn is_traversal(reference: &str) -> bool {
    Path::new(reference)
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
}

/// Check every file or directory a project file references.
fn compute_reference_checks(data: &Value, project_path: &Path) -> Vec<ReferenceCheck> {
    let root = parent_of(project_path);
    let object = data.as_object();
    let mut checks = Vec::with_capacity(REFERENCE_FIELDS.len());
    for (field, expected) in REFERENCE_FIELDS {
        let Some(value) = object.and_then(|object| object.get(field)) else {
            continue; // optional reference (e.g. `history_dir`) not configured
        };
        let reference = match value {
            Value::String(reference) => reference.as_str(),
            Value::Null => continue,
            other => {
                checks.push(ReferenceCheck {
                    field: field.to_string(),
                    path: other.to_string(),
                    expected,
                    status: "wrong_type",
                });
                continue;
            }
        };
        if reference.trim().is_empty() || is_traversal(reference) {
            checks.push(ReferenceCheck {
                field: field.to_string(),
                path: reference.to_string(),
                expected,
                status: "wrong_type",
            });
            continue;
        }
        let resolved = root.join(reference);
        let status = if !resolved.exists() {
            "missing"
        } else if (expected == "file" && !resolved.is_file())
            || (expected == "directory" && !resolved.is_dir())
        {
            "wrong_type"
        } else {
            "ok"
        };
        checks.push(ReferenceCheck {
            field: field.to_string(),
            path: reference.to_string(),
            expected,
            status,
        });
    }
    checks
}

/// Build an actionable guidance message for a non-`ok` reference check.
fn guidance_for_check(check: &ReferenceCheck) -> Option<String> {
    if check.status == "ok" {
        return None;
    }
    let expected = if check.expected == "file" {
        "a file"
    } else {
        "a directory"
    };
    if is_traversal(&check.path) || check.path.trim().is_empty() {
        Some(format!(
            "Field '{}' escapes the project workspace: reference '{}' must stay inside the project folder.",
            check.field, check.path
        ))
    } else if check.status == "missing" {
        Some(format!(
            "Field '{}' is missing: expected {} at '{}'.",
            check.field, expected, check.path
        ))
    } else {
        Some(format!(
            "Field '{}' has the wrong type: expected {} at '{}'.",
            check.field, expected, check.path
        ))
    }
}

/// Collect guidance for every non-`ok` reference check.
fn guidance_for_checks(checks: &[ReferenceCheck]) -> Vec<String> {
    checks
        .iter()
        .filter(|check| check.status != "ok")
        .filter_map(guidance_for_check)
        .collect()
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/// Read and parse a UTF-8 JSON object file, returning a clear error otherwise.
fn read_json_file(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Err(format!("Input file not found: {}", path.display()));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("Could not read input file {}: {error}", path.display()))?;
    let data: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))?;
    if !data.is_object() {
        return Err(format!(
            "Invalid JSON in {}: top-level value must be an object.",
            path.display()
        ));
    }
    Ok(data)
}

/// A unique sibling temporary path next to `output` for an atomic rename.
fn temp_sibling_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    parent_of(output).join(format!(".{name}.{}.{nanos}.tmp", process::id()))
}

/// Errors from a single atomic-write attempt.
enum TempWriteError {
    AlreadyExists,
    Other(String),
}

/// Write `data` to a fresh temp file and atomically rename it over `output`.
fn write_temp_then_rename(
    data: &Value,
    output: &Path,
    temp: &Path,
    existing_mode: Option<u32>,
) -> Result<(), TempWriteError> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                TempWriteError::AlreadyExists
            } else {
                TempWriteError::Other(format!(
                    "Could not create temporary file {}: {error}",
                    temp.display()
                ))
            }
        })?;
    let mut file = file;
    let mut bytes = serde_json::to_vec_pretty(data)
        .map_err(|error| TempWriteError::Other(format!("Could not serialize migrated JSON: {error}")))?;
    bytes.push(b'\n');
    file.write_all(&bytes).map_err(|error| {
        TempWriteError::Other(format!("Could not write migrated JSON file {}: {error}", output.display()))
    })?;
    file.sync_all().map_err(|error| {
        TempWriteError::Other(format!("Could not flush migrated JSON file {}: {error}", output.display()))
    })?;
    drop(file);
    if let Some(mode) = existing_mode {
        fs::set_permissions(temp, fs::Permissions::from_mode(mode)).map_err(|error| {
            TempWriteError::Other(format!(
                "Could not set permissions on migrated JSON file {}: {error}",
                output.display()
            ))
        })?;
    }
    fs::rename(temp, output).map_err(|error| {
        TempWriteError::Other(format!(
            "Could not atomically write migrated JSON file {}: {error}",
            output.display()
        ))
    })?;
    Ok(())
}

/// Write a complete sibling file before atomically replacing the destination.
fn write_json_atomic(data: &Value, output: &Path) -> Result<(), String> {
    fs::create_dir_all(parent_of(output))
        .map_err(|error| format!("Could not prepare migrated JSON file {}: {error}", output.display()))?;
    let existing_mode = fs::metadata(output).ok().map(|metadata| metadata.permissions().mode());

    let mut temp = temp_sibling_path(output);
    for _ in 0..8 {
        match write_temp_then_rename(data, output, &temp, existing_mode) {
            Ok(()) => return Ok(()),
            Err(TempWriteError::AlreadyExists) => {
                temp = temp_sibling_path(output);
            }
            Err(TempWriteError::Other(message)) => {
                let _ = fs::remove_file(&temp);
                return Err(message);
            }
        }
    }
    let _ = fs::remove_file(&temp);
    Err(format!(
        "Could not create a unique temporary file for {}",
        output.display()
    ))
}

/// Copy `path` to the next unused sibling `.bak` backup.
fn create_backup(path: &Path) -> Result<PathBuf, String> {
    create_named_backup(path, ".bak")
}

/// Copy `path` to the next unused sibling backup name using `suffix`.
fn create_named_backup(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut backup = parent_of(path).join(format!("{name}{suffix}"));
    let mut index = 1;
    while backup.exists() {
        backup = parent_of(path).join(format!("{name}{suffix}.{index}"));
        index += 1;
    }
    fs::copy(path, &backup).map_err(|error| {
        format!(
            "Could not back up {} to {}: {error}",
            path.display(),
            backup.display()
        )
    })?;
    Ok(backup)
}

/// Whether two paths refer to the same file (best effort).
fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// Restore a backup atomically, keeping a safety copy when asked to.
fn restore_backup(
    backup: &Path,
    destination: &Path,
    create_safety_backup: bool,
) -> Result<Option<PathBuf>, String> {
    if same_path(backup, destination) {
        return Err("A migration backup and its destination must differ.".to_string());
    }
    let backup_data = read_json_file(backup)?;
    if destination.exists() && !destination.is_file() {
        return Err(format!(
            "Migration destination is not a file: {}",
            destination.display()
        ));
    }
    let safety_backup = if create_safety_backup && destination.exists() {
        Some(create_named_backup(destination, ".pre-restore.bak")?)
    } else {
        None
    };
    write_json_atomic(&backup_data, destination)?;
    Ok(safety_backup)
}

// ---------------------------------------------------------------------------
// Single-artifact migration
// ---------------------------------------------------------------------------

/// The next unused sibling output path for a non-in-place migration.
fn resolve_output_path(source: &Path) -> Result<PathBuf, String> {
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    let mut candidate = parent_of(source).join(format!("{stem}.migrated.json"));
    let mut suffix = 2;
    while candidate.exists() {
        candidate = parent_of(source).join(format!("{stem}.migrated-{suffix}.json"));
        suffix += 1;
    }
    Ok(candidate)
}

/// Reparse a written artifact so the API can report post-write validity.
fn validate_written_artifact(path: &Path) -> Result<(), String> {
    let data = read_json_file(path)?;
    detect_artifact(&data, &path.display().to_string())?;
    Ok(())
}

/// Undo one migration whose post-write validation failed.
fn rollback_single_write(backup_path: &Option<PathBuf>, output_path: &Option<PathBuf>, in_place: bool) {
    let Some(output) = output_path else {
        return;
    };
    if let Some(backup) = backup_path {
        let _ = restore_backup(backup, output, false);
    } else if !in_place {
        let _ = fs::remove_file(output);
    }
}

/// Validate, normalize, and (optionally) write one project artifact.
fn migrate_internal(source: &Path, in_place: bool, dry_run: bool) -> Result<MigrationResponse, String> {
    if !source.is_file() {
        return Err(format!(
            "The selected project artifact does not exist: {}",
            source.display()
        ));
    }
    let original = read_json_file(source)?;
    let kind = detect_artifact(&original, &source.display().to_string())?;
    let overlay = normalized_overlay(&original, kind);
    let merged = merge_normalized(&original, &overlay);
    let (change_count, changes) = change_summary(&original, &merged);

    let output_path = if in_place {
        Some(source.to_path_buf())
    } else if dry_run {
        None
    } else {
        Some(resolve_output_path(source)?)
    };

    let backup_path = if dry_run || !in_place {
        None
    } else {
        Some(create_backup(source)?)
    };

    if !dry_run {
        let output = output_path
            .as_ref()
            .ok_or_else(|| "internal error: missing migration output path".to_string())?;
        write_json_atomic(&merged, output)?;
        if let Err(error) = validate_written_artifact(output) {
            rollback_single_write(&backup_path, &output_path, in_place);
            return Err(error);
        }
    }

    let reference_checks = if kind == ArtifactKind::Project {
        compute_reference_checks(&original, source)
    } else {
        Vec::new()
    };

    let schema_version = merged
        .get("schema_version")
        .cloned()
        .unwrap_or(Value::Null);

    Ok(MigrationResponse {
        api_version: API_VERSION,
        project_path: source.display().to_string(),
        source_path: source.display().to_string(),
        artifact: kind.label().to_string(),
        schema_version,
        output_path: output_path.as_ref().map(|path| path.display().to_string()),
        backup_path: backup_path.as_ref().map(|path| path.display().to_string()),
        dry_run,
        before_valid: true,
        after_valid: if dry_run { None } else { Some(true) },
        rollback_available: backup_path.is_some() || !in_place,
        change_count,
        changes,
        reference_checks,
    })
}

// ---------------------------------------------------------------------------
// Public JSON API
// ---------------------------------------------------------------------------

/// Preview a migration of the project artifact at `project_path`.
pub fn migration_preview_json(project_path: &str) -> Result<String, String> {
    let response = migrate_internal(Path::new(project_path), false, true)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize migration preview: {error}"))
}

/// Apply a migration to the project artifact at `project_path`.
pub fn migration_apply_json(project_path: &str, in_place: bool) -> Result<String, String> {
    let response = migrate_internal(Path::new(project_path), in_place, false)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize migration result: {error}"))
}

/// Report per-field reference status with guidance for a project artifact.
pub fn migration_reference_checks_json(project_path: &str) -> Result<String, String> {
    let source = Path::new(project_path);
    if !source.is_file() {
        return Err(format!(
            "The selected project artifact does not exist: {}",
            source.display()
        ));
    }
    let data = read_json_file(source)?;
    let kind = detect_artifact(&data, &source.display().to_string())?;
    let (checks, ready) = if kind == ArtifactKind::Project {
        let checks = compute_reference_checks(&data, source);
        let ready = checks.iter().all(|check| check.status == "ok");
        (checks, ready)
    } else {
        (Vec::new(), true)
    };
    let guidance = guidance_for_checks(&checks);
    let response = ReferenceChecksResponse {
        api_version: API_VERSION,
        project_path: source.display().to_string(),
        ready,
        checks,
        guidance,
    };
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize migration reference checks: {error}"))
}

/// Validate a batch request list of project paths.
fn validate_batch_paths(project_paths: &[String]) -> Result<(), String> {
    if project_paths.len() < 2 || project_paths.len() > 20 {
        return Err("project_paths must contain between 2 and 20 project files.".to_string());
    }
    let mut seen = HashSet::new();
    for project_path in project_paths {
        let trimmed = project_path.trim();
        if trimmed.is_empty() {
            return Err("project_paths must contain non-empty strings.".to_string());
        }
        if !seen.insert(trimmed) {
            return Err("project_paths must not contain duplicates.".to_string());
        }
    }
    Ok(())
}

/// Count the distinct projects owning a set of reference checks.
fn distinct_projects(owners: &[(String, String)]) -> usize {
    let mut projects: Vec<&str> = owners.iter().map(|(project, _)| project.as_str()).collect();
    projects.sort();
    projects.dedup();
    projects.len()
}

/// Preview several migrations and detect references shared across projects.
fn migration_batch_preview_inner(project_paths: &[String]) -> Result<BatchResponse, String> {
    validate_batch_paths(project_paths)?;
    let mut previews = Vec::with_capacity(project_paths.len());
    let mut references: HashMap<PathBuf, Vec<(String, String)>> = HashMap::new();
    let mut ready = true;
    for project_path in project_paths {
        let source = Path::new(project_path);
        let preview = migrate_internal(source, false, true)?;
        if preview.artifact == "project" {
            let root = parent_of(source);
            for check in &preview.reference_checks {
                if check.status != "ok" {
                    ready = false;
                    continue;
                }
                references
                    .entry(root.join(&check.path))
                    .or_default()
                    .push((project_path.clone(), check.field.clone()));
            }
        }
        previews.push(preview);
    }
    let mut shared_references: Vec<SharedReference> = references
        .into_iter()
        .filter(|(_path, owners)| distinct_projects(owners) >= 2)
        .map(|(path, owners)| {
            let mut projects: Vec<String> = owners
                .iter()
                .map(|(project, _field)| project.clone())
                .collect();
            let mut fields: Vec<String> = owners
                .iter()
                .map(|(_project, field)| field.clone())
                .collect();
            projects.sort();
            projects.dedup();
            fields.sort();
            fields.dedup();
            SharedReference {
                path: path.display().to_string(),
                projects,
                fields,
            }
        })
        .collect();
    shared_references.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(BatchResponse {
        api_version: API_VERSION,
        projects: previews,
        shared_references,
        ready,
    })
}

/// Best-effort rollback of artifacts written before a batch failure.
fn rollback_batch(applied: &[MigrationResponse], in_place: bool) {
    for result in applied.iter().rev() {
        if in_place {
            if let Some(backup) = &result.backup_path {
                let _ = restore_backup(Path::new(backup), Path::new(&result.source_path), false);
            }
        } else if let Some(output) = &result.output_path {
            let _ = fs::remove_file(Path::new(output));
        }
    }
}

/// Preview several project migrations together.
pub fn migration_batch_preview_json(project_paths: &[String]) -> Result<String, String> {
    let response = migration_batch_preview_inner(project_paths)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize migration batch preview: {error}"))
}

/// Apply several project migrations, rolling back on a mid-batch failure.
pub fn migration_batch_apply_json(project_paths: &[String], in_place: bool) -> Result<String, String> {
    validate_batch_paths(project_paths)?;
    let preview = migration_batch_preview_inner(project_paths)?;
    if !preview.ready {
        return Err("Review the migration reference checks before writing this batch.".to_string());
    }
    let mut applied = Vec::with_capacity(project_paths.len());
    for project_path in project_paths {
        match migrate_internal(Path::new(project_path), in_place, false) {
            Ok(response) => applied.push(response),
            Err(error) => {
                rollback_batch(&applied, in_place);
                return Err(format!(
                    "The migration batch was not completed; earlier changes were rolled back: {error}"
                ));
            }
        }
    }
    let response = BatchResponse {
        api_version: API_VERSION,
        projects: applied,
        shared_references: preview.shared_references,
        ready: true,
    };
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize migration batch result: {error}"))
}

/// Restore a migration backup to its original artifact destination.
pub fn migration_restore_json(backup_path: &str, destination: &str) -> Result<String, String> {
    let backup = Path::new(backup_path);
    let destination = Path::new(destination);
    if same_path(backup, destination) {
        return Err("A migration backup and its destination must differ.".to_string());
    }
    if !backup.is_file() {
        return Err(format!(
            "The migration backup does not exist: {}",
            backup.display()
        ));
    }
    let destination_name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let backup_name = backup
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let expected_prefix = format!("{destination_name}.bak");
    if !backup_name.starts_with(&expected_prefix) {
        return Err("The migration backup does not belong to the selected project artifact.".to_string());
    }
    let safety_backup = restore_backup(backup, destination, true)?;
    let restored = read_json_file(destination)?;
    let kind = detect_artifact(&restored, &destination.display().to_string())?;
    let schema_version = restored
        .get("schema_version")
        .cloned()
        .unwrap_or_else(|| artifact_schema_version(kind));
    let response = RestoreResponse {
        api_version: API_VERSION,
        project_path: destination.display().to_string(),
        source_path: destination.display().to_string(),
        backup_path: backup.display().to_string(),
        safety_backup_path: safety_backup.as_ref().map(|path| path.display().to_string()),
        artifact: kind.label().to_string(),
        schema_version,
        restored_valid: true,
    };
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize migration restore result: {error}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal project file missing the canonical fields a migration adds.
    const PROJECT_FIXTURE: &str = r#"{
        "name": "Mig Test",
        "students": "students.csv",
        "layout": "layout.json",
        "rules": "rules.json",
        "history_dir": "history",
        "outputs_dir": "outputs"
    }"#;

    /// An isolated temporary directory removed on drop.
    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "seattrellis_migration_test_{tag}_{}_{}",
                process::id(),
                nanos
            ));
            fs::create_dir_all(&path).unwrap();
            TestDir { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.path.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// Parse a JSON string for content comparison.
    fn parse_json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    /// Write a fully resolved project workspace (all references present).
    fn write_project_workspace(dir: &TestDir, name: &str) -> PathBuf {
        dir.write("students.csv", "id,name\n");
        dir.write("layout.json", r#"{"rows": []}"#);
        dir.write("rules.json", r#"{"seed": 1}"#);
        let _ = fs::create_dir_all(dir.path().join("history"));
        let _ = fs::create_dir_all(dir.path().join("outputs"));
        dir.write(name, PROJECT_FIXTURE)
    }

    #[test]
    fn preview_reports_normalization_changes() {
        let dir = TestDir::new("preview");
        let project = write_project_workspace(&dir, "project.json");
        let json = migration_preview_json(&project.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["api_version"], "1");
        assert_eq!(value["artifact"], "project");
        assert_eq!(value["schema_version"], 1);
        assert!(value["dry_run"].as_bool().unwrap());
        assert!(value["output_path"].is_null());
        assert!(value["backup_path"].is_null());
        assert!(value["before_valid"].as_bool().unwrap());
        assert!(value["after_valid"].is_null());
        assert!(value["rollback_available"].as_bool().unwrap());
        let change_count = value["change_count"].as_u64().unwrap();
        assert!(change_count >= 5, "unexpected change count: {change_count}");
        let changes = value["changes"].as_array().unwrap();
        let paths: Vec<&str> = changes
            .iter()
            .map(|change| change["path"].as_str().unwrap())
            .collect();
        for expected in ["kind", "schema_version", "default_candidates", "default_candidate", "default_export_format"] {
            assert!(paths.contains(&expected), "missing change for {expected}: {paths:?}");
        }
        assert!(changes.iter().all(|change| change["change"] == "added"));
        let checks = value["reference_checks"].as_array().unwrap();
        assert_eq!(checks.len(), 5);
    }

    #[test]
    fn preview_rejects_missing_and_invalid_files() {
        let dir = TestDir::new("preview_bad");
        let missing = dir.path().join("nope.json");
        let error = migration_preview_json(&missing.display().to_string()).unwrap_err();
        assert!(error.contains("does not exist"), "unexpected: {error}");

        let invalid = dir.write("bad.json", "{not json");
        let error = migration_preview_json(&invalid.display().to_string()).unwrap_err();
        assert!(error.contains("Invalid JSON"), "unexpected: {error}");

        let scalar = dir.write("scalar.json", r#"[1, 2, 3]"#);
        let error = migration_preview_json(&scalar.display().to_string()).unwrap_err();
        assert!(error.contains("top-level value must be an object"), "unexpected: {error}");
    }

    #[test]
    fn preview_rejects_unidentifiable_artifact() {
        let dir = TestDir::new("preview_unknown");
        let unknown = dir.write("weird.json", r#"{"foo": 1, "bar": 2}"#);
        let error = migration_preview_json(&unknown.display().to_string()).unwrap_err();
        assert!(error.contains("Cannot identify"), "unexpected: {error}");
    }

    #[test]
    fn apply_non_inplace_writes_sibling_output_and_keeps_source() {
        let dir = TestDir::new("apply_out");
        let project = write_project_workspace(&dir, "project.json");
        let original_source = fs::read_to_string(&project).unwrap();
        let json = migration_apply_json(&project.display().to_string(), false).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert!(!value["dry_run"].as_bool().unwrap());
        let output = value["output_path"].as_str().unwrap().to_string();
        assert!(output.ends_with("project.migrated.json"), "unexpected output: {output}");
        assert!(Path::new(&output).is_file());
        assert!(value["backup_path"].is_null());
        assert_eq!(value["after_valid"].as_bool(), Some(true));
        assert!(value["rollback_available"].as_bool().unwrap());
        // The source artifact is left untouched.
        assert_eq!(fs::read_to_string(&project).unwrap(), original_source);
        // The output carries the normalized fields.
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_eq!(written["kind"], "seattrellis_project");
        assert_eq!(written["schema_version"], 1);
        assert_eq!(written["default_candidates"], 5);
    }

    #[test]
    fn apply_inplace_creates_backup_and_rewrites_source() {
        let dir = TestDir::new("apply_inplace");
        let project = write_project_workspace(&dir, "project.json");
        let original_source = fs::read_to_string(&project).unwrap();
        let json = migration_apply_json(&project.display().to_string(), true).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["output_path"].as_str().unwrap(), value["source_path"].as_str().unwrap());
        let backup = value["backup_path"].as_str().unwrap().to_string();
        assert!(backup.ends_with("project.json.bak"), "unexpected backup: {backup}");
        assert!(Path::new(&backup).is_file());
        assert_eq!(fs::read_to_string(&backup).unwrap(), original_source);
        assert_eq!(value["after_valid"].as_bool(), Some(true));
        assert!(value["rollback_available"].as_bool().unwrap());
        // The source now holds the normalized artifact.
        let rewritten: Value =
            serde_json::from_str(&fs::read_to_string(&project).unwrap()).unwrap();
        assert_eq!(rewritten["kind"], "seattrellis_project");
        assert_eq!(rewritten["schema_version"], 1);
        assert_ne!(fs::read_to_string(&project).unwrap(), original_source);
    }

    #[test]
    fn apply_deduplicates_output_name_when_taken() {
        let dir = TestDir::new("apply_dedupe");
        let project = write_project_workspace(&dir, "project.json");
        dir.write("project.migrated.json", r#"{"kind": "occupied"}"#);
        let json = migration_apply_json(&project.display().to_string(), false).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        let output = value["output_path"].as_str().unwrap();
        assert!(output.ends_with("project.migrated-2.json"), "unexpected: {output}");
        assert!(Path::new(output).is_file());
    }

    #[test]
    fn apply_preserves_unknown_extension_fields() {
        let dir = TestDir::new("forward_safe");
        let project = dir.write(
            "project.json",
            r#"{
                "name": "Mig",
                "students": "students.csv",
                "layout": "layout.json",
                "rules": "rules.json",
                "teacher_email": "t@example.com",
                "custom_flag": true
            }"#,
        );
        dir.write("students.csv", "id\n");
        dir.write("layout.json", "{}");
        dir.write("rules.json", "{}");
        let json = migration_apply_json(&project.display().to_string(), false).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        let output = value["output_path"].as_str().unwrap();
        let written: Value = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(written["teacher_email"], "t@example.com");
        assert_eq!(written["custom_flag"], true);
        assert_eq!(written["kind"], "seattrellis_project");
        assert_eq!(written["schema_version"], 1);
    }

    #[test]
    fn reference_checks_classify_existing_missing_and_wrong_type() {
        let dir = TestDir::new("ref_checks");
        dir.write("students.csv", "id,name\n");
        let _ = fs::create_dir_all(dir.path().join("rules.json")); // a directory where a file is expected
        let project = dir.write(
            "project.json",
            r#"{
                "name": "Mig",
                "students": "students.csv",
                "layout": "layout.json",
                "rules": "rules.json",
                "outputs_dir": "outputs"
            }"#,
        );
        let json = migration_reference_checks_json(&project.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert!(!value["ready"].as_bool().unwrap());
        let checks = value["checks"].as_array().unwrap();
        let by_field: HashMap<&str, &Value> = checks
            .iter()
            .map(|check| (check["field"].as_str().unwrap(), check))
            .collect();
        assert_eq!(by_field["students"]["status"], "ok");
        assert_eq!(by_field["students"]["expected"], "file");
        assert_eq!(by_field["layout"]["status"], "missing");
        assert_eq!(by_field["rules"]["status"], "wrong_type");
        assert_eq!(by_field["outputs_dir"]["status"], "missing");
        assert_eq!(by_field["outputs_dir"]["expected"], "directory");
        let guidance = value["guidance"].as_array().unwrap();
        assert_eq!(guidance.len(), 3, "unexpected guidance: {guidance:?}");
    }

    #[test]
    fn reference_checks_reject_path_traversal() {
        let dir = TestDir::new("ref_traversal");
        let project = dir.write(
            "project.json",
            r#"{
                "name": "Mig",
                "students": "../outside.csv",
                "layout": "layout.json",
                "rules": "rules.json"
            }"#,
        );
        let json = migration_reference_checks_json(&project.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert!(!value["ready"].as_bool().unwrap());
        let checks = value["checks"].as_array().unwrap();
        let students = checks
            .iter()
            .find(|check| check["field"] == "students")
            .unwrap();
        assert_ne!(students["status"], "ok");
        let guidance = value["guidance"].as_array().unwrap();
        let joined = guidance
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("students") && joined.contains("workspace"),
            "unexpected guidance: {joined}"
        );
    }

    #[test]
    fn batch_preview_reports_shared_references() {
        let dir = TestDir::new("batch_shared");
        let project_a = write_project_workspace(&dir, "a.json");
        let project_b = write_project_workspace(&dir, "b.json");
        let paths = vec![
            project_a.display().to_string(),
            project_b.display().to_string(),
        ];
        let json = migration_batch_preview_json(&paths).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert!(value["ready"].as_bool().unwrap());
        let shared_refs = value["shared_references"].as_array().unwrap();
        assert!(!shared_refs.is_empty(), "expected shared references");
        let students_ref = shared_refs
            .iter()
            .find(|reference| {
                reference["fields"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::String("students".to_string()))
            })
            .unwrap();
        assert_eq!(students_ref["projects"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn batch_preview_reports_not_ready_on_missing_reference() {
        let dir = TestDir::new("batch_notready");
        dir.write("students.csv", "id\n");
        dir.write("layout.json", "{}");
        dir.write("rules.json", "{}");
        let project_a = dir.write(
            "a.json",
            r#"{"name":"A","students":"students.csv","layout":"layout.json","rules":"rules.json"}"#,
        );
        let project_b = dir.write(
            "b.json",
            r#"{"name":"B","students":"missing.csv","layout":"layout.json","rules":"rules.json"}"#,
        );
        let paths = vec![
            project_a.display().to_string(),
            project_b.display().to_string(),
        ];
        let json = migration_batch_preview_json(&paths).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert!(!value["ready"].as_bool().unwrap());
        // Batch apply refuses to write while any reference check is not ok.
        let error = migration_batch_apply_json(&paths, false).unwrap_err();
        assert!(error.contains("reference checks"), "unexpected: {error}");
    }

    #[test]
    fn batch_apply_rolls_back_earlier_writes_on_failure() {
        let root = TestDir::new("batch_rollback");
        let dir_a = root.path().join("a");
        let dir_b = root.path().join("b");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();
        for dir in [&dir_a, &dir_b] {
            fs::write(dir.join("students.csv"), "id\n").unwrap();
            fs::write(dir.join("layout.json"), "{}").unwrap();
            fs::write(dir.join("rules.json"), "{}").unwrap();
            fs::create_dir_all(dir.join("history")).unwrap();
            fs::create_dir_all(dir.join("outputs")).unwrap();
            fs::write(dir.join("project.json"), PROJECT_FIXTURE).unwrap();
        }
        let original_a = fs::read_to_string(dir_a.join("project.json")).unwrap();

        // Make the second project's directory unwritable so its in-place
        // backup fails mid-batch, after the first project was already written.
        fs::set_permissions(&dir_b, fs::Permissions::from_mode(0o500)).unwrap();
        let paths = vec![
            dir_a.join("project.json").display().to_string(),
            dir_b.join("project.json").display().to_string(),
        ];
        let result = migration_batch_apply_json(&paths, true);
        fs::set_permissions(&dir_b, fs::Permissions::from_mode(0o755)).unwrap();

        let error = result.unwrap_err();
        assert!(error.contains("rolled back"), "unexpected: {error}");
        // The first project was rolled back to its pre-migration content.
        assert_eq!(
            parse_json(&fs::read_to_string(dir_a.join("project.json")).unwrap()),
            parse_json(&original_a)
        );
        // The backup created for the first project is retained for recovery.
        assert!(dir_a.join("project.json.bak").is_file());
        // The second project was never modified.
        assert_eq!(fs::read_to_string(dir_b.join("project.json")).unwrap(), PROJECT_FIXTURE);
    }

    #[test]
    fn restore_backup_restores_original_and_creates_safety_copy() {
        let dir = TestDir::new("restore");
        let project = write_project_workspace(&dir, "project.json");
        let original = fs::read_to_string(&project).unwrap();
        let apply = migration_apply_json(&project.display().to_string(), true).unwrap();
        let apply_value: Value = serde_json::from_str(&apply).unwrap();
        let backup = apply_value["backup_path"].as_str().unwrap().to_string();
        assert_ne!(fs::read_to_string(&project).unwrap(), original);

        let json = migration_restore_json(&backup, &project.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert!(value["restored_valid"].as_bool().unwrap());
        assert_eq!(value["backup_path"].as_str().unwrap(), backup);
        assert_eq!(value["artifact"], "project");
        assert_eq!(value["schema_version"], 1);
        let safety = value["safety_backup_path"].as_str().unwrap();
        assert!(safety.ends_with("project.json.pre-restore.bak"), "unexpected safety: {safety}");
        assert!(Path::new(safety).is_file());
        assert_eq!(
            parse_json(&fs::read_to_string(&project).unwrap()),
            parse_json(&original)
        );
    }

    #[test]
    fn restore_rejects_same_path_and_unrelated_backup() {
        let dir = TestDir::new("restore_bad");
        let project = write_project_workspace(&dir, "project.json");

        let error = migration_restore_json(
            &project.display().to_string(),
            &project.display().to_string(),
        )
        .unwrap_err();
        assert!(error.contains("must differ"), "unexpected: {error}");

        let unrelated = dir.write("other.json.bak", r#"{"kind": "seattrellis_project"}"#);
        let error = migration_restore_json(
            &unrelated.display().to_string(),
            &project.display().to_string(),
        )
        .unwrap_err();
        assert!(error.contains("does not belong"), "unexpected: {error}");

        let missing = dir.path().join("missing.json.bak");
        let error = migration_restore_json(
            &missing.display().to_string(),
            &project.display().to_string(),
        )
        .unwrap_err();
        assert!(error.contains("does not exist"), "unexpected: {error}");
    }

    #[test]
    fn snapshot_ruleset_and_rotation_artifacts_preview() {
        let dir = TestDir::new("other_artifacts");

        let snapshot = dir.write(
            "snapshot.json",
            r#"{
                "schema_version": "1.0",
                "students": [],
                "layout": {"rows": []},
                "rules": {"seed": 1},
                "assignments": {}
            }"#,
        );
        let json = migration_preview_json(&snapshot.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["artifact"], "snapshot");
        assert_eq!(value["schema_version"], "1.0");
        assert!(value["changes"].as_array().unwrap().is_empty());
        assert!(value["reference_checks"].as_array().unwrap().is_empty());

        let ruleset = dir.write("ruleset.json", r#"{"seed": 42, "hard": [], "soft": [], "groups": {}}"#);
        let json = migration_preview_json(&ruleset.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["artifact"], "ruleset");
        assert_eq!(value["schema_version"], 1);

        let rotation = dir.write(
            "rotation.json",
            r#"{"kind": "rotation_plan", "schema_version": "1.0", "periods": []}"#,
        );
        let json = migration_preview_json(&rotation.display().to_string()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["artifact"], "rotation plan");
        assert_eq!(value["schema_version"], "1.0");
    }

    #[test]
    fn batch_rejects_duplicate_and_too_short_path_lists() {
        let dir = TestDir::new("batch_validate");
        let project_a = write_project_workspace(&dir, "a.json");
        let project_b = write_project_workspace(&dir, "b.json");
        let a = project_a.display().to_string();
        let b = project_b.display().to_string();

        let error = migration_batch_preview_json(&[a.clone()]).unwrap_err();
        assert!(error.contains("between 2 and 20"), "unexpected: {error}");

        let error = migration_batch_preview_json(&[a.clone(), a.clone()]).unwrap_err();
        assert!(error.contains("duplicates"), "unexpected: {error}");

        let error = migration_batch_preview_json(&[a, String::new()]).unwrap_err();
        assert!(error.contains("non-empty"), "unexpected: {error}");
        let _ = b;
    }
}
