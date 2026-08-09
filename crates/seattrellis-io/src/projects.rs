//! Project workspace domain: recent-project listing, history browsing, privacy
//! scanning, bundle pack/restore, and a small in-memory recent-project record.
//!
//! This is the self-contained Rust port of the Python project handlers in
//! `src/seattrellis/api/handlers.py` and the backup helpers in
//! `src/seattrellis/project_bundle.py`. It exposes loopback-friendly JSON
//! helpers ([`list_projects_json`], [`project_history_json`],
//! [`project_privacy_json`], [`pack_project_json`], [`restore_project_json`])
//! plus typed building blocks the server can reuse.
//!
//! JSON shapes match `clients/web/src/api/types.ts` (`snake_case`):
//!
//! * `ProjectListResponse`   -> `{ api_version, root, projects: RecentProject[] }`
//!   where `RecentProject = { name, path, modified_at }`.
//! * `ProjectHistoryResponse`-> `{ api_version, project_name, project_path,
//!   history: ProjectArtifact[], outputs: ProjectArtifact[], warnings: string[] }`.
//!   `ProjectArtifact` carries only metadata/counts — student records are never
//!   returned, mirroring the Python "keep student data server-side" policy.
//! * `ProjectPrivacyResponse`-> `{ api_version, project_path, files_scanned,
//!   safe_for_public_sharing, findings: PrivacyFinding[] }` where
//!   `PrivacyFinding = { file, fields: string[] }`.
//! * `ProjectRestoreResponse`-> `{ api_version, project_path, output_dir }`.
//!
//! Security notes:
//!
//! * Every project reference (students/layout/rules/history/outputs) is
//!   resolved and verified to stay inside the project root before it is read,
//!   packed, or restored — traversal via `..`, absolute paths, or symlinks is
//!   rejected with a clear error.
//! * Bundle restore validates the manifest, rejects unsafe entry names
//!   (absolute, `..`, backslash, NUL), rejects symlink entries, caps per-file
//!   and total uncompressed sizes, and stages into a temp directory before
//!   moving files into the destination.
//!
//! This module never panics on malformed input; all failures surface as
//! `Err(String)`.

use std::collections::HashSet;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// On-disk format version written into every bundle manifest.
pub const BUNDLE_FORMAT_VERSION: u32 = 1;

/// Maximum size of a single file inside a bundle or privacy scan, in bytes.
pub const MAX_BUNDLE_FILE_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum total uncompressed size of a bundle, in bytes.
pub const MAX_BUNDLE_TOTAL_BYTES: u64 = 500 * 1024 * 1024;

/// A manifest lists file names; cap its uncompressed size before reading so a
/// high-compression manifest cannot expand unboundedly in memory.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Maximum project file size read by [`load_project`].
const MAX_PROJECT_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Upper bound for the in-memory recent-project record.
const MAX_RECENT_PROJECTS: usize = 20;

/// Default project display name (matches `SeatTrellisProject.name`).
const DEFAULT_PROJECT_NAME: &str = "SeatTrellis Project";

/// Keys treated as potentially identifying or educationally sensitive by the
/// privacy scan (mirrors `project_bundle._SENSITIVE_KEYS`).
const SENSITIVE_KEYS: &[&str] = &[
    "student_id",
    "student_key",
    "student_name",
    "score",
    "grade",
    "notes",
    "note",
    "special_needs",
    "special_need",
    "height",
    "vision",
    "email",
    "phone",
    "name",
];

// ---------------------------------------------------------------------------
// Wire types (JSON shapes match clients/web/src/api/types.ts)
// ---------------------------------------------------------------------------

/// One recently-seen project (`RecentProject` in `types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentProject {
    pub name: String,
    pub path: String,
    pub modified_at: String,
}

/// Provenance summary for a history artifact (`ProjectArtifactProvenance`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectArtifactProvenance {
    /// One of `generated`, `manual_edit`, `rotation_edit`, `restored`, `unknown`.
    pub source: String,
    pub parent_name: Option<String>,
    pub operation_count: Option<u64>,
}

/// One history/output artifact (`ProjectArtifact` in `types.ts`). Student
/// records are intentionally never serialized.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectArtifact {
    pub name: String,
    pub path: String,
    /// One of `snapshot`, `candidate_set`, `rotation_plan`, `unknown`.
    pub kind: String,
    pub modified_at: String,
    pub created_at: Option<String>,
    pub size_bytes: u64,
    pub student_count: Option<usize>,
    pub period_count: Option<usize>,
    pub provenance: Option<ProjectArtifactProvenance>,
}

/// One privacy finding (`ProjectPrivacyFinding` in `types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PrivacyFinding {
    pub file: String,
    pub fields: Vec<String>,
}

/// Full project history response (`ProjectHistoryResponse` in `types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectHistory {
    pub api_version: &'static str,
    pub project_name: String,
    pub project_path: String,
    pub history: Vec<ProjectArtifact>,
    pub outputs: Vec<ProjectArtifact>,
    pub warnings: Vec<String>,
}

/// Full privacy scan response (`ProjectPrivacyResponse` in `types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectPrivacy {
    pub api_version: &'static str,
    pub project_path: String,
    pub files_scanned: usize,
    pub safe_for_public_sharing: bool,
    pub findings: Vec<PrivacyFinding>,
}

// ---------------------------------------------------------------------------
// Project file model (subset of SeatTrellisProject)
// ---------------------------------------------------------------------------

/// Portable configuration for a SeatTrellis project workspace. Mirrors the
/// fields the Rust backend actually needs from `SeatTrellisProject`.
#[derive(Debug, Deserialize)]
struct ProjectFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    students: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    rules: Option<String>,
    #[serde(default)]
    history_dir: Option<String>,
    #[serde(default = "default_outputs_dir")]
    outputs_dir: String,
}

fn default_outputs_dir() -> String {
    "outputs".to_string()
}

/// Resolved project references, all guaranteed (when present) to live inside
/// the project root.
#[derive(Debug, Clone)]
struct ResolvedProject {
    project_file: PathBuf,
    root: PathBuf,
    students: PathBuf,
    layout: PathBuf,
    rules: PathBuf,
    history_dir: Option<PathBuf>,
    outputs_dir: PathBuf,
}

/// Load and validate a project file without touching any referenced inputs.
fn load_project(path: &Path) -> Result<ProjectFile, String> {
    let bytes = read_file_capped(path, MAX_PROJECT_FILE_BYTES)
        .map_err(|e| format!("Invalid project file: {} ({e})", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid project file: {} ({e})", path.display()))?;
    let obj = value.as_object().ok_or_else(|| {
        format!("Invalid project file: {} (not a JSON object)", path.display())
    })?;
    if obj.get("kind").and_then(Value::as_str) != Some("seattrellis_project") {
        return Err(format!(
            "Invalid project file: {} (expected kind \"seattrellis_project\")",
            path.display()
        ));
    }
    if let Some(version) = obj.get("schema_version") {
        if version.as_i64() != Some(1) {
            return Err(format!(
                "Invalid project file: {} (unsupported schema_version {version})",
                path.display()
            ));
        }
    }
    let project: ProjectFile = serde_json::from_value(value)
        .map_err(|e| format!("Invalid project file: {} ({e})", path.display()))?;
    require_relative_path(project.students.as_deref(), "students")?;
    require_relative_path(project.layout.as_deref(), "layout")?;
    require_relative_path(project.rules.as_deref(), "rules")?;
    require_relative_path(Some(&project.outputs_dir), "outputs_dir")?;
    if let Some(history) = project.history_dir.as_deref() {
        require_relative_path(Some(history), "history_dir")?;
    }
    Ok(project)
}

fn require_relative_path(value: Option<&str>, field: &str) -> Result<(), String> {
    let text = value.ok_or_else(|| format!("Project field \"{field}\" is required."))?.trim();
    if text.is_empty() {
        return Err(format!("Project field \"{field}\" cannot be empty."));
    }
    let looks_absolute = Path::new(text).is_absolute() || (text.len() >= 2 && text.as_bytes()[1] == b':');
    if looks_absolute {
        return Err(format!("Project field \"{field}\" must be a relative path."));
    }
    Ok(())
}

fn required_field<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .ok_or_else(|| format!("Project field \"{field}\" is required."))
}

/// Resolve a project file and (optionally) require its input references to
/// exist. Paths are canonicalized and checked to stay inside the project root.
fn resolve_project(project_path: &Path, require_inputs: bool) -> Result<(ProjectFile, ResolvedProject), String> {
    let project_file = fs::canonicalize(project_path)
        .map_err(|e| format!("Project file not found or unreadable: {} ({e})", project_path.display()))?;
    if !project_file.is_file() {
        return Err(format!("Project file not found: {}", project_file.display()));
    }
    let project = load_project(&project_file)?;
    let root = project_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_file.clone());
    let students = resolve_reference(&root, required_field(&project.students, "students")?, "students", require_inputs)?;
    let layout = resolve_reference(&root, required_field(&project.layout, "layout")?, "layout", require_inputs)?;
    let rules = resolve_reference(&root, required_field(&project.rules, "rules")?, "rules", require_inputs)?;
    let history_dir = match project.history_dir.as_deref() {
        Some(dir) => Some(resolve_reference(&root, dir, "history_dir", false)?),
        None => None,
    };
    let outputs_dir = resolve_reference(&root, &project.outputs_dir, "outputs_dir", false)?;
    Ok((
        project,
        ResolvedProject {
            project_file,
            root,
            students,
            layout,
            rules,
            history_dir,
            outputs_dir,
        },
    ))
}

/// Resolve a project reference and reject it when it escapes the root.
fn resolve_reference(root: &Path, relative: &str, label: &str, require: bool) -> Result<PathBuf, String> {
    let candidate = root.join(relative);
    match fs::canonicalize(&candidate) {
        Ok(resolved) => {
            ensure_inside(&resolved, root, label)?;
            Ok(resolved)
        }
        Err(err) => {
            if require {
                Err(format!(
                    "Project reference \"{label}\" not found: {} ({err})",
                    candidate.display()
                ))
            } else {
                Ok(candidate)
            }
        }
    }
}

fn ensure_inside(path: &Path, root: &Path, label: &str) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err(format!(
            "Project reference \"{label}\" points outside the project root: {}",
            path.display()
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Recent projects
// ---------------------------------------------------------------------------

/// List recent projects under `root` without reading student records into the
/// response. Matches `list_recent_projects` in `project_bundle.py`.
pub fn list_projects(root: &str, limit: usize) -> Result<Vec<RecentProject>, String> {
    let directory = canonical_project_root(root)?;
    validate_limit(limit)?;
    list_projects_in(&directory, limit)
}

/// JSON form of [`list_projects`], matching `ProjectListResponse`.
pub fn list_projects_json(root: &str, limit: usize) -> Result<String, String> {
    let directory = canonical_project_root(root)?;
    validate_limit(limit)?;
    let projects = list_projects_in(&directory, limit)?;
    let value = json!({
        "api_version": "1",
        "root": directory.to_string_lossy(),
        "projects": projects,
    });
    serde_json::to_string(&value).map_err(|e| format!("Could not serialize project list: {e}"))
}

fn canonical_project_root(root: &str) -> Result<PathBuf, String> {
    let directory = fs::canonicalize(Path::new(root))
        .map_err(|e| format!("Projects directory not found: {root} ({e})"))?;
    if !directory.is_dir() {
        return Err(format!("Projects directory not found: {root}"));
    }
    Ok(directory)
}

fn validate_limit(limit: usize) -> Result<(), String> {
    if limit == 0 || limit > 100 {
        return Err("The project list limit must be between 1 and 100.".to_string());
    }
    Ok(())
}

fn list_projects_in(directory: &Path, limit: usize) -> Result<Vec<RecentProject>, String> {
    let mut results: Vec<RecentProject> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![directory.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if is_project_file_name(&path) {
                if let Ok(project) = load_project(&path) {
                    let name = project
                        .name
                        .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string());
                    results.push(RecentProject {
                        name,
                        path: path.to_string_lossy().into_owned(),
                        modified_at: iso_from_mtime(&path).unwrap_or_default(),
                    });
                }
            }
        }
    }
    results.sort_by(|a, b| {
        (b.modified_at.as_str(), b.path.as_str()).cmp(&(a.modified_at.as_str(), a.path.as_str()))
    });
    results.truncate(limit);
    Ok(results)
}

fn is_project_file_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.ends_with(".seattrellis.json") || name.ends_with(".project.json")
}

// ---------------------------------------------------------------------------
// In-memory recent-project record
// ---------------------------------------------------------------------------

static RECENT_PROJECTS: OnceLock<Mutex<Vec<RecentProject>>> = OnceLock::new();

fn recent_store() -> &'static Mutex<Vec<RecentProject>> {
    RECENT_PROJECTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record a recently-opened project (most recent first, deduplicated by path).
pub fn record_recent_project(path: &str, name: &str) {
    if let Ok(mut store) = recent_store().lock() {
        store.retain(|project| project.path != path);
        store.insert(
            0,
            RecentProject {
                name: name.to_string(),
                path: path.to_string(),
                modified_at: now_iso(),
            },
        );
        store.truncate(MAX_RECENT_PROJECTS);
    }
}

/// Return the in-memory recent-project list (most recent first).
pub fn recent_projects() -> Vec<RecentProject> {
    recent_store().lock().map(|store| store.clone()).unwrap_or_default()
}

/// JSON form of the in-memory recent-project list.
pub fn recent_projects_json() -> Result<String, String> {
    let value = json!({
        "api_version": "1",
        "root": "",
        "projects": recent_projects(),
    });
    serde_json::to_string(&value).map_err(|e| format!("Could not serialize recent projects: {e}"))
}

// ---------------------------------------------------------------------------
// Project history
// ---------------------------------------------------------------------------

/// Build a history/output listing for a project. Student records are read only
/// to count them and are never returned.
pub fn project_history(project_path: &str) -> Result<ProjectHistory, String> {
    let (project, paths) = resolve_project(Path::new(project_path), false)?;
    let mut warnings: Vec<String> = Vec::new();
    let mut history: Vec<ProjectArtifact> = Vec::new();
    let mut outputs: Vec<ProjectArtifact> = Vec::new();
    match &paths.history_dir {
        None => warnings.push("This project does not configure a history directory.".to_string()),
        Some(dir) => {
            if !dir.is_dir() {
                warnings.push("The configured history directory is not available.".to_string());
            } else {
                history = artifact_items(dir, &mut warnings);
            }
        }
    }
    if paths.outputs_dir.is_dir() {
        outputs = artifact_items(&paths.outputs_dir, &mut warnings);
    }
    dedupe_warnings(&mut warnings);
    Ok(ProjectHistory {
        api_version: "1",
        project_name: project.name.unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string()),
        project_path: paths.project_file.to_string_lossy().into_owned(),
        history,
        outputs,
        warnings,
    })
}

/// JSON form of [`project_history`], matching `ProjectHistoryResponse`.
pub fn project_history_json(project_path: &str) -> Result<String, String> {
    let history = project_history(project_path)?;
    serde_json::to_string(&history).map_err(|e| format!("Could not serialize project history: {e}"))
}

fn dedupe_warnings(warnings: &mut Vec<String>) {
    let mut seen: HashSet<String> = HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
}

/// Read every `*.json` artifact in `dir`, newest first, mapping failures to
/// warnings (mirrors `_artifact_items` in `handlers.py`).
fn artifact_items(dir: &Path, warnings: &mut Vec<String>) -> Vec<ProjectArtifact> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                paths.push(path);
            }
        }
    }
    paths.sort_by(|a, b| {
        (mtime_nanos(b), b.as_os_str()).cmp(&(mtime_nanos(a), a.as_os_str()))
    });
    let mut items: Vec<ProjectArtifact> = Vec::new();
    for path in paths {
        match artifact_from_file(&path) {
            Ok(item) => items.push(item),
            Err(_) => {
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                warnings.push(format!("Could not read project artifact {name}."));
            }
        }
    }
    items
}

fn artifact_from_file(path: &Path) -> Result<ProjectArtifact, String> {
    let bytes = read_file_capped(path, MAX_BUNDLE_FILE_BYTES)?;
    let metadata = fs::metadata(path).map_err(|e| format!("Could not stat {}: {e}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid JSON in {}: {e}", path.display()))?;
    let kind = artifact_kind(&value);
    let provenance = artifact_provenance(&value, &kind);
    let created_at = value
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    let period_count = value.get("periods").and_then(Value::as_array).map(Vec::len);
    Ok(ProjectArtifact {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        path: path.to_string_lossy().into_owned(),
        kind,
        modified_at: iso_from_mtime(path).unwrap_or_default(),
        created_at,
        size_bytes: metadata.len(),
        student_count: student_count_of(&value),
        period_count,
        provenance,
    })
}

fn artifact_kind(value: &Value) -> String {
    match value.get("kind").and_then(Value::as_str) {
        Some("candidate_set") => "candidate_set".to_string(),
        Some("rotation_plan") => "rotation_plan".to_string(),
        Some("snapshot") => "snapshot".to_string(),
        Some(_) => "unknown".to_string(),
        None if value.get("assignments").is_some() => "snapshot".to_string(),
        None => "unknown".to_string(),
    }
}

/// Count students from the artifact itself or its first period/candidate
/// snapshot — records are counted, never returned.
fn student_count_of(value: &Value) -> Option<usize> {
    if let Some(students) = value.get("students").and_then(Value::as_array) {
        return Some(students.len());
    }
    if let Some(periods) = value.get("periods").and_then(Value::as_array) {
        if let Some(snapshot) = periods.first().and_then(|period| period.get("snapshot")) {
            if let Some(students) = snapshot.get("students").and_then(Value::as_array) {
                return Some(students.len());
            }
        }
    }
    None
}

/// Summarize artifact origin without returning student-sensitive metadata
/// (mirrors `_artifact_provenance` in `handlers.py`).
fn artifact_provenance(value: &Value, kind: &str) -> Option<ProjectArtifactProvenance> {
    let metadata_values = artifact_metadata_values(value, kind);
    let mut parent_name: Option<String> = None;
    let mut operation_count: u64 = 0;
    let mut has_operation_count = false;
    let mut source: Option<&'static str> = None;

    for metadata in &metadata_values {
        if parent_name.is_none() {
            if let Some(restored_from) = metadata.get("restored_from").and_then(Value::as_str) {
                if !restored_from.trim().is_empty() {
                    parent_name = Some(file_name_of(restored_from));
                }
            }
        }
        if let Some(persistence) = metadata.get("project_persistence") {
            if let Some(artifact_path) = persistence.get("artifact_path").and_then(Value::as_str) {
                if parent_name.is_none() && !artifact_path.trim().is_empty() {
                    parent_name = Some(file_name_of(artifact_path));
                }
            }
            if persistence.get("source").and_then(Value::as_str) == Some("react_rotation_editor") {
                source = Some("rotation_edit");
            }
        }
        if metadata.get("saved_from").and_then(Value::as_str) == Some("react_rotation_editor") {
            source = Some("rotation_edit");
        }
        let manual_edit = metadata
            .get("manual_edit")
            .filter(|v| v.is_object())
            .or_else(|| metadata.get("source_manual_edit").filter(|v| v.is_object()));
        if let Some(manual_edit) = manual_edit {
            if let Some(count) = manual_edit.get("operation_count").and_then(Value::as_u64) {
                operation_count += count;
                has_operation_count = true;
            }
            if source != Some("rotation_edit") {
                source = Some("manual_edit");
            }
        }
    }

    if parent_name.is_some() {
        source = Some("restored");
    }
    if source.is_none() {
        let generated = value.get("solver_status").is_some_and(|v| !v.is_null())
            || kind == "candidate_set"
            || kind == "rotation_plan";
        source = Some(if generated { "generated" } else { "unknown" });
    }

    Some(ProjectArtifactProvenance {
        source: source.unwrap_or("unknown").to_string(),
        parent_name,
        operation_count: if has_operation_count {
            Some(operation_count.min(100_000))
        } else {
            None
        },
    })
}

/// Collect nested metadata for provenance (mirrors `_artifact_metadata_entries`).
fn artifact_metadata_values(value: &Value, kind: &str) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::new();
    if let Some(metadata) = value.get("metadata") {
        if metadata.is_object() {
            values.push(metadata.clone());
        }
    }
    match kind {
        "rotation_plan" => {
            if let Some(periods) = value.get("periods").and_then(Value::as_array) {
                for period in periods {
                    if let Some(metadata) = period.get("snapshot").and_then(|s| s.get("metadata")) {
                        if metadata.is_object() {
                            values.push(metadata.clone());
                        }
                    }
                }
            }
        }
        "candidate_set" => {
            if let Some(candidates) = value.get("candidates").and_then(Value::as_array) {
                for candidate in candidates {
                    if let Some(metadata) = candidate.get("snapshot").and_then(|s| s.get("metadata")) {
                        if metadata.is_object() {
                            values.push(metadata.clone());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    values
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

// ---------------------------------------------------------------------------
// Privacy scan
// ---------------------------------------------------------------------------

/// Scan a project's referenced text files for sensitive fields, mirroring
/// `scan_project_privacy` in `project_bundle.py`. File contents are inspected
/// locally and reduced to field names only.
pub fn project_privacy(project_path: &str) -> Result<ProjectPrivacy, String> {
    let (_, paths) = resolve_project(Path::new(project_path), true)?;
    let (files_scanned, findings) = privacy_report(&paths, true)?;
    Ok(ProjectPrivacy {
        api_version: "1",
        project_path: paths.project_file.to_string_lossy().into_owned(),
        files_scanned,
        safe_for_public_sharing: findings.is_empty(),
        findings,
    })
}

/// JSON form of [`project_privacy`], matching `ProjectPrivacyResponse`.
pub fn project_privacy_json(project_path: &str) -> Result<String, String> {
    let privacy = project_privacy(project_path)?;
    serde_json::to_string(&privacy).map_err(|e| format!("Could not serialize project privacy: {e}"))
}

/// Collect (files_scanned, findings) for a resolved project.
fn privacy_report(paths: &ResolvedProject, include_outputs: bool) -> Result<(usize, Vec<PrivacyFinding>), String> {
    let files = collect_project_files(paths, include_outputs)?;
    let mut findings: Vec<PrivacyFinding> = Vec::new();
    for path in &files {
        let fields = scan_file(path);
        if !fields.is_empty() {
            findings.push(PrivacyFinding {
                file: rel_posix(path, &paths.root),
                fields,
            });
        }
    }
    Ok((files.len(), findings))
}

/// Collect every file that belongs to a project bundle, refusing to follow
/// references that escape the project root (mirrors `project_files`).
fn collect_project_files(paths: &ResolvedProject, include_outputs: bool) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    add_resolved_file(&mut files, &mut seen, paths.project_file.clone());

    for (path, label) in [
        (&paths.students, "students"),
        (&paths.layout, "layout"),
        (&paths.rules, "rules"),
    ] {
        let resolved = fs::canonicalize(path)
            .map_err(|e| format!("Project reference \"{label}\" not found: {} ({e})", path.display()))?;
        ensure_inside(&resolved, &paths.root, label)?;
        if !resolved.is_file() {
            return Err(format!("Project reference \"{label}\" is not a file: {}", resolved.display()));
        }
        add_resolved_file(&mut files, &mut seen, resolved);
    }

    if let Some(dir) = &paths.history_dir {
        if dir.is_dir() {
            add_directory_files(&mut files, &mut seen, dir, &paths.root, "history_dir")?;
        }
    }
    if include_outputs && paths.outputs_dir.is_dir() {
        add_directory_files(&mut files, &mut seen, &paths.outputs_dir, &paths.root, "outputs_dir")?;
    }

    let root = &paths.root;
    files.sort_by_key(|path| rel_posix(path, root));
    Ok(files)
}

fn add_resolved_file(files: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, resolved: PathBuf) {
    if resolved.is_file() && seen.insert(resolved.clone()) {
        files.push(resolved);
    }
}

fn add_directory_files(
    files: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    dir: &Path,
    root: &Path,
    label: &str,
) -> Result<(), String> {
    let resolved_dir = fs::canonicalize(dir)
        .map_err(|e| format!("Project reference \"{label}\" is not a directory: {} ({e})", dir.display()))?;
    ensure_inside(&resolved_dir, root, label)?;
    let mut stack: Vec<PathBuf> = vec![resolved_dir];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".DS_Store") {
                continue;
            }
            if path.is_dir() && !is_symlink(&path) {
                stack.push(path);
            } else if path.is_file() && !is_symlink(&path) {
                let resolved = match fs::canonicalize(&path) {
                    Ok(resolved) => resolved,
                    Err(_) => continue,
                };
                ensure_inside(&resolved, root, label)?;
                add_resolved_file(files, seen, resolved);
            }
        }
    }
    Ok(())
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

/// Inspect one text file for sensitive fields (mirrors `_scan_file`).
fn scan_file(path: &Path) -> Vec<String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Vec::new(),
    };
    if metadata.len() > MAX_BUNDLE_FILE_BYTES {
        return Vec::new();
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "csv" {
        let header_line = text.lines().next().unwrap_or("");
        return csv_fields(header_line)
            .into_iter()
            .filter(|field| is_sensitive_key(field))
            .collect();
    }
    if extension != "json" {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => {
            let mut found: Vec<String> = Vec::new();
            sensitive_keys_in_json(&value, &mut found);
            found.sort();
            found.dedup();
            found
        }
        Err(_) => Vec::new(),
    }
}

/// Parse one CSV line into fields, honoring RFC 4180 double-quoted fields.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        current.push('"');
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            }
            ',' => {
                if in_quotes {
                    current.push(ch);
                } else {
                    fields.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn sensitive_keys_in_json(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_key(key) {
                    found.push(key.clone());
                }
                sensitive_keys_in_json(child, found);
            }
        }
        Value::Array(items) => {
            for child in items {
                sensitive_keys_in_json(child, found);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    SENSITIVE_KEYS.contains(&normalized.as_str()) || normalized.ends_with("_name")
}

// ---------------------------------------------------------------------------
// Bundle pack
// ---------------------------------------------------------------------------

/// Pack a project into a self-contained `.seattrellis.zip` byte stream
/// (manifest.json + referenced files), refusing references outside the root.
pub fn pack_project(project_path: &str) -> Result<Vec<u8>, String> {
    let (_, paths) = resolve_project(Path::new(project_path), true)?;
    let files = collect_project_files(&paths, true)?;
    let root = &paths.root;
    let (files_scanned, findings) = privacy_report(&paths, true)?;
    let manifest = json!({
        "kind": "seattrellis_project_bundle",
        "format_version": BUNDLE_FORMAT_VERSION,
        "created_at": now_iso(),
        "project_file": rel_posix(&paths.project_file, root),
        "include_outputs": true,
        "files": files.iter().map(|path| rel_posix(path, root)).collect::<Vec<_>>(),
        "privacy": {
            "files_scanned": files_scanned,
            "safe_for_public_sharing": findings.is_empty(),
            "findings": findings
                .iter()
                .map(|finding| json!({ "file": finding.file, "fields": finding.fields }))
                .collect::<Vec<_>>(),
        },
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("Could not serialize project bundle manifest: {e}"))?;
    pack_files_zip(&files, root, &manifest_bytes)
}

/// JSON/API form of [`pack_project`]: returns the raw zip bytes for download.
pub fn pack_project_json(project_path: &str) -> Result<Vec<u8>, String> {
    pack_project(project_path)
}

/// Suggested download filename for a packed project, e.g. `name.seattrellis.zip`.
pub fn default_bundle_name(project_path: &str) -> Result<String, String> {
    let name = Path::new(project_path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| format!("Invalid project path: {project_path}"))?;
    for suffix in [".seattrellis.json", ".project.json", ".json"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return Ok(format!("{stripped}.seattrellis.zip"));
        }
    }
    Ok(format!("{name}.seattrellis.zip"))
}

fn pack_files_zip(files: &[PathBuf], root: &Path, manifest: &[u8]) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o100644);
        writer
            .start_file("manifest.json", options)
            .map_err(|e| format!("Could not create project bundle: {e}"))?;
        writer
            .write_all(manifest)
            .map_err(|e| format!("Could not write project bundle manifest: {e}"))?;
        for path in files {
            let name = rel_posix(path, root);
            let bytes = fs::read(path)
                .map_err(|e| format!("Could not read project file {}: {e}", path.display()))?;
            writer
                .start_file(&name, options)
                .map_err(|e| format!("Could not create project bundle: {e}"))?;
            writer
                .write_all(&bytes)
                .map_err(|e| format!("Could not write project file {}: {e}", path.display()))?;
        }
        writer
            .finish()
            .map_err(|e| format!("Could not finalize project bundle: {e}"))?;
    }
    Ok(cursor.into_inner())
}

// ---------------------------------------------------------------------------
// Bundle restore
// ---------------------------------------------------------------------------

struct Manifest {
    project_file: String,
    files: Vec<String>,
}

struct ValidatedEntry {
    index: usize,
    name: String,
    size: u64,
}

/// Validate and restore a project bundle without allowing path traversal or
/// symlink escape. Returns the path to the restored project file.
pub fn restore_project_bundle(bundle_bytes: &[u8], output_dir: &str, overwrite: bool) -> Result<PathBuf, String> {
    let mut archive = ZipArchive::new(Cursor::new(bundle_bytes))
        .map_err(|e| format!("Could not read project bundle: {e}"))?;
    let manifest = read_manifest(&mut archive)?;
    let entries = validated_entries(&mut archive)?;

    let listed_files: HashSet<String> = manifest.files.iter().cloned().collect();
    let entry_files: HashSet<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    if listed_files != entry_files {
        return Err("Project bundle manifest does not match its file entries.".to_string());
    }
    let project_name = safe_archive_name(&manifest.project_file)?;
    if !listed_files.contains(&project_name) {
        return Err("Project bundle manifest does not include its project file.".to_string());
    }
    let total_size: u64 = entries.iter().map(|entry| entry.size).sum();
    if total_size > MAX_BUNDLE_TOTAL_BYTES {
        return Err("Project bundle is too large to restore safely.".to_string());
    }

    let destination = PathBuf::from(output_dir);
    let dest_name = destination
        .file_name()
        .map(|name| name.to_os_string())
        .ok_or_else(|| format!("Invalid restore destination: {output_dir}"))?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent)
        .map_err(|e| format!("Could not create restore destination {}: {e}", parent.display()))?;
    let parent_abs = fs::canonicalize(&parent)
        .map_err(|e| format!("Restore destination unavailable: {} ({e})", parent.display()))?;
    let dest_abs = parent_abs.join(&dest_name);

    if dest_abs.exists() {
        let non_empty = fs::read_dir(&dest_abs)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true);
        if non_empty && !overwrite {
            return Err(format!("Restore destination is not empty: {}", dest_abs.display()));
        }
    }

    let staging = parent_abs.join(format!(".seattrellis-restore-{}", now_nanos()));
    if let Err(err) = fs::create_dir_all(&staging) {
        return Err(format!("Could not create staging directory: {err}"));
    }
    let extraction = extract_bundle(&mut archive, &entries, &staging);
    if let Err(err) = extraction {
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }

    // Validate the restored project file parses before moving it into place.
    let restored_staged = staging.join(&project_name);
    if let Err(err) = load_project(&restored_staged) {
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }

    if !dest_abs.exists() {
        if let Err(err) = fs::create_dir_all(&dest_abs) {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("Could not create restore destination {}: {err}", dest_abs.display()));
        }
    }
    if let Err(err) = copy_tree(&staging, &dest_abs) {
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }
    let _ = fs::remove_dir_all(&staging);

    Ok(dest_abs.join(&project_name))
}

/// JSON form of [`restore_project_bundle`], matching `ProjectRestoreResponse`.
/// `overwrite` defaults to false.
pub fn restore_project_json(bundle_bytes: &[u8], output_dir: &str) -> Result<String, String> {
    let restored = restore_project_bundle(bundle_bytes, output_dir, false)?;
    let destination = restored
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(output_dir));
    let value = json!({
        "api_version": "1",
        "project_path": restored.to_string_lossy(),
        "output_dir": destination.to_string_lossy(),
    });
    serde_json::to_string(&value).map_err(|e| format!("Could not serialize restore response: {e}"))
}

fn read_manifest(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Manifest, String> {
    let mut file = archive
        .by_name("manifest.json")
        .map_err(|_| "Project bundle is missing manifest.json.".to_string())?;
    if file.size() > MAX_MANIFEST_BYTES {
        return Err("Project bundle manifest is unexpectedly large.".to_string());
    }
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read project bundle manifest: {e}"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Project bundle manifest is not valid UTF-8 JSON.".to_string())?;
    let obj = value
        .as_object()
        .ok_or_else(|| "Project bundle has an unknown manifest kind.".to_string())?;
    if obj.get("kind").and_then(Value::as_str) != Some("seattrellis_project_bundle") {
        return Err("Project bundle has an unknown manifest kind.".to_string());
    }
    if obj.get("format_version").and_then(Value::as_u64) != Some(u64::from(BUNDLE_FORMAT_VERSION)) {
        let version = obj.get("format_version").cloned().unwrap_or(Value::Null);
        return Err(format!("Unsupported project bundle format_version {version}."));
    }
    let project_file = obj
        .get("project_file")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Project bundle manifest is incomplete.".to_string())?;
    let files = obj
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "Project bundle manifest is incomplete.".to_string())?
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect::<Option<Vec<String>>>()
        .ok_or_else(|| "Project bundle manifest files must be strings.".to_string())?;
    let unique: HashSet<&String> = files.iter().collect();
    if unique.len() != files.len() {
        return Err("Project bundle manifest contains duplicate file entries.".to_string());
    }
    Ok(Manifest { project_file, files })
}

fn validated_entries(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<ValidatedEntry>, String> {
    let mut entries: Vec<ValidatedEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|e| format!("Could not read project bundle: {e}"))?;
        if file.name() == "manifest.json" || file.is_dir() {
            continue;
        }
        let name = safe_archive_name(file.name())?;
        if !seen.insert(name.clone()) {
            return Err(format!("Project bundle contains duplicate file: {name}"));
        }
        if file.size() > MAX_BUNDLE_FILE_BYTES {
            return Err(format!("Project bundle file is too large: {name}"));
        }
        if let Some(mode) = file.unix_mode() {
            if (mode & 0o170000) == 0o120000 {
                return Err(format!("Project bundle cannot restore symlinks: {name}"));
            }
        }
        entries.push(ValidatedEntry {
            index,
            name,
            size: file.size(),
        });
    }
    Ok(entries)
}

fn safe_archive_name(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("Unsafe project bundle path: empty entry name".to_string());
    }
    if value.contains('\\') || value.contains('\0') {
        return Err(format!("Unsafe project bundle path: {value:?}"));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(format!("Unsafe project bundle path: {value:?}"));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!("Unsafe project bundle path: {value:?}"));
        }
    }
    Ok(value.to_string())
}

fn extract_bundle(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    entries: &[ValidatedEntry],
    staging: &Path,
) -> Result<(), String> {
    for entry in entries {
        let target = staging.join(&entry.name);
        if !target.starts_with(staging) {
            return Err(format!("Unsafe project bundle path: {:?}", entry.name));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create directory {}: {e}", parent.display()))?;
        }
        let mut source = archive
            .by_index(entry.index)
            .map_err(|e| format!("Could not read project bundle: {e}"))?;
        let mut output = fs::File::create(&target)
            .map_err(|e| format!("Could not write restored file {}: {e}", target.display()))?;
        std::io::copy(&mut source, &mut output)
            .map_err(|e| format!("Could not write restored file {}: {e}", target.display()))?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let entries = fs::read_dir(source)
        .map_err(|e| format!("Could not read restored files: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if is_symlink(&target) {
            return Err(format!("Restore destination contains a symlink: {}", target.display()));
        }
        if path.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| format!("Could not create directory {}: {e}", target.display()))?;
            copy_tree(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target)
                .map_err(|e| format!("Could not copy restored file {}: {e}", target.display()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// File + time helpers
// ---------------------------------------------------------------------------

fn read_file_capped(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    if metadata.len() > max_bytes {
        return Err(format!(
            "File is too large to read: {} ({} bytes)",
            path.display(),
            metadata.len()
        ));
    }
    fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))
}

fn rel_posix(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

fn mtime_nanos(path: &Path) -> i64 {
    match fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(time) => time
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as i64)
            .unwrap_or(0),
        Err(_) => 0,
    }
}

fn iso_from_mtime(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let secs = modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(iso_from_epoch_secs(secs))
}

fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    iso_from_epoch_secs(secs)
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// Format a Unix timestamp as a UTC ISO-8601 string (`YYYY-MM-DDTHH:MM:SS+00:00`)
/// using Howard Hinnant's `civil_from_days` algorithm (no external date crate).
fn iso_from_epoch_secs(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+00:00")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "seattrellis-projects-{}-{}-{}",
            std::process::id(),
            name,
            now_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a complete minimal project at `root` and return the project file.
    fn write_project(root: &Path, name: &str, students_csv: &str, with_history: bool) -> PathBuf {
        let mut project = json!({
            "kind": "seattrellis_project",
            "schema_version": 1,
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "outputs_dir": "outputs",
        });
        project["name"] = json!(name);
        if with_history {
            project["history_dir"] = json!("history");
        }
        let project_file = root.join("project.seattrellis.json");
        fs::write(&project_file, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
        fs::write(root.join("students.csv"), students_csv).unwrap();
        fs::write(root.join("classroom.json"), r#"{"rows":5,"cols":6}"#).unwrap();
        fs::write(root.join("rules.json"), r#"{"mode":"default"}"#).unwrap();
        project_file
    }

    fn write_snapshot(root: &Path, name: &str, body: &Value) -> PathBuf {
        let dir = root.join("history");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, serde_json::to_vec_pretty(body).unwrap()).unwrap();
        path
    }

    /// Build a raw zip from (name, content, unix_mode) tuples.
    fn make_zip(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            for (name, content, mode) in entries {
                let options = SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Stored)
                    .unix_permissions(*mode);
                writer.start_file(*name, options).unwrap();
                writer.write_all(content).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn simple_manifest(files: &[&str], project_file: &str) -> Vec<u8> {
        let manifest = json!({
            "kind": "seattrellis_project_bundle",
            "format_version": 1,
            "project_file": project_file,
            "files": files,
        });
        format!("{}\n", serde_json::to_string(&manifest).unwrap()).into_bytes()
    }

    /// Build a zip with a genuine symlink entry (stored with the Unix file-type
    /// bits set, as real archive tools produce).
    fn make_zip_with_symlink(manifest: &[u8], name: &str, target: &str) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let options = SimpleFileOptions::default();
            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(manifest).unwrap();
            writer.add_symlink(name, target, options).unwrap();
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn list_finds_projects_recursively_and_skips_hidden() {
        let root = temp_root("list-find");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        write_project(&root.join("a"), "Alpha", "student_ref,gender\n", false);
        write_project(&root.join("a/b"), "Beta", "student_ref,gender\n", false);
        // Hidden project must be ignored.
        write_project(&root.join(".hidden"), "Hidden", "student_ref,gender\n", false);

        let canonical_root = fs::canonicalize(&root).unwrap();
        let json = list_projects_json(root.to_str().unwrap(), 100).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["api_version"], "1");
        assert_eq!(value["root"], canonical_root.to_str().unwrap());
        let projects = value["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        let names: Vec<&str> = projects.iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Alpha"));
        assert!(names.contains(&"Beta"));
        for project in projects {
            assert!(project["path"].as_str().unwrap().contains("project.seattrellis.json"));
            assert!(project["modified_at"].as_str().unwrap().ends_with("+00:00"));
        }
    }

    #[test]
    fn list_limit_bounds() {
        let root = temp_root("list-limit");
        write_project(&root, "One", "student_ref,gender\n", false);
        assert!(list_projects(root.to_str().unwrap(), 0).is_err());
        assert!(list_projects(root.to_str().unwrap(), 101).is_err());
        let projects = list_projects(root.to_str().unwrap(), 1).unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[test]
    fn list_unknown_root_errors() {
        let missing = std::env::temp_dir().join(format!("does-not-exist-{}", now_nanos()));
        assert!(list_projects(missing.to_str().unwrap(), 20).is_err());
        assert!(list_projects_json(missing.to_str().unwrap(), 20).is_err());
    }

    #[test]
    fn history_lists_snapshots_without_student_data() {
        let root = temp_root("history-clean");
        let project_file = write_project(&root, "Demo", "student_ref,gender\n", true);
        write_snapshot(
            &root,
            "week1.snapshot.json",
            &json!({
                "schema_version": 1,
                "created_at": "2026-06-01T00:00:00+00:00",
                "metadata": {"version": "0.2.1"},
                "students": [{"student_id": "S1", "name": "Alice"}],
                "assignments": [],
                "solver_status": "solved"
            }),
        );

        let json = project_history_json(project_file.to_str().unwrap()).unwrap();
        // Student records must not leak into the response.
        assert!(!json.contains("\"students\""));
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["api_version"], "1");
        assert_eq!(value["project_name"], "Demo");
        let history = value["history"].as_array().unwrap();
        assert_eq!(history.len(), 1);
        let item = &history[0];
        assert_eq!(item["name"], "week1.snapshot.json");
        assert_eq!(item["kind"], "snapshot");
        assert_eq!(item["created_at"], "2026-06-01T00:00:00+00:00");
        assert_eq!(item["student_count"], 1);
        assert!(item["modified_at"].as_str().unwrap().ends_with("+00:00"));
        assert_eq!(item["provenance"]["source"], "generated");
    }

    #[test]
    fn history_missing_dir_warns() {
        let root = temp_root("history-warn");
        let project_file = write_project(&root, "NoHistory", "student_ref,gender\n", false);
        let json = project_history_json(project_file.to_str().unwrap()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        let warnings = value["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("history directory")));
        assert_eq!(value["history"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn privacy_detects_sensitive_fields() {
        let root = temp_root("privacy-sensitive");
        let project_file = write_project(
            &root,
            "Sensitive",
            "student_id,name,gender,score,vision,notes\nSTU001,Alice,F,92,poor,\n",
            false,
        );
        let json = project_privacy_json(project_file.to_str().unwrap()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["safe_for_public_sharing"], false);
        let findings = value["findings"].as_array().unwrap();
        let students = findings.iter().find(|f| f["file"] == "students.csv");
        assert!(students.is_some(), "expected a students.csv finding, got {findings:?}");
        let fields: Vec<&str> = students
            .unwrap()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        assert!(fields.contains(&"student_id"));
        assert!(fields.contains(&"score"));
        assert!(fields.contains(&"notes"));
        // gender is not sensitive.
        assert!(!fields.contains(&"gender"));
    }

    #[test]
    fn privacy_clean_project_is_safe() {
        let root = temp_root("privacy-clean");
        // No sensitive keys anywhere: the project file omits `name` and the CSV
        // headers are neutral.
        let project_file = root.join("project.seattrellis.json");
        fs::write(
            &project_file,
            r#"{"kind":"seattrellis_project","schema_version":1,"students":"students.csv","layout":"classroom.json","rules":"rules.json"}"#,
        )
        .unwrap();
        fs::write(root.join("students.csv"), "student_ref,gender\n").unwrap();
        fs::write(root.join("classroom.json"), r#"{"rows":5,"cols":6}"#).unwrap();
        fs::write(root.join("rules.json"), r#"{"mode":"default"}"#).unwrap();

        let json = project_privacy_json(project_file.to_str().unwrap()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["safe_for_public_sharing"], true);
        assert_eq!(value["files_scanned"], 4); // project + students + layout + rules
        assert_eq!(value["findings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn pack_restore_roundtrip() {
        let root = temp_root("roundtrip");
        let project_file = write_project(&root, "Roundtrip", "student_ref,gender\nA,F\nB,M\n", true);
        write_snapshot(
            &root,
            "week1.snapshot.json",
            &json!({"kind": "snapshot", "assignments": [], "students": [{"student_id": "A"}]}),
        );

        let bytes = pack_project_json(project_file.to_str().unwrap()).unwrap();
        assert!(bytes.len() >= 4);
        assert_eq!(&bytes[..2], b"PK");

        let dest = temp_root("roundtrip-out");
        let json = restore_project_json(&bytes, dest.to_str().unwrap()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["api_version"], "1");

        let restored_project = PathBuf::from(value["project_path"].as_str().unwrap());
        assert!(restored_project.is_file());
        assert!(restored_project.parent().unwrap().join("students.csv").is_file());
        assert!(restored_project
            .parent()
            .unwrap()
            .join("history/week1.snapshot.json")
            .is_file());
        // The restored project file must validate.
        assert!(load_project(&restored_project).is_ok());
    }

    #[test]
    fn pack_rejects_traversal_reference() {
        let root = temp_root("pack-traversal");
        // The referenced file exists but sits one level above the project root,
        // so resolving it must be rejected as escaping the workspace.
        let outside = root.parent().unwrap().join("outside.csv");
        fs::write(&outside, "secret\n").unwrap();
        let project_file = root.join("project.seattrellis.json");
        fs::write(
            &project_file,
            r#"{"kind":"seattrellis_project","schema_version":1,"name":"X","students":"../outside.csv","layout":"classroom.json","rules":"rules.json"}"#,
        )
        .unwrap();
        fs::write(root.join("classroom.json"), r#"{"rows":5}"#).unwrap();
        fs::write(root.join("rules.json"), r#"{"mode":"default"}"#).unwrap();

        let err = pack_project(project_file.to_str().unwrap()).unwrap_err();
        assert!(err.contains("outside the project root"), "got: {err}");
    }

    #[test]
    fn restore_rejects_path_traversal() {
        let dest = temp_root("restore-traversal");
        let zip = make_zip(&[
            ("manifest.json", &simple_manifest(&["../evil.txt"], "../evil.txt"), 0o100644),
            ("../evil.txt", b"boom", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("Unsafe project bundle path"), "got: {err}");
    }

    #[test]
    fn restore_rejects_symlink_entry() {
        let dest = temp_root("restore-symlink");
        let zip = make_zip_with_symlink(&simple_manifest(&["link"], "link"), "link", "students.csv");
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("symlink"), "got: {err}");
    }

    #[test]
    fn restore_rejects_corrupt_zip() {
        let dest = temp_root("restore-corrupt");
        let err = restore_project_bundle(b"this is definitely not a zip archive", dest.to_str().unwrap(), false)
            .unwrap_err();
        assert!(err.contains("Could not read project bundle"), "got: {err}");
    }

    #[test]
    fn restore_rejects_manifest_mismatch() {
        let dest = temp_root("restore-mismatch");
        // Manifest claims a.txt but the archive carries b.txt.
        let zip = make_zip(&[
            ("manifest.json", &simple_manifest(&["a.txt"], "a.txt"), 0o100644),
            ("b.txt", b"x", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("does not match"), "got: {err}");
    }

    #[test]
    fn restore_rejects_nonempty_destination() {
        let dest = temp_root("restore-nonempty");
        fs::write(dest.join("existing.txt"), b"keep").unwrap();
        let zip = make_zip(&[
            ("manifest.json", &simple_manifest(&["proj.json"], "proj.json"), 0o100644),
            ("proj.json", b"{}", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("not empty"), "got: {err}");
        // With overwrite, the same bundle passes the empty-destination check and
        // then fails validating the (invalid) project file.
        assert!(restore_project_bundle(&zip, dest.to_str().unwrap(), true).is_err());
    }

    #[test]
    fn record_recent_projects_orders_and_dedups() {
        // Reset the global store for a deterministic assertion.
        if let Ok(mut store) = recent_store().lock() {
            store.clear();
        }
        record_recent_project("/tmp/proj-a", "A");
        record_recent_project("/tmp/proj-b", "B");
        record_recent_project("/tmp/proj-a", "A2");
        let recent = recent_projects();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, "/tmp/proj-a");
        assert_eq!(recent[0].name, "A2");
        assert_eq!(recent[1].path, "/tmp/proj-b");
        let json = recent_projects_json().unwrap();
        assert!(json.contains("\"api_version\":\"1\""));
    }

    #[test]
    fn default_bundle_name_matches_suffixes() {
        assert_eq!(default_bundle_name("/d/x.seattrellis.json").unwrap(), "x.seattrellis.zip");
        assert_eq!(default_bundle_name("/d/x.project.json").unwrap(), "x.seattrellis.zip");
        assert_eq!(default_bundle_name("/d/plain.json").unwrap(), "plain.seattrellis.zip");
        assert_eq!(default_bundle_name("/d/weird.txt").unwrap(), "weird.txt.seattrellis.zip");
    }

    #[test]
    fn iso_formatting_rounds_trip() {
        assert_eq!(iso_from_epoch_secs(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso_from_epoch_secs(1_752_900_000), "2025-07-19T04:40:00+00:00");
        assert_eq!(iso_from_epoch_secs(1_780_272_000), "2026-06-01T00:00:00+00:00");
    }
}
