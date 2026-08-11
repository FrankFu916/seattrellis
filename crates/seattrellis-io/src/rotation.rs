//! Multi-period rotation and group-register domain module for the SeatTrellis
//! desktop backend.
//!
//! This is the self-contained Rust port of the Python rotation and
//! group-register pipeline (`src/seattrellis/api/handlers.py` — the
//! `project_rotation_*` and `project_group_register*` handlers plus the
//! on-disk artifact written by `service.compute_rotation_plan`). It exposes
//! JSON-in / JSON-out helpers that a loopback HTTP server can wire up with no
//! third-party filesystem or date crate:
//!
//! * [`rotation_save_json`] — write a `RotationPlan` to the project's
//!   `outputs/rotation-plan.json` atomically (a fresh `-2` / `-3` suffix is
//!   chosen instead of overwriting an existing artifact), returning the
//!   `ProjectRotationSaveResponse` shape.
//! * [`rotation_load_json`] — read the saved plan back, returning the
//!   `ProjectRotationLoadResponse` envelope with the full `rotation_plan`.
//! * [`group_register_preview_json`] — summarize one period's members grouped
//!   by seat row and by seat column, with member seat numbers.
//! * [`group_register_html_json`] / [`group_register_csv_json`] — render a
//!   downloadable printable register (inline-styled table / CSV guarded
//!   against formula injection) as raw bytes.
//! * [`group_register_save_json`] — persist a group register to
//!   `outputs/group-register.json`.
//!
//! JSON shapes match `clients/web/src/api/types.ts` (`snake_case`):
//! `ProjectRotationSaveResponse`, `ProjectRotationLoadResponse`, and the
//! register preview envelope (`api_version` / `project_path` / `artifact_path`
//! / `plan_name` / `period_count`) are produced field-for-field where this
//! module owns the data. Loading returns the plan document itself; turning it
//! into the editable `editor` / `period_editors` drafts is the wiring layer's
//! job (it owns the `EditorDraftStore`), mirroring `project_rotation_load`.
//!
//! Every project reference is resolved the same way as [`crate::projects`]:
//! paths are canonicalized and must stay inside the project root, and all
//! writes go through a same-directory temp file renamed into place.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value};

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// Canonical output file name for a saved rotation plan.
pub const ROTATION_PLAN_FILE: &str = "rotation-plan.json";
/// Canonical output file name for a persisted group register.
pub const GROUP_REGISTER_FILE: &str = "group-register.json";

/// Maximum accepted project file size, in bytes (8 MiB).
const MAX_PROJECT_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum accepted rotation-plan artifact size, in bytes (64 MiB).
const MAX_ROTATION_PLAN_BYTES: u64 = 64 * 1024 * 1024;
/// Upper bound on `-2`, `-3`, ... collision suffixes before a timestamped name
/// is used instead (defensive; a real project never hits this).
const MAX_OUTPUT_SUFFIX_ATTEMPTS: u32 = 10_000;

/// Maximum number of atomic-write temp-name collisions to retry before giving up.
const MAX_TEMP_WRITE_ATTEMPTS: u8 = 8;

#[cfg(unix)]
fn existing_permission_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    Some(metadata.permissions().mode())
}

#[cfg(not(unix))]
fn existing_permission_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn set_permission_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_permission_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    // Windows does not expose POSIX mode bits. The default permissions on the
    // newly-created temporary file are the closest portable equivalent.
    Ok(())
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// `ProjectRotationSaveResponse` (`clients/web/src/api/types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RotationSaveResponse {
    pub api_version: &'static str,
    pub project_path: String,
    pub output_path: String,
    pub period_count: usize,
    pub saved_at: String,
}

/// `ProjectRotationLoadResponse` envelope (`clients/web/src/api/types.ts`).
///
/// `editor` / `period_editors` are produced by the wiring layer from the
/// returned plan, so they are intentionally not part of this self-contained
/// module's response.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RotationLoadResponse {
    pub api_version: &'static str,
    pub project_path: String,
    pub artifact_path: String,
    pub rotation_plan: Value,
}

/// One member inside a row or column group of the register preview.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct GroupMember {
    pub student_key: String,
    pub student_name: String,
    pub seat_id: String,
    pub row: Option<i64>,
    pub column: Option<i64>,
}

/// Members seated in one classroom row (or all unseated members, `row: null`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RowGroup {
    pub row: Option<i64>,
    pub member_count: usize,
    pub members: Vec<GroupMember>,
}

/// Members seated in one classroom column (or all unseated members, `column: null`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ColumnGroup {
    pub column: Option<i64>,
    pub member_count: usize,
    pub members: Vec<GroupMember>,
}

/// Register preview for one selected period. Carries the
/// `ProjectGroupRegisterPreviewResponse` envelope fields plus the row / column
/// grouping for the selected period.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisterPreviewResponse {
    pub api_version: &'static str,
    pub project_path: String,
    pub artifact_path: String,
    pub plan_name: String,
    pub period_count: usize,
    pub period: i64,
    pub period_label: String,
    pub row_groups: Vec<RowGroup>,
    pub column_groups: Vec<ColumnGroup>,
}

/// Result of persisting a group register to the project's outputs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisterSaveResponse {
    pub api_version: &'static str,
    pub project_path: String,
    pub output_path: String,
    pub group_count: usize,
    pub saved_at: String,
}

// ---------------------------------------------------------------------------
// Parsed plan types (subset of the rotation-plan schema we operate on)
// ---------------------------------------------------------------------------

/// The parts of a `RotationPlan` the module reads for previews and registers.
#[derive(Debug, Clone, Deserialize)]
struct RotationPlanData {
    #[serde(default)]
    name: Option<String>,
    periods: Vec<RotationPeriodData>,
}

#[derive(Debug, Clone, Deserialize)]
struct RotationPeriodData {
    period: i64,
    label: String,
    snapshot: SnapshotData,
}

/// The parts of a `SeatingSnapshot` needed to group members by seat position.
/// Other snapshot fields (`solver_status`, `metrics`, ...) are ignored by serde.
#[derive(Debug, Clone, Deserialize)]
struct SnapshotData {
    assignments: Vec<AssignmentData>,
    #[serde(default)]
    students: Option<Vec<StudentData>>,
    #[serde(default)]
    layout: Option<LayoutData>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssignmentData {
    student_key: String,
    #[serde(default)]
    student_name: Option<String>,
    seat_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StudentData {
    #[serde(default)]
    student_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LayoutData {
    seats: Vec<SeatData>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeatData {
    seat_id: String,
    row: i64,
    col: i64,
}

/// Internal register member with a status derived from whether a seat position
/// could be recovered.
#[derive(Debug, Clone)]
struct RegisterMember {
    student_key: String,
    student_name: String,
    seat_id: String,
    row: Option<i64>,
    column: Option<i64>,
    status: String,
}

impl RegisterMember {
    fn into_group_member(self) -> GroupMember {
        GroupMember {
            student_key: self.student_key,
            student_name: self.student_name,
            seat_id: self.seat_id,
            row: self.row,
            column: self.column,
        }
    }
}

/// Members sharing one row or column key (used for both preview and register).
#[derive(Debug, Clone)]
struct MemberGroup {
    key: Option<i64>,
    members: Vec<RegisterMember>,
}

/// The project context loaded for one register operation.
struct LoadedPeriod {
    project_file: PathBuf,
    artifact_path: PathBuf,
    plan_name: String,
    period_count: usize,
    period: RotationPeriodData,
}

// ---------------------------------------------------------------------------
// Public API: save / load
// ---------------------------------------------------------------------------

/// Persist a `RotationPlan` JSON document to the project's outputs and return
/// the `ProjectRotationSaveResponse` JSON.
///
/// The plan is validated (object with a non-empty `periods` array where every
/// period has `period` / `label` / `snapshot.assignments`), stamped with a
/// `metadata.saved_at`, then written atomically. An existing
/// `rotation-plan.json` is never overwritten: the first available
/// `rotation-plan-2.json`, `rotation-plan-3.json`, ... suffix is used.
pub fn rotation_save_json(project_path: &str, rotation_plan_json: &str) -> Result<String, String> {
    let plan: Value = serde_json::from_str(rotation_plan_json)
        .map_err(|e| format!("Invalid rotation plan JSON: {e}"))?;
    validate_rotation_plan(&plan)?;
    // Stronger typed validation with clear per-period errors.
    let _plan: RotationPlanData =
        serde_json::from_value(plan.clone()).map_err(|e| format!("Invalid rotation plan: {e}"))?;

    let (project_file, project) = load_project_file(project_path)?;
    let root = parent_dir(&project_file);
    let outputs_dir = resolve_outputs_dir(&root, &project)?;
    let output_path = next_rotation_output_path(&outputs_dir);
    let saved_at = now_iso8601();
    let mut plan = plan;
    stamp_saved_at(&mut plan, &saved_at);
    atomic_write_json(&output_path, &plan)?;

    let response = RotationSaveResponse {
        api_version: "1",
        project_path: project_file.to_string_lossy().into_owned(),
        output_path: output_path.to_string_lossy().into_owned(),
        period_count: plan_period_count(&plan),
        saved_at,
    };
    serde_json::to_string(&response)
        .map_err(|e| format!("Could not serialize rotation save response: {e}"))
}

/// Read the saved `rotation-plan.json` and return the
/// `ProjectRotationLoadResponse` JSON envelope with the full plan document.
pub fn rotation_load_json(project_path: &str) -> Result<String, String> {
    let (project_file, project) = load_project_file(project_path)?;
    let root = parent_dir(&project_file);
    let outputs_dir = resolve_outputs_dir(&root, &project)?;
    let artifact_path = outputs_dir.join(ROTATION_PLAN_FILE);
    if !artifact_path.is_file() {
        return Err(format!(
            "No saved rotation plan found: {}",
            artifact_path.display()
        ));
    }
    let plan = read_rotation_plan(&artifact_path)?;

    let response = RotationLoadResponse {
        api_version: "1",
        project_path: project_file.to_string_lossy().into_owned(),
        artifact_path: artifact_path.to_string_lossy().into_owned(),
        rotation_plan: plan,
    };
    serde_json::to_string(&response)
        .map_err(|e| format!("Could not serialize rotation load response: {e}"))
}

// ---------------------------------------------------------------------------
// Public API: group register
// ---------------------------------------------------------------------------

/// Summarize one rotation period's members grouped by seat row and by seat
/// column, returning the register-preview JSON (`snake_case`).
///
/// `period_index` is the 1-based period number used in the plan
/// (`RotationPeriod.period`, matching `clients/web/src/api/types.ts`).
pub fn group_register_preview_json(
    project_path: &str,
    period_index: i64,
) -> Result<String, String> {
    let LoadedPeriod {
        project_file,
        artifact_path,
        plan_name,
        period_count,
        period,
    } = load_period(project_path, period_index)?;
    let (row_groups, column_groups) = build_preview(&period.snapshot);

    let response = RegisterPreviewResponse {
        api_version: "1",
        project_path: project_file.to_string_lossy().into_owned(),
        artifact_path: artifact_path.to_string_lossy().into_owned(),
        plan_name,
        period_count,
        period: period.period,
        period_label: period.label.clone(),
        row_groups,
        column_groups,
    };
    serde_json::to_string(&response)
        .map_err(|e| format!("Could not serialize group register preview: {e}"))
}

/// Render a printable HTML register (inline-styled table) for one rotation
/// period, returning the raw bytes. Cell values and the plan name are HTML
/// escaped so names cannot inject markup.
pub fn group_register_html_json(project_path: &str, period_index: i64) -> Result<Vec<u8>, String> {
    let LoadedPeriod {
        plan_name, period, ..
    } = load_period(project_path, period_index)?;
    let rows = register_rows(&period);
    let html = render_register_html(&plan_name, &period.label, &rows);
    Ok(html.into_bytes())
}

/// Render a CSV register for one rotation period, returning the raw bytes.
/// Cells that would start a spreadsheet formula (`=`, `+`, `@`) are prefixed
/// with a single quote so opening the file cannot execute them. The output is
/// UTF-8 with a BOM (`utf-8-sig`) for Excel compatibility.
pub fn group_register_csv_json(project_path: &str, period_index: i64) -> Result<Vec<u8>, String> {
    let LoadedPeriod { period, .. } = load_period(project_path, period_index)?;
    let rows = register_rows(&period);
    Ok(render_register_csv(&rows))
}

/// Persist a group register to `outputs/group-register.json` (atomically) and
/// return the `RegisterSaveResponse` JSON. The groups payload may be a JSON
/// array or an object with a `groups` array.
pub fn group_register_save_json(project_path: &str, groups_json: &str) -> Result<String, String> {
    let groups: Value =
        serde_json::from_str(groups_json).map_err(|e| format!("Invalid groups JSON: {e}"))?;
    let group_count = count_groups(&groups)?;

    let (project_file, project) = load_project_file(project_path)?;
    let root = parent_dir(&project_file);
    let outputs_dir = resolve_outputs_dir(&root, &project)?;
    let output_path = outputs_dir.join(GROUP_REGISTER_FILE);
    let saved_at = now_iso8601();
    atomic_write_json(&output_path, &groups)?;

    let response = RegisterSaveResponse {
        api_version: "1",
        project_path: project_file.to_string_lossy().into_owned(),
        output_path: output_path.to_string_lossy().into_owned(),
        group_count,
        saved_at,
    };
    serde_json::to_string(&response)
        .map_err(|e| format!("Could not serialize group register save response: {e}"))
}

// ---------------------------------------------------------------------------
// Plan helpers
// ---------------------------------------------------------------------------

/// Validate the high-level shape of a rotation plan (`object` with a non-empty
/// `periods` array).
fn validate_rotation_plan(value: &Value) -> Result<(), String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "Invalid rotation plan: not a JSON object.".to_string())?;
    let periods = obj
        .get("periods")
        .and_then(Value::as_array)
        .ok_or_else(|| "Invalid rotation plan: missing \"periods\" array.".to_string())?;
    if periods.is_empty() {
        return Err(
            "Invalid rotation plan: \"periods\" must contain at least one period.".to_string(),
        );
    }
    Ok(())
}

/// Number of periods in a validated plan value.
fn plan_period_count(plan: &Value) -> usize {
    plan.get("periods")
        .and_then(Value::as_array)
        .map(|periods| periods.len())
        .unwrap_or(0)
}

/// Stamp `metadata.saved_at` / `metadata.saved_from` into a plan so the on-disk
/// artifact records when and how it was persisted (mirrors the Python handler).
fn stamp_saved_at(plan: &mut Value, saved_at: &str) {
    if let Some(obj) = plan.as_object_mut() {
        let metadata = obj
            .entry("metadata".to_string())
            .or_insert_with(|| Value::Object(JsonMap::new()));
        if let Value::Object(map) = metadata {
            map.insert("saved_at".to_string(), Value::String(saved_at.to_string()));
            map.insert(
                "saved_from".to_string(),
                Value::String("seattrellis_app".to_string()),
            );
        }
    }
}

/// Read and validate a saved rotation plan artifact.
fn read_rotation_plan(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("Could not read rotation plan {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_ROTATION_PLAN_BYTES {
        return Err(format!("Rotation plan too large: {}", path.display()));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid rotation plan file {}: {e}", path.display()))?;
    validate_rotation_plan(&value)?;
    Ok(value)
}

/// Locate one period by its 1-based period number, or a clear out-of-range error.
fn find_period(plan: &RotationPlanData, period_index: i64) -> Result<&RotationPeriodData, String> {
    if plan.periods.is_empty() {
        return Err("Rotation plan contains no periods.".to_string());
    }
    let highest = plan
        .periods
        .iter()
        .map(|period| period.period)
        .max()
        .unwrap_or(0);
    plan.periods
        .iter()
        .find(|period| period.period == period_index)
        .ok_or_else(|| {
            format!(
                "Rotation period {period_index} is out of range (plan has periods 1..={highest})."
            )
        })
}

/// Resolve a project + selected period for the register endpoints.
fn load_period(project_path: &str, period_index: i64) -> Result<LoadedPeriod, String> {
    let (project_file, project) = load_project_file(project_path)?;
    let root = parent_dir(&project_file);
    let outputs_dir = resolve_outputs_dir(&root, &project)?;
    let artifact_path = outputs_dir.join(ROTATION_PLAN_FILE);
    if !artifact_path.is_file() {
        return Err(format!(
            "No saved rotation plan found: {}",
            artifact_path.display()
        ));
    }
    let plan = read_rotation_plan(&artifact_path)?;
    let plan_data: RotationPlanData =
        serde_json::from_value(plan).map_err(|e| format!("Invalid saved rotation plan: {e}"))?;
    let plan_name = plan_data.name.clone().unwrap_or_default();
    let period_count = plan_data.periods.len();
    let period = find_period(&plan_data, period_index)?.clone();
    Ok(LoadedPeriod {
        project_file,
        artifact_path,
        plan_name,
        period_count,
        period,
    })
}

/// Count groups in a register payload (array or `{"groups": [...]}`).
fn count_groups(value: &Value) -> Result<usize, String> {
    if let Some(array) = value.as_array() {
        return Ok(array.len());
    }
    if let Some(groups) = value.get("groups").and_then(Value::as_array) {
        return Ok(groups.len());
    }
    Err("Invalid groups: expected an array or an object with a \"groups\" array.".to_string())
}

// ---------------------------------------------------------------------------
// Register preview / rendering
// ---------------------------------------------------------------------------

/// Build the row and column groupings for one period's snapshot.
fn build_preview(snapshot: &SnapshotData) -> (Vec<RowGroup>, Vec<ColumnGroup>) {
    let members = build_register_members(snapshot);
    let row_groups = group_by_row(&members)
        .into_iter()
        .map(|group| RowGroup {
            row: group.key,
            member_count: group.members.len(),
            members: group
                .members
                .into_iter()
                .map(RegisterMember::into_group_member)
                .collect(),
        })
        .collect();
    let column_groups = group_by_column(&members)
        .into_iter()
        .map(|group| ColumnGroup {
            column: group.key,
            member_count: group.members.len(),
            members: group
                .members
                .into_iter()
                .map(RegisterMember::into_group_member)
                .collect(),
        })
        .collect();
    (row_groups, column_groups)
}

/// The row-based register view: one `[period, group, student, seat, status]`
/// row per member, grouped by seat row (unseated members last).
fn register_rows(period: &RotationPeriodData) -> Vec<Vec<String>> {
    let members = build_register_members(&period.snapshot);
    let mut rows = Vec::new();
    for group in group_by_row(&members) {
        let group_label = match group.key {
            Some(row) => format!("第{row}行"),
            None => "未入座".to_string(),
        };
        for member in group.members {
            rows.push(vec![
                period.label.clone(),
                group_label.clone(),
                member.student_key,
                member.student_name,
                member.seat_id,
                status_label(&member.status),
            ]);
        }
    }
    rows
}

/// Collect every assigned member plus any roster student left unassigned, in
/// (row, column, key) display order with unseated members last.
fn build_register_members(snapshot: &SnapshotData) -> Vec<RegisterMember> {
    let mut members = Vec::new();
    let mut assigned: HashSet<String> = HashSet::new();

    for assignment in &snapshot.assignments {
        assigned.insert(assignment.student_key.clone());
        let position = seat_position(snapshot, &assignment.seat_id);
        members.push(RegisterMember {
            student_key: assignment.student_key.clone(),
            student_name: student_display_name(
                snapshot,
                &assignment.student_key,
                assignment.student_name.as_deref(),
            ),
            seat_id: assignment.seat_id.clone(),
            row: position.map(|(row, _)| row),
            column: position.map(|(_, column)| column),
            status: if position.is_some() {
                "seated".to_string()
            } else {
                "unseated".to_string()
            },
        });
    }

    if let Some(students) = &snapshot.students {
        for student in students {
            let key = student.student_id.clone().unwrap_or_default();
            if key.trim().is_empty() || assigned.contains(&key) {
                continue;
            }
            let name = student.name.clone().unwrap_or_default();
            if name.trim().is_empty() {
                continue;
            }
            members.push(RegisterMember {
                student_key: key,
                student_name: name,
                seat_id: String::new(),
                row: None,
                column: None,
                status: "unseated".to_string(),
            });
        }
    }

    members.sort_by(|a, b| match (a.row, b.row) {
        (Some(left), Some(right)) => left
            .cmp(&right)
            .then_with(|| a.column.cmp(&b.column))
            .then_with(|| a.student_key.cmp(&b.student_key)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.student_key.cmp(&b.student_key),
    });
    members
}

/// Best display name for a student: the assignment's stored name, then the
/// roster name, then the key itself.
fn student_display_name(
    snapshot: &SnapshotData,
    key: &str,
    assignment_name: Option<&str>,
) -> String {
    if let Some(name) = assignment_name {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if let Some(students) = &snapshot.students {
        for student in students {
            if student.student_id.as_deref() == Some(key) {
                if let Some(name) = &student.name {
                    let name = name.trim();
                    if !name.is_empty() {
                        return name.to_string();
                    }
                }
            }
        }
    }
    key.to_string()
}

/// Recover a seat's (row, column): first from the layout's seat grid, then by
/// parsing the canonical `R{r}C{c}` id.
fn seat_position(snapshot: &SnapshotData, seat_id: &str) -> Option<(i64, i64)> {
    if let Some(layout) = &snapshot.layout {
        if let Some(seat) = layout.seats.iter().find(|seat| seat.seat_id == seat_id) {
            return Some((seat.row, seat.col));
        }
    }
    parse_seat_id(seat_id)
}

/// Parse a `R{row}C{col}` seat id (case-insensitive, e.g. `r2c3`).
fn parse_seat_id(seat_id: &str) -> Option<(i64, i64)> {
    let upper = seat_id.trim().to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.first() != Some(&b'R') {
        return None;
    }
    let column_pos = upper[1..].find('C').map(|position| position + 1)?;
    let row_text = &upper[1..column_pos];
    let column_text = &upper[column_pos + 1..];
    if row_text.is_empty() || column_text.is_empty() {
        return None;
    }
    let row = row_text.parse::<i64>().ok()?;
    let column = column_text.parse::<i64>().ok()?;
    Some((row, column))
}

/// Group already-sorted members into contiguous runs by row.
fn group_by_row(members: &[RegisterMember]) -> Vec<MemberGroup> {
    group_by_key(members, |member| member.row)
}

/// Group already-sorted members into contiguous runs by column.
fn group_by_column(members: &[RegisterMember]) -> Vec<MemberGroup> {
    group_by_key(members, |member| member.column)
}

/// Contiguous-run grouping over a sorted member list using a key extractor,
/// with groups ordered by key (seated first, unseated `None` last).
fn group_by_key(
    members: &[RegisterMember],
    key: impl Fn(&RegisterMember) -> Option<i64>,
) -> Vec<MemberGroup> {
    let mut groups: Vec<MemberGroup> = Vec::new();
    for member in members {
        let member_key = key(member);
        if let Some(last) = groups.last_mut() {
            if last.key == member_key {
                last.members.push(member.clone());
                continue;
            }
        }
        groups.push(MemberGroup {
            key: member_key,
            members: vec![member.clone()],
        });
    }
    groups.sort_by(|a, b| match (a.key, b.key) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    groups
}

/// Chinese status label for the register table.
fn status_label(status: &str) -> String {
    match status {
        "seated" => "已入座".to_string(),
        "unseated" => "未入座".to_string(),
        other => other.to_string(),
    }
}

/// HTML register document: an inline-styled table with everything escaped.
fn render_register_html(plan_name: &str, period_label: &str, rows: &[Vec<String>]) -> String {
    let title = "小组登记表";
    let headers = ["期次", "小组", "学生编号", "姓名", "座位", "状态"];
    let mut header_cells = String::new();
    for header in headers {
        header_cells.push_str(&format!("<th>{}</th>", html_escape(header)));
    }
    let mut body_cells = String::new();
    for row in rows {
        body_cells.push_str("<tr>");
        for cell in row {
            body_cells.push_str(&format!("<td>{}</td>", html_escape(cell)));
        }
        body_cells.push_str("</tr>");
    }
    format!(
        "<!doctype html>\n<html lang=\"zh\">\n<head><meta charset=\"utf-8\"><title>{title} · {plan_name}</title>\n\
<style>\n\
body {{ font: 14px -apple-system,BlinkMacSystemFont,\"Segoe UI\",sans-serif; margin: 32px; color: #1f2933; }}\n\
h1 {{ margin-bottom: 4px; }}\n\
p {{ color: #52606d; }}\n\
table {{ border-collapse: collapse; width: 100%; }}\n\
th, td {{ border: 1px solid #cbd5e1; padding: 7px 9px; text-align: left; }}\n\
th {{ background: #eef2f6; }}\n\
@media print {{ body {{ margin: 10mm; }} }}\n\
</style></head>\n\
<body><h1>{title}</h1><p>{plan_name} · {period_label}</p>\n\
<table><thead><tr>{header_cells}</tr></thead><tbody>{body_cells}</tbody></table>\n\
</body></html>",
        title = html_escape(title),
        plan_name = html_escape(plan_name),
        period_label = html_escape(period_label),
    )
}

/// CSV register document: UTF-8 with BOM, formula-injection guarded.
fn render_register_csv(rows: &[Vec<String>]) -> Vec<u8> {
    let headers = ["期次", "小组", "学生编号", "姓名", "座位", "状态"];
    let mut out = String::from("\u{feff}");
    for (index, header) in headers.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&csv_field(header));
    }
    out.push('\n');
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&csv_field(cell));
        }
        out.push('\n');
    }
    out.into_bytes()
}

/// Guard a spreadsheet cell: prefix values that would start a formula so the
/// exported file cannot run them when opened.
fn sanitize_cell(value: &str) -> String {
    let leading = value.trim_start();
    if leading.starts_with('=') || leading.starts_with('+') || leading.starts_with('@') {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// Encode one CSV field: formula guard, then quote + escape when needed.
fn csv_field(value: &str) -> String {
    let value = sanitize_cell(value);
    let needs_quotes = value.contains(',')
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r')
        || value.starts_with(' ')
        || value.starts_with('\t');
    if needs_quotes {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// Escape text for safe inclusion in HTML (text and attribute contexts).
fn html_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// Project resolution and atomic writes
// ---------------------------------------------------------------------------

/// Load and validate a project file, returning it and its parsed JSON.
fn load_project_file(project_path: &str) -> Result<(PathBuf, Value), String> {
    let project_file = fs::canonicalize(project_path)
        .map_err(|e| format!("Project file not found or unreadable: {project_path} ({e})"))?;
    if !project_file.is_file() {
        return Err(format!(
            "Project file not found: {}",
            project_file.display()
        ));
    }
    let metadata = fs::metadata(&project_file).map_err(|e| {
        format!(
            "Could not stat project file {}: {e}",
            project_file.display()
        )
    })?;
    if metadata.len() > MAX_PROJECT_FILE_BYTES {
        return Err(format!(
            "Project file too large: {}",
            project_file.display()
        ));
    }
    let bytes = fs::read(&project_file).map_err(|e| {
        format!(
            "Could not read project file {}: {e}",
            project_file.display()
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid project file: {} ({e})", project_file.display()))?;
    let obj = value.as_object().ok_or_else(|| {
        format!(
            "Invalid project file: {} (not a JSON object)",
            project_file.display()
        )
    })?;
    if obj.get("kind").and_then(Value::as_str) != Some("seattrellis_project") {
        return Err(format!(
            "Invalid project file: {} (expected kind \"seattrellis_project\")",
            project_file.display()
        ));
    }
    if let Some(version) = obj.get("schema_version") {
        if version.as_i64() != Some(1) {
            return Err(format!(
                "Invalid project file: {} (unsupported schema_version {version})",
                project_file.display()
            ));
        }
    }
    Ok((project_file, value))
}

fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve the project's outputs directory, rejecting absolute or escaping
/// values. Existing directories are canonicalized and must stay inside the
/// project root; missing directories are resolved through their nearest
/// existing ancestor so a symlink cannot smuggle writes outside the root.
fn resolve_outputs_dir(root: &Path, project: &Value) -> Result<PathBuf, String> {
    let relative = project
        .get("outputs_dir")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("outputs");
    let looks_absolute = Path::new(relative).is_absolute()
        || (relative.len() >= 2 && relative.as_bytes()[1] == b':');
    if looks_absolute {
        return Err("Project field \"outputs_dir\" must be a relative path.".to_string());
    }
    let candidate = root.join(relative);
    if candidate.is_symlink() || candidate.exists() {
        let resolved = fs::canonicalize(&candidate).map_err(|e| {
            format!(
                "Could not resolve outputs directory {}: {e}",
                candidate.display()
            )
        })?;
        if !resolved.is_dir() {
            return Err(format!(
                "Project reference \"outputs_dir\" is not a directory: {}",
                candidate.display()
            ));
        }
        ensure_inside(&resolved, root, "outputs_dir")?;
        Ok(resolved)
    } else {
        resolve_lexically_inside(&candidate, root, "outputs_dir")
    }
}

/// Resolve a not-yet-existing path against the project root: canonicalize the
/// nearest existing ancestor, re-append the missing components, then verify the
/// result stays inside `root`.
fn resolve_lexically_inside(candidate: &Path, root: &Path, label: &str) -> Result<PathBuf, String> {
    let mut tail: Vec<OsString> = Vec::new();
    let mut probe = candidate.to_path_buf();
    while !probe.exists() && !probe.is_symlink() {
        match probe.file_name() {
            Some(name) => tail.push(name.to_os_string()),
            None => break,
        }
        if !probe.pop() {
            break;
        }
    }
    let anchor = fs::canonicalize(&probe)
        .map_err(|e| format!("Could not resolve {} ({e})", candidate.display()))?;
    let resolved = tail.iter().rev().fold(anchor, |mut path, name| {
        path.push(name);
        path
    });
    ensure_inside(&resolved, root, label)?;
    Ok(resolved)
}

/// Reject a resolved path that escapes the project root (mirrors `projects.rs`).
fn ensure_inside(path: &Path, root: &Path, label: &str) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err(format!(
            "Project reference \"{label}\" points outside the project root: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Choose `rotation-plan.json`, or the first free `rotation-plan-2.json`,
/// `rotation-plan-3.json`, ... without overwriting an existing artifact.
fn next_rotation_output_path(outputs_dir: &Path) -> PathBuf {
    let base = outputs_dir.join(ROTATION_PLAN_FILE);
    if !base.exists() {
        return base;
    }
    let stem = base
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rotation-plan".to_string());
    let extension = base
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned());
    for index in 2..MAX_OUTPUT_SUFFIX_ATTEMPTS {
        let candidate = match &extension {
            Some(extension) => outputs_dir.join(format!("{stem}-{index}.{extension}")),
            None => outputs_dir.join(format!("{stem}-{index}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    outputs_dir.join(format!("{stem}-{}.json", now_nanos()))
}

/// Write a JSON value to a fresh sibling temp file and atomically rename it
/// over `output`, preserving the destination's permissions when it exists.
fn atomic_write_json(output: &Path, value: &Value) -> Result<(), String> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Could not prepare output directory {}: {e}",
            parent.display()
        )
    })?;
    let existing_mode = fs::metadata(output)
        .ok()
        .and_then(|metadata| existing_permission_mode(&metadata));

    let mut temp = temp_sibling_path(output);
    for _ in 0..MAX_TEMP_WRITE_ATTEMPTS {
        match write_temp_then_rename(value, output, &temp, existing_mode) {
            Ok(()) => return Ok(()),
            Err(TempWriteError::AlreadyExists) => temp = temp_sibling_path(output),
            Err(TempWriteError::Other(message)) => return Err(message),
        }
    }
    Err(format!(
        "Could not allocate a temporary file next to {}",
        output.display()
    ))
}

/// A unique sibling temporary path next to `output` for an atomic rename.
fn temp_sibling_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    let parent = parent_dir(output);
    parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        now_nanos()
    ))
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
        .map_err(|error| TempWriteError::Other(format!("Could not serialize JSON: {error}")))?;
    bytes.push(b'\n');
    file.write_all(&bytes).map_err(|error| {
        TempWriteError::Other(format!(
            "Could not write JSON file {}: {error}",
            output.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        TempWriteError::Other(format!(
            "Could not flush JSON file {}: {error}",
            output.display()
        ))
    })?;
    drop(file);
    if let Some(mode) = existing_mode {
        set_permission_mode(temp, mode).map_err(|error| {
            TempWriteError::Other(format!(
                "Could not set permissions on JSON file {}: {error}",
                output.display()
            ))
        })?;
    }
    // Test-only fault injection (revised plan §17.2.4): fail at the atomic rename
    // so the single-file write paths (rotation save, group register) prove
    // the old target survives and the temp is cleaned up.
    #[cfg(test)]
    if crate::transaction::inject_commit_failure() {
        let _ = std::fs::remove_file(temp);
        return Err(TempWriteError::Other(
            "injected rename failure after staging (SEATTRELLIS fault-injection test)".to_string(),
        ));
    }
    fs::rename(temp, output).map_err(|error| {
        TempWriteError::Other(format!(
            "Could not atomically write JSON file {}: {error}",
            output.display()
        ))
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Current UTC time as an ISO-8601 string (`YYYY-MM-DDTHH:MM:SS+00:00`).
fn now_iso8601() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    iso_from_epoch_secs(seconds)
}

/// Format a Unix timestamp as a UTC ISO-8601 string using Howard Hinnant's
/// `civil_from_days` algorithm (no external date crate), matching `projects.rs`.
fn iso_from_epoch_secs(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let remaining = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (remaining / 3600, (remaining % 3600) / 60, remaining % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+00:00")
}

/// A monotone-ish nanosecond timestamp for unique temp / fallback names.
fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_root(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "seattrellis-rotation-{}-{}-{}",
            std::process::id(),
            name,
            now_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a minimal project file and return its path.
    fn write_project(root: &Path, outputs_dir: &str) -> PathBuf {
        let project = json!({
            "kind": "seattrellis_project",
            "schema_version": 1,
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "outputs_dir": outputs_dir,
        });
        let project_file = root.join("project.seattrellis.json");
        fs::write(&project_file, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
        fs::write(root.join("students.csv"), "student_id,name\n").unwrap();
        fs::write(root.join("classroom.json"), r#"{"seats":[]}"#).unwrap();
        fs::write(root.join("rules.json"), r#"{}"#).unwrap();
        project_file
    }

    fn sample_plan() -> Value {
        json!({
            "schema_version": "1.0",
            "kind": "rotation_plan",
            "created_at": "2026-08-01T00:00:00+00:00",
            "name": "Weekly Rotation",
            "periods": [
                {
                    "period": 1,
                    "label": "Week 1",
                    "snapshot": {
                        "solver_status": "FEASIBLE",
                        "assignments": [
                            {"student_key": "STU001", "student_name": "Alice", "seat_id": "R1C1"},
                            {"student_key": "STU002", "student_name": "Bob", "seat_id": "R1C3"},
                            {"student_key": "STU003", "student_name": "Carol", "seat_id": "R2C2"}
                        ],
                        "students": [
                            {"student_id": "STU001", "name": "Alice"},
                            {"student_id": "STU002", "name": "Bob"},
                            {"student_id": "STU003", "name": "Carol"}
                        ],
                        "layout": {"seats": [
                            {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
                            {"seat_id": "R1C3", "row": 1, "col": 3, "enabled": true},
                            {"seat_id": "R2C2", "row": 2, "col": 2, "enabled": true}
                        ]}
                    }
                },
                {
                    "period": 2,
                    "label": "Week 2",
                    "snapshot": {
                        "solver_status": "FEASIBLE",
                        "assignments": [
                            {"student_key": "STU001", "student_name": "Alice", "seat_id": "R2C2"}
                        ]
                    }
                }
            ],
            "base_history_count": 0,
            "fairness_summary": {},
            "pair_repeat_summary": {},
            "warnings": []
        })
    }

    fn plan_json(value: &Value) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn save_and_load_round_trip() {
        let root = temp_root("round-trip");
        let project_file = write_project(&root, "outputs");
        let saved_json =
            rotation_save_json(project_file.to_str().unwrap(), &plan_json(&sample_plan())).unwrap();
        let saved: Value = serde_json::from_str(&saved_json).unwrap();
        assert_eq!(saved["api_version"], "1");
        assert_eq!(saved["period_count"], 2);
        assert!(saved["saved_at"].as_str().unwrap().ends_with("+00:00"));
        let output_path = saved["output_path"].as_str().unwrap();
        assert!(output_path.ends_with("rotation-plan.json"));

        let loaded_json = rotation_load_json(project_file.to_str().unwrap()).unwrap();
        let loaded: Value = serde_json::from_str(&loaded_json).unwrap();
        assert_eq!(loaded["api_version"], "1");
        assert_eq!(loaded["artifact_path"].as_str().unwrap(), output_path);
        assert_eq!(loaded["project_path"], saved["project_path"]);
        let plan = &loaded["rotation_plan"];
        assert_eq!(plan["name"], "Weekly Rotation");
        assert_eq!(plan["periods"].as_array().unwrap().len(), 2);
        // saved_at must have been stamped into the persisted metadata.
        assert!(plan["metadata"]["saved_at"].is_string());
    }

    #[test]
    fn save_never_overwrites_and_uses_suffix() {
        let root = temp_root("suffix");
        let project_file = write_project(&root, "outputs");
        let first =
            rotation_save_json(project_file.to_str().unwrap(), &plan_json(&sample_plan())).unwrap();
        let first_path: Value = serde_json::from_str(&first).unwrap();
        assert!(first_path["output_path"]
            .as_str()
            .unwrap()
            .ends_with("rotation-plan.json"));

        let second =
            rotation_save_json(project_file.to_str().unwrap(), &plan_json(&sample_plan())).unwrap();
        let second_path: Value = serde_json::from_str(&second).unwrap();
        assert!(
            second_path["output_path"]
                .as_str()
                .unwrap()
                .ends_with("rotation-plan-2.json"),
            "second save must not overwrite the first artifact"
        );

        let outputs = root.join("outputs");
        assert!(outputs.join("rotation-plan.json").is_file());
        assert!(outputs.join("rotation-plan-2.json").is_file());
    }

    #[test]
    fn preview_groups_by_row_and_column() {
        let root = temp_root("preview");
        let project_file = write_project(&root, "outputs");
        rotation_save_json(project_file.to_str().unwrap(), &plan_json(&sample_plan())).unwrap();

        let json = group_register_preview_json(project_file.to_str().unwrap(), 1).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["api_version"], "1");
        assert_eq!(value["plan_name"], "Weekly Rotation");
        assert_eq!(value["period"], 1);
        assert_eq!(value["period_label"], "Week 1");
        assert_eq!(value["period_count"], 2);

        let rows = value["row_groups"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "expected rows 1 and 2");
        assert_eq!(rows[0]["row"], 1);
        assert_eq!(rows[0]["member_count"], 2);
        let row1 = rows[0]["members"].as_array().unwrap();
        assert_eq!(row1[0]["student_key"], "STU001");
        assert_eq!(row1[0]["seat_id"], "R1C1");
        assert_eq!(row1[1]["student_key"], "STU002");
        assert_eq!(rows[1]["row"], 2);
        assert_eq!(rows[1]["member_count"], 1);
        assert_eq!(rows[1]["members"][0]["student_key"], "STU003");

        let columns = value["column_groups"].as_array().unwrap();
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0]["column"], 1);
        assert_eq!(columns[0]["members"][0]["student_key"], "STU001");
        assert_eq!(columns[1]["column"], 2);
        assert_eq!(columns[1]["members"][0]["student_key"], "STU003");
        assert_eq!(columns[2]["column"], 3);
        assert_eq!(columns[2]["members"][0]["student_key"], "STU002");
    }

    #[test]
    fn register_html_is_escaped_and_structured() {
        let root = temp_root("html");
        let project_file = write_project(&root, "outputs");
        let plan = json!({
            "kind": "rotation_plan",
            "name": "Html <Plan>",
            "periods": [{
                "period": 1,
                "label": "Week 1",
                "snapshot": {
                    "assignments": [
                        {"student_key": "STU001", "student_name": "A<B>&C", "seat_id": "R1C1"}
                    ],
                    "layout": {"seats": [{"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true}]}
                }
            }]
        });
        rotation_save_json(project_file.to_str().unwrap(), &plan_json(&plan)).unwrap();

        let bytes = group_register_html_json(project_file.to_str().unwrap(), 1).unwrap();
        let html = String::from_utf8(bytes).unwrap();
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>期次</th>"));
        assert!(html.contains("第1行"));
        // Escaped cells and title: no raw markup may leak through.
        assert!(html.contains("A&lt;B&gt;&amp;C"));
        assert!(html.contains("Html &lt;Plan&gt;"));
        assert!(!html.contains("A<B>"));
        assert!(!html.contains("<Plan>"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn register_csv_guards_formula_injection() {
        let root = temp_root("csv");
        let project_file = write_project(&root, "outputs");
        let plan = json!({
            "kind": "rotation_plan",
            "name": "Injection",
            "periods": [{
                "period": 1,
                "label": "Week 1",
                "snapshot": {
                    "assignments": [
                        {"student_key": "=1+1", "student_name": "Eve", "seat_id": "R1C1"},
                        {"student_key": "+SUM(A1)", "student_name": "Mallory", "seat_id": "R1C2"},
                        {"student_key": "@cmd", "student_name": "Oscar", "seat_id": "R2C1"}
                    ],
                    "layout": {"seats": [
                        {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
                        {"seat_id": "R1C2", "row": 1, "col": 2, "enabled": true},
                        {"seat_id": "R2C1", "row": 2, "col": 1, "enabled": true}
                    ]}
                }
            }]
        });
        rotation_save_json(project_file.to_str().unwrap(), &plan_json(&plan)).unwrap();

        let bytes = group_register_csv_json(project_file.to_str().unwrap(), 1).unwrap();
        // UTF-8 BOM for Excel.
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        let csv = String::from_utf8(bytes).unwrap();
        assert!(csv.contains("期次"));
        assert!(csv.contains("'=1+1"));
        assert!(csv.contains("'+SUM(A1)"));
        assert!(csv.contains("'@cmd"));
        assert!(!csv.contains("\n=1+1"));
    }

    #[test]
    fn invalid_period_rejected() {
        let root = temp_root("invalid-period");
        let project_file = write_project(&root, "outputs");
        rotation_save_json(project_file.to_str().unwrap(), &plan_json(&sample_plan())).unwrap();

        for bad in [0, 3, 99] {
            let error = group_register_preview_json(project_file.to_str().unwrap(), bad)
                .expect_err("out-of-range period must be rejected");
            assert!(
                error.contains("out of range"),
                "unexpected error message: {error}"
            );
        }
        assert!(group_register_html_json(project_file.to_str().unwrap(), 0).is_err());
        assert!(group_register_csv_json(project_file.to_str().unwrap(), 99).is_err());
    }

    #[test]
    fn path_traversal_rejected() {
        let root = temp_root("traversal");
        let project_file = write_project(&root, "../escape");
        let error = rotation_save_json(project_file.to_str().unwrap(), &plan_json(&sample_plan()))
            .expect_err("escaping outputs_dir must be rejected");
        assert!(error.contains("outputs_dir"), "unexpected error: {error}");
        assert!(!root
            .join("..")
            .join("escape")
            .join(ROTATION_PLAN_FILE)
            .exists());
    }

    #[test]
    fn unknown_project_errors() {
        let missing = std::env::temp_dir().join(format!("no-such-project-{}", now_nanos()));
        let missing = missing.join("project.seattrellis.json");
        let path = missing.to_str().unwrap();
        let error = rotation_save_json(path, &plan_json(&sample_plan()))
            .expect_err("a missing project must be rejected");
        assert!(error.contains("Project file"), "unexpected error: {error}");
        assert!(rotation_load_json(path).is_err());
        assert!(group_register_preview_json(path, 1).is_err());
    }

    #[test]
    fn invalid_rotation_plan_rejected() {
        let root = temp_root("invalid-plan");
        let project_file = write_project(&root, "outputs");
        assert!(rotation_save_json(project_file.to_str().unwrap(), "not json").is_err());
        assert!(rotation_save_json(project_file.to_str().unwrap(), "{}").is_err());
        assert!(rotation_save_json(
            project_file.to_str().unwrap(),
            &plan_json(&json!({"kind": "rotation_plan", "periods": []}))
        )
        .is_err());
        // A period without an assignments snapshot is malformed.
        let bad_period = json!({
            "kind": "rotation_plan",
            "periods": [{"period": 1, "label": "Week 1", "snapshot": {"solver_status": "ok"}}]
        });
        assert!(
            rotation_save_json(project_file.to_str().unwrap(), &plan_json(&bad_period)).is_err()
        );
    }

    #[test]
    fn save_groups_writes_output() {
        let root = temp_root("save-groups");
        let project_file = write_project(&root, "outputs");
        let groups = json!([
            {"name": "Row 1", "members": ["STU001", "STU002"]},
            {"name": "Row 2", "members": ["STU003"]}
        ]);
        let saved_json =
            group_register_save_json(project_file.to_str().unwrap(), &plan_json(&groups)).unwrap();
        let saved: Value = serde_json::from_str(&saved_json).unwrap();
        assert_eq!(saved["api_version"], "1");
        assert_eq!(saved["group_count"], 2);
        let output_path = saved["output_path"].as_str().unwrap();
        assert!(output_path.ends_with("group-register.json"));
        let on_disk: Value = serde_json::from_slice(
            &fs::read(root.join("outputs").join(GROUP_REGISTER_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_seat_id_handles_casing_and_padding() {
        assert_eq!(parse_seat_id("R1C1"), Some((1, 1)));
        assert_eq!(parse_seat_id("r2c3"), Some((2, 3)));
        assert_eq!(parse_seat_id("R01C02"), Some((1, 2)));
        assert_eq!(parse_seat_id("seat-7"), None);
        assert_eq!(parse_seat_id(""), None);
    }
}
