//! Visual classroom layout editing domain module for the SeatTrellis desktop
//! backend.
//!
//! This module ports the Python layout editor (`src/seattrellis/api/layouts.py`
//! and `src/seattrellis/application/layout_editor.py`) to Rust so the React
//! workbench's layout editor (`LayoutEditorPanel`) can run in-process against
//! the native backend. It is self-contained: the server wires it up by calling
//! the JSON entry points below.
//!
//! # Wire contract
//!
//! * [`LayoutStateResponse`] serializes to exactly the frontend
//!   `LayoutStateResponse` shape in `clients/web/src/api/types.ts` (`kind` /
//!   `api_version` / `draft_id` / `revision` / `name` / `rows` / `columns` /
//!   `cells` / `undo_depth` / `redo_depth` / `usable_seat_count`).
//! * [`LayoutCommandRequest`] deserializes the frontend `LayoutCommand` shape
//!   and dispatches on each operation's `kind` + `payload`.
//! * [`CompiledLayoutResponse`] serializes the strict solver `Layout` shape
//!   (`layout_id` / `name` / `seats` / `adjacency`) — the same shape
//!   `crate::room_templates` emits for the native solver.
//!
//! # Semantics
//!
//! A [`LayoutDraft`] is a permissive grid: a cell may be a `seat`, `aisle`,
//! `platform`, or `empty`, and a draft may temporarily hold no usable seats
//! while a teacher reshapes the room. Conversion to the strict solver layout
//! performs the validation at the workflow boundary. Commands are applied
//! atomically — either every operation succeeds and the revision advances by
//! one (one snapshot is pushed on the undo stack), or the draft is left
//! untouched. Undo/redo are snapshot-based over whole commands, mirroring the
//! Python draft. A command that targets a stale `base_revision` or a
//! `command_id` that was already applied is rejected as a revision conflict.
//!
//! # Concurrency
//!
//! [`LayoutDraft`] is `Send + Sync`, and [`LayoutDraftStore`] (a
//! `Mutex<HashMap<..>>`, matching `seattrellis_domain::editing`) is provided so a
//! thread-per-connection server can share drafts safely. The JSON entry points
//! below dispatch through an in-process global store; the `*_in_store`
//! variants accept an explicit store for testing and embedding.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use seattrellis_core::models::{AdjacencyConfig, Layout, Seat};
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

/// `kind` discriminator for layout state documents.
pub const LAYOUT_STATE_KIND: &str = "seattrellis_layout_state";
/// Major API version shared by layout responses.
pub const LAYOUT_API_VERSION: &str = "1";

/// Maximum grid rows for a layout draft (mirrors `MAX_LAYOUT_ROWS`).
pub const MAX_LAYOUT_ROWS: i32 = 50;
/// Maximum grid columns for a layout draft.
pub const MAX_LAYOUT_COLUMNS: i32 = 50;
/// Maximum number of grid cells (`rows * columns`).
pub const MAX_LAYOUT_CELLS: i32 = 1_000;

const ACTION_APPLY: &str = "apply";
const ACTION_UNDO: &str = "undo";
const ACTION_REDO: &str = "redo";

/// Supported layout operation kinds, in the order used by error messages.
const OPERATION_KINDS: &[&str] = &[
    "set_cell",
    "insert_row",
    "delete_row",
    "insert_column",
    "delete_column",
    "translate",
    "mirror_horizontal",
    "flip_vertical",
];

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// The kind of one visible cell in the layout editor grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutCellKind {
    Seat,
    Aisle,
    Platform,
    Empty,
}

impl LayoutCellKind {
    /// The wire name of this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            LayoutCellKind::Seat => "seat",
            LayoutCellKind::Aisle => "aisle",
            LayoutCellKind::Platform => "platform",
            LayoutCellKind::Empty => "empty",
        }
    }
}

/// One visible cell in the layout editor grid (frontend `LayoutCellState`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutCell {
    pub row: i32,
    pub column: i32,
    pub kind: LayoutCellKind,
    #[serde(default)]
    pub seat_id: Option<String>,
}

/// The full editable state for one layout draft (frontend `LayoutStateResponse`).
#[derive(Debug, Clone, Serialize)]
pub struct LayoutStateResponse {
    pub kind: String,
    pub api_version: String,
    pub draft_id: String,
    pub revision: u64,
    pub name: String,
    pub rows: i32,
    pub columns: i32,
    pub cells: Vec<LayoutCell>,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub usable_seat_count: usize,
}

/// One editing operation inside an apply command (frontend `LayoutOperation`).
#[derive(Debug, Clone, Deserialize)]
pub struct LayoutOperationRequest {
    pub kind: String,
    #[serde(default)]
    pub payload: JsonMap<String, JsonValue>,
}

/// One layout command (frontend `LayoutCommand`).
#[derive(Debug, Clone, Deserialize)]
pub struct LayoutCommandRequest {
    pub command_id: String,
    pub draft_id: String,
    pub base_revision: u64,
    pub action: String,
    #[serde(default)]
    pub operation: Option<LayoutOperationRequest>,
}

/// The compiled solver layout for a draft (frontend `CompiledLayoutResponse`).
#[derive(Debug, Clone, Serialize)]
pub struct CompiledLayoutResponse {
    pub api_version: String,
    pub draft_id: String,
    pub revision: u64,
    pub layout: Layout,
}

// ---------------------------------------------------------------------------
// Draft model
// ---------------------------------------------------------------------------

/// The new grid produced by an operation: `(rows, columns, cells)`.
type LayoutGrid = (i32, i32, HashMap<(i32, i32), LayoutCell>);

/// A full copy of the editable state used for snapshot-based undo/redo.
///
/// Stores the complete row-major grid (including `empty` cells) so that any
/// visual change — including filling or clearing a cell — registers as a
/// history step.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LayoutSnapshot {
    rows: i32,
    columns: i32,
    cells: Vec<LayoutCell>,
}

/// Mutable classroom grid with atomic commands and undo/redo history.
///
/// Only non-`empty` cells are stored; the surrounding grid is the canvas
/// background and is reconstructed on transport via [`LayoutDraft::ordered_cells`].
#[derive(Debug, Clone)]
pub struct LayoutDraft {
    draft_id: String,
    name: String,
    revision: u64,
    rows: i32,
    columns: i32,
    cells: HashMap<(i32, i32), LayoutCell>,
    undo_stack: Vec<LayoutSnapshot>,
    redo_stack: Vec<LayoutSnapshot>,
    applied_command_ids: HashSet<String>,
}

impl LayoutDraft {
    /// Build and validate a draft. `cells` may omit grid positions (they
    /// render as `empty`); every provided cell must be a valid position whose
    /// `row`/`column` match its map key.
    fn new(
        draft_id: String,
        name: String,
        rows: i32,
        columns: i32,
        cells: HashMap<(i32, i32), LayoutCell>,
    ) -> Result<LayoutDraft, String> {
        let name = name.trim().to_string();
        let draft_id = draft_id.trim().to_string();
        if name.is_empty() {
            return Err("Layout name cannot be empty.".to_string());
        }
        if draft_id.is_empty() {
            return Err("Layout draft_id cannot be empty.".to_string());
        }
        validate_dimensions(rows, columns)?;
        let mut normalized: HashMap<(i32, i32), LayoutCell> = HashMap::new();
        for cell in cells.into_values() {
            validate_cell(&cell)?;
            if !(1..=rows).contains(&cell.row) || !(1..=columns).contains(&cell.column) {
                return Err(format!(
                    "Cell position row {}, column {} is outside the layout.",
                    cell.row, cell.column
                ));
            }
            normalized.insert((cell.row, cell.column), cell);
        }
        validate_unique_seat_ids(&normalized)?;
        Ok(LayoutDraft {
            draft_id,
            name,
            revision: 0,
            rows,
            columns,
            cells: normalized,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            applied_command_ids: HashSet::new(),
        })
    }

    /// A stable row-major view of the grid for transport and rendering, with
    /// missing positions materialized as `empty` cells.
    pub fn ordered_cells(&self) -> Vec<LayoutCell> {
        ordered_cells_for(self.rows, self.columns, &self.cells)
    }

    /// Apply one operation atomically, returning the new revision. On success
    /// the revision advances by exactly one and the previous state is pushed
    /// onto the undo stack (the redo stack is cleared). A no-op operation that
    /// changes nothing advances nothing.
    fn apply(&mut self, operation: &LayoutOperationRequest) -> Result<u64, String> {
        let (rows, columns, cells) = self.apply_operation(operation)?;
        validate_dimensions(rows, columns)?;
        validate_unique_seat_ids(&cells)?;
        let before = self.snapshot();
        let after = LayoutSnapshot {
            rows,
            columns,
            cells: ordered_cells_for(rows, columns, &cells),
        };
        if after != before {
            self.undo_stack.push(before);
            self.redo_stack.clear();
            self.revision += 1;
            self.rows = rows;
            self.columns = columns;
            self.cells = cells;
        }
        Ok(self.revision)
    }

    /// Pop the undo stack and return to the previous state, pushing the current
    /// state onto the redo stack.
    fn undo(&mut self) -> Result<u64, String> {
        let previous = self
            .undo_stack
            .pop()
            .ok_or_else(|| "There is no layout change to undo.".to_string())?;
        let current = self.snapshot();
        self.restore_snapshot(&previous);
        self.redo_stack.push(current);
        self.revision += 1;
        Ok(self.revision)
    }

    /// Pop the redo stack and replay a following state, pushing the current
    /// state back onto the undo stack.
    fn redo(&mut self) -> Result<u64, String> {
        let following = self
            .redo_stack
            .pop()
            .ok_or_else(|| "There is no layout change to redo.".to_string())?;
        let current = self.snapshot();
        self.restore_snapshot(&following);
        self.undo_stack.push(current);
        self.revision += 1;
        Ok(self.revision)
    }

    /// Validate and compile the draft into the strict solver `Layout` shape.
    fn to_layout(&self) -> Result<Layout, String> {
        let mut nodes: Vec<Seat> = Vec::new();
        for cell in self.ordered_cells() {
            match cell.kind {
                LayoutCellKind::Seat => nodes.push(Seat {
                    seat_id: cell.seat_id.unwrap_or_default(),
                    row: cell.row,
                    col: cell.column,
                    enabled: true,
                    near_platform: self.has_platform_in_front(cell.row),
                    ..Seat::new("", cell.row, cell.column)
                }),
                LayoutCellKind::Aisle | LayoutCellKind::Platform => {
                    let kind = cell.kind.as_str();
                    nodes.push(Seat {
                        seat_id: format!(
                            "{}-R{}C{}",
                            kind.to_ascii_uppercase(),
                            cell.row,
                            cell.column
                        ),
                        row: cell.row,
                        col: cell.column,
                        enabled: false,
                        zone: Some(kind.to_string()),
                        ..Seat::new("", cell.row, cell.column)
                    });
                }
                LayoutCellKind::Empty => {}
            }
        }
        if !nodes.iter().any(|seat| seat.enabled) {
            return Err("The classroom needs at least one seat before it can be used.".to_string());
        }
        Ok(Layout {
            layout_id: self.draft_id.trim().to_string(),
            name: self.name.clone(),
            seats: nodes,
            adjacency: AdjacencyConfig::default(),
        })
    }

    /// Compute the new grid for an operation without mutating the draft. Any
    /// error leaves the draft untouched.
    fn apply_operation(&self, operation: &LayoutOperationRequest) -> Result<LayoutGrid, String> {
        let mut cells = self.cells.clone();
        let mut rows = self.rows;
        let mut columns = self.columns;
        match operation.kind.as_str() {
            "set_cell" => {
                let row = required_int(&operation.payload, "set_cell", "row")?;
                let column = required_int(&operation.payload, "set_cell", "column")?;
                self.require_position(row, column)?;
                let raw_kind = operation
                    .payload
                    .get("kind")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| "Cell kind must be seat, aisle, platform, or empty.".to_string())?;
                let kind = parse_cell_kind(raw_kind)?;
                let seat_id = if kind == LayoutCellKind::Seat {
                    match operation.payload.get("seat_id") {
                        Some(JsonValue::String(value)) if !value.trim().is_empty() => {
                            Some(value.trim().to_string())
                        }
                        _ => Some(self.next_seat_id(row, column)),
                    }
                } else {
                    None
                };
                cells.insert(
                    (row, column),
                    LayoutCell {
                        row,
                        column,
                        kind,
                        seat_id,
                    },
                );
            }
            "insert_row" => {
                let index = required_int(&operation.payload, "insert_row", "index")?;
                if !(1..=self.rows + 1).contains(&index) {
                    return Err("Inserted row index is outside the layout.".to_string());
                }
                validate_dimensions(self.rows + 1, self.columns)?;
                let mut shifted = HashMap::with_capacity(cells.len());
                for cell in self.cells.values() {
                    let new_row = cell.row + i32::from(cell.row >= index);
                    shifted.insert(
                        (new_row, cell.column),
                        LayoutCell {
                            row: new_row,
                            column: cell.column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = shifted;
                rows += 1;
            }
            "delete_row" => {
                if self.rows == 1 {
                    return Err("A layout must keep at least one row.".to_string());
                }
                let index = required_int(&operation.payload, "delete_row", "index")?;
                if !(1..=self.rows).contains(&index) {
                    return Err("Deleted row index is outside the layout.".to_string());
                }
                let mut shifted = HashMap::with_capacity(cells.len());
                for cell in self.cells.values() {
                    if cell.row == index {
                        continue;
                    }
                    let new_row = cell.row - i32::from(cell.row > index);
                    shifted.insert(
                        (new_row, cell.column),
                        LayoutCell {
                            row: new_row,
                            column: cell.column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = shifted;
                rows -= 1;
            }
            "insert_column" => {
                let index = required_int(&operation.payload, "insert_column", "index")?;
                if !(1..=self.columns + 1).contains(&index) {
                    return Err("Inserted column index is outside the layout.".to_string());
                }
                validate_dimensions(self.rows, self.columns + 1)?;
                let mut shifted = HashMap::with_capacity(cells.len());
                for cell in self.cells.values() {
                    let new_column = cell.column + i32::from(cell.column >= index);
                    shifted.insert(
                        (cell.row, new_column),
                        LayoutCell {
                            row: cell.row,
                            column: new_column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = shifted;
                columns += 1;
            }
            "delete_column" => {
                if self.columns == 1 {
                    return Err("A layout must keep at least one column.".to_string());
                }
                let index = required_int(&operation.payload, "delete_column", "index")?;
                if !(1..=self.columns).contains(&index) {
                    return Err("Deleted column index is outside the layout.".to_string());
                }
                let mut shifted = HashMap::with_capacity(cells.len());
                for cell in self.cells.values() {
                    if cell.column == index {
                        continue;
                    }
                    let new_column = cell.column - i32::from(cell.column > index);
                    shifted.insert(
                        (cell.row, new_column),
                        LayoutCell {
                            row: cell.row,
                            column: new_column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = shifted;
                columns -= 1;
            }
            "translate" => {
                let row_delta = required_int(&operation.payload, "translate", "row_delta")?;
                let column_delta =
                    required_int(&operation.payload, "translate", "column_delta")?;
                let mut moved: HashMap<(i32, i32), LayoutCell> = HashMap::new();
                for cell in self.cells.values() {
                    // Empty cells are the canvas background: moving them would
                    // make a useful one-cell shift impossible whenever the
                    // draft has an empty border, so only physical cells move.
                    if cell.kind == LayoutCellKind::Empty {
                        continue;
                    }
                    let row = cell.row + row_delta;
                    let column = cell.column + column_delta;
                    self.require_position(row, column)?;
                    moved.insert(
                        (row, column),
                        LayoutCell {
                            row,
                            column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = moved;
            }
            "mirror_horizontal" => {
                let mut mirrored: HashMap<(i32, i32), LayoutCell> = HashMap::new();
                for cell in self.cells.values() {
                    let column = self.columns + 1 - cell.column;
                    mirrored.insert(
                        (cell.row, column),
                        LayoutCell {
                            row: cell.row,
                            column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = mirrored;
            }
            "flip_vertical" => {
                let mut flipped: HashMap<(i32, i32), LayoutCell> = HashMap::new();
                for cell in self.cells.values() {
                    let row = self.rows + 1 - cell.row;
                    flipped.insert(
                        (row, cell.column),
                        LayoutCell {
                            row,
                            column: cell.column,
                            kind: cell.kind,
                            seat_id: cell.seat_id.clone(),
                        },
                    );
                }
                cells = flipped;
            }
            _ => {
                return Err(format!(
                    "Unsupported layout command: {:?}. Supported: {}.",
                    operation.kind,
                    OPERATION_KINDS.join(", ")
                ));
            }
        }
        Ok((rows, columns, cells))
    }

    fn snapshot(&self) -> LayoutSnapshot {
        LayoutSnapshot {
            rows: self.rows,
            columns: self.columns,
            cells: self.ordered_cells(),
        }
    }

    fn restore_snapshot(&mut self, snapshot: &LayoutSnapshot) {
        self.rows = snapshot.rows;
        self.columns = snapshot.columns;
        self.cells = snapshot
            .cells
            .iter()
            .filter(|cell| cell.kind != LayoutCellKind::Empty)
            .map(|cell| ((cell.row, cell.column), cell.clone()))
            .collect();
    }

    fn require_position(&self, row: i32, column: i32) -> Result<(), String> {
        if !(1..=self.rows).contains(&row) || !(1..=self.columns).contains(&column) {
            return Err(format!(
                "Cell position row {row}, column {column} is outside the layout."
            ));
        }
        Ok(())
    }

    fn next_seat_id(&self, row: i32, column: i32) -> String {
        let existing: HashSet<String> = self
            .cells
            .values()
            .filter_map(|cell| cell.seat_id.clone())
            .collect();
        let preferred = format!("R{row}C{column}");
        if !existing.contains(&preferred) {
            return preferred;
        }
        let mut suffix = 2;
        loop {
            let candidate = format!("{preferred}-{suffix}");
            if !existing.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn has_platform_in_front(&self, row: i32) -> bool {
        self.cells
            .values()
            .any(|cell| cell.kind == LayoutCellKind::Platform && cell.row < row)
    }
}

// ---------------------------------------------------------------------------
// Draft store
// ---------------------------------------------------------------------------

/// A thread-safe registry of in-flight layout drafts (matches `editing.rs`).
pub type LayoutDraftStore = Mutex<HashMap<String, LayoutDraft>>;

/// Create an empty draft store.
pub fn new_layout_draft_store() -> LayoutDraftStore {
    Mutex::new(HashMap::new())
}

fn global_store() -> &'static LayoutDraftStore {
    static STORE: OnceLock<LayoutDraftStore> = OnceLock::new();
    STORE.get_or_init(new_layout_draft_store)
}

/// Create a layout draft from a `CreateLayoutDraftRequest` JSON document and
/// return its initial [`LayoutStateResponse`] JSON.
///
/// Exactly one source is allowed: `template_id`, `layout`, or both `rows` and
/// `columns`. Template and layout sources map their seats onto editor cells
/// (enabled seats become `seat` cells; disabled `aisle`/`platform` cells keep
/// their kind; everything else renders empty).
pub fn create_layout_draft(store: &LayoutDraftStore, request_json: &str) -> Result<String, String> {
    let value: JsonValue = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid layout draft request: {error}"))?;
    let draft = build_draft_from_request(&value)?;
    let state = build_state(&draft);
    let json = serde_json::to_string(&state)
        .map_err(|error| format!("could not serialize layout state: {error}"))?;
    let mut guard = store
        .lock()
        .map_err(|_| "layout draft store is poisoned".to_string())?;
    if guard.contains_key(&draft.draft_id) {
        return Err(format!(
            "a layout draft already exists with id: {}",
            draft.draft_id
        ));
    }
    guard.insert(draft.draft_id.clone(), draft);
    Ok(json)
}

/// Return the current [`LayoutStateResponse`] JSON for a stored draft.
pub fn get_layout_state(store: &LayoutDraftStore, draft_id: &str) -> Result<String, String> {
    let cleaned = clean_id(draft_id)?;
    let guard = store
        .lock()
        .map_err(|_| "layout draft store is poisoned".to_string())?;
    let draft = guard
        .get(&cleaned)
        .ok_or_else(|| format!("unknown layout draft: {cleaned}"))?;
    let state = build_state(draft);
    serde_json::to_string(&state)
        .map_err(|error| format!("could not serialize layout state: {error}"))
}

/// Dispatch a `LayoutCommand` JSON document against a stored draft and return
/// the resulting [`LayoutStateResponse`] JSON.
///
/// The draft id is taken from the URL path; a command whose `draft_id` does
/// not match it, whose `command_id` was already applied, or whose
/// `base_revision` is stale is rejected as a revision conflict. Undo/redo are
/// snapshot-based and share the same revision-conflict checks.
pub fn dispatch_layout_command(
    store: &LayoutDraftStore,
    draft_id: &str,
    command_json: &str,
) -> Result<String, String> {
    let cleaned = clean_id(draft_id)?;
    let command: LayoutCommandRequest = serde_json::from_str(command_json)
        .map_err(|error| format!("invalid layout command: {error}"))?;
    validate_command(&command)?;
    let mut guard = store
        .lock()
        .map_err(|_| "layout draft store is poisoned".to_string())?;
    let draft = guard
        .get_mut(&cleaned)
        .ok_or_else(|| format!("unknown layout draft: {cleaned}"))?;
    if command.draft_id.trim() != cleaned {
        return Err("The layout command targets a different draft.".to_string());
    }
    if draft.applied_command_ids.contains(&command.command_id) {
        return Err("This layout command has already been applied.".to_string());
    }
    if command.base_revision != draft.revision {
        return Err(format!(
            "The layout command targets a stale revision: base revision {}, current revision {}.",
            command.base_revision, draft.revision
        ));
    }
    match command.action.as_str() {
        ACTION_APPLY => {
            let operation = command
                .operation
                .as_ref()
                .ok_or_else(|| "Apply commands require an operation.".to_string())?;
            draft.apply(operation)?;
        }
        ACTION_UNDO => {
            draft.undo()?;
        }
        ACTION_REDO => {
            draft.redo()?;
        }
        _ => {
            return Err(format!(
                "Unsupported layout command action: {:?}.",
                command.action
            ));
        }
    }
    draft.applied_command_ids.insert(command.command_id.clone());
    let state = build_state(draft);
    serde_json::to_string(&state)
        .map_err(|error| format!("could not serialize layout state: {error}"))
}

/// Compile a stored draft into the strict solver layout and return the
/// [`CompiledLayoutResponse`] JSON.
pub fn compile_layout(store: &LayoutDraftStore, draft_id: &str) -> Result<String, String> {
    let cleaned = clean_id(draft_id)?;
    let guard = store
        .lock()
        .map_err(|_| "layout draft store is poisoned".to_string())?;
    let draft = guard
        .get(&cleaned)
        .ok_or_else(|| format!("unknown layout draft: {cleaned}"))?;
    let layout = draft.to_layout()?;
    let response = CompiledLayoutResponse {
        api_version: LAYOUT_API_VERSION.to_string(),
        draft_id: draft.draft_id.clone(),
        revision: draft.revision,
        layout,
    };
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize compiled layout: {error}"))
}

/// Remove a stored draft immediately, returning whether it existed.
pub fn delete_layout_draft_in_store(store: &LayoutDraftStore, draft_id: &str) -> bool {
    let cleaned = match clean_id(draft_id) {
        Ok(cleaned) => cleaned,
        Err(_) => return false,
    };
    store
        .lock()
        .map(|mut guard| guard.remove(&cleaned).is_some())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Global JSON entry points (the server-facing API)
// ---------------------------------------------------------------------------

/// Create a layout draft through the in-process global store.
pub fn create_layout_draft_json(request_json: &str) -> Result<String, String> {
    create_layout_draft(global_store(), request_json)
}

/// Fetch a layout draft's state through the in-process global store.
pub fn get_layout_state_json(draft_id: &str) -> Result<String, String> {
    get_layout_state(global_store(), draft_id)
}

/// Dispatch a layout command through the in-process global store.
pub fn dispatch_layout_command_json(draft_id: &str, command_json: &str) -> Result<String, String> {
    dispatch_layout_command(global_store(), draft_id, command_json)
}

/// Compile a layout draft through the in-process global store.
pub fn compile_layout_draft_json(draft_id: &str) -> Result<String, String> {
    compile_layout(global_store(), draft_id)
}

/// Delete a layout draft from the in-process global store. Returns whether it
/// existed.
pub fn delete_layout_draft(draft_id: &str) -> bool {
    delete_layout_draft_in_store(global_store(), draft_id)
}

// ---------------------------------------------------------------------------
// Draft construction
// ---------------------------------------------------------------------------

/// Build a validated draft from a parsed `CreateLayoutDraftRequest`.
fn build_draft_from_request(value: &JsonValue) -> Result<LayoutDraft, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "create layout request must be a JSON object".to_string())?;

    let name = match object.get("name") {
        None | Some(JsonValue::Null) => "Classroom".to_string(),
        Some(JsonValue::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err("Layout name cannot be empty.".to_string());
            }
            trimmed.to_string()
        }
        Some(_) => return Err("Layout name must be a string.".to_string()),
    };

    let template_id = optional_trimmed_str(object, "template_id")?;
    let layout_value = match object.get("layout") {
        Some(value) if !value.is_null() => Some(value),
        _ => None,
    };
    let rows = optional_int(object, "rows")?;
    let columns = optional_int(object, "columns")?;

    let sources = i32::from(template_id.is_some())
        + i32::from(layout_value.is_some())
        + i32::from(rows.is_some() || columns.is_some());
    if sources != 1 {
        return Err("Choose one template, existing layout, or rows and columns.".to_string());
    }

    let (draft_rows, draft_columns, cells) = if let Some(template_id) = &template_id {
        let grid = crate::room_templates::room_template_grid(template_id)?;
        (
            grid.rows,
            grid.grid_columns,
            cells_from_template_layout(&grid.layout),
        )
    } else if let Some(layout_value) = layout_value {
        cells_from_layout_value(layout_value)?
    } else {
        let rows = rows
            .ok_or_else(|| "Both rows and columns are required.".to_string())?;
        let columns = columns
            .ok_or_else(|| "Both rows and columns are required.".to_string())?;
        (rows, columns, rectangular_cells(rows, columns))
    };

    LayoutDraft::new(new_draft_id(), name, draft_rows, draft_columns, cells)
}

/// Map a template's full layout (every cell, including disabled aisles) onto
/// editor cells.
fn cells_from_template_layout(layout: &crate::room_templates::Layout) -> HashMap<(i32, i32), LayoutCell> {
    let mut cells = HashMap::new();
    for seat in &layout.seats {
        let kind = if seat.enabled {
            LayoutCellKind::Seat
        } else {
            match seat.zone.as_deref() {
                Some("platform") => LayoutCellKind::Platform,
                Some("aisle") => LayoutCellKind::Aisle,
                _ => LayoutCellKind::Empty,
            }
        };
        let seat_id = if kind == LayoutCellKind::Seat {
            Some(seat.seat_id.clone())
        } else {
            None
        };
        cells.insert(
            (seat.row, seat.col),
            LayoutCell {
                row: seat.row,
                column: seat.col,
                kind,
                seat_id,
            },
        );
    }
    cells
}

/// Map an existing solver `Layout` JSON document (the create request's
/// `layout` field) onto editor cells, returning the grid dimensions from the
/// maximum seat position.
fn cells_from_layout_value(value: &JsonValue) -> Result<LayoutGrid, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "layout must be a JSON object".to_string())?;
    let seats = object
        .get("seats")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "layout must contain a seats array".to_string())?;

    let mut cells = HashMap::new();
    let mut max_row = 0;
    let mut max_column = 0;
    for (index, seat) in seats.iter().enumerate() {
        let seat_object = seat
            .as_object()
            .ok_or_else(|| format!("layout seat {} must be an object", index + 1))?;
        let row = required_int_from_object(seat_object, "row")?;
        let column = required_int_from_object(seat_object, "col")?;
        let enabled = seat_object
            .get("enabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true);
        let zone = seat_object
            .get("zone")
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        let cell_type = seat_object
            .get("attributes")
            .and_then(JsonValue::as_object)
            .and_then(|attributes| attributes.get("cell_type"))
            .and_then(JsonValue::as_str)
            .map(str::to_string);

        let (kind, seat_id) = if enabled {
            let seat_id = seat_object
                .get("seat_id")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim();
            if seat_id.is_empty() {
                (LayoutCellKind::Seat, None)
            } else {
                (LayoutCellKind::Seat, Some(seat_id.to_string()))
            }
        } else {
            let cell_type = cell_type.as_deref().unwrap_or("").trim().to_ascii_lowercase();
            if cell_type == "platform" || zone.as_deref() == Some("platform") {
                (LayoutCellKind::Platform, None)
            } else if cell_type == "aisle" || zone.as_deref() == Some("aisle") {
                (LayoutCellKind::Aisle, None)
            } else {
                (LayoutCellKind::Empty, None)
            }
        };

        max_row = max_row.max(row);
        max_column = max_column.max(column);
        cells.insert(
            (row, column),
            LayoutCell {
                row,
                column,
                kind,
                seat_id,
            },
        );
    }
    Ok((max_row, max_column, cells))
}

/// A draft grid with every cell filled with an enabled seat.
fn rectangular_cells(rows: i32, columns: i32) -> HashMap<(i32, i32), LayoutCell> {
    let mut cells = HashMap::new();
    for row in 1..=rows {
        for column in 1..=columns {
            cells.insert(
                (row, column),
                LayoutCell {
                    row,
                    column,
                    kind: LayoutCellKind::Seat,
                    seat_id: Some(format!("R{row}C{column}")),
                },
            );
        }
    }
    cells
}

// ---------------------------------------------------------------------------
// State serialization
// ---------------------------------------------------------------------------

fn build_state(draft: &LayoutDraft) -> LayoutStateResponse {
    let cells = draft.ordered_cells();
    let usable_seat_count = cells
        .iter()
        .filter(|cell| cell.kind == LayoutCellKind::Seat)
        .count();
    LayoutStateResponse {
        kind: LAYOUT_STATE_KIND.to_string(),
        api_version: LAYOUT_API_VERSION.to_string(),
        draft_id: draft.draft_id.clone(),
        revision: draft.revision,
        name: draft.name.clone(),
        rows: draft.rows,
        columns: draft.columns,
        cells,
        undo_depth: draft.undo_stack.len(),
        redo_depth: draft.redo_stack.len(),
        usable_seat_count,
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn clean_id(draft_id: &str) -> Result<String, String> {
    let cleaned = draft_id.trim();
    if cleaned.is_empty() {
        return Err("layout draft id cannot be empty".to_string());
    }
    Ok(cleaned.to_string())
}

fn validate_command(command: &LayoutCommandRequest) -> Result<(), String> {
    if command.command_id.trim().is_empty() {
        return Err("Layout command command_id cannot be empty.".to_string());
    }
    if command.draft_id.trim().is_empty() {
        return Err("Layout command draft_id cannot be empty.".to_string());
    }
    match command.action.as_str() {
        ACTION_APPLY => {
            if command.operation.is_none() {
                return Err("Apply commands require an operation.".to_string());
            }
        }
        ACTION_UNDO | ACTION_REDO => {
            if command.operation.is_some() {
                return Err(format!(
                    "{} commands cannot contain an operation.",
                    command.action
                ));
            }
        }
        _ => {
            return Err(format!(
                "Unsupported layout command action: {:?}.",
                command.action
            ));
        }
    }
    Ok(())
}

fn parse_cell_kind(raw: &str) -> Result<LayoutCellKind, String> {
    match raw {
        "seat" => Ok(LayoutCellKind::Seat),
        "aisle" => Ok(LayoutCellKind::Aisle),
        "platform" => Ok(LayoutCellKind::Platform),
        "empty" => Ok(LayoutCellKind::Empty),
        _ => Err("Cell kind must be seat, aisle, platform, or empty.".to_string()),
    }
}

fn validate_dimensions(rows: i32, columns: i32) -> Result<(), String> {
    if !(1..=MAX_LAYOUT_ROWS).contains(&rows) {
        return Err(format!("Layout rows must be between 1 and {MAX_LAYOUT_ROWS}."));
    }
    if !(1..=MAX_LAYOUT_COLUMNS).contains(&columns) {
        return Err(format!(
            "Layout columns must be between 1 and {MAX_LAYOUT_COLUMNS}."
        ));
    }
    if rows * columns > MAX_LAYOUT_CELLS {
        return Err(format!(
            "Layout grids may contain at most {MAX_LAYOUT_CELLS} cells."
        ));
    }
    Ok(())
}

fn validate_cell(cell: &LayoutCell) -> Result<(), String> {
    if cell.row < 1 || cell.column < 1 {
        return Err("Layout cell positions must be positive.".to_string());
    }
    match cell.kind {
        LayoutCellKind::Seat => {
            let has_id = cell
                .seat_id
                .as_deref()
                .map(|id| !id.trim().is_empty())
                .unwrap_or(false);
            if !has_id {
                return Err("Seat cells require a seat_id.".to_string());
            }
        }
        _ => {
            if cell.seat_id.is_some() {
                return Err("Only seat cells may have a seat_id.".to_string());
            }
        }
    }
    Ok(())
}

fn validate_unique_seat_ids(cells: &HashMap<(i32, i32), LayoutCell>) -> Result<(), String> {
    let mut ids: Vec<&str> = cells
        .values()
        .filter(|cell| cell.kind == LayoutCellKind::Seat)
        .filter_map(|cell| cell.seat_id.as_deref())
        .collect();
    ids.sort_unstable();
    let mut duplicates: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < ids.len() {
        let id = ids[index];
        let mut count = 0;
        while index < ids.len() && ids[index] == id {
            count += 1;
            index += 1;
        }
        if count > 1 {
            duplicates.push(id);
        }
    }
    if !duplicates.is_empty() {
        return Err(format!("Seat IDs must be unique: {}", duplicates.join(", ")));
    }
    Ok(())
}

fn ordered_cells_for(
    rows: i32,
    columns: i32,
    cells: &HashMap<(i32, i32), LayoutCell>,
) -> Vec<LayoutCell> {
    let mut result = Vec::with_capacity((rows * columns) as usize);
    for row in 1..=rows {
        for column in 1..=columns {
            result.push(
                cells
                    .get(&(row, column))
                    .cloned()
                    .unwrap_or(LayoutCell {
                        row,
                        column,
                        kind: LayoutCellKind::Empty,
                        seat_id: None,
                    }),
            );
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Payload / request parsing helpers
// ---------------------------------------------------------------------------

/// Read a required integer field from an operation payload, rejecting booleans
/// and non-integer numbers.
fn required_int(
    payload: &JsonMap<String, JsonValue>,
    op_kind: &str,
    key: &str,
) -> Result<i32, String> {
    match payload.get(key) {
        Some(JsonValue::Number(number)) => match number.as_i64() {
            Some(value) if (i32::MIN as i64..=i32::MAX as i64).contains(&value) => {
                Ok(value as i32)
            }
            _ => Err(format!("{op_kind} requires an integer payload field: {key}.")),
        },
        Some(_) => Err(format!("{op_kind} requires an integer payload field: {key}.")),
        None => Err(format!("{op_kind} requires payload field: {key}.")),
    }
}

/// Read a required integer field from a create-request or layout object.
fn required_int_from_object(
    object: &JsonMap<String, JsonValue>,
    key: &str,
) -> Result<i32, String> {
    match object.get(key) {
        Some(JsonValue::Number(number)) => match number.as_i64() {
            Some(value) if (i32::MIN as i64..=i32::MAX as i64).contains(&value) => {
                Ok(value as i32)
            }
            _ => Err(format!("{key} must be an integer.")),
        },
        Some(_) => Err(format!("{key} must be an integer.")),
        None => Err(format!("{key} is required.")),
    }
}

/// Read an optional non-empty string field from a create request.
fn optional_trimmed_str(
    object: &JsonMap<String, JsonValue>,
    key: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(format!("{key} cannot be empty."));
            }
            Ok(Some(trimmed.to_string()))
        }
        Some(_) => Err(format!("{key} must be a string.")),
    }
}

/// Read an optional integer field from a create request.
fn optional_int(object: &JsonMap<String, JsonValue>, key: &str) -> Result<Option<i32>, String> {
    match object.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Number(number)) => match number.as_i64() {
            Some(value) if (i32::MIN as i64..=i32::MAX as i64).contains(&value) => {
                Ok(Some(value as i32))
            }
            _ => Err(format!("{key} must be an integer.")),
        },
        Some(_) => Err(format!("{key} must be an integer.")),
    }
}

static DRAFT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Generate a unique opaque draft id (nanos + counter, hex), matching the
/// roster module's approach.
fn new_draft_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = DRAFT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{seq:x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> LayoutDraftStore {
        new_layout_draft_store()
    }

    fn create(store: &LayoutDraftStore, request: &str) -> JsonValue {
        let json = create_layout_draft(store, request)
            .unwrap_or_else(|error| panic!("create {request}: {error}"));
        serde_json::from_str(&json).expect("state is valid JSON")
    }

    fn create_rect(store: &LayoutDraftStore, rows: i32, columns: i32) -> JsonValue {
        create(store, &format!(r#"{{"rows":{rows},"columns":{columns}}}"#))
    }

    fn cell(state: &JsonValue, row: i32, column: i32) -> JsonValue {
        state["cells"]
            .as_array()
            .expect("cells array")
            .iter()
            .find(|cell| cell["row"] == row && cell["column"] == column)
            .unwrap_or_else(|| panic!("missing cell {row},{column}"))
            .clone()
    }

    fn dispatch(
        store: &LayoutDraftStore,
        state: &JsonValue,
        command_id: &str,
        action: &str,
        operation: Option<JsonValue>,
    ) -> JsonValue {
        let mut command = serde_json::Map::new();
        command.insert("command_id".to_string(), json!(command_id));
        command.insert("draft_id".to_string(), state["draft_id"].clone());
        command.insert("base_revision".to_string(), state["revision"].clone());
        command.insert("action".to_string(), json!(action));
        if let Some(operation) = operation {
            command.insert("operation".to_string(), operation);
        }
        let command_json = JsonValue::Object(command).to_string();
        let json = dispatch_layout_command(
            store,
            state["draft_id"].as_str().unwrap(),
            &command_json,
        )
        .unwrap_or_else(|error| panic!("dispatch {command_id} ({command_json}): {error}"));
        serde_json::from_str(&json).expect("state is valid JSON")
    }

    fn set_cell_op(row: i32, column: i32, kind: &str) -> JsonValue {
        json!({"kind": "set_cell", "payload": {"row": row, "column": column, "kind": kind}})
    }

    #[test]
    fn create_rectangular_draft_state_shape() {
        let store = store();
        let state = create_rect(&store, 2, 3);
        assert_eq!(state["kind"], "seattrellis_layout_state");
        assert_eq!(state["api_version"], "1");
        assert_eq!(state["name"], "Classroom");
        assert_eq!(state["rows"], 2);
        assert_eq!(state["columns"], 3);
        assert_eq!(state["revision"], 0);
        assert_eq!(state["undo_depth"], 0);
        assert_eq!(state["redo_depth"], 0);
        assert_eq!(state["usable_seat_count"], 6);
        let cells = state["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 6, "row-major grid includes every cell");
        assert_eq!(cells[0]["seat_id"], "R1C1");
        assert_eq!(cells[0]["kind"], "seat");
        assert_eq!(cells[5]["seat_id"], "R2C3");
        assert!(state["draft_id"].as_str().unwrap().len() > 8);
    }

    #[test]
    fn create_template_draft_matches_room_grid() {
        let store = store();
        let state = create(&store, r#"{"template_id": "standard-30", "name": "Period 2"}"#);
        assert_eq!(state["name"], "Period 2");
        assert_eq!(state["rows"], 5);
        assert_eq!(state["columns"], 7, "6 seats + 1 aisle column");
        assert_eq!(state["usable_seat_count"], 30);
        assert_eq!(state["cells"].as_array().unwrap().len(), 35);
        // The template aisle shows as an aisle cell, not a seat.
        let aisle = cell(&state, 1, 4);
        assert_eq!(aisle["kind"], "aisle");
        assert!(aisle["seat_id"].is_null());
        let seat = cell(&state, 5, 7);
        assert_eq!(seat["kind"], "seat");
        assert_eq!(seat["seat_id"], "R5C7");
    }

    #[test]
    fn create_from_layout_json_preserves_kinds() {
        let store = store();
        let request = json!({
            "name": "Lab",
            "layout": {
                "seats": [
                    {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true, "zone": "front"},
                    {"seat_id": "AISLE-R1C2", "row": 1, "col": 2, "enabled": false, "zone": "aisle"},
                    {"seat_id": "BLOCKED-R1C3", "row": 1, "col": 3, "enabled": false, "zone": "front"},
                    {"seat_id": "STAGE-R2C1", "row": 2, "col": 1, "enabled": false, "zone": "platform"}
                ]
            }
        });
        let state = create(&store, &request.to_string());
        assert_eq!(state["name"], "Lab");
        assert_eq!(state["rows"], 2);
        assert_eq!(state["columns"], 3);
        assert_eq!(cell(&state, 1, 1)["kind"], "seat");
        assert_eq!(cell(&state, 1, 2)["kind"], "aisle");
        assert_eq!(cell(&state, 1, 3)["kind"], "empty", "blocked seats are empty cells");
        assert_eq!(cell(&state, 2, 1)["kind"], "platform");
        assert_eq!(state["usable_seat_count"], 1);
    }

    #[test]
    fn create_requires_exactly_one_source() {
        let store = store();
        let error = create_layout_draft(&store, r#"{"name": "X"}"#).expect_err("no source");
        assert_eq!(error, "Choose one template, existing layout, or rows and columns.");

        let error = create_layout_draft(
            &store,
            r#"{"template_id": "standard-30", "rows": 2, "columns": 3}"#,
        )
        .expect_err("two sources");
        assert_eq!(error, "Choose one template, existing layout, or rows and columns.");

        let error = create_layout_draft(&store, r#"{"rows": 2}"#).expect_err("missing columns");
        assert_eq!(error, "Both rows and columns are required.");
    }

    #[test]
    fn create_rejects_oversized_grid() {
        let store = store();
        let error = create_layout_draft(&store, r#"{"rows": 50, "columns": 50}"#)
            .expect_err("50x50 exceeds cell cap");
        assert!(error.contains("at most 1000 cells"), "{error}");

        let error = create_layout_draft(&store, r#"{"rows": 51, "columns": 1}"#)
            .expect_err("51 rows");
        assert!(error.contains("between 1 and 50"), "{error}");
    }

    #[test]
    fn create_rejects_unknown_template() {
        let store = store();
        let error = create_layout_draft(&store, r#"{"template_id": "banana"}"#)
            .expect_err("unknown template");
        assert!(error.contains("Unknown room template"), "{error}");
        assert!(error.contains("standard-30"), "{error}");
    }

    #[test]
    fn set_cell_changes_kind_and_auto_ids() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        // Clear one seat into an aisle: usable count drops, revision advances.
        let state = dispatch(&store, &state, "c1", "apply", Some(set_cell_op(1, 1, "aisle")));
        assert_eq!(state["usable_seat_count"], 3);
        assert_eq!(state["revision"], 1);
        assert_eq!(state["undo_depth"], 1);
        assert_eq!(cell(&state, 1, 1)["kind"], "aisle");

        // Setting a seat with an explicit id keeps it.
        let state = dispatch(
            &store,
            &state,
            "c2",
            "apply",
            Some(json!({"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "seat", "seat_id": "MySeat"}})),
        );
        assert_eq!(cell(&state, 1, 1)["seat_id"], "MySeat");

        // Setting a seat without an id auto-generates a unique one when the
        // preferred id is already taken by the cell being replaced.
        let state = dispatch(&store, &state, "c3", "apply", Some(set_cell_op(2, 2, "seat")));
        assert_eq!(cell(&state, 2, 2)["seat_id"], "R2C2-2", "suffix avoids the existing id");
    }

    #[test]
    fn set_cell_duplicate_seat_id_rejected() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "dup",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "set_cell", "payload": {"row": 2, "column": 2, "kind": "seat", "seat_id": "R1C1"}}
            })
            .to_string(),
        )
        .expect_err("duplicate seat id");
        assert!(error.contains("Seat IDs must be unique"), "{error}");
        // The failed command did not advance the revision.
        let after = get_layout_state(&store, state["draft_id"].as_str().unwrap()).unwrap();
        let after: JsonValue = serde_json::from_str(&after).unwrap();
        assert_eq!(after["revision"], 0);
        assert_eq!(after["usable_seat_count"], 4);
    }

    #[test]
    fn insert_and_delete_rows_shift_seats() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let state = dispatch(
            &store,
            &state,
            "insert",
            "apply",
            Some(json!({"kind": "insert_row", "payload": {"index": 1}})),
        );
        assert_eq!(state["rows"], 3);
        // Row 1 is now empty; the old row 1 seats moved to row 2.
        assert_eq!(cell(&state, 1, 1)["kind"], "empty");
        assert_eq!(cell(&state, 2, 1)["seat_id"], "R1C1");
        assert_eq!(cell(&state, 3, 2)["seat_id"], "R2C2");

        let state = dispatch(
            &store,
            &state,
            "delete",
            "apply",
            Some(json!({"kind": "delete_row", "payload": {"index": 1}})),
        );
        assert_eq!(state["rows"], 2);
        assert_eq!(cell(&state, 1, 1)["seat_id"], "R1C1");
        assert_eq!(cell(&state, 2, 2)["seat_id"], "R2C2");
    }

    #[test]
    fn insert_and_delete_columns_shift_seats() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let state = dispatch(
            &store,
            &state,
            "insert",
            "apply",
            Some(json!({"kind": "insert_column", "payload": {"index": 1}})),
        );
        assert_eq!(state["columns"], 3);
        assert_eq!(cell(&state, 1, 2)["seat_id"], "R1C1");
        assert_eq!(cell(&state, 2, 3)["seat_id"], "R2C2");

        let state = dispatch(
            &store,
            &state,
            "delete",
            "apply",
            Some(json!({"kind": "delete_column", "payload": {"index": 3}})),
        );
        assert_eq!(state["columns"], 2);
        assert_eq!(cell(&state, 1, 2)["seat_id"], "R1C1");
        assert_eq!(cell(&state, 2, 2)["seat_id"], "R2C1", "the deleted column held R2C2");
    }

    #[test]
    fn cannot_delete_the_last_row_or_column() {
        let store = store();
        let state = create_rect(&store, 1, 3);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "del-row",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "delete_row", "payload": {"index": 1}}
            })
            .to_string(),
        )
        .expect_err("last row");
        assert_eq!(error, "A layout must keep at least one row.");

        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "del-col",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "delete_column", "payload": {"index": 99}}
            })
            .to_string(),
        )
        .expect_err("bad column index");
        assert_eq!(error, "Deleted column index is outside the layout.");
    }

    #[test]
    fn translate_moves_only_physical_cells() {
        let store = store();
        let mut state = create_rect(&store, 3, 3);
        // Carve empty margin along the last row and last column.
        for column in 1..=3 {
            state = dispatch(
                &store,
                &state,
                &format!("r{column}"),
                "apply",
                Some(set_cell_op(3, column, "empty")),
            );
        }
        state = dispatch(&store, &state, "c3", "apply", Some(set_cell_op(1, 3, "empty")));
        state = dispatch(&store, &state, "c3b", "apply", Some(set_cell_op(2, 3, "empty")));
        assert_eq!(state["usable_seat_count"], 4, "only the 2x2 top-left block remains");

        let state = dispatch(
            &store,
            &state,
            "translate",
            "apply",
            Some(json!({"kind": "translate", "payload": {"row_delta": 1, "column_delta": 1}})),
        );
        // Seats moved down-right into the empty margin.
        assert_eq!(cell(&state, 2, 2)["seat_id"], "R1C1");
        assert_eq!(cell(&state, 2, 3)["seat_id"], "R1C2");
        assert_eq!(cell(&state, 3, 2)["seat_id"], "R2C1");
        assert_eq!(cell(&state, 3, 3)["seat_id"], "R2C2");
        assert_eq!(state["usable_seat_count"], 4);
    }

    #[test]
    fn translate_out_of_bounds_is_atomic() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "bad-translate",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "translate", "payload": {"row_delta": 0, "column_delta": 1}}
            })
            .to_string(),
        )
        .expect_err("rightmost column leaves the grid");
        assert!(error.contains("outside the layout"), "{error}");

        let after = get_layout_state(&store, state["draft_id"].as_str().unwrap()).unwrap();
        let after: JsonValue = serde_json::from_str(&after).unwrap();
        assert_eq!(after["revision"], 0, "failed command left the draft untouched");
        assert_eq!(after["usable_seat_count"], 4);
        assert_eq!(after["undo_depth"], 0);
    }

    #[test]
    fn mirror_horizontal_and_flip_vertical() {
        let store = store();
        let state = create_rect(&store, 2, 3);
        let state = dispatch(
            &store,
            &state,
            "mirror",
            "apply",
            Some(json!({"kind": "mirror_horizontal", "payload": {}})),
        );
        assert_eq!(cell(&state, 1, 1)["seat_id"], "R1C3");
        assert_eq!(cell(&state, 1, 3)["seat_id"], "R1C1");
        assert_eq!(cell(&state, 2, 2)["seat_id"], "R2C2");
        assert_eq!(state["revision"], 1);

        let state = dispatch(
            &store,
            &state,
            "flip",
            "apply",
            Some(json!({"kind": "flip_vertical", "payload": {}})),
        );
        assert_eq!(cell(&state, 1, 1)["seat_id"], "R2C3");
        assert_eq!(cell(&state, 2, 3)["seat_id"], "R1C1");
    }

    #[test]
    fn undo_redo_round_trip() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let state = dispatch(&store, &state, "c1", "apply", Some(set_cell_op(1, 1, "aisle")));
        assert_eq!(state["revision"], 1);
        assert_eq!(state["usable_seat_count"], 3);

        let state = dispatch(&store, &state, "c2", "undo", None);
        assert_eq!(state["revision"], 2);
        assert_eq!(state["undo_depth"], 0);
        assert_eq!(state["redo_depth"], 1);
        assert_eq!(state["usable_seat_count"], 4, "undo restored the seat");

        let state = dispatch(&store, &state, "c3", "redo", None);
        assert_eq!(state["revision"], 3);
        assert_eq!(state["redo_depth"], 0);
        assert_eq!(state["usable_seat_count"], 3, "redo re-applied the aisle");

        // A new command after undo clears the redo stack.
        let state = dispatch(&store, &state, "c4", "undo", None);
        let state = dispatch(&store, &state, "c5", "apply", Some(set_cell_op(2, 2, "platform")));
        assert_eq!(state["redo_depth"], 0);
        assert_eq!(cell(&state, 2, 2)["kind"], "platform");
    }

    #[test]
    fn undo_without_history_errors() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "undo",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "undo"
            })
            .to_string(),
        )
        .expect_err("nothing to undo");
        assert_eq!(error, "There is no layout change to undo.");
    }

    #[test]
    fn stale_revision_rejected() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        // Advance the draft past revision 0.
        dispatch(&store, &state, "c1", "apply", Some(set_cell_op(1, 1, "aisle")));
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "stale",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 2, "kind": "aisle"}}
            })
            .to_string(),
        )
        .expect_err("stale base revision");
        assert!(error.contains("stale revision"), "{error}");
        assert!(error.contains("base revision 0"), "{error}");
    }

    #[test]
    fn duplicate_command_id_rejected() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        dispatch(&store, &state, "same-id", "apply", Some(set_cell_op(1, 1, "aisle")));
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "same-id",
                "draft_id": state["draft_id"],
                "base_revision": 1,
                "action": "apply",
                "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 2, "kind": "aisle"}}
            })
            .to_string(),
        )
        .expect_err("command id already used");
        assert_eq!(error, "This layout command has already been applied.");
    }

    #[test]
    fn wrong_draft_rejected() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "other",
                "draft_id": "some-other-draft",
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "aisle"}}
            })
            .to_string(),
        )
        .expect_err("command targets a different draft");
        assert_eq!(error, "The layout command targets a different draft.");
    }

    #[test]
    fn apply_requires_an_operation_and_undo_forbids_one() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "no-op",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply"
            })
            .to_string(),
        )
        .expect_err("apply without operation");
        assert_eq!(error, "Apply commands require an operation.");

        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "undo-op",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "undo",
                "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "aisle"}}
            })
            .to_string(),
        )
        .expect_err("undo with operation");
        assert_eq!(error, "undo commands cannot contain an operation.");
    }

    #[test]
    fn unknown_operation_kind_rejected() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let error = dispatch_layout_command(
            &store,
            state["draft_id"].as_str().unwrap(),
            &json!({
                "command_id": "bogus",
                "draft_id": state["draft_id"],
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "rotate_90", "payload": {}}
            })
            .to_string(),
        )
        .expect_err("unsupported operation");
        assert!(error.contains("Unsupported layout command"), "{error}");
    }

    #[test]
    fn compile_to_core_layout_shape() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        // Add a platform at the front so near_platform is observable.
        let state = dispatch(
            &store,
            &state,
            "platform",
            "apply",
            Some(json!({"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "platform"}})),
        );
        let draft_id = state["draft_id"].as_str().unwrap().to_string();
        let json = compile_layout(&store, &draft_id).expect("compiles");
        let compiled: JsonValue = serde_json::from_str(&json).unwrap();
        assert_eq!(compiled["api_version"], "1");
        assert_eq!(compiled["draft_id"], draft_id);
        assert_eq!(compiled["revision"], 1);
        let layout = &compiled["layout"];
        assert_eq!(layout["layout_id"], draft_id);
        assert_eq!(layout["name"], "Classroom");
        assert_eq!(layout["adjacency"]["include_horizontal"], true);
        assert_eq!(layout["adjacency"]["include_vertical"], false);

        let seats = layout["seats"].as_array().unwrap();
        assert_eq!(seats.len(), 4, "platform + three remaining seats");
        let platform = seats.iter().find(|seat| seat["seat_id"] == "PLATFORM-R1C1").unwrap();
        assert_eq!(platform["enabled"], false);
        assert_eq!(platform["zone"], "platform");
        let seat = seats.iter().find(|seat| seat["seat_id"] == "R2C2").unwrap();
        assert_eq!(seat["enabled"], true);
        assert_eq!(seat["near_platform"], true, "seat behind the platform row");
    }

    #[test]
    fn compile_requires_a_usable_seat() {
        let store = store();
        let state = create_rect(&store, 1, 2);
        let state = dispatch(&store, &state, "a", "apply", Some(set_cell_op(1, 1, "empty")));
        let state = dispatch(&store, &state, "b", "apply", Some(set_cell_op(1, 2, "empty")));
        let error = compile_layout(&store, state["draft_id"].as_str().unwrap())
            .expect_err("no seats remain");
        assert_eq!(
            error,
            "The classroom needs at least one seat before it can be used."
        );
    }

    #[test]
    fn unknown_draft_rejected() {
        let store = store();
        let error = get_layout_state(&store, "missing").expect_err("unknown draft");
        assert_eq!(error, "unknown layout draft: missing");

        let error = compile_layout(&store, "missing").expect_err("unknown draft");
        assert_eq!(error, "unknown layout draft: missing");

        let error = dispatch_layout_command(
            &store,
            "missing",
            &json!({
                "command_id": "x",
                "draft_id": "missing",
                "base_revision": 0,
                "action": "apply",
                "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "seat"}}
            })
            .to_string(),
        )
        .expect_err("unknown draft");
        assert_eq!(error, "unknown layout draft: missing");
    }

    #[test]
    fn delete_layout_draft_lifecycle() {
        let store = store();
        let state = create_rect(&store, 2, 2);
        let draft_id = state["draft_id"].as_str().unwrap().to_string();
        assert!(get_layout_state(&store, &draft_id).is_ok());
        assert!(delete_layout_draft_in_store(&store, &draft_id));
        assert!(get_layout_state(&store, &draft_id).is_err(), "draft removed");
        assert!(!delete_layout_draft_in_store(&store, &draft_id), "second delete misses");
        assert!(!delete_layout_draft_in_store(&store, "   "), "blank id misses");
    }

    #[test]
    fn global_json_api_round_trip() {
        // Exercise the server-facing global entry points end-to-end.
        let created = create_layout_draft_json(r#"{"rows": 2, "columns": 2, "name": "Global"}"#)
            .expect("creates through the global store");
        let state: JsonValue = serde_json::from_str(&created).unwrap();
        assert_eq!(state["name"], "Global");
        let draft_id = state["draft_id"].as_str().unwrap().to_string();

        let fetched = get_layout_state_json(&draft_id).expect("fetches");
        assert_eq!(serde_json::from_str::<JsonValue>(&fetched).unwrap()["rows"], 2);

        let command = json!({
            "command_id": "global-1",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "aisle"}}
        })
        .to_string();
        let after = dispatch_layout_command_json(&draft_id, &command).expect("dispatches");
        assert_eq!(serde_json::from_str::<JsonValue>(&after).unwrap()["revision"], 1);

        let compiled = compile_layout_draft_json(&draft_id).expect("compiles");
        assert_eq!(serde_json::from_str::<JsonValue>(&compiled).unwrap()["draft_id"], draft_id);

        assert!(delete_layout_draft(&draft_id), "deletes through the global store");
        assert!(get_layout_state_json(&draft_id).is_err(), "draft gone");
    }
}
