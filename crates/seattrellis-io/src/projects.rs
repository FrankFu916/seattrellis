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
//!   verdict, safe_for_public_sharing, findings: PrivacyFinding[] }` where
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

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use seattrellis_schema::{
    aggregate_verdicts, classify_findings, classify_scan, classify_unscanned, is_sensitive_key,
    scan_document, PrivacyVerdict,
};
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
    /// Additive v2 privacy state. Older clients may continue reading the
    /// existing boolean; the explicit enum prevents incomplete scans from
    /// being confused with a clean scan.
    pub verdict: PrivacyVerdict,
    pub safe_for_public_sharing: bool,
    pub findings: Vec<PrivacyFinding>,
}

#[derive(Debug, Clone, PartialEq)]
struct PrivacyReport {
    files_scanned: usize,
    verdict: PrivacyVerdict,
    findings: Vec<PrivacyFinding>,
}

#[derive(Debug, Clone, PartialEq)]
struct FilePrivacyScan {
    verdict: PrivacyVerdict,
    fields: Vec<String>,
}

impl FilePrivacyScan {
    fn indeterminate() -> Self {
        Self {
            verdict: classify_unscanned(),
            fields: Vec::new(),
        }
    }
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
        format!(
            "Invalid project file: {} (not a JSON object)",
            path.display()
        )
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
    let text = value
        .ok_or_else(|| format!("Project field \"{field}\" is required."))?
        .trim();
    if text.is_empty() {
        return Err(format!("Project field \"{field}\" cannot be empty."));
    }
    let looks_absolute =
        Path::new(text).is_absolute() || (text.len() >= 2 && text.as_bytes()[1] == b':');
    if looks_absolute {
        return Err(format!(
            "Project field \"{field}\" must be a relative path."
        ));
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
fn resolve_project(
    project_path: &Path,
    require_inputs: bool,
) -> Result<(ProjectFile, ResolvedProject), String> {
    let project_file = fs::canonicalize(project_path).map_err(|e| {
        format!(
            "Project file not found or unreadable: {} ({e})",
            project_path.display()
        )
    })?;
    if !project_file.is_file() {
        return Err(format!(
            "Project file not found: {}",
            project_file.display()
        ));
    }
    let project = load_project(&project_file)?;
    let root = project_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_file.clone());
    let students = resolve_reference(
        &root,
        required_field(&project.students, "students")?,
        "students",
        require_inputs,
    )?;
    let layout = resolve_reference(
        &root,
        required_field(&project.layout, "layout")?,
        "layout",
        require_inputs,
    )?;
    let rules = resolve_reference(
        &root,
        required_field(&project.rules, "rules")?,
        "rules",
        require_inputs,
    )?;
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
fn resolve_reference(
    root: &Path,
    relative: &str,
    label: &str,
    require: bool,
) -> Result<PathBuf, String> {
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
// Project workspace resolution and solve-request compilation
// ---------------------------------------------------------------------------

/// A project workspace with every sibling reference resolved and verified to
/// stay inside the project root (canonical containment; `..`, absolute paths
/// and symlink escapes are rejected).
#[derive(Debug, Clone)]
pub struct ResolvedProjectWorkspace {
    pub project_file: PathBuf,
    pub root: PathBuf,
    pub students: PathBuf,
    pub layout: PathBuf,
    pub rules: PathBuf,
    pub history_dir: Option<PathBuf>,
    pub outputs_dir: PathBuf,
}

/// Load a project document (kind/schema checked) and resolve every sibling
/// reference with canonical containment. Referenced files must exist.
pub fn resolve_project_workspace(project_path: &Path) -> Result<ResolvedProjectWorkspace, String> {
    let (_, resolved) = resolve_project(project_path, true)?;
    Ok(ResolvedProjectWorkspace {
        project_file: resolved.project_file,
        root: resolved.root,
        students: resolved.students,
        layout: resolved.layout,
        rules: resolved.rules,
        history_dir: resolved.history_dir,
        outputs_dir: resolved.outputs_dir,
    })
}

/// Resolve a single project reference relative to `root` with canonical
/// containment. Fails when the file is missing or escapes the root.
pub fn resolve_project_reference(
    root: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, String> {
    resolve_reference(root, relative, label, true)
}

/// Load a project document without requiring referenced inputs to exist.
/// Returns the raw document plus the canonical project root.
pub fn load_project_document(project_path: &Path) -> Result<(Value, PathBuf), String> {
    let project_file = fs::canonicalize(project_path).map_err(|error| {
        format!(
            "Project file not found or unreadable: {} ({error})",
            project_path.display()
        )
    })?;
    if !project_file.is_file() {
        return Err(format!(
            "Project file not found: {}",
            project_file.display()
        ));
    }
    // Reuse the same document validation as resolve_project, then hand back
    // the raw JSON so callers can render their own views of the workspace.
    load_project(&project_file)?;
    let bytes = fs::read(&project_file)
        .map_err(|error| format!("could not read {}: {error}", project_file.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid project file: {} ({error})", project_file.display()))?;
    let root = project_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_file.clone());
    Ok((value, root))
}

/// Compile the core `CoreSolveRequest` JSON from a project workspace:
/// roster CSV -> student records, layout JSON -> enabled seat grid +
/// adjacency edges, rules JSON -> soft rules + resolved hard-rule pairs.
///
/// This is the single workspace -> request conversion used by the CLI
/// project commands (plan §5.5: CLI and local API call the same library).
pub fn build_project_solve_request(project_path: &Path) -> Result<Value, String> {
    let workspace = resolve_project_workspace(project_path)?;

    // Roster CSV -> core student records (automatic header mapping).
    let roster_bytes = fs::read(&workspace.students)
        .map_err(|error| format!("could not read {}: {error}", workspace.students.display()))?;
    let students = crate::roster::parse_roster_students(&roster_bytes)?;
    let core_students: Vec<Value> = students
        .iter()
        .map(|student| {
            let key = student
                .student_id
                .clone()
                .filter(|id| !id.is_empty())
                .or_else(|| student.name.clone())
                .unwrap_or_default();
            json!({
                "key": key,
                "display_name": student.name,
                "height_cm": student.height_cm,
                "score": student.score,
                "vision": student.vision.as_ref().map(|v| match v {
                    crate::roster::VisionValue::Num(value) => value.to_string(),
                    crate::roster::VisionValue::Str(value) => value.clone(),
                }),
                "tags": student.tags,
                "needs": student.needs,
            })
        })
        .collect();
    if core_students.iter().any(|student| {
        student
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    }) {
        return Err("roster rows must carry a student_id or name".to_string());
    }

    // Layout JSON -> enabled seat grid + adjacency.
    let layout_text = fs::read_to_string(&workspace.layout)
        .map_err(|error| format!("could not read {}: {error}", workspace.layout.display()))?;
    let layout: Value = serde_json::from_str(&layout_text)
        .map_err(|error| format!("layout file is not valid JSON: {error}"))?;
    let seats = layout
        .get("seats")
        .and_then(Value::as_array)
        .ok_or_else(|| "layout has no seats array".to_string())?;
    let enabled: Vec<&Value> = seats
        .iter()
        .filter(|seat| seat.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .collect();
    if enabled.is_empty() {
        return Err("layout has no enabled seats".to_string());
    }
    let seat_positions: Vec<[f64; 2]> = enabled
        .iter()
        .map(|seat| {
            let x = seat.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = seat.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            [x, y]
        })
        .collect();
    let seat_index_by_id: HashMap<&str, usize> = enabled
        .iter()
        .enumerate()
        .filter_map(|(index, seat)| {
            seat.get("seat_id")
                .and_then(Value::as_str)
                .map(|seat_id| (seat_id, index))
        })
        .collect();
    // Layout adjacency: reject custom edges that reference unknown or
    // disabled seats. The Python oracle rejects these at load time; a
    // silently dropped edge would weaken the constraint set and could let an
    // illegal plan through.
    if let Some(adjacency) = layout.get("adjacency").and_then(Value::as_object) {
        if let Some(edges) = adjacency.get("custom_edges").and_then(Value::as_array) {
            for edge in edges {
                let list = edge.as_array().ok_or_else(|| {
                    "custom_edges entries must be [seat_id, seat_id] pairs".to_string()
                })?;
                let first = list
                    .first()
                    .and_then(Value::as_str)
                    .ok_or_else(|| "custom_edges entry is missing its first seat_id".to_string())?;
                let second = list.get(1).and_then(Value::as_str).ok_or_else(|| {
                    "custom_edges entry is missing its second seat_id".to_string()
                })?;
                let known = |seat_id: &str| seat_index_by_id.contains_key(seat_id);
                if !known(first) || !known(second) {
                    return Err(format!(
                        "layout adjacency references unknown seat_id {first:?} or {second:?}"
                    ));
                }
            }
        }
    }
    // The core requires layout.seats to be aligned with seat_positions
    // (layout.seats[i] <-> seat_positions[i]), so strip disabled seats.
    let core_layout_value = json!({
        "layout_id": layout.get("layout_id").cloned().unwrap_or_else(|| json!("project")),
        "name": layout.get("name").cloned().unwrap_or_else(|| json!("Project")),
        "seats": enabled,
        "adjacency": layout.get("adjacency").cloned().unwrap_or_else(|| json!({})),
    });
    let core_layout: seattrellis_core::models::Layout =
        serde_json::from_value(core_layout_value)
            .map_err(|error| format!("layout is not core-compatible: {error}"))?;
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for (first, second) in seattrellis_core::objectives::build_adjacency_edges(&core_layout) {
        let (Some(&first_index), Some(&second_index)) = (
            seat_index_by_id.get(first.as_str()),
            seat_index_by_id.get(second.as_str()),
        ) else {
            continue;
        };
        edges.push([first_index.min(second_index), first_index.max(second_index)]);
    }
    edges.sort_unstable();
    edges.dedup();

    // Rules JSON -> soft rules + resolved hard-rule index pairs.
    let rules_text = fs::read_to_string(&workspace.rules)
        .map_err(|error| format!("could not read {}: {error}", workspace.rules.display()))?;
    let rules: Value = serde_json::from_str(&rules_text)
        .map_err(|error| format!("rules file is not valid JSON: {error}"))?;
    // Strict schema mirroring the Python RuleSet models (extra="forbid"):
    // unknown rule kinds / soft objectives must never be silently dropped —
    // a dropped constraint changes the plan the teacher asked for.
    if let Some(object) = rules.as_object() {
        const KNOWN_TOP: [&str; 5] = ["schema_version", "seed", "hard", "soft", "groups"];
        for key in object.keys() {
            if !KNOWN_TOP.contains(&key.as_str()) {
                return Err(format!("rules file contains unknown top-level key {key:?}"));
            }
        }
    }
    let hard = rules.get("hard").cloned().unwrap_or_else(|| json!({}));
    if let Some(object) = hard.as_object() {
        const KNOWN_HARD: [&str; 4] = [
            "fixed_seats",
            "must_be_adjacent",
            "cannot_be_adjacent",
            "min_distance",
        ];
        for key in object.keys() {
            if !KNOWN_HARD.contains(&key.as_str()) {
                return Err(format!("unknown hard rule kind {key:?} in rules file"));
            }
        }
    }
    let soft = rules.get("soft").cloned().unwrap_or_else(|| json!({}));
    if let Some(object) = soft.as_object() {
        const KNOWN_SOFT: [&str; 10] = [
            "vision_front",
            "height_back",
            "randomize",
            "score_balance",
            "score_position",
            "score_distribution",
            "mentor_pairing",
            "fair_rotation",
            "avoid_recent_neighbors",
            "cooling",
        ];
        for key in object.keys() {
            if !KNOWN_SOFT.contains(&key.as_str()) {
                return Err(format!("unknown soft objective {key:?} in rules file"));
            }
        }
    }
    let student_index: HashMap<&str, usize> = core_students
        .iter()
        .enumerate()
        .filter_map(|(index, student)| {
            student
                .get("key")
                .and_then(Value::as_str)
                .map(|key| (key, index))
        })
        .collect();

    let resolve_pair = |pair: &Value| -> Result<[usize; 2], String> {
        // Accept both the pair-rule object {students: [k1, k2]} and the
        // plain [k1, k2] array (Python PairRule vs index-pair shapes).
        let list = pair
            .get("students")
            .and_then(Value::as_array)
            .or_else(|| pair.as_array())
            .ok_or_else(|| "hard rule pair must be {students: [a, b]} or [a, b]".to_string())?;
        let first = list
            .first()
            .and_then(Value::as_str)
            .and_then(|key| student_index.get(key).copied())
            .ok_or_else(|| format!("hard rule references unknown student: {pair:?}"))?;
        let second = list
            .get(1)
            .and_then(Value::as_str)
            .and_then(|key| student_index.get(key).copied())
            .ok_or_else(|| format!("hard rule references unknown student: {pair:?}"))?;
        Ok([first.min(second), first.max(second)])
    };

    let hard = rules.get("hard").cloned().unwrap_or_else(|| json!({}));
    let mut fixed_seats: Vec<[usize; 2]> = Vec::new();
    if let Some(list) = hard.get("fixed_seats").and_then(Value::as_array) {
        for entry in list {
            let student = entry.get("student").and_then(Value::as_str).unwrap_or("");
            let seat_id = entry.get("seat_id").and_then(Value::as_str).unwrap_or("");
            let student_index = student_index
                .get(student)
                .copied()
                .ok_or_else(|| format!("fixed seat references unknown student {student:?}"))?;
            let seat_index = seat_index_by_id
                .get(seat_id)
                .copied()
                .ok_or_else(|| format!("fixed seat references unknown seat {seat_id:?}"))?;
            fixed_seats.push([student_index, seat_index]);
        }
    }
    let must_be_adjacent = hard
        .get("must_be_adjacent")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .map(resolve_pair)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let cannot_be_adjacent = hard
        .get("cannot_be_adjacent")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .map(resolve_pair)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let min_distance: Vec<Value> = hard
        .get("min_distance")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .map(|rule| -> Result<Value, String> {
                    let students = rule
                        .get("students")
                        .and_then(Value::as_array)
                        .ok_or_else(|| "min_distance rule is missing students".to_string())?;
                    let first = students
                        .first()
                        .and_then(Value::as_str)
                        .and_then(|key| student_index.get(key).copied())
                        .ok_or_else(|| "min_distance references unknown student".to_string())?;
                    let second = students
                        .get(1)
                        .and_then(Value::as_str)
                        .and_then(|key| student_index.get(key).copied())
                        .ok_or_else(|| "min_distance references unknown student".to_string())?;
                    let distance = rule
                        .get("distance")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| "min_distance rule is missing distance".to_string())?;
                    let metric = rule
                        .get("metric")
                        .and_then(Value::as_str)
                        .unwrap_or("euclidean")
                        .to_string();
                    Ok(json!({
                        "students": [first, second],
                        "distance": distance,
                        "metric": metric,
                    }))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(json!({
        "api_version": 2,
        "student_count": core_students.len(),
        "seat_positions": seat_positions,
        "edges": edges,
        "fixed_seats": fixed_seats,
        "must_be_adjacent": must_be_adjacent,
        "cannot_be_adjacent": cannot_be_adjacent,
        "min_distance": min_distance,
        "seed": rules.get("seed").and_then(Value::as_u64).unwrap_or(42),
        "students": core_students,
        "layout": json!({
            "layout_id": layout.get("layout_id").cloned().unwrap_or_else(|| json!("project")),
            "name": layout.get("name").cloned().unwrap_or_else(|| json!("Project")),
            "seats": enabled,
            "adjacency": layout.get("adjacency").cloned().unwrap_or_else(|| json!({})),
        }),
        "rules": {
            "seed": rules.get("seed").and_then(Value::as_u64).unwrap_or(42),
            "soft": rules.get("soft").cloned().unwrap_or_else(|| json!({})),
            "groups": rules.get("groups").cloned().unwrap_or_else(|| json!([])),
        },
    }))
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
    recent_store()
        .lock()
        .map(|store| store.clone())
        .unwrap_or_default()
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
        project_name: project
            .name
            .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string()),
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
    paths.sort_by(|a, b| (mtime_nanos(b), b.as_os_str()).cmp(&(mtime_nanos(a), a.as_os_str())));
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
    let metadata =
        fs::metadata(path).map_err(|e| format!("Could not stat {}: {e}", path.display()))?;
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
                    if let Some(metadata) =
                        candidate.get("snapshot").and_then(|s| s.get("metadata"))
                    {
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
    let report = privacy_report(&paths, true)?;
    Ok(ProjectPrivacy {
        api_version: "1",
        project_path: paths.project_file.to_string_lossy().into_owned(),
        files_scanned: report.files_scanned,
        verdict: report.verdict,
        safe_for_public_sharing: report.verdict.is_safe_for_public_sharing(),
        findings: report.findings,
    })
}

/// JSON form of [`project_privacy`], matching `ProjectPrivacyResponse`.
pub fn project_privacy_json(project_path: &str) -> Result<String, String> {
    let privacy = project_privacy(project_path)?;
    serde_json::to_string(&privacy).map_err(|e| format!("Could not serialize project privacy: {e}"))
}

/// Collect a fail-closed privacy verdict for every file in a resolved project.
fn privacy_report(paths: &ResolvedProject, include_outputs: bool) -> Result<PrivacyReport, String> {
    let files = collect_project_files(paths, include_outputs)?;
    let mut findings: Vec<PrivacyFinding> = Vec::new();
    let mut verdicts = Vec::with_capacity(files.len());
    for path in &files {
        let scan = scan_file(path);
        verdicts.push(scan.verdict);
        if !scan.fields.is_empty() {
            findings.push(PrivacyFinding {
                file: rel_posix(path, &paths.root),
                fields: scan.fields,
            });
        }
    }
    Ok(PrivacyReport {
        files_scanned: files.len(),
        verdict: aggregate_verdicts(verdicts),
        findings,
    })
}

/// Collect every file that belongs to a project bundle, refusing to follow
/// references that escape the project root (mirrors `project_files`).
fn collect_project_files(
    paths: &ResolvedProject,
    include_outputs: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    add_resolved_file(&mut files, &mut seen, paths.project_file.clone());

    for (path, label) in [
        (&paths.students, "students"),
        (&paths.layout, "layout"),
        (&paths.rules, "rules"),
    ] {
        let resolved = fs::canonicalize(path).map_err(|e| {
            format!(
                "Project reference \"{label}\" not found: {} ({e})",
                path.display()
            )
        })?;
        ensure_inside(&resolved, &paths.root, label)?;
        if !resolved.is_file() {
            return Err(format!(
                "Project reference \"{label}\" is not a file: {}",
                resolved.display()
            ));
        }
        add_resolved_file(&mut files, &mut seen, resolved);
    }

    if let Some(dir) = &paths.history_dir {
        if dir.is_dir() {
            add_directory_files(&mut files, &mut seen, dir, &paths.root, "history_dir")?;
        }
    }
    if include_outputs && paths.outputs_dir.is_dir() {
        add_directory_files(
            &mut files,
            &mut seen,
            &paths.outputs_dir,
            &paths.root,
            "outputs_dir",
        )?;
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
    let resolved_dir = fs::canonicalize(dir).map_err(|e| {
        format!(
            "Project reference \"{label}\" is not a directory: {} ({e})",
            dir.display()
        )
    })?;
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

/// Inspect one supported text file using the central schema privacy policy.
/// Any input that cannot be fully inspected is explicitly `Indeterminate`;
/// scan failures must never collapse to an empty (apparently safe) finding set.
fn scan_file(path: &Path) -> FilePrivacyScan {
    scan_file_with_limit(path, MAX_BUNDLE_FILE_BYTES)
}

fn scan_file_with_limit(path: &Path, max_bytes: u64) -> FilePrivacyScan {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return FilePrivacyScan::indeterminate(),
    };
    if !metadata.is_file() || metadata.len() > max_bytes {
        return FilePrivacyScan::indeterminate();
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return FilePrivacyScan::indeterminate(),
    };
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return FilePrivacyScan::indeterminate();
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return FilePrivacyScan::indeterminate(),
    };
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "csv" {
        return match csv_fields(&text) {
            Ok(fields) => {
                let mut fields: Vec<String> = fields
                    .into_iter()
                    .filter(|field| is_sensitive_key(field))
                    .collect();
                fields.sort();
                fields.dedup();
                FilePrivacyScan {
                    verdict: classify_findings(true, !fields.is_empty()),
                    fields,
                }
            }
            Err(()) => FilePrivacyScan::indeterminate(),
        };
    }
    if extension != "json" {
        return FilePrivacyScan::indeterminate();
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => {
            let schema_findings = scan_document(&value);
            let verdict = classify_scan(true, &schema_findings);
            let mut fields: Vec<String> = schema_findings
                .into_iter()
                .map(|finding| finding.key)
                .collect();
            fields.sort();
            fields.dedup();
            FilePrivacyScan { verdict, fields }
        }
        Err(_) => FilePrivacyScan::indeterminate(),
    }
}

/// Parse the first CSV record, honoring RFC 4180 quoting (including a newline
/// inside a quoted header). Malformed quoting is an incomplete privacy scan.
fn csv_fields(text: &str) -> Result<Vec<String>, ()> {
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut just_closed_quote = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        current.push('"');
                    } else {
                        in_quotes = false;
                        just_closed_quote = true;
                    }
                } else if current.trim().is_empty() && !just_closed_quote {
                    current.clear();
                    in_quotes = true;
                } else {
                    return Err(());
                }
            }
            ',' => {
                if in_quotes {
                    current.push(ch);
                } else {
                    fields.push(current.trim().to_string());
                    current.clear();
                    just_closed_quote = false;
                }
            }
            '\r' | '\n' if !in_quotes => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                fields.push(current.trim().to_string());
                return Ok(fields);
            }
            ch if just_closed_quote && !ch.is_whitespace() => return Err(()),
            _ => current.push(ch),
        }
    }
    if in_quotes {
        return Err(());
    }
    fields.push(current.trim().to_string());
    Ok(fields)
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
    let privacy = privacy_report(&paths, true)?;
    let manifest = json!({
        "kind": "seattrellis_project_bundle",
        "format_version": BUNDLE_FORMAT_VERSION,
        "created_at": now_iso(),
        "project_file": rel_posix(&paths.project_file, root),
        "include_outputs": true,
        "files": files.iter().map(|path| rel_posix(path, root)).collect::<Vec<_>>(),
        "privacy": {
            "files_scanned": privacy.files_scanned,
            "verdict": privacy.verdict,
            "safe_for_public_sharing": privacy.verdict.is_safe_for_public_sharing(),
            "findings": privacy.findings
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
pub fn restore_project_bundle(
    bundle_bytes: &[u8],
    output_dir: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
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
    fs::create_dir_all(&parent).map_err(|e| {
        format!(
            "Could not create restore destination {}: {e}",
            parent.display()
        )
    })?;
    let parent_abs = fs::canonicalize(&parent).map_err(|e| {
        format!(
            "Restore destination unavailable: {} ({e})",
            parent.display()
        )
    })?;
    let dest_abs = parent_abs.join(&dest_name);

    if dest_abs.exists() {
        let non_empty = fs::read_dir(&dest_abs)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true);
        if non_empty && !overwrite {
            return Err(format!(
                "Restore destination is not empty: {}",
                dest_abs.display()
            ));
        }
    }

    // Staging is named with the destination so parallel restores (and other
    // tests) never collide in the shared parent directory.
    let staging = parent_abs.join(format!(
        ".seattrellis-restore-{}-{}",
        dest_name.to_string_lossy(),
        now_nanos()
    ));
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

    // Publish atomically through the journaled transaction layer: the
    // staging directory is adopted as the transaction temp, the previous
    // destination (if any) moves to a unique backup, and the batch commits
    // only after the final path re-validates. A failure or crash never
    // leaves a partial destination.
    let journal_dir = parent_abs.join(crate::transaction::JOURNAL_DIR_NAME);
    crate::transaction::recover_leftover_transactions_with_root(&journal_dir, &parent_abs)?;
    let mut transaction =
        crate::transaction::FileTransaction::begin_with_root(&journal_dir, &parent_abs)?;
    if dest_abs.exists() {
        transaction.stage_directory(&dest_abs, &staging)?;
    } else {
        transaction.stage_new_directory(&dest_abs, &staging)?;
    }
    transaction
        .commit_with_receipt(|path| {
            let project_file = path.join(&project_name);
            load_project(&project_file).map(|_| ())
        })
        .map_err(|error| format!("Could not publish restored project: {error}"))?;

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

// ---------------------------------------------------------------------------
// Artifact compare + restore (M2 parity, ledger A.2/A.3)
// ---------------------------------------------------------------------------

/// The comparable view of a project artifact: kind, metadata and the
/// assignment map, extracted from a snapshot / candidate_set / rotation_plan
/// document (mirrors Python's `_snapshot_for_artifact`).
struct ArtifactSnapshotView {
    kind: String,
    created_at: Option<String>,
    /// (student_key, seat_id) pairs, ordered as in the document.
    assignments: Vec<(String, String)>,
    student_count: usize,
    enabled_seat_count: usize,
    layout: Option<Value>,
    rules: Option<Value>,
    solver_status: Option<String>,
}

/// Extract the snapshot view from any supported artifact document.
fn artifact_snapshot_view(path: &Path) -> Result<ArtifactSnapshotView, String> {
    let bytes = read_file_capped(path, MAX_BUNDLE_FILE_BYTES)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid JSON in {}: {e}", path.display()))?;
    let kind = artifact_kind(&value);
    let created_at = value
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Locate the inner snapshot document by kind.
    let snapshot: &Value = match kind.as_str() {
        "candidate_set" => {
            let recommended = value
                .get("recommended_candidate_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            value
                .get("candidates")
                .and_then(Value::as_array)
                .and_then(|candidates| {
                    candidates.iter().find(|candidate| {
                        candidate.get("candidate_id").and_then(Value::as_str) == Some(recommended)
                    })
                })
                .or_else(|| {
                    value
                        .get("candidates")
                        .and_then(Value::as_array)
                        .and_then(|candidates| candidates.first())
                })
                .and_then(|candidate| candidate.get("snapshot"))
                .ok_or_else(|| format!("Candidate set has no snapshot: {}", path.display()))?
        }
        "rotation_plan" => value
            .get("periods")
            .and_then(Value::as_array)
            .and_then(|periods| periods.first())
            .and_then(|period| period.get("snapshot"))
            .ok_or_else(|| format!("Rotation plan has no period snapshot: {}", path.display()))?,
        "snapshot" => &value,
        _ => {
            return Err(format!(
                "Unsupported project artifact kind for comparison: {kind}"
            ))
        }
    };

    let mut assignments: Vec<(String, String)> = Vec::new();
    if let Some(list) = snapshot.get("assignments").and_then(Value::as_array) {
        for assignment in list {
            if let (Some(student), Some(seat)) = (
                assignment.get("student_key").and_then(Value::as_str),
                assignment.get("seat_id").and_then(Value::as_str),
            ) {
                assignments.push((student.to_string(), seat.to_string()));
            }
        }
    }
    let student_count = snapshot
        .get("students")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let enabled_seat_count = snapshot
        .get("layout")
        .and_then(|layout| layout.get("seats"))
        .and_then(Value::as_array)
        .map(|seats| {
            seats
                .iter()
                .filter(|seat| seat.get("enabled").and_then(Value::as_bool).unwrap_or(true))
                .count()
        })
        .unwrap_or(0);
    let solver_status = snapshot
        .get("solver_status")
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(ArtifactSnapshotView {
        kind,
        created_at,
        assignments,
        student_count,
        enabled_seat_count,
        layout: snapshot.get("layout").cloned(),
        rules: snapshot.get("rules").cloned(),
        solver_status,
    })
}

/// Compare two project artifacts (ledger A.2): summaries plus an
/// assignment/roster/layout/rules diff. Never returns student data — only
/// anonymized `student-N` references and seat ids.
pub fn compare_artifacts_json(
    project_path: &str,
    artifact_path: &str,
    compare_to_path: &str,
) -> Result<String, String> {
    let _ = resolve_project(Path::new(project_path), false)?;
    let left_path = Path::new(artifact_path);
    let right_path = Path::new(compare_to_path);
    if left_path == right_path {
        return Err("An artifact cannot be compared with itself.".to_string());
    }
    let left = artifact_snapshot_view(left_path)?;
    let right = artifact_snapshot_view(right_path)?;

    let summary = |view: &ArtifactSnapshotView, path: &Path| {
        json!({
            "name": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "path": path.to_string_lossy(),
            "kind": view.kind,
            "created_at": view.created_at,
            "student_count": view.student_count,
            "assignment_count": view.assignments.len(),
            "enabled_seat_count": view.enabled_seat_count,
            "solver_status": view.solver_status,
        })
    };

    let left_map: std::collections::HashMap<&str, &str> = left
        .assignments
        .iter()
        .map(|(student, seat)| (student.as_str(), seat.as_str()))
        .collect();
    let right_map: std::collections::HashMap<&str, &str> = right
        .assignments
        .iter()
        .map(|(student, seat)| (student.as_str(), seat.as_str()))
        .collect();
    let mut all_students: Vec<&str> = left_map.keys().chain(right_map.keys()).copied().collect();
    all_students.sort_unstable();
    all_students.dedup();

    let mut assignment_changes = 0;
    let mut roster_added = 0;
    let mut roster_removed = 0;
    let mut details: Vec<Value> = Vec::new();
    for (index, student) in all_students.iter().enumerate() {
        let before = left_map.get(student).copied();
        let after = right_map.get(student).copied();
        if before == after {
            continue;
        }
        assignment_changes += 1;
        if before.is_none() {
            roster_added += 1;
        }
        if after.is_none() {
            roster_removed += 1;
        }
        let change = if before.is_none() {
            "seated"
        } else if after.is_none() {
            "unseated"
        } else {
            "moved"
        };
        details.push(json!({
            "student_ref": format!("student-{}", index + 1),
            "change": change,
            "before_seat_id": before,
            "after_seat_id": after,
        }));
    }

    let diff = json!({
        "assignment_changes": assignment_changes,
        "roster_added": roster_added,
        "roster_removed": roster_removed,
        "layout_changed": left.layout != right.layout,
        "rules_changed": left.rules != right.rules,
        "solver_status_changed": left.solver_status != right.solver_status,
        "assignment_details": details,
    });

    let response = json!({
        "api_version": "1",
        "left": summary(&left, left_path),
        "right": summary(&right, right_path),
        "diff": diff,
    });
    serde_json::to_string(&response)
        .map_err(|e| format!("Could not serialize compare response: {e}"))
}

/// Restore an artifact as a new output snapshot without overwriting history
/// (ledger A.3). Rotation plans are rejected (pick a period snapshot first),
/// mirroring Python.
pub fn restore_artifact_json(project_path: &str, artifact_path: &str) -> Result<String, String> {
    let (_, paths) = resolve_project(Path::new(project_path), false)?;
    let source_path = Path::new(artifact_path);
    let view = artifact_snapshot_view(source_path)?;
    if view.kind == "rotation_plan" {
        return Err(
            "Select a snapshot or candidate set inside a rotation plan before restoring it."
                .to_string(),
        );
    }
    fs::create_dir_all(&paths.outputs_dir)
        .map_err(|e| format!("Could not create outputs directory: {e}"))?;

    // Rebuild the snapshot document with restoration metadata.
    let bytes = read_file_capped(source_path, MAX_BUNDLE_FILE_BYTES)?;
    let mut document: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid JSON in {}: {e}", source_path.display()))?;
    let source_name = source_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(metadata) = document.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.insert("restored_from".to_string(), json!(source_name));
    } else if let Some(object) = document.as_object_mut() {
        object.insert(
            "metadata".to_string(),
            json!({ "restored_from": source_name }),
        );
    }
    if let Some(object) = document.as_object_mut() {
        object.insert(
            "restored_at".to_string(),
            json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0)),
        );
    }

    // restored-{stem}.snapshot.json, never overwriting an existing file.
    let stem = source_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    let clean_stem = stem.strip_suffix(".snapshot").unwrap_or(&stem);
    let mut target = paths
        .outputs_dir
        .join(format!("restored-{clean_stem}.snapshot.json"));
    let mut index = 2;
    while target.exists() {
        target = paths
            .outputs_dir
            .join(format!("restored-{clean_stem}-{index}.snapshot.json"));
        index += 1;
    }
    fs::write(
        &target,
        serde_json::to_vec(&document)
            .map_err(|e| format!("Could not serialize restored artifact: {e}"))?,
    )
    .map_err(|e| {
        format!(
            "Could not write restored artifact {}: {e}",
            target.display()
        )
    })?;

    let response = json!({
        "api_version": "1",
        "project_path": paths.project_file.to_string_lossy(),
        "source_artifact": source_path.to_string_lossy(),
        "restored_artifact": target.to_string_lossy(),
    });
    serde_json::to_string(&response)
        .map_err(|e| format!("Could not serialize restore response: {e}"))
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
        return Err(format!(
            "Unsupported project bundle format_version {version}."
        ));
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
    Ok(Manifest {
        project_file,
        files,
    })
}

fn validated_entries(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<Vec<ValidatedEntry>, String> {
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

// ---------------------------------------------------------------------------
// File + time helpers
// ---------------------------------------------------------------------------

fn read_file_capped(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
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
    use crate::migration::migration_apply_json;

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
        write_project(
            &root.join(".hidden"),
            "Hidden",
            "student_ref,gender\n",
            false,
        );

        let canonical_root = fs::canonicalize(&root).unwrap();
        let json = list_projects_json(root.to_str().unwrap(), 100).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["api_version"], "1");
        assert_eq!(value["root"], canonical_root.to_str().unwrap());
        let projects = value["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        let names: Vec<&str> = projects
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"Alpha"));
        assert!(names.contains(&"Beta"));
        for project in projects {
            assert!(project["path"]
                .as_str()
                .unwrap()
                .contains("project.seattrellis.json"));
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
        assert!(warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("history directory")));
        assert_eq!(value["history"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn privacy_detects_sensitive_fields() {
        let root = temp_root("privacy-sensitive");
        let project_file = write_project(
            &root,
            "Sensitive",
            "student_id,name,gender,height_cm,score,vision,needs,notes\nSTU001,Alice,F,168,92,poor,front,\n",
            false,
        );
        let json = project_privacy_json(project_file.to_str().unwrap()).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["verdict"], "Unsafe");
        assert_eq!(value["safe_for_public_sharing"], false);
        let findings = value["findings"].as_array().unwrap();
        let students = findings.iter().find(|f| f["file"] == "students.csv");
        assert!(
            students.is_some(),
            "expected a students.csv finding, got {findings:?}"
        );
        let fields: Vec<&str> = students.unwrap()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        assert!(fields.contains(&"student_id"));
        assert!(fields.contains(&"height_cm"));
        assert!(fields.contains(&"score"));
        assert!(fields.contains(&"needs"));
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
        assert_eq!(value["verdict"], "Safe");
        assert_eq!(value["safe_for_public_sharing"], true);
        assert_eq!(value["files_scanned"], 4); // project + students + layout + rules
        assert_eq!(value["findings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn privacy_scan_failures_are_indeterminate_not_empty_safe_results() {
        let root = temp_root("privacy-indeterminate-files");

        let missing = root.join("missing.json");
        assert_eq!(scan_file(&missing).verdict, PrivacyVerdict::Indeterminate);

        let oversized = root.join("oversized.json");
        fs::write(&oversized, b"{}").unwrap();
        assert_eq!(
            scan_file_with_limit(&oversized, 1).verdict,
            PrivacyVerdict::Indeterminate
        );

        let non_utf8 = root.join("non-utf8.json");
        fs::write(&non_utf8, [0xff, 0xfe]).unwrap();
        assert_eq!(scan_file(&non_utf8).verdict, PrivacyVerdict::Indeterminate);

        let unknown = root.join("unknown.txt");
        fs::write(&unknown, b"{}").unwrap();
        assert_eq!(scan_file(&unknown).verdict, PrivacyVerdict::Indeterminate);

        let invalid_json = root.join("invalid.json");
        fs::write(&invalid_json, b"{").unwrap();
        assert_eq!(
            scan_file(&invalid_json).verdict,
            PrivacyVerdict::Indeterminate
        );

        let invalid_csv = root.join("invalid.csv");
        fs::write(&invalid_csv, b"\"unclosed").unwrap();
        assert_eq!(
            scan_file(&invalid_csv).verdict,
            PrivacyVerdict::Indeterminate
        );
    }

    #[test]
    fn indeterminate_project_is_never_safe_in_response_or_bundle_manifest() {
        let root = temp_root("privacy-indeterminate-project");
        let project_file = root.join("project.seattrellis.json");
        fs::write(
            &project_file,
            r#"{"kind":"seattrellis_project","schema_version":1,"students":"students.csv","layout":"classroom.json","rules":"rules.json","outputs_dir":"outputs"}"#,
        )
        .unwrap();
        fs::write(root.join("students.csv"), "student_ref,gender\n").unwrap();
        fs::write(root.join("classroom.json"), r#"{"rows":5,"cols":6}"#).unwrap();
        fs::write(root.join("rules.json"), r#"{"mode":"default"}"#).unwrap();
        fs::create_dir_all(root.join("outputs")).unwrap();
        // A format with no complete scanner cannot prove the project safe.
        fs::write(root.join("outputs/result.bin"), b"opaque").unwrap();

        let response = project_privacy_json(project_file.to_str().unwrap()).unwrap();
        let response: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["verdict"], "Indeterminate");
        assert_eq!(response["safe_for_public_sharing"], false);
        assert_eq!(response["findings"], json!([]));

        let bundle = pack_project(project_file.to_str().unwrap()).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bundle.as_slice())).unwrap();
        let mut manifest_text = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest_text)
            .unwrap();
        let manifest: Value = serde_json::from_str(&manifest_text).unwrap();
        assert_eq!(manifest["privacy"]["verdict"], "Indeterminate");
        assert_eq!(manifest["privacy"]["safe_for_public_sharing"], false);
    }

    #[test]
    fn one_hundred_open_save_pack_restore_cycles_are_stable() {
        // §11.9: 100 consecutive project open/save (migrate) plus pack/
        // restore cycles; the workspace must stay valid throughout and the
        // final artifact must still compile into a solve request.
        let root = temp_root("long-run-project");
        let project_file = root.join("project.seattrellis.json");
        fs::write(
            &project_file,
            r#"{"kind":"seattrellis_project","schema_version":1,"name":"LongRun","students":"students.csv","layout":"classroom.json","rules":"rules.json","outputs_dir":"outputs"}"#,
        )
        .unwrap();
        fs::write(root.join("students.csv"), "id,name\n1,A\n2,B\n3,C\n4,D\n").unwrap();
        fs::write(
            root.join("classroom.json"),
            r#"{"layout_id":"l","name":"Room","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}}"#,
        )
        .unwrap();
        fs::write(root.join("rules.json"), r#"{"seed":7,"soft":{}}"#).unwrap();

        let mut last_restored = project_file.clone();
        for cycle in 0..100 {
            // Open: load and validate the project document.
            let (document, _) = load_project_document(&last_restored).unwrap();
            assert_eq!(document["kind"], "seattrellis_project");

            // Save: apply the migration in place (journaled, with backup).
            let json = migration_apply_json(&last_restored.display().to_string(), true).unwrap();
            let value: Value = serde_json::from_str(&json).unwrap();
            assert!(value["backup_path"].as_str().is_some());

            // Pack + restore into a fresh directory (journaled directory
            // transaction); the restored project must open again.
            let bundle = pack_project_json(&last_restored.display().to_string()).unwrap();
            let dest = temp_root(&format!("long-run-restore-{cycle}"));
            let restored_json = restore_project_json(&bundle, dest.to_str().unwrap()).unwrap();
            let restored: Value = serde_json::from_str(&restored_json).unwrap();
            last_restored = PathBuf::from(restored["project_path"].as_str().unwrap());
            let (restored_document, _) = load_project_document(&last_restored).unwrap();
            assert_eq!(restored_document["kind"], "seattrellis_project");
        }

        // The final workspace still compiles into a valid solve request.
        assert!(build_project_solve_request(&project_file).is_ok());
        assert!(build_project_solve_request(&last_restored).is_ok());
    }

    #[test]
    fn pack_restore_roundtrip() {
        let root = temp_root("roundtrip");
        let project_file =
            write_project(&root, "Roundtrip", "student_ref,gender\nA,F\nB,M\n", true);
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
        assert!(restored_project
            .parent()
            .unwrap()
            .join("students.csv")
            .is_file());
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
            (
                "manifest.json",
                &simple_manifest(&["../evil.txt"], "../evil.txt"),
                0o100644,
            ),
            ("../evil.txt", b"boom", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("Unsafe project bundle path"), "got: {err}");
    }

    #[test]
    fn workspace_request_builder_rejects_unknown_rules_and_bad_adjacency() {
        // The workspace compiler mirrors Python's extra="forbid" rule models:
        // unknown rule kinds / soft objectives and bad adjacency references
        // must be rejected instead of silently dropped (a dropped constraint
        // would change the plan the teacher asked for).
        let root = temp_root("workspace-strict");
        let project_file = root.join("project.json");
        fs::write(
            &project_file,
            r#"{"kind":"seattrellis_project","schema_version":1,"students":"students.csv","layout":"classroom.json","rules":"rules.json","outputs_dir":"outputs"}"#,
        )
        .unwrap();
        fs::write(root.join("students.csv"), "id,name\n1,A\n2,B\n3,C\n4,D\n").unwrap();
        let layout = r#"{"layout_id":"l","name":"Room","seats":[
            {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
            {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
            {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
            {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
        ],"adjacency":{"edges":[["R1C1","R1C2"]]}}"#;
        fs::write(root.join("classroom.json"), layout).unwrap();

        // A valid workspace builds.
        fs::write(root.join("rules.json"), r#"{"seed":7,"soft":{}}"#).unwrap();
        assert!(build_project_solve_request(&project_file).is_ok());

        // Unknown top-level key.
        fs::write(root.join("rules.json"), r#"{"mode":"default"}"#).unwrap();
        let err = build_project_solve_request(&project_file).unwrap_err();
        assert!(err.contains("unknown top-level key"), "got: {err}");

        // Unknown hard rule kind (Python HardRules extra="forbid").
        fs::write(
            root.join("rules.json"),
            r#"{"hard":{"teleport_students":[{"student":"1"}]}}"#,
        )
        .unwrap();
        let err = build_project_solve_request(&project_file).unwrap_err();
        assert!(err.contains("unknown hard rule kind"), "got: {err}");

        // Unknown soft objective (Python SoftRules extra="forbid").
        fs::write(
            root.join("rules.json"),
            r#"{"soft":{"magic_seating":{"enabled":true,"weight":5}}}"#,
        )
        .unwrap();
        let err = build_project_solve_request(&project_file).unwrap_err();
        assert!(err.contains("unknown soft objective"), "got: {err}");

        // Bad adjacency: a custom edge referencing an unknown seat must be
        // rejected, not silently dropped.
        let bad_layout = layout.replace(
            r#""edges":[["R1C1","R1C2"]]"#,
            r#""custom_edges":[["R1C1","R99C99"]]"#,
        );
        fs::write(root.join("classroom.json"), bad_layout).unwrap();
        fs::write(root.join("rules.json"), r#"{"seed":7,"soft":{}}"#).unwrap();
        let err = build_project_solve_request(&project_file).unwrap_err();
        assert!(err.contains("unknown seat_id"), "got: {err}");
    }

    #[test]
    fn restore_rejects_symlink_entry() {
        let dest = temp_root("restore-symlink");
        let zip =
            make_zip_with_symlink(&simple_manifest(&["link"], "link"), "link", "students.csv");
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("symlink"), "got: {err}");
    }

    #[test]
    fn restore_rejects_corrupt_zip() {
        let dest = temp_root("restore-corrupt");
        let err = restore_project_bundle(
            b"this is definitely not a zip archive",
            dest.to_str().unwrap(),
            false,
        )
        .unwrap_err();
        assert!(err.contains("Could not read project bundle"), "got: {err}");
    }

    #[test]
    fn restore_rejects_manifest_mismatch() {
        let dest = temp_root("restore-mismatch");
        // Manifest claims a.txt but the archive carries b.txt.
        let zip = make_zip(&[
            (
                "manifest.json",
                &simple_manifest(&["a.txt"], "a.txt"),
                0o100644,
            ),
            ("b.txt", b"x", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("does not match"), "got: {err}");
    }

    #[test]
    fn restore_rejects_nonempty_destination() {
        // Destination lives in its own parent so the transaction journal
        // anchor is isolated from other tests sharing the temp dir.
        let parent = temp_root("restore-nonempty-parent");
        let dest = parent.join("restored");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("existing.txt"), b"keep").unwrap();
        let zip = make_zip(&[
            (
                "manifest.json",
                &simple_manifest(&["proj.json"], "proj.json"),
                0o100644,
            ),
            ("proj.json", b"{}", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("not empty"), "got: {err}");
        // With overwrite, the same bundle passes the empty-destination check and
        // then fails validating the (invalid) project file. The journaled
        // transaction validates before publishing anything, so the existing
        // destination must be completely untouched (no partial restore).
        assert!(restore_project_bundle(&zip, dest.to_str().unwrap(), true).is_err());
        assert_eq!(fs::read(dest.join("existing.txt")).unwrap(), b"keep");
        // No staging leftovers from this destination's restore attempts.
        let dest_name = dest.file_name().unwrap().to_string_lossy().into_owned();
        let leftovers: Vec<_> = fs::read_dir(&parent)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".seattrellis-restore-") && name.contains(&dest_name))
            .collect();
        assert!(leftovers.is_empty(), "staging leftovers: {leftovers:?}");
    }

    #[test]
    fn restore_overwrite_publishes_atomically_and_keeps_backup() {
        // A valid bundle restored over a non-empty destination replaces the
        // whole tree atomically: no staging/journal leftovers, and the
        // previous destination survives as a transaction backup. The
        // destination lives in its own parent so the journal anchor is
        // isolated from other tests.
        let parent = temp_root("restore-overwrite-parent");
        let dest = parent.join("restored");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old.txt"), b"old").unwrap();
        let project = temp_root("restore-overwrite-src");
        fs::write(
            project.join("proj.json"),
            r#"{"kind":"seattrellis_project","schema_version":1,"name":"R","students":"students.csv","layout":"layout.json","rules":"rules.json"}"#,
        )
        .unwrap();
        fs::write(project.join("students.csv"), "id,name\n").unwrap();
        fs::write(project.join("layout.json"), r#"{"seats":[]}"#).unwrap();
        fs::write(project.join("rules.json"), r#"{}"#).unwrap();
        let bundle = pack_project(project.join("proj.json").to_str().unwrap()).unwrap();

        let restored = restore_project_bundle(&bundle, dest.to_str().unwrap(), true).unwrap();
        let restored_dir = restored.parent().unwrap();
        assert!(restored_dir.join("students.csv").is_file());
        assert!(!dest.join("old.txt").exists(), "old tree must be replaced");
        // The old destination is retained as a unique backup next to dest.
        let dest_name = dest.file_name().unwrap().to_string_lossy().into_owned();
        let backups: Vec<_> = fs::read_dir(&parent)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&format!(".{dest_name}.seattrellis-backup-")))
            .collect();
        assert_eq!(backups.len(), 1, "expected one backup: {backups:?}");
        // The journal dir (recovery anchor) may persist but must be empty.
        let journal_dir = parent.join(".seattrellis-transactions");
        if journal_dir.exists() {
            let remaining: Vec<_> = fs::read_dir(&journal_dir)
                .unwrap()
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            assert!(remaining.is_empty(), "journal leftovers: {remaining:?}");
        }
        let leftovers: Vec<_> = fs::read_dir(&parent)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".seattrellis-restore-") && name.contains(&dest_name))
            .collect();
        assert!(leftovers.is_empty(), "staging leftovers: {leftovers:?}");
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
        assert_eq!(
            default_bundle_name("/d/x.seattrellis.json").unwrap(),
            "x.seattrellis.zip"
        );
        assert_eq!(
            default_bundle_name("/d/x.project.json").unwrap(),
            "x.seattrellis.zip"
        );
        assert_eq!(
            default_bundle_name("/d/plain.json").unwrap(),
            "plain.seattrellis.zip"
        );
        assert_eq!(
            default_bundle_name("/d/weird.txt").unwrap(),
            "weird.txt.seattrellis.zip"
        );
    }

    #[test]
    fn iso_formatting_rounds_trip() {
        assert_eq!(iso_from_epoch_secs(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(
            iso_from_epoch_secs(1_752_900_000),
            "2025-07-19T04:40:00+00:00"
        );
        assert_eq!(
            iso_from_epoch_secs(1_780_272_000),
            "2026-06-01T00:00:00+00:00"
        );
    }
    #[test]
    fn restore_rejects_zip_bomb_expansion() {
        // A bundle whose uncompressed payload exceeds the total cap (500MB)
        // must be rejected *before* extraction: six 100MB files would expand
        // to 600MB on disk. The payload is all zeros so the archive itself
        // is tiny — the classic zip-bomb shape.
        let dest = temp_root("restore-zipbomb");
        let manifest = simple_manifest(
            &["a1.bin", "a2.bin", "a3.bin", "a4.bin", "a5.bin", "a6.bin"],
            "a1.bin",
        );
        let zeros = vec![0u8; 100 * 1024 * 1024];
        let zeros_ref: &[u8] = &zeros;
        let mut entries: Vec<(&str, &[u8], u32)> = vec![("manifest.json", &manifest, 0o100644)];
        for index in 1..=6 {
            entries.push((
                Box::leak(format!("a{index}.bin").into_boxed_str()),
                zeros_ref,
                0o100644,
            ));
        }
        let zip = make_zip(&entries);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("too large"), "got: {err}");
        assert!(
            !dest.exists()
                || fs::read_dir(dest)
                    .map(|mut it| it.next().is_none())
                    .unwrap_or(true),
            "nothing may be extracted from a rejected bundle"
        );
    }

    #[test]
    fn restore_rejects_symlink_to_absolute_target() {
        // A symlink whose target escapes the staging directory must be
        // rejected even though the entry name itself looks safe.
        let dest = temp_root("restore-symlink-abs");
        let zip = make_zip_with_symlink(&simple_manifest(&["link"], "link"), "link", "/etc/passwd");
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(err.contains("symlink"), "got: {err}");
    }

    #[test]
    fn restore_rejects_duplicate_entries() {
        let dest = temp_root("restore-dup");
        // Two entries with the same normalized name must be rejected.
        let manifest = simple_manifest(&["a.txt"], "a.txt");
        // A "./a.txt" entry is rejected by the path-safety layer (defense in
        // depth); if it ever slipped through, the duplicate check would catch
        // it. Either defense must stop the restore.
        let zip = make_zip(&[
            ("manifest.json", &manifest, 0o100644),
            ("a.txt", b"first", 0o100644),
            ("./a.txt", b"second", 0o100644),
        ]);
        let err = restore_project_bundle(&zip, dest.to_str().unwrap(), false).unwrap_err();
        assert!(
            err.contains("Unsafe") || err.contains("duplicate"),
            "got: {err}"
        );
    }

    // ---- M2 parity: artifact compare + restore (ledger A.2/A.3) ----

    const SNAPSHOT_A: &str = r#"{
        "kind": "snapshot",
        "created_at": "2026-08-09T00:00:00Z",
        "students": [
            {"student_id": "S1", "name": "Alice"},
            {"student_id": "S2", "name": "Bob"}
        ],
        "layout": {"layout_id": "l", "seats": [
            {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "zone": "front", "enabled": true},
            {"seat_id": "R1C2", "row": 1, "col": 2, "x": 2.0, "y": 1.0, "zone": "front", "enabled": true}
        ]},
        "rules": {"seed": 42},
        "assignments": [
            {"student_key": "S1", "student_name": "Alice", "seat_id": "R1C1"},
            {"student_key": "S2", "student_name": "Bob", "seat_id": "R1C2"}
        ],
        "solver_status": "FEASIBLE"
    }"#;

    const SNAPSHOT_B: &str = r#"{
        "kind": "snapshot",
        "created_at": "2026-08-09T01:00:00Z",
        "students": [
            {"student_id": "S1", "name": "Alice"},
            {"student_id": "S2", "name": "Bob"}
        ],
        "layout": {"layout_id": "l", "seats": [
            {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "zone": "front", "enabled": true},
            {"seat_id": "R1C2", "row": 1, "col": 2, "x": 2.0, "y": 1.0, "zone": "front", "enabled": true}
        ]},
        "rules": {"seed": 7},
        "assignments": [
            {"student_key": "S1", "student_name": "Alice", "seat_id": "R1C2"},
            {"student_key": "S3", "student_name": "Carol", "seat_id": "R1C1"}
        ],
        "solver_status": "OPTIMAL"
    }"#;

    #[test]
    fn compare_artifacts_reports_assignment_and_roster_diff() {
        let root = temp_root("compare");
        fs::write(
            root.join("project.json"),
            r#"{
            "kind": "seattrellis_project",
            "name": "Demo",
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "outputs_dir": "outputs"
        }"#,
        )
        .unwrap();
        fs::write(root.join("a.json"), SNAPSHOT_A).unwrap();
        fs::write(root.join("b.json"), SNAPSHOT_B).unwrap();

        let result = compare_artifacts_json(
            root.join("project.json").to_str().unwrap(),
            root.join("a.json").to_str().unwrap(),
            root.join("b.json").to_str().unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["left"]["kind"], "snapshot");
        assert_eq!(value["left"]["student_count"], 2);
        assert_eq!(value["right"]["assignment_count"], 2);
        let diff = &value["diff"];
        // S1 moved R1C1->R1C2, S2 unseated, S3 seated: 3 assignment changes,
        // 1 roster added, 1 roster removed.
        assert_eq!(diff["assignment_changes"], 3);
        assert_eq!(diff["roster_added"], 1);
        assert_eq!(diff["roster_removed"], 1);
        assert_eq!(diff["layout_changed"], false);
        assert_eq!(diff["rules_changed"], true);
        assert_eq!(diff["solver_status_changed"], true);
        let details = diff["assignment_details"].as_array().unwrap();
        assert!(details.iter().any(|d| d["change"] == "moved"));
        assert!(details.iter().any(|d| d["change"] == "seated"));
        assert!(details.iter().any(|d| d["change"] == "unseated"));
        // No raw student identifiers leak into the diff.
        let serialized = serde_json::to_string(&value).unwrap();
        assert!(!serialized.contains("Alice") && !serialized.contains("Carol"));
    }

    #[test]
    fn compare_rejects_self_comparison() {
        let root = temp_root("compare-self");
        fs::write(
            root.join("project.json"),
            r#"{
            "kind": "seattrellis_project",
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json"
        }"#,
        )
        .unwrap();
        fs::write(root.join("a.json"), SNAPSHOT_A).unwrap();
        let err = compare_artifacts_json(
            root.join("project.json").to_str().unwrap(),
            root.join("a.json").to_str().unwrap(),
            root.join("a.json").to_str().unwrap(),
        )
        .unwrap_err();
        assert!(err.contains("compared with itself"), "{err}");
    }

    #[test]
    fn restore_artifact_writes_output_snapshot_with_metadata() {
        let root = temp_root("restore-artifact");
        fs::write(
            root.join("project.json"),
            r#"{
            "kind": "seattrellis_project",
            "name": "Demo",
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "outputs_dir": "outputs"
        }"#,
        )
        .unwrap();
        fs::write(root.join("plan.snapshot.json"), SNAPSHOT_A).unwrap();

        let result = restore_artifact_json(
            root.join("project.json").to_str().unwrap(),
            root.join("plan.snapshot.json").to_str().unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&result).unwrap();
        let restored = value["restored_artifact"].as_str().unwrap();
        assert!(
            restored.ends_with("restored-plan.snapshot.json"),
            "{restored}"
        );
        assert!(Path::new(restored).is_file());

        let document: Value = serde_json::from_str(&fs::read_to_string(restored).unwrap()).unwrap();
        assert_eq!(document["metadata"]["restored_from"], "plan.snapshot.json");
        assert!(document["restored_at"].is_number());
        // The assignment content survived.
        assert_eq!(document["assignments"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn restore_rejects_rotation_plan_and_never_overwrites() {
        let root = temp_root("restore-rot");
        fs::write(
            root.join("project.json"),
            r#"{
            "kind": "seattrellis_project",
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "outputs_dir": "outputs"
        }"#,
        )
        .unwrap();
        fs::write(
            root.join("rot.json"),
            r#"{
            "kind": "rotation_plan",
            "periods": [{"period": 1, "label": "P1", "snapshot": {"assignments": []}}]
        }"#,
        )
        .unwrap();
        let err = restore_artifact_json(
            root.join("project.json").to_str().unwrap(),
            root.join("rot.json").to_str().unwrap(),
        )
        .unwrap_err();
        assert!(err.contains("rotation plan"), "{err}");

        // Restoring the same snapshot twice yields two distinct files.
        fs::write(root.join("plan.snapshot.json"), SNAPSHOT_A).unwrap();
        let first = restore_artifact_json(
            root.join("project.json").to_str().unwrap(),
            root.join("plan.snapshot.json").to_str().unwrap(),
        )
        .unwrap();
        let second = restore_artifact_json(
            root.join("project.json").to_str().unwrap(),
            root.join("plan.snapshot.json").to_str().unwrap(),
        )
        .unwrap();
        let first_path: Value = serde_json::from_str(&first).unwrap();
        let second_path: Value = serde_json::from_str(&second).unwrap();
        assert_ne!(
            first_path["restored_artifact"], second_path["restored_artifact"],
            "restore must never overwrite an existing snapshot"
        );
    }
}
