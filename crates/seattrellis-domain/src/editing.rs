//! Versioned editor protocol domain module for the SeatTrellis desktop backend.
//!
//! This module ports the Python editing protocol (`src/seattrellis/
//! editing_protocol.py` + `src/seattrellis/editing.py`) to Rust so the React
//! workbench's manual-adjustment commands (swap / move / lock / undo / redo)
//! can run in-process against a native solve result. It is self-contained:
//! the server wires it up by creating drafts from solve results and applying
//! [`EditorCommandEnvelope`] commands.
//!
//! # Wire contract
//!
//! * [`EditorState`] serializes to exactly the frontend `EditorState` shape in
//!   `clients/web/src/api/types.ts` (`kind` / `protocol_version` / `draft_id`
//!   / `revision` / `candidate_id` / `undo_depth` / `redo_depth` / `students`
//!   / `seats`).
//! * [`EditorCommandEnvelope`] deserializes the frontend `EditorCommand`
//!   shape and dispatches on each operation's `kind` + `payload`.
//!
//! # Semantics
//!
//! Commands are applied atomically: either every operation succeeds and the
//! revision advances by one (one snapshot pushed on the undo stack), or none
//! of them take effect and the draft is left untouched. Undo/redo are
//! snapshot-based over whole command batches, mirroring the web layer's
//! command-level history (`operation_batches` / `redo_operation_batches`).
//!
//! # Concurrency
//!
//! [`EditorDraft`] is `Send + Sync`, and [`EditorDraftStore`] (a
//! `Mutex<HashMap<..>>`) is provided so a thread-per-connection server can
//! share drafts safely. [`apply_command_in_store`] holds the store lock for the
//! whole apply, so concurrent commands never interleave.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

/// Protocol version shared by editor state and command envelopes.
pub const EDITOR_PROTOCOL_VERSION: &str = "1.0";
/// `kind` discriminator for editor state documents.
pub const EDITOR_STATE_KIND: &str = "seattrellis_editor_state";
/// `kind` discriminator for editor command envelopes.
pub const EDITOR_COMMAND_KIND: &str = "seattrellis_editor_command";

/// Command actions understood by [`apply_command`].
pub const ACTION_APPLY: &str = "apply";
pub const ACTION_UNDO: &str = "undo";
pub const ACTION_REDO: &str = "redo";

/// Supported editor operation kinds, in the order used by error messages.
const OPERATION_KINDS: &[&str] = &[
    "swap_students",
    "move_student",
    "batch_move",
    "seat_student",
    "unseat_student",
    "lock_student",
    "unlock_student",
    "lock_seat",
    "unlock_seat",
];

/// Upper bound on the number of expanded operations in one command, mirroring
/// the Python envelope limit (each `batch_move` item counts as one).
const MAX_EXPANDED_OPERATIONS: usize = 100;

/// Classified command action, produced by [`validate_command`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorCommandAction {
    Apply,
    Undo,
    Redo,
}

/// One student in an editing draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorStudent {
    pub student_key: String,
    pub display_name: String,
    pub seat_id: Option<String>,
    pub locked: bool,
}

/// One seat in an editing draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSeat {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    pub enabled: bool,
    pub student_key: Option<String>,
    pub locked: bool,
}

/// Immutable description of a seat used to build a draft from a solve result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSeatSpec {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    pub enabled: bool,
}

/// Upper bound for the undo/redo stacks (backend audit 2026-08-12):
/// every entry is a full snapshot, so an unbounded stack grows without
/// limit in long sessions. 100 steps is far beyond any realistic edit
/// session; the oldest step is dropped when the bound is exceeded.
const MAX_UNDO_DEPTH: usize = 100;

/// Push a snapshot onto a bounded undo/redo stack.
fn push_bounded(stack: &mut Vec<EditorSnapshot>, snapshot: EditorSnapshot) {
    stack.push(snapshot);
    if stack.len() > MAX_UNDO_DEPTH {
        stack.remove(0);
    }
}

/// A full copy of the editable state used for snapshot-based undo/redo.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorSnapshot {
    students: Vec<EditorStudent>,
    seats: Vec<EditorSeat>,
}

/// In-process editing draft backed by a native solve result.
///
/// Holds the mutable student/seat state plus command-level undo/redo history.
/// All mutations go through [`apply_command`] (or the store helpers), which
/// keep the draft internally consistent and advance `revision`.
#[derive(Debug, Clone)]
pub struct EditorDraft {
    draft_id: String,
    candidate_id: Option<String>,
    revision: u64,
    students: Vec<EditorStudent>,
    seats: Vec<EditorSeat>,
    undo_stack: Vec<EditorSnapshot>,
    redo_stack: Vec<EditorSnapshot>,
    applied_command_ids: Vec<String>,
}

impl EditorDraft {
    /// Build a draft from a solve result: student keys, a seat grid, and one
    /// assignment (`student_key -> seat_id`). Identifiers are trimmed; empty,
    /// duplicate, unknown, or disabled references are rejected with clear
    /// errors, mirroring the Python `EditingSession.__post_init__` checks.
    ///
    /// `display_name` is initialised to the student key unless `display_names`
    /// supplies a roster name for that key (the solve request carries the
    /// full roster; the draft mirrors it, matching the Python oracle).
    /// All students and seats start unlocked.
    pub fn new(
        draft_id: impl Into<String>,
        candidate_id: Option<String>,
        student_keys: &[&str],
        seats: Vec<EditorSeatSpec>,
        assignment: &[(&str, &str)],
        display_names: Option<&HashMap<String, String>>,
    ) -> Result<EditorDraft, String> {
        let mut students = Vec::with_capacity(student_keys.len());
        let mut seen_students: HashSet<String> = HashSet::new();
        for key in student_keys {
            let trimmed = key.trim();
            if trimmed.is_empty() {
                return Err("student keys cannot be empty".to_string());
            }
            if !seen_students.insert(trimmed.to_string()) {
                return Err(format!("Student keys must be unique: {trimmed}."));
            }
            let display_name = display_names
                .and_then(|names| names.get(trimmed))
                .cloned()
                .unwrap_or_else(|| trimmed.to_string());
            students.push(EditorStudent {
                student_key: trimmed.to_string(),
                display_name,
                seat_id: None,
                locked: false,
            });
        }

        let mut validated_seats = Vec::with_capacity(seats.len());
        let mut seen_seats: HashSet<String> = HashSet::new();
        for spec in seats {
            let seat_id = spec.seat_id.trim().to_string();
            if seat_id.is_empty() {
                return Err("seat ids cannot be empty".to_string());
            }
            if !seen_seats.insert(seat_id.clone()) {
                return Err(format!("Seat ids must be unique: {seat_id}."));
            }
            validated_seats.push(EditorSeat {
                seat_id,
                row: spec.row,
                col: spec.col,
                enabled: spec.enabled,
                student_key: None,
                locked: false,
            });
        }

        let student_by_key: HashMap<String, usize> = students
            .iter()
            .enumerate()
            .map(|(index, student)| (student.student_key.clone(), index))
            .collect();
        let enabled_seat_ids: HashSet<String> = validated_seats
            .iter()
            .filter(|seat| seat.enabled)
            .map(|seat| seat.seat_id.clone())
            .collect();

        let mut assigned_students: HashSet<String> = HashSet::new();
        let mut assigned_seats: HashSet<String> = HashSet::new();
        let mut unknown_students: Vec<String> = Vec::new();
        let mut unknown_seats: Vec<String> = Vec::new();
        for (student_key, seat_id) in assignment {
            let student_key = student_key.trim().to_string();
            let seat_id = seat_id.trim().to_string();
            if !student_by_key.contains_key(&student_key) {
                unknown_students.push(student_key);
                continue;
            }
            if !enabled_seat_ids.contains(&seat_id) {
                unknown_seats.push(seat_id);
                continue;
            }
            if !assigned_students.insert(student_key.clone()) {
                return Err(format!("Duplicate student assignments: {student_key}."));
            }
            if !assigned_seats.insert(seat_id.clone()) {
                return Err(format!("Duplicate seat assignments: {seat_id}."));
            }
            students[student_by_key[&student_key]].seat_id = Some(seat_id);
        }
        if !unknown_students.is_empty() {
            return Err(format!(
                "Assignments reference unknown students: {}.",
                unknown_students.join(", ")
            ));
        }
        if !unknown_seats.is_empty() {
            return Err(format!(
                "Assignments reference unknown or disabled seats: {}.",
                unknown_seats.join(", ")
            ));
        }

        let mut draft = EditorDraft {
            draft_id: draft_id.into(),
            candidate_id,
            revision: 0,
            students,
            seats: validated_seats,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            applied_command_ids: Vec::new(),
        };
        draft.sync_seat_students();
        Ok(draft)
    }

    /// The draft's identifier.
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }

    /// The candidate the draft was created from, if any.
    pub fn candidate_id(&self) -> Option<&str> {
        self.candidate_id.as_deref()
    }

    /// Current revision (the number of commands applied so far).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Students in roster order.
    pub fn students(&self) -> &[EditorStudent] {
        &self.students
    }

    /// Seats in layout order.
    pub fn seats(&self) -> &[EditorSeat] {
        &self.seats
    }

    /// Number of commands that can be undone.
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of commands that can be redone.
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    fn capture_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            students: self.students.clone(),
            seats: self.seats.clone(),
        }
    }

    fn restore_snapshot(&mut self, snapshot: &EditorSnapshot) {
        self.students = snapshot.students.clone();
        self.seats = snapshot.seats.clone();
    }

    /// Recompute each seat's `student_key` from the students' `seat_id`s so
    /// the two views can never drift apart.
    fn sync_seat_students(&mut self) {
        let mut by_seat: HashMap<String, String> = HashMap::new();
        for student in &self.students {
            if let Some(seat_id) = &student.seat_id {
                by_seat.insert(seat_id.clone(), student.student_key.clone());
            }
        }
        for seat in &mut self.seats {
            seat.student_key = by_seat.get(&seat.seat_id).cloned();
        }
    }

    fn require_known_student(&self, key: &str) -> Result<usize, String> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return Err("Student key cannot be empty.".to_string());
        }
        self.students
            .iter()
            .position(|student| student.student_key == trimmed)
            .ok_or_else(|| format!("Unknown student: {trimmed}."))
    }

    fn require_enabled_seat(&self, seat_id: &str) -> Result<usize, String> {
        let trimmed = seat_id.trim();
        if trimmed.is_empty() {
            return Err("Seat id cannot be empty.".to_string());
        }
        self.seats
            .iter()
            .position(|seat| seat.seat_id == trimmed && seat.enabled)
            .ok_or_else(|| format!("Unknown or disabled seat: {trimmed}."))
    }

    fn ensure_student_can_move(&self, index: usize) -> Result<(), String> {
        if self.students[index].locked {
            return Err(format!(
                "Student is locked and cannot be moved: {}.",
                self.students[index].student_key
            ));
        }
        Ok(())
    }

    fn ensure_seat_can_change(&self, seat_id: &str) -> Result<(), String> {
        let seat = self
            .seats
            .iter()
            .find(|seat| seat.seat_id == seat_id)
            .ok_or_else(|| format!("Unknown or disabled seat: {seat_id}."))?;
        if seat.locked {
            return Err(format!("Seat is locked and cannot be changed: {seat_id}."));
        }
        Ok(())
    }

    fn op_swap_students(&mut self, first: String, second: String) -> Result<(), String> {
        let first_index = self.require_known_student(&first)?;
        let second_index = self.require_known_student(&second)?;
        if first_index == second_index {
            return Ok(());
        }
        let first_seat = self.students[first_index].seat_id.clone().ok_or_else(|| {
            "Both students must be seated before they can be swapped.".to_string()
        })?;
        let second_seat = self.students[second_index].seat_id.clone().ok_or_else(|| {
            "Both students must be seated before they can be swapped.".to_string()
        })?;
        self.ensure_student_can_move(first_index)?;
        self.ensure_student_can_move(second_index)?;
        self.ensure_seat_can_change(&first_seat)?;
        self.ensure_seat_can_change(&second_seat)?;

        self.students[first_index].seat_id = Some(second_seat.clone());
        self.students[second_index].seat_id = Some(first_seat.clone());
        self.sync_seat_students();
        Ok(())
    }

    fn op_move_student(&mut self, student_key: String, seat_id: String) -> Result<(), String> {
        let student_index = self.require_known_student(&student_key)?;
        let target_index = self.require_enabled_seat(&seat_id)?;
        let target_seat_id = self.seats[target_index].seat_id.clone();
        let current_seat = self.students[student_index].seat_id.clone();
        if current_seat.as_deref() == Some(target_seat_id.as_str()) {
            return Ok(());
        }

        self.ensure_student_can_move(student_index)?;
        if let Some(current) = &current_seat {
            self.ensure_seat_can_change(current)?;
        }
        self.ensure_seat_can_change(&target_seat_id)?;

        // Moving onto an occupied seat unseats the previous occupant (who must
        // be movable), mirroring `_move_student`.
        if let Some(occupant_key) = self.seats[target_index].student_key.clone() {
            if occupant_key != self.students[student_index].student_key {
                let occupant_index = self.require_known_student(&occupant_key)?;
                self.ensure_student_can_move(occupant_index)?;
                self.students[occupant_index].seat_id = None;
            }
        }

        self.students[student_index].seat_id = Some(target_seat_id);
        self.sync_seat_students();
        Ok(())
    }

    fn op_unseat_student(&mut self, student_key: String) -> Result<(), String> {
        let student_index = self.require_known_student(&student_key)?;
        let Some(current_seat) = self.students[student_index].seat_id.clone() else {
            return Ok(());
        };
        self.ensure_student_can_move(student_index)?;
        self.ensure_seat_can_change(&current_seat)?;
        self.students[student_index].seat_id = None;
        self.sync_seat_students();
        Ok(())
    }

    fn op_lock_student(&mut self, student_key: String) -> Result<(), String> {
        let index = self.require_known_student(&student_key)?;
        self.students[index].locked = true;
        Ok(())
    }

    fn op_unlock_student(&mut self, student_key: String) -> Result<(), String> {
        let index = self.require_known_student(&student_key)?;
        self.students[index].locked = false;
        Ok(())
    }

    fn op_lock_seat(&mut self, seat_id: String) -> Result<(), String> {
        let index = self.require_enabled_seat(&seat_id)?;
        self.seats[index].locked = true;
        Ok(())
    }

    fn op_unlock_seat(&mut self, seat_id: String) -> Result<(), String> {
        let index = self.require_enabled_seat(&seat_id)?;
        self.seats[index].locked = false;
        Ok(())
    }

    fn op_batch_move(&mut self, moves: Vec<(String, String)>) -> Result<(), String> {
        let mut normalized: Vec<(usize, usize)> = Vec::with_capacity(moves.len());
        for (student_key, seat_id) in &moves {
            let student_index = self.require_known_student(student_key)?;
            let seat_index = self.require_enabled_seat(seat_id)?;
            normalized.push((student_index, seat_index));
        }

        let mut seen_students: HashSet<usize> = HashSet::new();
        let mut seen_seats: HashSet<usize> = HashSet::new();
        for &(student_index, seat_index) in &normalized {
            let student_key = &self.students[student_index].student_key;
            let seat_id = &self.seats[seat_index].seat_id;
            if !seen_students.insert(student_index) {
                return Err(format!(
                    "Batch move contains duplicate students: {student_key}."
                ));
            }
            if !seen_seats.insert(seat_index) {
                return Err(format!(
                    "Batch move contains duplicate target seats: {seat_id}."
                ));
            }
        }

        // Only students whose target differs from their current seat move.
        let active: Vec<(usize, usize)> = normalized
            .into_iter()
            .filter(|&(student_index, seat_index)| {
                self.students[student_index].seat_id.as_deref()
                    != Some(self.seats[seat_index].seat_id.as_str())
            })
            .collect();
        let moving_students: HashSet<usize> = active
            .iter()
            .map(|&(student_index, _)| student_index)
            .collect();

        for &(student_index, target_index) in &active {
            self.ensure_student_can_move(student_index)?;
            if let Some(current) = &self.students[student_index].seat_id {
                self.ensure_seat_can_change(current)?;
            }
            self.ensure_seat_can_change(&self.seats[target_index].seat_id)?;
            if let Some(occupant_key) = self.seats[target_index].student_key.clone() {
                let occupant_index = self.require_known_student(&occupant_key)?;
                if !moving_students.contains(&occupant_index) {
                    return Err(format!(
                        "Batch move target is occupied by a student outside the batch: {} ({}).",
                        self.seats[target_index].seat_id, occupant_key
                    ));
                }
            }
        }

        // Pop every moving student first, then place them, so cycles (A->B,
        // B->A) resolve atomically like the Python batch move.
        for &(student_index, _) in &active {
            self.students[student_index].seat_id = None;
        }
        for &(student_index, target_index) in &active {
            self.students[student_index].seat_id = Some(self.seats[target_index].seat_id.clone());
        }
        self.sync_seat_students();
        Ok(())
    }

    /// Apply a sequence of operations to this draft. On failure the draft is
    /// left in its prior state (the caller restores the `before` snapshot).
    fn apply_operations(&mut self, operations: &[EditorOperation]) -> Result<(), String> {
        for operation in operations {
            match operation.kind.as_str() {
                "swap_students" => {
                    let first =
                        required_payload_str(&operation.payload, &operation.kind, "first_student")?;
                    let second = required_payload_str(
                        &operation.payload,
                        &operation.kind,
                        "second_student",
                    )?;
                    self.op_swap_students(first, second)?;
                }
                "move_student" | "seat_student" => {
                    let student_key =
                        required_payload_str(&operation.payload, &operation.kind, "student_key")?;
                    let seat_id =
                        required_payload_str(&operation.payload, &operation.kind, "seat_id")?;
                    self.op_move_student(student_key, seat_id)?;
                }
                "unseat_student" => {
                    let student_key =
                        required_payload_str(&operation.payload, &operation.kind, "student_key")?;
                    self.op_unseat_student(student_key)?;
                }
                "lock_student" => {
                    let student_key =
                        required_payload_str(&operation.payload, &operation.kind, "student_key")?;
                    self.op_lock_student(student_key)?;
                }
                "unlock_student" => {
                    let student_key =
                        required_payload_str(&operation.payload, &operation.kind, "student_key")?;
                    self.op_unlock_student(student_key)?;
                }
                "lock_seat" => {
                    let seat_id =
                        required_payload_str(&operation.payload, &operation.kind, "seat_id")?;
                    self.op_lock_seat(seat_id)?;
                }
                "unlock_seat" => {
                    let seat_id =
                        required_payload_str(&operation.payload, &operation.kind, "seat_id")?;
                    self.op_unlock_seat(seat_id)?;
                }
                "batch_move" => {
                    let moves = parse_batch_moves(&operation.payload)?;
                    self.op_batch_move(moves)?;
                }
                other => {
                    return Err(format!(
                        "unknown editor operation kind {other:?}; expected one of: {}",
                        OPERATION_KINDS.join(", ")
                    ));
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Transport DTOs
// ---------------------------------------------------------------------------

/// One operation inside an [`EditorCommandEnvelope`], mirroring the frontend
/// `EditorOperation` (`kind` + `payload`). The payload is kept as a JSON object
/// and its fields are parsed per kind at dispatch time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorOperation {
    pub kind: String,
    #[serde(default)]
    pub payload: JsonMap<String, JsonValue>,
}

/// A versioned editor command, mirroring the frontend `EditorCommand` shape.
///
/// `action` is `"apply"`, `"undo"`, or `"redo"`; `operations` must be non-empty
/// for `apply` and empty for `undo`/`redo` (enforced by [`apply_command`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorCommandEnvelope {
    pub kind: String,
    pub protocol_version: String,
    pub command_id: String,
    pub draft_id: String,
    pub base_revision: u64,
    pub action: String,
    #[serde(default)]
    pub operations: Vec<EditorOperation>,
}

/// A data-minimized state document serialized for editor clients, matching the
/// frontend `EditorState` shape exactly.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorState {
    pub kind: String,
    pub protocol_version: String,
    pub draft_id: String,
    pub revision: u64,
    pub candidate_id: Option<String>,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub students: Vec<EditorStudentState>,
    pub seats: Vec<EditorSeatState>,
}

/// `EditorStudentState` from the frontend contract.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorStudentState {
    pub student_key: String,
    pub display_name: String,
    pub seat_id: Option<String>,
    pub locked: bool,
}

/// `EditorSeatState` from the frontend contract.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditorSeatState {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    pub enabled: bool,
    pub student_key: Option<String>,
    pub locked: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate a command envelope against a draft and classify its action.
///
/// Rejects envelope/protocol mismatches, commands for another draft, already
/// applied command ids, stale base revisions, and invalid operation policies.
fn validate_command(
    draft: &EditorDraft,
    command: &EditorCommandEnvelope,
) -> Result<EditorCommandAction, String> {
    if command.kind != EDITOR_COMMAND_KIND {
        return Err(format!(
            "invalid editor command kind {:?}; expected {:?}",
            command.kind, EDITOR_COMMAND_KIND
        ));
    }
    if command.protocol_version != EDITOR_PROTOCOL_VERSION {
        return Err(format!(
            "editor command protocol version {} does not match {}",
            command.protocol_version, EDITOR_PROTOCOL_VERSION
        ));
    }
    if command.draft_id != draft.draft_id {
        return Err("The editor command targets a different draft.".to_string());
    }
    if command.command_id.trim().is_empty() {
        return Err("editor command requires a non-empty command_id.".to_string());
    }
    if draft.applied_command_ids.contains(&command.command_id) {
        return Err(format!(
            "Editor command {:?} has already been applied.",
            command.command_id
        ));
    }
    if command.base_revision != draft.revision {
        return Err(format!(
            "The editor command is stale: base revision {}, current revision {}.",
            command.base_revision, draft.revision
        ));
    }

    let action = classify_action(&command.action)?;
    match action {
        EditorCommandAction::Apply => {
            if command.operations.is_empty() {
                return Err("apply commands require at least one operation".to_string());
            }
        }
        EditorCommandAction::Undo | EditorCommandAction::Redo => {
            if !command.operations.is_empty() {
                return Err(format!(
                    "{} commands must not contain operations",
                    command.action
                ));
            }
        }
    }

    let mut expanded = 0usize;
    for operation in &command.operations {
        expanded += if operation.kind == "batch_move" {
            operation
                .payload
                .get("moves")
                .and_then(JsonValue::as_array)
                .map_or(1, Vec::len)
        } else {
            1
        };
    }
    if expanded > MAX_EXPANDED_OPERATIONS {
        return Err(format!(
            "editor commands may contain at most {MAX_EXPANDED_OPERATIONS} expanded operations"
        ));
    }
    Ok(action)
}

fn classify_action(action: &str) -> Result<EditorCommandAction, String> {
    match action {
        ACTION_APPLY => Ok(EditorCommandAction::Apply),
        ACTION_UNDO => Ok(EditorCommandAction::Undo),
        ACTION_REDO => Ok(EditorCommandAction::Redo),
        other => Err(format!(
            "unknown editor action {other:?}; expected one of: apply, undo, redo"
        )),
    }
}

/// Apply a versioned editor command to a draft.
///
/// Returns the resulting [`EditorState`] on success. On error the draft is
/// left completely unchanged (a `apply` command is atomic; `undo`/`redo` never
/// mutate on failure).
///
/// * `apply` runs every operation in order; the whole batch becomes one undo
///   entry and the revision advances by one.
/// * `undo` restores the previous snapshot; `redo` re-applies it. Both advance
///   the revision by one, matching the web layer's command-level history.
pub fn apply_command(
    draft: &mut EditorDraft,
    command: &EditorCommandEnvelope,
) -> Result<EditorState, String> {
    let action = validate_command(draft, command)?;

    match action {
        EditorCommandAction::Apply => {
            let before = draft.capture_snapshot();
            if let Err(error) = draft.apply_operations(&command.operations) {
                draft.restore_snapshot(&before);
                return Err(error);
            }
            push_bounded(&mut draft.undo_stack, before);
            draft.redo_stack.clear();
        }
        EditorCommandAction::Undo => {
            let Some(previous) = draft.undo_stack.pop() else {
                return Err("There is no editing operation to undo.".to_string());
            };
            let current = draft.capture_snapshot();
            push_bounded(&mut draft.redo_stack, current);
            draft.restore_snapshot(&previous);
        }
        EditorCommandAction::Redo => {
            let Some(next) = draft.redo_stack.pop() else {
                return Err("There is no editing operation to redo.".to_string());
            };
            let current = draft.capture_snapshot();
            push_bounded(&mut draft.undo_stack, current);
            draft.restore_snapshot(&next);
        }
    }

    draft.revision += 1;
    draft.applied_command_ids.push(command.command_id.clone());
    Ok(build_editor_state(draft))
}

/// Build the wire [`EditorState`] for a draft.
pub fn build_editor_state(draft: &EditorDraft) -> EditorState {
    EditorState {
        kind: EDITOR_STATE_KIND.to_string(),
        protocol_version: EDITOR_PROTOCOL_VERSION.to_string(),
        draft_id: draft.draft_id.clone(),
        revision: draft.revision,
        candidate_id: draft.candidate_id.clone(),
        undo_depth: draft.undo_stack.len(),
        redo_depth: draft.redo_stack.len(),
        students: draft
            .students
            .iter()
            .map(|student| EditorStudentState {
                student_key: student.student_key.clone(),
                display_name: student.display_name.clone(),
                seat_id: student.seat_id.clone(),
                locked: student.locked,
            })
            .collect(),
        seats: draft
            .seats
            .iter()
            .map(|seat| EditorSeatState {
                seat_id: seat.seat_id.clone(),
                row: seat.row,
                col: seat.col,
                enabled: seat.enabled,
                student_key: seat.student_key.clone(),
                locked: seat.locked,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Thread-safe draft store
// ---------------------------------------------------------------------------

/// A thread-safe registry of in-flight editor drafts. The server holds one of
/// these (typically behind an `Arc`) and shares it across connection threads.
pub type EditorDraftStore = Mutex<HashMap<String, EditorDraft>>;

/// Cap on concurrently stored editor drafts: a long-lived server must not
/// accumulate one draft per generation forever. Draft ids are
/// server-generated monotonic (`draft-<nanos><seq>`), so the smallest key is
/// the oldest; the cap evicts it deterministically (FIFO, alpha.2/M7 item).
pub const MAX_EDITOR_DRAFTS: usize = 64;

/// Create an empty draft store.
pub fn new_draft_store() -> EditorDraftStore {
    Mutex::new(HashMap::new())
}

/// Insert a draft into the store, rejecting a duplicate `draft_id`. At
/// [`MAX_EDITOR_DRAFTS`] the oldest draft (smallest draft_id) is evicted.
pub fn store_draft(store: &EditorDraftStore, draft: EditorDraft) -> Result<(), String> {
    let mut guard = store
        .lock()
        .map_err(|_| "editor draft store is poisoned".to_string())?;
    if guard.contains_key(&draft.draft_id) {
        return Err(format!(
            "an editor draft already exists with id: {}",
            draft.draft_id
        ));
    }
    guard.insert(draft.draft_id.clone(), draft);
    if guard.len() > MAX_EDITOR_DRAFTS {
        if let Some(oldest) = guard.keys().min().cloned() {
            guard.remove(&oldest);
        }
    }
    Ok(())
}

/// Clone a draft out of the store by id.
pub fn get_draft(store: &EditorDraftStore, draft_id: &str) -> Option<EditorDraft> {
    store.lock().ok()?.get(draft_id).cloned()
}

/// Build the wire state for a stored draft, or a clear error for an unknown id.
pub fn fetch_state(store: &EditorDraftStore, draft_id: &str) -> Result<EditorState, String> {
    let guard = store
        .lock()
        .map_err(|_| "editor draft store is poisoned".to_string())?;
    let draft = guard
        .get(draft_id)
        .ok_or_else(|| format!("unknown editor draft: {draft_id}"))?;
    Ok(build_editor_state(draft))
}

/// Create, validate, and store a draft from a solve result in one call,
/// returning its initial [`EditorState`].
pub fn create_draft(
    store: &EditorDraftStore,
    draft_id: impl Into<String>,
    candidate_id: Option<String>,
    student_keys: &[&str],
    seats: Vec<EditorSeatSpec>,
    assignment: &[(&str, &str)],
    display_names: Option<&HashMap<String, String>>,
) -> Result<EditorState, String> {
    let draft = EditorDraft::new(
        draft_id,
        candidate_id,
        student_keys,
        seats,
        assignment,
        display_names,
    )?;
    let state = build_editor_state(&draft);
    store_draft(store, draft)?;
    Ok(state)
}

/// Apply a command to a stored draft while holding the store lock for the whole
/// apply, so concurrent commands never interleave on the same draft.
pub fn apply_command_in_store(
    store: &EditorDraftStore,
    command: &EditorCommandEnvelope,
) -> Result<EditorState, String> {
    let mut guard = store
        .lock()
        .map_err(|_| "editor draft store is poisoned".to_string())?;
    let draft = guard
        .get_mut(&command.draft_id)
        .ok_or_else(|| format!("unknown editor draft: {}", command.draft_id))?;
    apply_command(draft, command)
}

// ---------------------------------------------------------------------------
// Payload parsing helpers
// ---------------------------------------------------------------------------

/// Read a required, non-empty string field from an operation payload. Values
/// are trimmed, mirroring the Python `_required_payload` (which also strips).
fn required_payload_str(
    payload: &JsonMap<String, JsonValue>,
    op_kind: &str,
    key: &str,
) -> Result<String, String> {
    match payload.get(key) {
        Some(JsonValue::String(value)) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        Some(_) => Err(format!(
            "{op_kind} requires a non-empty string payload field: {key}."
        )),
        None => Err(format!("{op_kind} requires payload field: {key}.")),
    }
}

/// Parse the `moves` list of a `batch_move` operation into `(student, seat)`
/// pairs, rejecting missing/empty/non-object entries.
fn parse_batch_moves(
    payload: &JsonMap<String, JsonValue>,
) -> Result<Vec<(String, String)>, String> {
    let Some(value) = payload.get("moves") else {
        return Err("batch_move requires a moves list.".to_string());
    };
    let Some(moves) = value.as_array() else {
        return Err("batch_move requires a moves list.".to_string());
    };
    if moves.is_empty() {
        return Err("batch_move requires at least one move.".to_string());
    }
    let mut result = Vec::with_capacity(moves.len());
    for (index, item) in moves.iter().enumerate() {
        let Some(entry) = item.as_object() else {
            return Err(format!("batch_move item {} must be an object.", index + 1));
        };
        let student_key = required_payload_str(entry, "batch_move", "student_key")?;
        let seat_id = required_payload_str(entry, "batch_move", "seat_id")?;
        result.push((student_key, seat_id));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use std::thread;

    fn test_seats() -> Vec<EditorSeatSpec> {
        vec![
            EditorSeatSpec {
                seat_id: "A1".to_string(),
                row: 1,
                col: 1,
                enabled: true,
            },
            EditorSeatSpec {
                seat_id: "A2".to_string(),
                row: 1,
                col: 2,
                enabled: true,
            },
            EditorSeatSpec {
                seat_id: "B1".to_string(),
                row: 2,
                col: 1,
                enabled: true,
            },
            EditorSeatSpec {
                seat_id: "B2".to_string(),
                row: 2,
                col: 2,
                enabled: true,
            },
            EditorSeatSpec {
                seat_id: "X1".to_string(),
                row: 3,
                col: 1,
                enabled: false,
            },
        ]
    }

    fn test_draft() -> EditorDraft {
        EditorDraft::new(
            "draft-1",
            Some("candidate-1".to_string()),
            &["s1", "s2", "s3"],
            test_seats(),
            &[("s1", "A1"), ("s2", "A2"), ("s3", "B1")],
            None,
        )
        .expect("test draft builds")
    }

    fn command(
        draft_id: &str,
        command_id: &str,
        base_revision: u64,
        action: &str,
        operations: Vec<EditorOperation>,
    ) -> EditorCommandEnvelope {
        EditorCommandEnvelope {
            kind: EDITOR_COMMAND_KIND.to_string(),
            protocol_version: EDITOR_PROTOCOL_VERSION.to_string(),
            command_id: command_id.to_string(),
            draft_id: draft_id.to_string(),
            base_revision,
            action: action.to_string(),
            operations,
        }
    }

    fn op(kind: &str, payload: JsonValue) -> EditorOperation {
        EditorOperation {
            kind: kind.to_string(),
            payload: payload.as_object().cloned().unwrap_or_default(),
        }
    }

    fn student_seat(state: &EditorState, key: &str) -> Option<String> {
        state
            .students
            .iter()
            .find(|student| student.student_key == key)
            .and_then(|student| student.seat_id.clone())
    }

    fn seat_student(state: &EditorState, seat_id: &str) -> Option<String> {
        state
            .seats
            .iter()
            .find(|seat| seat.seat_id == seat_id)
            .and_then(|seat| seat.student_key.clone())
    }

    fn student_locked(state: &EditorState, key: &str) -> bool {
        state
            .students
            .iter()
            .find(|student| student.student_key == key)
            .map(|student| student.locked)
            .unwrap_or(false)
    }

    #[test]
    fn swap_students_updates_students_and_seats() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op(
                    "swap_students",
                    json!({ "first_student": "s1", "second_student": "s2" }),
                )],
            ),
        )
        .expect("swap should apply");

        assert_eq!(state.revision, 1);
        assert_eq!(state.undo_depth, 1);
        assert_eq!(state.redo_depth, 0);
        assert_eq!(student_seat(&state, "s1").as_deref(), Some("A2"));
        assert_eq!(student_seat(&state, "s2").as_deref(), Some("A1"));
        assert_eq!(student_seat(&state, "s3").as_deref(), Some("B1"));
        assert_eq!(seat_student(&state, "A1").as_deref(), Some("s2"));
        assert_eq!(seat_student(&state, "A2").as_deref(), Some("s1"));
    }

    #[test]
    fn move_student_to_occupied_seat_unseats_previous_student() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op(
                    "move_student",
                    json!({ "student_key": "s1", "seat_id": "A2" }),
                )],
            ),
        )
        .expect("move should apply");

        assert_eq!(student_seat(&state, "s1").as_deref(), Some("A2"));
        assert!(student_seat(&state, "s2").is_none());
        assert_eq!(seat_student(&state, "A2").as_deref(), Some("s1"));
        assert_eq!(seat_student(&state, "A1").as_deref(), None);
    }

    #[test]
    fn seat_and_unseat_student() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op("unseat_student", json!({ "student_key": "s3" }))],
            ),
        )
        .expect("unseat should apply");

        assert!(student_seat(&state, "s3").is_none());
        assert_eq!(seat_student(&state, "B1").as_deref(), None);

        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                1,
                "apply",
                vec![op(
                    "seat_student",
                    json!({ "student_key": "s3", "seat_id": "B2" }),
                )],
            ),
        )
        .expect("seat should apply");

        assert_eq!(student_seat(&state, "s3").as_deref(), Some("B2"));
        assert_eq!(seat_student(&state, "B2").as_deref(), Some("s3"));
    }

    #[test]
    fn lock_and_unlock_student_and_seat() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![
                    op("lock_student", json!({ "student_key": "s1" })),
                    op("lock_seat", json!({ "seat_id": "A1" })),
                ],
            ),
        )
        .expect("lock commands should apply");

        assert!(student_locked(&state, "s1"));
        assert!(
            state
                .seats
                .iter()
                .find(|seat| seat.seat_id == "A1")
                .unwrap()
                .locked
        );

        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                1,
                "apply",
                vec![
                    op("unlock_student", json!({ "student_key": "s1" })),
                    op("unlock_seat", json!({ "seat_id": "A1" })),
                ],
            ),
        )
        .expect("unlock commands should apply");

        assert!(!student_locked(&state, "s1"));
        assert!(
            !state
                .seats
                .iter()
                .find(|seat| seat.seat_id == "A1")
                .unwrap()
                .locked
        );
    }

    #[test]
    fn locks_prevent_swaps_and_moves() {
        let mut draft = test_draft();
        apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "s1" }))],
            ),
        )
        .expect("lock applies");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                1,
                "apply",
                vec![op(
                    "move_student",
                    json!({ "student_key": "s1", "seat_id": "B2" }),
                )],
            ),
        )
        .expect_err("locked student cannot move");
        assert!(error.contains("Student is locked"), "{error}");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-3",
                1,
                "apply",
                vec![op(
                    "swap_students",
                    json!({ "first_student": "s1", "second_student": "s2" }),
                )],
            ),
        )
        .expect_err("locked student cannot swap");
        assert!(error.contains("Student is locked"), "{error}");

        // Undo the lock, then lock the seat instead.
        apply_command(&mut draft, &command("draft-1", "cmd-4", 1, "undo", vec![]))
            .expect("undo applies");
        apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-5",
                2,
                "apply",
                vec![op("lock_seat", json!({ "seat_id": "A2" }))],
            ),
        )
        .expect("seat lock applies");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-6",
                3,
                "apply",
                vec![op(
                    "move_student",
                    json!({ "student_key": "s1", "seat_id": "A2" }),
                )],
            ),
        )
        .expect_err("locked seat cannot change");
        assert!(error.contains("Seat is locked"), "{error}");
    }

    #[test]
    fn swap_requires_both_students_seated() {
        let mut draft = test_draft();
        apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op("unseat_student", json!({ "student_key": "s3" }))],
            ),
        )
        .expect("unseat applies");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                1,
                "apply",
                vec![op(
                    "swap_students",
                    json!({ "first_student": "s1", "second_student": "s3" }),
                )],
            ),
        )
        .expect_err("swapping an unseated student is invalid");
        assert!(error.contains("Both students must be seated"), "{error}");
    }

    #[test]
    fn undo_restores_and_redo_reapplies() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op(
                    "swap_students",
                    json!({ "first_student": "s1", "second_student": "s2" }),
                )],
            ),
        )
        .expect("swap applies");
        assert_eq!(student_seat(&state, "s1").as_deref(), Some("A2"));

        let undone = apply_command(&mut draft, &command("draft-1", "cmd-2", 1, "undo", vec![]))
            .expect("undo applies");
        assert_eq!(undone.revision, 2);
        assert_eq!(undone.undo_depth, 0);
        assert_eq!(undone.redo_depth, 1);
        assert_eq!(student_seat(&undone, "s1").as_deref(), Some("A1"));
        assert_eq!(student_seat(&undone, "s2").as_deref(), Some("A2"));

        let redone = apply_command(&mut draft, &command("draft-1", "cmd-3", 2, "redo", vec![]))
            .expect("redo applies");
        assert_eq!(redone.revision, 3);
        assert_eq!(redone.undo_depth, 1);

        // The redo restores the swapped state and both stacks stay bounded.
        assert_eq!(student_seat(&redone, "s1").as_deref(), Some("A2"));
        assert_eq!(redone.redo_depth, 0);
    }

    #[test]
    fn undo_stack_is_bounded_to_max_undo_depth() {
        // Backend audit F2 (2026-08-12): every edit pushes a full snapshot;
        // the stack must not grow without limit in long sessions.
        let mut draft = test_draft();
        for index in 0..(MAX_UNDO_DEPTH + 20) {
            apply_command(
                &mut draft,
                &command(
                    "draft-1",
                    &format!("cmd-{index}"),
                    index as u64,
                    "apply",
                    vec![op(
                        "move_student",
                        json!({ "student_key": "s1", "seat_id": "A2" }),
                    )],
                ),
            )
            .expect("move applies");
        }
        let state = build_editor_state(&draft);
        assert_eq!(
            state.undo_depth, MAX_UNDO_DEPTH,
            "the oldest snapshots are dropped beyond the bound"
        );
        // The most recent step is still undoable.
        let base_revision = draft.revision();
        let undone = apply_command(
            &mut draft,
            &command("draft-1", "cmd-last", base_revision, "undo", vec![]),
        )
        .expect("undo applies");
        assert_eq!(undone.undo_depth, MAX_UNDO_DEPTH - 1);
    }

    #[test]
    fn undo_redo_raise_when_stack_is_empty() {
        let mut draft = test_draft();
        let error = apply_command(&mut draft, &command("draft-1", "cmd-1", 0, "undo", vec![]))
            .expect_err("empty undo stack");
        assert!(error.contains("undo"), "{error}");

        let error = apply_command(&mut draft, &command("draft-1", "cmd-2", 0, "redo", vec![]))
            .expect_err("empty redo stack");
        assert!(error.contains("redo"), "{error}");

        // Failed history commands must not advance the revision.
        assert_eq!(draft.revision(), 0);
        assert_eq!(draft.undo_depth(), 0);
    }

    #[test]
    fn revision_increments_per_command_batch() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![
                    op("lock_student", json!({ "student_key": "s1" })),
                    op("lock_seat", json!({ "seat_id": "A1" })),
                ],
            ),
        )
        .expect("two-operation batch applies");
        assert_eq!(state.revision, 1);
        assert_eq!(state.undo_depth, 1);

        let undone = apply_command(&mut draft, &command("draft-1", "cmd-2", 1, "undo", vec![]))
            .expect("undo applies");
        assert_eq!(undone.revision, 2);
        assert_eq!(undone.undo_depth, 0);
        assert_eq!(undone.redo_depth, 1);

        let redone = apply_command(&mut draft, &command("draft-1", "cmd-3", 2, "redo", vec![]))
            .expect("redo applies");
        assert_eq!(redone.revision, 3);
        assert_eq!(redone.undo_depth, 1);
        assert_eq!(redone.redo_depth, 0);
    }

    #[test]
    fn base_revision_mismatch_is_rejected() {
        let mut draft = test_draft();
        apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "s1" }))],
            ),
        )
        .expect("first command applies");
        assert_eq!(draft.revision(), 1);

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "s2" }))],
            ),
        )
        .expect_err("stale base revision rejected");
        assert!(error.contains("stale"), "{error}");
        assert!(error.contains("base revision 0"), "{error}");
        assert!(error.contains("current revision 1"), "{error}");

        // Nothing changed.
        assert_eq!(draft.revision(), 1);
        assert_eq!(draft.undo_depth(), 1);
        let state = build_editor_state(&draft);
        assert!(!student_locked(&state, "s2"));
    }

    #[test]
    fn command_batch_is_atomic() {
        let mut draft = test_draft();
        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![
                    op("lock_student", json!({ "student_key": "s1" })),
                    // Fails: X1 is a disabled seat.
                    op(
                        "move_student",
                        json!({ "student_key": "s1", "seat_id": "X1" }),
                    ),
                ],
            ),
        )
        .expect_err("batch must fail atomically");
        assert!(error.contains("Unknown or disabled seat"), "{error}");

        assert_eq!(draft.revision(), 0);
        assert_eq!(draft.undo_depth(), 0);
        let state = build_editor_state(&draft);
        assert!(!student_locked(&state, "s1"));
        assert_eq!(student_seat(&state, "s1").as_deref(), Some("A1"));
    }

    #[test]
    fn unknown_student_and_unknown_or_disabled_seat_errors() {
        let mut draft = test_draft();
        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op(
                    "move_student",
                    json!({ "student_key": "nobody", "seat_id": "A1" }),
                )],
            ),
        )
        .expect_err("unknown student rejected");
        assert!(error.contains("Unknown student"), "{error}");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                0,
                "apply",
                vec![op(
                    "move_student",
                    json!({ "student_key": "s1", "seat_id": "X1" }),
                )],
            ),
        )
        .expect_err("disabled seat rejected");
        assert!(error.contains("Unknown or disabled seat"), "{error}");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-3",
                0,
                "apply",
                vec![op(
                    "move_student",
                    json!({ "student_key": "s1", "seat_id": "NO_SUCH_SEAT" }),
                )],
            ),
        )
        .expect_err("unknown seat rejected");
        assert!(error.contains("Unknown or disabled seat"), "{error}");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-4",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "nobody" }))],
            ),
        )
        .expect_err("unknown student lock rejected");
        assert!(error.contains("Unknown student"), "{error}");

        assert_eq!(draft.revision(), 0);
    }

    #[test]
    fn batch_move_applies_and_rejects_conflicts() {
        let mut draft = test_draft();
        let state = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op(
                    "batch_move",
                    json!({
                        "moves": [
                            { "student_key": "s1", "seat_id": "B2" },
                            { "student_key": "s3", "seat_id": "A1" }
                        ]
                    }),
                )],
            ),
        )
        .expect("batch move applies");
        assert_eq!(student_seat(&state, "s1").as_deref(), Some("B2"));
        assert_eq!(student_seat(&state, "s3").as_deref(), Some("A1"));
        assert_eq!(student_seat(&state, "s2").as_deref(), Some("A2"));

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                1,
                "apply",
                vec![op(
                    "batch_move",
                    json!({
                        "moves": [
                            { "student_key": "s1", "seat_id": "A1" },
                            { "student_key": "s2", "seat_id": "A1" }
                        ]
                    }),
                )],
            ),
        )
        .expect_err("duplicate target seats rejected");
        assert!(error.contains("duplicate target seats"), "{error}");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-3",
                1,
                "apply",
                vec![op(
                    "batch_move",
                    json!({ "moves": [{ "student_key": "s1", "seat_id": "A2" }] }),
                )],
            ),
        )
        .expect_err("occupied-by-outsider rejected");
        assert!(error.contains("outside the batch"), "{error}");
    }

    #[test]
    fn editor_state_json_matches_frontend_contract() {
        let draft = test_draft();
        let state = build_editor_state(&draft);
        let value = serde_json::to_value(&state).expect("state serializes");
        let object = value.as_object().expect("state is an object");

        assert_eq!(object.len(), 9);
        assert_eq!(object["kind"], "seattrellis_editor_state");
        assert_eq!(object["protocol_version"], "1.0");
        assert_eq!(object["draft_id"], "draft-1");
        assert_eq!(object["revision"], 0);
        assert_eq!(object["candidate_id"], "candidate-1");
        assert_eq!(object["undo_depth"], 0);
        assert_eq!(object["redo_depth"], 0);

        let students = object["students"].as_array().expect("students array");
        assert_eq!(students.len(), 3);
        for student in students {
            let fields = student.as_object().expect("student object");
            assert_eq!(fields.len(), 4);
            for key in ["student_key", "display_name", "seat_id", "locked"] {
                assert!(fields.contains_key(key), "missing student field {key}");
            }
        }

        let seats = object["seats"].as_array().expect("seats array");
        assert_eq!(seats.len(), 5);
        for seat in seats {
            let fields = seat.as_object().expect("seat object");
            assert_eq!(fields.len(), 6);
            for key in ["seat_id", "row", "col", "enabled", "student_key", "locked"] {
                assert!(fields.contains_key(key), "missing seat field {key}");
            }
        }

        // Consistency: seat A1 holds s1 and student s1 sits at A1.
        assert_eq!(seats[0]["student_key"], "s1");
        assert_eq!(students[0]["seat_id"], "A1");
    }

    #[test]
    fn command_envelope_parses_and_dispatches_from_json() {
        let raw = json!({
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "cmd-swap",
            "draft_id": "draft-1",
            "base_revision": 0,
            "action": "apply",
            "operations": [
                {
                    "kind": "swap_students",
                    "payload": { "first_student": "s1", "second_student": "s2" }
                }
            ]
        });
        let command: EditorCommandEnvelope = serde_json::from_value(raw).expect("envelope parses");

        let mut draft = test_draft();
        let state = apply_command(&mut draft, &command).expect("command applies");
        assert_eq!(state.revision, 1);
        assert_eq!(student_seat(&state, "s1").as_deref(), Some("A2"));
        assert_eq!(seat_student(&state, "A1").as_deref(), Some("s2"));
    }

    #[test]
    fn different_draft_and_duplicate_command_are_rejected() {
        let mut draft = test_draft();
        let error = apply_command(
            &mut draft,
            &command(
                "other-draft",
                "cmd-1",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "s1" }))],
            ),
        )
        .expect_err("other draft rejected");
        assert!(error.contains("different draft"), "{error}");

        apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "s1" }))],
            ),
        )
        .expect("first command applies");

        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-2",
                1,
                "apply",
                vec![op("unlock_student", json!({ "student_key": "s1" }))],
            ),
        )
        .expect_err("duplicate command id rejected");
        assert!(error.contains("already been applied"), "{error}");

        // Protocol / action / kind mismatches are also rejected clearly.
        let mut bad = command(
            "draft-1",
            "cmd-3",
            1,
            "apply",
            vec![op("lock_student", json!({ "student_key": "s2" }))],
        );
        bad.protocol_version = "9.9".to_string();
        assert!(apply_command(&mut draft, &bad)
            .unwrap_err()
            .contains("protocol version"));

        let mut bad = command(
            "draft-1",
            "cmd-4",
            1,
            "apply",
            vec![op("lock_student", json!({ "student_key": "s2" }))],
        );
        bad.kind = "something_else".to_string();
        assert!(apply_command(&mut draft, &bad)
            .unwrap_err()
            .contains("editor command kind"));

        let mut bad = command(
            "draft-1",
            "cmd-5",
            1,
            "apply",
            vec![op("lock_student", json!({ "student_key": "s2" }))],
        );
        bad.action = "delete".to_string();
        assert!(apply_command(&mut draft, &bad)
            .unwrap_err()
            .contains("unknown editor action"));

        // undo/redo must not carry operations; apply must carry at least one.
        let error = apply_command(
            &mut draft,
            &command(
                "draft-1",
                "cmd-6",
                1,
                "undo",
                vec![op("lock_student", json!({ "student_key": "s2" }))],
            ),
        )
        .expect_err("undo with operations rejected");
        assert!(error.contains("must not contain operations"), "{error}");

        let error = apply_command(&mut draft, &command("draft-1", "cmd-7", 1, "apply", vec![]))
            .expect_err("apply without operations rejected");
        assert!(error.contains("require at least one operation"), "{error}");
    }

    #[test]
    fn draft_store_roundtrip_and_unknown_drafts() {
        let store = new_draft_store();
        let state = create_draft(
            &store,
            "draft-1",
            Some("candidate-1".to_string()),
            &["s1", "s2", "s3"],
            test_seats(),
            &[("s1", "A1"), ("s2", "A2"), ("s3", "B1")],
            None,
        )
        .expect("draft created");
        assert_eq!(state.revision, 0);
        assert_eq!(state.undo_depth, 0);

        assert!(
            create_draft(
                &store,
                "draft-1",
                None,
                &["s1"],
                vec![EditorSeatSpec {
                    seat_id: "A1".to_string(),
                    row: 1,
                    col: 1,
                    enabled: true,
                }],
                &[("s1", "A1")],
                None,
            )
            .is_err(),
            "duplicate draft id rejected"
        );

        let state = apply_command_in_store(
            &store,
            &command(
                "draft-1",
                "cmd-1",
                0,
                "apply",
                vec![op(
                    "swap_students",
                    json!({ "first_student": "s1", "second_student": "s2" }),
                )],
            ),
        )
        .expect("store command applies");
        assert_eq!(state.revision, 1);

        let state = fetch_state(&store, "draft-1").expect("draft fetched");
        assert_eq!(student_seat(&state, "s1").as_deref(), Some("A2"));

        assert!(fetch_state(&store, "nope")
            .unwrap_err()
            .contains("unknown editor draft"));
        assert!(apply_command_in_store(
            &store,
            &command(
                "nope",
                "cmd-2",
                0,
                "apply",
                vec![op("lock_student", json!({ "student_key": "s1" }))],
            ),
        )
        .unwrap_err()
        .contains("unknown editor draft"));
    }

    #[test]
    fn draft_store_evicts_the_oldest_draft_at_the_cap() {
        // alpha.2/M7 item: a long-lived server must not accumulate one
        // draft per generation forever. Draft ids are monotonic
        // (`draft-<nanos><seq>`), so the smallest id is the oldest.
        let store = new_draft_store();
        for index in 0..(MAX_EDITOR_DRAFTS + 8) {
            let id = format!("draft-{index:06}");
            create_draft(
                &store,
                &id,
                Some(id.clone()),
                &["s1", "s2", "s3"],
                test_seats(),
                &[("s1", "A1"), ("s2", "A2"), ("s3", "B1")],
                None,
            )
            .expect("draft created");
        }
        let guard = store.lock().unwrap();
        assert_eq!(guard.len(), MAX_EDITOR_DRAFTS, "store stays at the cap");
        assert!(
            !guard.contains_key("draft-000000"),
            "oldest draft evicted first"
        );
        assert!(
            !guard.contains_key("draft-000007"),
            "the oldest 8 of 72 inserts are gone"
        );
        assert!(
            guard.contains_key("draft-000008"),
            "the cap counts from the oldest"
        );
        assert!(
            guard.contains_key(&format!("draft-{:06}", MAX_EDITOR_DRAFTS + 7)),
            "newest drafts survive"
        );
    }

    #[test]
    fn create_draft_mirrors_roster_display_names() {
        let store = new_draft_store();
        let mut names = HashMap::new();
        names.insert("s1".to_string(), "Alpha".to_string());
        names.insert("s2".to_string(), "Beta".to_string());
        let state = create_draft(
            &store,
            "draft-names",
            Some("candidate-1".to_string()),
            &["s1", "s2", "s3"],
            test_seats(),
            &[("s1", "A1"), ("s2", "A2"), ("s3", "B1")],
            Some(&names),
        )
        .expect("draft created");
        // Named students keep their roster names; unknown keys fall back to
        // the key (mirroring the Python oracle's editor state).
        let by_key: HashMap<_, _> = state
            .students
            .iter()
            .map(|student| (student.student_key.as_str(), student.display_name.as_str()))
            .collect();
        assert_eq!(by_key.get("s1"), Some(&"Alpha"));
        assert_eq!(by_key.get("s2"), Some(&"Beta"));
        assert_eq!(by_key.get("s3"), Some(&"s3"));
    }

    #[test]
    fn draft_store_supports_concurrent_access() {
        let store = Arc::new(new_draft_store());
        let mut handles = Vec::new();
        for index in 0..4 {
            let store = Arc::clone(&store);
            handles.push(thread::spawn(move || {
                let draft_id = format!("draft-{index}");
                let state = create_draft(
                    &store,
                    draft_id.clone(),
                    None,
                    &["s1"],
                    vec![EditorSeatSpec {
                        seat_id: "A1".to_string(),
                        row: 1,
                        col: 1,
                        enabled: true,
                    }],
                    &[("s1", "A1")],
                    None,
                )
                .expect("concurrent draft created");
                assert_eq!(state.revision, 0);
                let fetched = fetch_state(&store, &draft_id).expect("draft fetched");
                assert_eq!(fetched.draft_id, draft_id);
            }));
        }
        for handle in handles {
            handle.join().expect("worker thread completes");
        }
        assert_eq!(
            get_draft(&store, "draft-0")
                .expect("draft exists")
                .students()
                .len(),
            1
        );
    }
}
