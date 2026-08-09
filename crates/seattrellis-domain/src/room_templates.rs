//! Standard classroom templates converted to the `seattrellis_core` solver's
//! grid shapes (`seat_positions` / `edges` / `layout`).
//!
//! This module mirrors the Python template definitions in
//! `src/seattrellis/application/room_templates.py` (the
//! `ROOM_TEMPLATE_30 / ROOM_TEMPLATE_48 / ROOM_TEMPLATE_60` layouts):
//!
//! | template_id  | rows | seats_per_row | aisle after | capacity |
//! |--------------|------|---------------|-------------|----------|
//! | `standard-30`|    5 |             6 | logical 3   |       30 |
//! | `standard-48`|    6 |             8 | logical 4   |       48 |
//! | `standard-60`|    6 |            10 | logical 5   |       60 |
//!
//! It is self-contained: the mirror [`Layout`], [`Seat`] and [`AdjacencyConfig`]
//! types below serialize to exactly the shapes the native solver deserializes
//! into `seattrellis_core::models::{Layout, Seat, AdjacencyConfig}`. When this
//! file is dropped into a crate that links `seattrellis_core`, the mirror type
//! definitions can be deleted and replaced with `use seattrellis_core::models::{
//! Layout, Seat, AdjacencyConfig };` — the rest of the code is unchanged.
//!
//! Grid convention (matches `seattrellis_core` and `app/src/render.rs`):
//!
//! * `seat_positions[i]` is `[col, row]` in 1-based grid coordinates
//!   (`x = col`, `y = row`), listed row-major and left-to-right.
//! * `edges` are `[usize; 2]` index pairs into `seat_positions`. Only
//!   horizontally adjacent enabled seats in the same row are connected.
//! * An aisle is a disabled grid cell: it never appears in `seat_positions`
//!   and splits each row into separate banks so adjacency cannot bridge it,
//!   but it *does* appear in `layout.seats` with `enabled == false` so
//!   renderers can show the physical gap.
//!
//! Zones and landmarks (mirroring Python's `_seat_zone` and near_* rules):
//!
//! * `zone`: row 1 → `"front"`, last row → `"back"`, otherwise `"middle"`.
//! * `near_platform`: first row.
//! * `near_window`: first grid column (`col == 1`).
//! * `near_door`: the very last grid cell of the last row (always an enabled
//!   seat because an aisle is never the final grid column).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Mirror of seattrellis_core::models (byte-compatible JSON shapes)
// ---------------------------------------------------------------------------

fn default_enabled() -> bool {
    true
}

/// A single seat node, shape-mirroring `seattrellis_core::models::Seat`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Seat {
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default = "default_enabled")]
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
}

impl Seat {
    /// Mirror of `SeatNode.new(seat_id, row, col)`: an enabled seat with no
    /// zone or landmark flags.
    pub fn new(seat_id: impl Into<String>, row: i32, col: i32) -> Self {
        Self {
            seat_id: seat_id.into(),
            row,
            col,
            x: None,
            y: None,
            enabled: true,
            zone: None,
            group_id: None,
            near_window: false,
            near_door: false,
            near_platform: false,
            near_ac: false,
        }
    }
}

fn default_horizontal() -> bool {
    true
}

fn default_use_xy() -> bool {
    true
}

fn default_one() -> i32 {
    1
}

/// Mirror of `seattrellis_core::models::AdjacencyConfig`.
///
/// The horizontal-only config matches Python's emitted adjacency; it describes
/// how a *derived* adjacency graph would look, while the explicit `edges`
/// list on [`RoomGrid`] is what the solver actually consumes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjacencyConfig {
    #[serde(default = "default_horizontal")]
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
    #[serde(default = "default_use_xy")]
    pub use_xy_distance: bool,
    #[serde(default)]
    pub custom_edges: Vec<(String, String)>,
}

impl Default for AdjacencyConfig {
    fn default() -> Self {
        horizontal_adjacency()
    }
}

fn default_layout_id() -> String {
    "default-layout".to_string()
}

fn default_layout_name() -> String {
    "Classroom".to_string()
}

/// Mirror of `seattrellis_core::models::Layout`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default = "default_layout_id")]
    pub layout_id: String,
    #[serde(default = "default_layout_name")]
    pub name: String,
    pub seats: Vec<Seat>,
    #[serde(default)]
    pub adjacency: AdjacencyConfig,
}

impl Layout {
    /// Mirror of `Layout.new(seats)`: default id/name and the horizontal-only
    /// adjacency config.
    pub fn new(seats: Vec<Seat>) -> Self {
        Self {
            layout_id: default_layout_id(),
            name: default_layout_name(),
            seats,
            adjacency: horizontal_adjacency(),
        }
    }

    /// Enabled seats, in layout order (mirrors `Layout.enabled_seats`).
    pub fn enabled_seats(&self) -> Vec<&Seat> {
        self.seats.iter().filter(|seat| seat.enabled).collect()
    }

    /// The seat with the given id, if present.
    pub fn seat_by_id(&self, seat_id: &str) -> Option<&Seat> {
        self.seats.iter().find(|seat| seat.seat_id == seat_id)
    }
}

/// The adjacency config emitted by the Python templates.
fn horizontal_adjacency() -> AdjacencyConfig {
    AdjacencyConfig {
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

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

/// Immutable description of a rectangular room.
struct RoomSpec {
    template_id: &'static str,
    name: &'static str,
    rows: i32,
    seats_per_row: i32,
    /// Logical seat positions after which a full-length aisle is inserted.
    aisles_after: &'static [i32],
}

impl RoomSpec {
    /// Enabled (student) capacity: aisle cells never consume a place.
    fn capacity(&self) -> i32 {
        self.rows * self.seats_per_row
    }

    /// Physical grid width, including aisle cells.
    fn grid_columns(&self) -> i32 {
        self.seats_per_row + self.aisles_after.len() as i32
    }
}

const STANDARD_30: RoomSpec = RoomSpec {
    template_id: "standard-30",
    name: "30-seat classroom",
    rows: 5,
    seats_per_row: 6,
    aisles_after: &[3],
};

const STANDARD_48: RoomSpec = RoomSpec {
    template_id: "standard-48",
    name: "48-seat classroom",
    rows: 6,
    seats_per_row: 8,
    aisles_after: &[4],
};

const STANDARD_60: RoomSpec = RoomSpec {
    template_id: "standard-60",
    name: "60-seat classroom",
    rows: 6,
    seats_per_row: 10,
    aisles_after: &[5],
};

const STANDARD_ROOM_TEMPLATES: [&RoomSpec; 3] = [&STANDARD_30, &STANDARD_48, &STANDARD_60];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// The grid a classroom template expands to — the `CoreSolveRequest` shapes.
///
/// [`Layout`] mirrors `seattrellis_core::models::Layout`, so
/// `grid.layout.seats.len() == rows * grid_columns` (every cell, including
/// disabled aisles) while `seat_positions.len()` counts only enabled seats.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoomGrid {
    pub layout_id: String,
    pub name: String,
    pub rows: i32,
    /// Physical grid width including aisle columns.
    pub grid_columns: i32,
    /// `[col, row]` grid coordinates for every enabled seat, row-major.
    pub seat_positions: Vec<[f64; 2]>,
    /// `[usize; 2]` index pairs into `seat_positions` (same-row neighbors,
    /// never bridging an aisle).
    pub edges: Vec<[usize; 2]>,
    /// The full room layout (`Layout` shape), including disabled aisle cells.
    pub layout: Layout,
}

impl RoomGrid {
    /// The layout serialized exactly as the `CoreSolveRequest.layout` value.
    pub fn layout_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.layout)
            .map_err(|error| format!("could not serialize layout: {error}"))
    }

    /// `seat_positions` + `edges` + `layout` serialized as the
    /// `CoreSolveRequest` fragment (merge into the request, add
    /// `api_version`/`student_count`, and pass the JSON to `solve_problem_json`).
    pub fn request_fragment_json(&self) -> Result<String, String> {
        let fragment = RequestFragment {
            seat_positions: &self.seat_positions,
            edges: &self.edges,
            layout: &self.layout,
        };
        serde_json::to_string(&fragment)
            .map_err(|error| format!("could not serialize request fragment: {error}"))
    }
}

#[derive(Serialize)]
struct RequestFragment<'a> {
    seat_positions: &'a [[f64; 2]],
    edges: &'a [[usize; 2]],
    layout: &'a Layout,
}

/// The canonical ids of every built-in template, in ascending capacity order.
pub fn list_room_template_ids() -> Vec<&'static str> {
    STANDARD_ROOM_TEMPLATES
        .iter()
        .map(|spec| spec.template_id)
        .collect()
}

/// Expand a built-in classroom template into a [`RoomGrid`].
///
/// Accepts `"standard-30"`, `"standard-48"` and `"standard-60"`, plus the
/// capacity aliases `"30"` / `"30-seat"` / `"30-seats"` (and 48 / 60
/// equivalents). Lookup is case-insensitive and treats `_` as `-`.
///
/// # Errors
///
/// Returns `Err` with a clear message for an empty or unknown template id.
pub fn room_template_grid(template_id: &str) -> Result<RoomGrid, String> {
    let spec = resolve_template(template_id)?;
    Ok(build_grid(spec))
}

/// Build a [`RoomGrid`] from a client-supplied layout document (the React room
/// builder's `draft.room.layout`). Mirrors [`build_grid`]'s derivation:
/// `seat_positions` are the enabled seats in layout order (`[x or col, y or
/// row]`) and `edges` follow the layout's adjacency config.
///
/// # Errors
///
/// Returns `Err` when the document is not a valid layout or contains no
/// enabled seats.
pub fn grid_from_layout(layout_value: &serde_json::Value) -> Result<RoomGrid, String> {
    let layout: Layout = serde_json::from_value(layout_value.clone())
        .map_err(|error| format!("invalid custom layout: {error}"))?;
    let enabled: Vec<&Seat> = layout.seats.iter().filter(|seat| seat.enabled).collect();
    if enabled.is_empty() {
        return Err("custom layout has no enabled seats".to_string());
    }
    let seat_positions: Vec<[f64; 2]> = enabled
        .iter()
        .map(|seat| {
            [
                seat.x.unwrap_or(seat.col as f64),
                seat.y.unwrap_or(seat.row as f64),
            ]
        })
        .collect();
    let edges = derive_edges(&layout, &enabled);
    let rows = enabled.iter().map(|seat| seat.row).max().unwrap_or(1);
    let grid_columns = enabled.iter().map(|seat| seat.col).max().unwrap_or(1);
    Ok(RoomGrid {
        layout_id: layout.layout_id.clone(),
        name: layout.name.clone(),
        rows,
        grid_columns,
        seat_positions,
        edges,
        layout,
    })
}

/// Derive the adjacency `edges` (enabled-seat index pairs) from a layout's
/// adjacency config. Row/column deltas gate horizontal/vertical/diagonal
/// neighbors; an optional `max_distance` under `use_xy_distance` also admits
/// nearby seats; `custom_edges` name additional `seat_id` pairs.
fn derive_edges(layout: &Layout, enabled: &[&Seat]) -> Vec<[usize; 2]> {
    let adjacency = &layout.adjacency;
    let mut edges: Vec<[usize; 2]> = Vec::new();
    for first in 0..enabled.len() {
        for second in (first + 1)..enabled.len() {
            let a = enabled[first];
            let b = enabled[second];
            let row_delta = (a.row - b.row).abs();
            let col_delta = (a.col - b.col).abs();
            let horizontal = row_delta == 0 && col_delta == 1;
            let vertical = col_delta == 0 && row_delta == 1;
            let diagonal = row_delta == 1 && col_delta == 1;
            let mut adjacent = (adjacency.include_horizontal && horizontal)
                || (adjacency.include_vertical && vertical)
                || (adjacency.include_diagonal && diagonal);
            if adjacency.use_xy_distance {
                if let Some(max_distance) = adjacency.max_distance {
                    let delta_x = a.x.unwrap_or(a.col as f64) - b.x.unwrap_or(b.col as f64);
                    let delta_y = a.y.unwrap_or(a.row as f64) - b.y.unwrap_or(b.row as f64);
                    if (delta_x * delta_x + delta_y * delta_y).sqrt() <= max_distance {
                        adjacent = true;
                    }
                }
            }
            if adjacent {
                edges.push([first, second]);
            }
        }
    }
    let index_by_id: HashMap<&str, usize> = enabled
        .iter()
        .enumerate()
        .map(|(index, seat)| (seat.seat_id.as_str(), index))
        .collect();
    for (first_id, second_id) in &adjacency.custom_edges {
        if let (Some(&first), Some(&second)) = (
            index_by_id.get(first_id.as_str()),
            index_by_id.get(second_id.as_str()),
        ) {
            if !edges.contains(&[first, second]) {
                edges.push([first, second]);
            }
        }
    }
    edges
}

// ---------------------------------------------------------------------------
// Lookup and grid construction
// ---------------------------------------------------------------------------

fn normalize_template_id(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace('_', "-")
}

fn resolve_template(template_id: &str) -> Result<&'static RoomSpec, String> {
    let key = normalize_template_id(template_id);
    if key.is_empty() {
        return Err("template_id cannot be empty".to_string());
    }
    for spec in STANDARD_ROOM_TEMPLATES {
        let capacity = spec.capacity();
        if spec.template_id == key
            || capacity.to_string() == key
            || format!("{capacity}-seat") == key
            || format!("{capacity}-seats") == key
        {
            return Ok(spec);
        }
    }
    let available = list_room_template_ids().join(", ");
    Err(format!(
        "Unknown room template {template_id:?}. Available templates: {available}."
    ))
}

fn seat_zone(row: i32, row_count: i32) -> &'static str {
    if row == 1 {
        "front"
    } else if row == row_count {
        "back"
    } else {
        "middle"
    }
}

fn build_grid(spec: &'static RoomSpec) -> RoomGrid {
    let rows = spec.rows;
    let seats_per_row = spec.seats_per_row;
    let grid_columns = spec.grid_columns();

    // One Seat per grid cell, row-major: the enabled logical seats plus a
    // disabled aisle cell after each listed logical position.
    let mut seats: Vec<Seat> = Vec::with_capacity((rows * grid_columns) as usize);
    for row in 1..=rows {
        let mut grid_col = 1;
        for logical_col in 1..=seats_per_row {
            seats.push(Seat {
                seat_id: format!("R{row}C{grid_col}"),
                row,
                col: grid_col,
                zone: Some(seat_zone(row, rows).to_string()),
                near_platform: row == 1,
                near_window: grid_col == 1,
                near_door: row == rows && grid_col == grid_columns,
                ..Seat::new("", row, grid_col)
            });
            grid_col += 1;
            if spec.aisles_after.contains(&logical_col) {
                seats.push(Seat {
                    seat_id: format!("AISLE-R{row}C{grid_col}"),
                    row,
                    col: grid_col,
                    enabled: false,
                    zone: Some("aisle".to_string()),
                    ..Seat::new("", row, grid_col)
                });
                grid_col += 1;
            }
        }
    }

    // seat_positions = enabled seats in layout order; edges = same-row enabled
    // neighbors whose grid columns differ by exactly one (aisles break banks).
    let mut seat_positions: Vec<[f64; 2]> = Vec::with_capacity(seats.len());
    let mut edges: Vec<[usize; 2]> = Vec::new();
    let mut prev_by_row: Vec<Option<(usize, i32)>> = vec![None; rows as usize + 1];
    for seat in &seats {
        if !seat.enabled {
            continue;
        }
        let pos = seat_positions.len();
        seat_positions.push([seat.col as f64, seat.row as f64]);
        if let Some((prev_pos, prev_col)) = prev_by_row[seat.row as usize] {
            if seat.col - prev_col == 1 {
                edges.push([prev_pos, pos]);
            }
        }
        prev_by_row[seat.row as usize] = Some((pos, seat.col));
    }

    RoomGrid {
        layout_id: spec.template_id.to_string(),
        name: spec.name.to_string(),
        rows,
        grid_columns,
        seat_positions,
        edges,
        layout: Layout {
            layout_id: spec.template_id.to_string(),
            name: spec.name.to_string(),
            seats,
            adjacency: horizontal_adjacency(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(template_id: &str) -> RoomGrid {
        room_template_grid(template_id).unwrap_or_else(|error| panic!("{template_id}: {error}"))
    }

    /// The grid-column of the enabled seat at `seat_positions[index]`.
    fn col_of(grid: &RoomGrid, index: usize) -> i32 {
        grid.seat_positions[index][0].round() as i32
    }

    fn row_of(grid: &RoomGrid, index: usize) -> i32 {
        grid.seat_positions[index][1].round() as i32
    }

    #[test]
    fn standard_30_dimensions() {
        let grid = grid("standard-30");
        assert_eq!(grid.rows, 5);
        assert_eq!(grid.grid_columns, 7, "6 seats + 1 aisle column");
        assert_eq!(grid.seat_positions.len(), 30);
        // Every cell (enabled + aisle) is present in the layout.
        assert_eq!(grid.layout.seats.len(), 5 * 7);
        assert_eq!(grid.layout.enabled_seats().len(), 30);
    }

    #[test]
    fn standard_48_dimensions() {
        let grid = grid("standard-48");
        assert_eq!(grid.rows, 6);
        assert_eq!(grid.grid_columns, 9, "8 seats + 1 aisle column");
        assert_eq!(grid.seat_positions.len(), 48);
        assert_eq!(grid.layout.seats.len(), 6 * 9);
        assert_eq!(grid.layout.enabled_seats().len(), 48);
    }

    #[test]
    fn standard_60_dimensions() {
        let grid = grid("standard-60");
        assert_eq!(grid.rows, 6);
        assert_eq!(grid.grid_columns, 11, "10 seats + 1 aisle column");
        assert_eq!(grid.seat_positions.len(), 60);
        assert_eq!(grid.layout.seats.len(), 6 * 11);
        assert_eq!(grid.layout.enabled_seats().len(), 60);
    }

    #[test]
    fn aisle_is_a_disabled_cell_not_a_seat() {
        let grid = grid("standard-30");
        let aisle = grid
            .layout
            .seat_by_id("AISLE-R1C4")
            .expect("aisle cell exists");
        assert!(!aisle.enabled);
        assert_eq!(aisle.row, 1);
        assert_eq!(aisle.col, 4);
        assert_eq!(aisle.zone.as_deref(), Some("aisle"));
        // One disabled aisle per row, always at the grid column after logical 3.
        let disabled: Vec<&Seat> = grid
            .layout
            .seats
            .iter()
            .filter(|seat| !seat.enabled)
            .collect();
        assert_eq!(disabled.len(), 5);
        for seat in &disabled {
            assert_eq!(seat.col, 4, "aisle splits row {} at column 4", seat.row);
        }
        // The aisle never yields a seat position.
        assert!(grid
            .seat_positions
            .iter()
            .all(|position| position[0].round() != 4.0));
        // Enabled neighbors on either side of the aisle exist.
        assert!(grid.layout.seat_by_id("R1C3").unwrap().enabled);
        assert!(grid.layout.seat_by_id("R1C5").unwrap().enabled);
    }

    #[test]
    fn edges_do_not_cross_the_aisle() {
        let grid = grid("standard-30");
        assert_eq!(grid.edges.len(), 20, "4 same-row pairs x 5 rows");
        for [first, second] in &grid.edges {
            assert_ne!(first, second);
            assert!(*first < grid.seat_positions.len());
            assert!(*second < grid.seat_positions.len());
            // Same row, and exactly one grid column apart (never across col 4).
            assert_eq!(row_of(&grid, *first), row_of(&grid, *second));
            assert_eq!((col_of(&grid, *second) - col_of(&grid, *first)).abs(), 1);
        }
        // The exact bank pairs for row 1: cols (1,2),(2,3) and (5,6),(6,7).
        let pairs: Vec<(usize, usize)> = grid.edges.iter().map(|e| (e[0], e[1])).collect();
        for expected in [(0usize, 1usize), (1, 2), (3, 4), (4, 5)] {
            assert!(pairs.contains(&expected), "missing edge {expected:?}");
        }
        assert!(!pairs.contains(&(2, 3)));
        assert!(!pairs.contains(&(2, 4)));
        assert!(!pairs.contains(&(3, 5)));
    }

    #[test]
    fn edge_counts_follow_the_template_shape() {
        assert_eq!(grid("standard-30").edges.len(), 5 * 4);
        assert_eq!(grid("standard-48").edges.len(), 6 * 6);
        assert_eq!(grid("standard-60").edges.len(), 6 * 8);
    }

    #[test]
    fn seat_positions_map_to_grid_coordinates() {
        let grid = grid("standard-30");
        assert_eq!(grid.seat_positions[0], [1.0, 1.0], "row 1 leftmost");
        assert_eq!(
            grid.seat_positions[6],
            [1.0, 2.0],
            "row 2 starts at index 6"
        );
        assert_eq!(grid.seat_positions[29], [7.0, 5.0], "last row rightmost");
        // seat_positions must be exactly the enabled seats, in layout order.
        let from_layout: Vec<[f64; 2]> = grid
            .layout
            .enabled_seats()
            .iter()
            .map(|seat| [seat.col as f64, seat.row as f64])
            .collect();
        assert_eq!(grid.seat_positions, from_layout);
    }

    #[test]
    fn aliases_resolve_to_the_same_template() {
        let canonical = grid("standard-30");
        for alias in [
            "standard-30",
            "30",
            "30-seat",
            "30-seats",
            "STANDARD_30",
            " standard-30 ",
            "30_SEAT",
        ] {
            let grid = room_template_grid(alias).unwrap_or_else(|error| panic!("{alias}: {error}"));
            assert_eq!(grid.layout_id, "standard-30", "alias {alias:?}");
            assert_eq!(grid.seat_positions.len(), 30);
            assert_eq!(grid.name, canonical.name);
        }
        assert_eq!(grid("48-seat").seat_positions.len(), 48);
        assert_eq!(grid("60-seat").seat_positions.len(), 60);
        assert_eq!(grid("48").layout_id, "standard-48");
        assert_eq!(grid("60").layout_id, "standard-60");
    }

    #[test]
    fn unknown_template_reports_a_clear_error() {
        for unknown in ["standard-99", "99-seat", "banana"] {
            let error = room_template_grid(unknown).expect_err("should reject unknown id");
            assert!(
                error.contains("Available templates"),
                "unexpected message for {unknown:?}: {error}"
            );
            assert!(error.contains("standard-30"), "lists standard-30: {error}");
            assert!(error.contains("standard-48"), "lists standard-48: {error}");
            assert!(error.contains("standard-60"), "lists standard-60: {error}");
        }
    }

    #[test]
    fn empty_template_id_is_rejected() {
        for empty in ["", "   "] {
            let error = room_template_grid(empty).expect_err("should reject an empty id");
            assert_eq!(error, "template_id cannot be empty");
        }
    }

    #[test]
    fn zone_follows_row_position() {
        let grid = grid("standard-30");
        let mut front = 0;
        let mut middle = 0;
        let mut back = 0;
        let mut aisles = 0;
        for seat in &grid.layout.seats {
            match seat.zone.as_deref() {
                Some("front") => front += 1,
                Some("middle") => middle += 1,
                Some("back") => back += 1,
                Some("aisle") => aisles += 1,
                other => panic!("unexpected zone {other:?} on {}", seat.seat_id),
            }
        }
        assert_eq!(front, 6, "row 1 has 6 seats");
        assert_eq!(middle, 18, "rows 2-4 have 18 seats");
        assert_eq!(back, 6, "row 5 has 6 seats");
        assert_eq!(aisles, 5, "one aisle per row");
        assert_eq!(grid.seat_positions.len(), front + middle + back);
    }

    #[test]
    fn landmark_flags_match_the_template() {
        let grid = grid("standard-30");
        let platform: Vec<&str> = grid
            .layout
            .seats
            .iter()
            .filter(|seat| seat.near_platform)
            .map(|seat| seat.seat_id.as_str())
            .collect();
        assert_eq!(platform.len(), 6, "one per first-row seat");
        assert!(platform.iter().all(|id| id.starts_with("R1C")));

        let window: Vec<&str> = grid
            .layout
            .seats
            .iter()
            .filter(|seat| seat.near_window)
            .map(|seat| seat.seat_id.as_str())
            .collect();
        assert_eq!(window.len(), 5, "one per row, at grid column 1");
        assert!(window.iter().all(|id| id.ends_with('1')));

        let door: Vec<&str> = grid
            .layout
            .seats
            .iter()
            .filter(|seat| seat.near_door)
            .map(|seat| seat.seat_id.as_str())
            .collect();
        assert_eq!(door, vec!["R5C7"], "last row, last grid column");
        let door_seat = grid.layout.seat_by_id("R5C7").unwrap();
        assert!(door_seat.enabled);
        assert_eq!(door_seat.zone.as_deref(), Some("back"));

        // Front-left corner carries platform + window together.
        let corner = grid.layout.seat_by_id("R1C1").unwrap();
        assert!(corner.near_platform && corner.near_window && !corner.near_door);
    }

    #[test]
    fn layout_json_matches_core_shape() {
        let grid = grid("standard-30");
        let value: serde_json::Value =
            serde_json::from_str(&grid.layout_json().unwrap()).expect("layout is JSON");
        assert_eq!(value["layout_id"], "standard-30");
        assert_eq!(value["name"], "30-seat classroom");
        assert_eq!(value["seats"].as_array().unwrap().len(), 35);
        assert_eq!(value["adjacency"]["include_horizontal"], true);
        assert_eq!(value["adjacency"]["include_vertical"], false);
        let first = &value["seats"][0];
        assert_eq!(first["seat_id"], "R1C1");
        assert_eq!(first["row"], 1);
        assert_eq!(first["col"], 1);
        assert_eq!(first["enabled"], true);
        assert_eq!(first["zone"], "front");
        assert_eq!(first["near_platform"], true);
        assert_eq!(first["near_window"], true);
        assert_eq!(first["near_door"], false);
        let aisle = value["seats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|seat| seat["seat_id"] == "AISLE-R1C4")
            .expect("aisle present");
        assert_eq!(aisle["enabled"], false);
        assert_eq!(aisle["zone"], "aisle");

        let fragment: serde_json::Value =
            serde_json::from_str(&grid.request_fragment_json().unwrap()).expect("fragment is JSON");
        assert_eq!(fragment["seat_positions"].as_array().unwrap().len(), 30);
        assert_eq!(fragment["edges"].as_array().unwrap().len(), 20);
        assert_eq!(fragment["layout"]["seats"].as_array().unwrap().len(), 35);
        assert_eq!(fragment["seat_positions"][0], serde_json::json!([1.0, 1.0]));
    }
}
