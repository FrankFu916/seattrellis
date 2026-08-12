//! Rendering of a solved seating plan to SVG, HTML, PNG, or PDF.
//!
//! This module is a copy of `historical crates/seattrellis-cli/src/render.rs` (kept
//! byte-consistent with the CLI's `render_svg`/`render_html`/`render_png`/
//! `render_pdf` so both entry points produce identical output and a future
//! extraction into a shared crate is a mechanical move). See the CLI source for
//! the design notes.
//!
//! App extension: [`render_pdf_with`] adds an optional [`PdfLayout`] so the
//! export domain module can honour `orientation` (swap A4 portrait/landscape)
//! and `page_scale` (extra fit-to-page multiplier) without changing the default
//! [`render_pdf`] behaviour.

use std::collections::HashMap;
use std::io::Write;

use seattrellis_core::{CoreSolveRequest, CoreSolveResponse};

// ---------------------------------------------------------------------------
// Grid model
// ---------------------------------------------------------------------------

/// One seat in the recovered grid, plus the student seated there (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct GridCell {
    pub row: i32,
    pub col: i32,
    pub seat_index: usize,
    pub student: Option<String>,
    /// Optional per-student detail line (height / vision), rendered under
    /// the name when the privacy options ask for it (C.8).
    pub detail: Option<String>,
    pub enabled: bool,
    /// The seated student's key (identifier), when the request carries one.
    /// Used by the Office (XLSX/DOCX/PPTX) writers to mirror the oracle's
    /// "Assignments" sheet; the SVG/HTML/PNG/PDF renderers ignore it.
    pub student_key: Option<String>,
}

/// The full classroom grid recovered from a problem + solved assignment.
#[derive(Debug, Clone)]
pub struct SeatingGrid {
    pub title: String,
    pub subtitle: String,
    pub cells: Vec<GridCell>,
    pub min_row: i32,
    pub max_row: i32,
    pub min_col: i32,
    pub max_col: i32,
}

/// Safety bounds for the recovered grid extent (guards against pathological
/// row/col values from crafted layouts or extreme-but-finite seat
/// coordinates): every renderer iterates the whole extent, so an unbounded
/// range would overflow i32 arithmetic and loop for effectively forever.
const MAX_GRID_EXTENT: i64 = 10_000;
const MAX_GRID_CELLS: i64 = 10_000;

impl SeatingGrid {
    /// Recover the grid from a solve request and a solve response.
    pub fn build(request: &CoreSolveRequest, response: &CoreSolveResponse) -> Result<Self, String> {
        let seat_count = request.seat_positions.len();
        if seat_count == 0 {
            return Err("problem has no seat_positions to render".to_string());
        }

        // Map seat -> assigned student data. The detail line must follow the
        // assignment's student index, not the seat index: using the latter can
        // attach one student's height/vision to another student's name after
        // any non-identity solve or manual edit.
        let mut student_by_seat: HashMap<usize, String> = HashMap::new();
        let mut key_by_seat: HashMap<usize, String> = HashMap::new();
        let mut detail_by_seat: HashMap<usize, String> = HashMap::new();
        for [student_index, seat_index] in &response.assignment {
            if *student_index >= request.student_count || *seat_index >= seat_count {
                continue;
            }
            student_by_seat.insert(*seat_index, student_label(request, *student_index));
            if let Some(key) = request.students.get(*student_index).map(|s| s.key.clone()) {
                key_by_seat.insert(*seat_index, key);
            }
            if let Some(detail) = request
                .students
                .get(*student_index)
                .and_then(student_detail)
            {
                detail_by_seat.insert(*seat_index, detail);
            }
        }

        let mut cells = Vec::with_capacity(seat_count);
        let mut min_row = i32::MAX;
        let mut max_row = i32::MIN;
        let mut min_col = i32::MAX;
        let mut max_col = i32::MIN;
        for (seat_index, position) in request.seat_positions.iter().enumerate() {
            let (row, col, enabled) = seat_row_col(request, seat_index, *position)?;
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            min_col = min_col.min(col);
            max_col = max_col.max(col);
            cells.push(GridCell {
                row,
                col,
                seat_index,
                student: student_by_seat.get(&seat_index).cloned(),
                student_key: key_by_seat.get(&seat_index).cloned(),
                detail: detail_by_seat.get(&seat_index).cloned(),
                enabled,
            });
        }

        // Reject pathological extents before any renderer iterates them
        // (positions are only required to be finite, so `round() as i32`
        // can saturate to the i32 extremes and produce a ~2^32-cell grid).
        let extent_rows = i64::from(max_row) - i64::from(min_row) + 1;
        let extent_cols = i64::from(max_col) - i64::from(min_col) + 1;
        if extent_rows > MAX_GRID_EXTENT
            || extent_cols > MAX_GRID_EXTENT
            || extent_rows * extent_cols > MAX_GRID_CELLS
        {
            return Err(format!(
                "grid extent {extent_rows}x{extent_cols} is too large to render \
                 (limit {MAX_GRID_EXTENT} rows/cols, {MAX_GRID_CELLS} cells)"
            ));
        }

        let title = match &request.layout {
            Some(layout) if !layout.name.is_empty() => layout.name.clone(),
            _ => "Seating Plan".to_string(),
        };
        let subtitle = format!(
            "{} students / {} seats / {}",
            request.student_count,
            seat_count,
            if response.feasible {
                "feasible"
            } else {
                "infeasible"
            }
        );

        Ok(SeatingGrid {
            title,
            subtitle,
            cells,
            min_row,
            max_row,
            min_col,
            max_col,
        })
    }

    /// The seat occupying grid position `(row, col)`, if any.
    pub fn cell_at(&self, row: i32, col: i32) -> Option<&GridCell> {
        self.cells
            .iter()
            .find(|cell| cell.row == row && cell.col == col)
    }
}

/// Derive a seat's grid coordinates. Prefer the layout's authoritative
/// row/col/enabled when present; otherwise round the raw coordinates
/// (seat_positions are `[x, y]` grid points, so `col = round(x)`, `row = round(y)`).
fn seat_row_col(
    request: &CoreSolveRequest,
    index: usize,
    position: [f64; 2],
) -> Result<(i32, i32, bool), String> {
    if !position[0].is_finite() || !position[1].is_finite() {
        return Err(format!("seat {index} has a non-finite position"));
    }
    if let Some(layout) = &request.layout {
        if let Some(seat) = layout.seats.get(index) {
            return Ok((seat.row, seat.col, seat.enabled));
        }
    }
    Ok((position[1].round() as i32, position[0].round() as i32, true))
}

/// The display label for a student: `display_name`, else `key`, else
/// "Student N" — never empty.
/// Per-student detail line: height and/or vision, ASCII-only so the PDF
/// renderer can draw it (CJK is a M5-04 render-parity item).
fn student_detail(student: &seattrellis_core::models::Student) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(height) = student.height_cm {
        if height.is_finite() && height > 0.0 {
            parts.push(format!("{} cm", height.round()));
        }
    }
    if let Some(vision) = student
        .vision
        .as_deref()
        .filter(|vision| !vision.is_empty())
    {
        parts.push(format!("vision {vision}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("  "))
    }
}

fn student_label(request: &CoreSolveRequest, index: usize) -> String {
    if !request.students.is_empty() {
        if let Some(student) = request.students.get(index) {
            let name = student
                .display_name
                .as_deref()
                .or(Some(student.key.as_str()))
                .filter(|candidate| !candidate.is_empty());
            if let Some(name) = name {
                return name.to_string();
            }
        }
    }
    format!("Student {}", index + 1)
}

// ---------------------------------------------------------------------------
// Shared geometry
// ---------------------------------------------------------------------------

const CELL_W: f64 = 110.0;
const CELL_H: f64 = 64.0;
const PAD: f64 = 24.0;
const HEADER_H: f64 = 52.0;
const RECT_W: f64 = 102.0;
const RECT_H: f64 = 56.0;

fn grid_cols(grid: &SeatingGrid) -> i64 {
    i64::from(grid.max_col) - i64::from(grid.min_col) + 1
}

fn grid_rows(grid: &SeatingGrid) -> i64 {
    i64::from(grid.max_row) - i64::from(grid.min_row) + 1
}

/// Top-left origin of the grid cell at `(row, col)`.
fn cell_origin(grid: &SeatingGrid, row: i32, col: i32) -> (f64, f64) {
    let x = PAD + (col - grid.min_col) as f64 * CELL_W;
    let y = HEADER_H + PAD + (row - grid.min_row) as f64 * CELL_H;
    (x, y)
}

/// Font size for a student name, shrinking to fit wider/longer labels.
fn name_font_size(name: &str) -> u8 {
    match name.chars().count() {
        0..=7 => 13,
        8..=12 => 10,
        _ => 8,
    }
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

/// Escape text for use inside XML/HTML text nodes. `&apos;` is a valid named
/// entity in both XML and HTML5, so one function serves both renderers.
/// Control characters illegal in XML 1.0 (anything below 0x20 except tab/LF/CR)
/// are dropped rather than emitted raw.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------

/// Render the plan as a self-contained SVG document (no scripts, no external
/// references, UTF-8 text so CJK names display with the system's sans-serif).
///
/// The document deliberately starts with the `<svg` root element (no XML
/// declaration) so it opens cleanly in browsers and embeds as-is.
pub fn render_svg(grid: &SeatingGrid) -> String {
    let cols = grid_cols(grid) as f64;
    let rows = grid_rows(grid) as f64;
    let width = PAD * 2.0 + cols * CELL_W;
    let height = HEADER_H + PAD * 2.0 + rows * CELL_H;
    let grid_w = cols * CELL_W;

    let mut out = String::with_capacity(4096 + grid.cells.len() * 160);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n"
    ));
    out.push_str("  <rect x=\"0\" y=\"0\" width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n");

    // Header: title + subtitle.
    out.push_str(&format!(
        "  <text x=\"{}\" y=\"24\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\" fill=\"#1c2733\">{}</text>\n",
        width / 2.0,
        escape_text(&grid.title)
    ));
    out.push_str(&format!(
        "  <text x=\"{}\" y=\"42\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#5a6b7f\">{}</text>\n",
        width / 2.0,
        escape_text(&grid.subtitle)
    ));

    // Front-of-room indicator (min_row is the front, matching the solver).
    let front_y = HEADER_H + 6.0;
    out.push_str(&format!(
        "  <line x1=\"{PAD}\" y1=\"{front_y}\" x2=\"{}\" y2=\"{front_y}\" stroke=\"#c8d2de\" stroke-width=\"1\"/>\n",
        PAD + grid_w
    ));
    out.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"10\" fill=\"#8a97a6\">front of room</text>\n",
        PAD + grid_w / 2.0,
        front_y - 5.0
    ));

    for row in grid.min_row..=grid.max_row {
        for col in grid.min_col..=grid.max_col {
            let (x, y) = cell_origin(grid, row, col);
            let center_x = x + CELL_W / 2.0;
            let center_y = y + CELL_H / 2.0;
            match grid.cell_at(row, col) {
                Some(cell) => match &cell.student {
                    Some(name) => {
                        let size = name_font_size(name);
                        out.push_str(&format!(
                        "  <rect x=\"{}\" y=\"{}\" width=\"{RECT_W}\" height=\"{RECT_H}\" rx=\"7\" fill=\"#e8f0fe\" stroke=\"#4a7fd4\" stroke-width=\"1.5\"/>\n",
                        x + 4.0,
                        y + 4.0
                    ));
                        out.push_str(&format!(
                        "  <text x=\"{center_x}\" y=\"{center_y}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-family=\"sans-serif\" font-size=\"{size}\" fill=\"#1c2733\">{}</text>\n",
                        escape_text(name)
                    ));
                        if let Some(detail) = &cell.detail {
                            out.push_str(&format!(
                            "  <text x=\"{center_x}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"9\" fill=\"#7b8ea8\">{}</text>\n",
                            y + CELL_H - 20.0,
                            escape_text(detail)
                        ));
                        }
                        out.push_str(&format!(
                        "  <text x=\"{center_x}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"9\" fill=\"#7b8ea8\">{}</text>\n",
                        y + CELL_H - 9.0,
                        cell.seat_index + 1
                    ));
                    }
                    None => {
                        let label = if cell.enabled { "empty" } else { "unused" };
                        out.push_str(&format!(
                            "  <rect x=\"{}\" y=\"{}\" width=\"{RECT_W}\" height=\"{RECT_H}\" rx=\"7\" fill=\"#f7f8f9\" stroke=\"#cfd8e2\" stroke-width=\"1\" stroke-dasharray=\"4 3\"/>\n",
                            x + 4.0,
                            y + 4.0
                        ));
                        out.push_str(&format!(
                            "  <text x=\"{center_x}\" y=\"{center_y}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-family=\"sans-serif\" font-size=\"9\" fill=\"#9aa7b5\">{label}</text>\n"
                        ));
                    }
                },
                None => {
                    // Grid position with no seat: faint skeleton cell.
                    out.push_str(&format!(
                        "  <rect x=\"{}\" y=\"{}\" width=\"{RECT_W}\" height=\"{RECT_H}\" rx=\"7\" fill=\"#fbfbfc\" stroke=\"#eceff2\" stroke-width=\"1\"/>\n",
                        x + 4.0,
                        y + 4.0
                    ));
                }
            }
        }
    }

    out.push_str("</svg>\n");
    out
}

// ---------------------------------------------------------------------------
// HTML
// ---------------------------------------------------------------------------

/// Render the plan as a self-contained HTML page: one `<tr>` per grid row, one
/// `<td>` per grid column, inline CSS only, no scripts.
pub fn render_html(grid: &SeatingGrid) -> String {
    let mut out = String::with_capacity(4096 + grid.cells.len() * 160);
    out.push_str("<!DOCTYPE html>\n");
    out.push_str("<html lang=\"zh-CN\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape_text(&grid.title)));
    out.push_str("<style>\n");
    out.push_str("  body { font-family: -apple-system, \"PingFang SC\", \"Microsoft YaHei\", \"Noto Sans CJK\", \"Hiragino Sans GB\", sans-serif; margin: 24px; color: #1c2733; }\n");
    out.push_str("  h1 { font-size: 22px; margin: 0 0 4px; text-align: center; }\n");
    out.push_str(
        "  p.sub { font-size: 14px; color: #5a6b7f; margin: 0 0 12px; text-align: center; }\n",
    );
    out.push_str(
        "  p.front { font-size: 12px; color: #8a97a6; margin: 0 0 6px; text-align: center; }\n",
    );
    // Fixed-layout table: cells share the printable width evenly and scale
    // down on narrow windows instead of overflowing the screen.
    out.push_str("  table.seating { border-collapse: separate; border-spacing: 6px; margin: 0 auto; width: 100%; max-width: 1000px; table-layout: fixed; }\n");
    out.push_str("  td { height: 58px; text-align: center; vertical-align: middle; border-radius: 7px; font-size: 14px; overflow: hidden; }\n");
    out.push_str("  td.seat { background: #e8f0fe; border: 1px solid #4a7fd4; }\n");
    out.push_str("  td.seat .name { font-weight: 600; font-size: 15px; }\n");
    out.push_str("  td.seat .detail { display: block; font-size: 11px; color: #7b8ea8; }\n");
    out.push_str(
        "  td.seat .num { display: block; font-size: 11px; color: #7b8ea8; margin-top: 2px; }\n",
    );
    out.push_str("  td.empty { background: #f7f8f9; border: 1px dashed #cfd8e2; color: #9aa7b5; font-size: 11px; }\n");
    out.push_str("  td.void { border: none; }\n");
    out.push_str("  @media (max-width: 640px) { td { height: 48px; } td.seat .name { font-size: 13px; } td.seat .detail, td.seat .num { font-size: 10px; } }\n");
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str(&format!("<h1>{}</h1>\n", escape_text(&grid.title)));
    out.push_str(&format!(
        "<p class=\"sub\">{}</p>\n",
        escape_text(&grid.subtitle)
    ));
    out.push_str("<p class=\"front\">front of room</p>\n");
    out.push_str("<table class=\"seating\">\n");

    for row in grid.min_row..=grid.max_row {
        out.push_str("  <tr>\n");
        for col in grid.min_col..=grid.max_col {
            match grid.cell_at(row, col) {
                Some(cell) => match &cell.student {
                    Some(name) => {
                        let detail = cell
                            .detail
                            .as_ref()
                            .map(|detail| {
                                format!("<span class=\"detail\">{}</span>", escape_text(detail))
                            })
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "    <td class=\"seat\"><span class=\"name\">{}</span>{detail}<span class=\"num\">{}</span></td>\n",
                            escape_text(name),
                            cell.seat_index + 1
                        ));
                    }
                    None => {
                        let label = if cell.enabled { "empty" } else { "unused" };
                        out.push_str(&format!("    <td class=\"empty\">{label}</td>\n"));
                    }
                },
                None => {
                    out.push_str("    <td class=\"void\"></td>\n");
                }
            }
        }
        out.push_str("  </tr>\n");
    }

    out.push_str("</table>\n</body>\n</html>\n");
    out
}

// ---------------------------------------------------------------------------
// PNG / PDF shared palette
// ---------------------------------------------------------------------------

/// RGB colors shared by the PNG and PDF renderers. Mirrors the SVG palette so
/// all four exports agree: occupied seats are blue-tinted, empty seats light
/// gray, disabled seats a muted gray, and void grid positions near-white.
type Rgb = [u8; 3];

const WHITE: Rgb = [0xff, 0xff, 0xff];
const OCCUPIED_FILL: Rgb = [0xe8, 0xf0, 0xfe];
const OCCUPIED_STROKE: Rgb = [0x4a, 0x7f, 0xd4];
const EMPTY_FILL: Rgb = [0xf7, 0xf8, 0xf9];
const EMPTY_STROKE: Rgb = [0xcf, 0xd8, 0xe2];
const DISABLED_FILL: Rgb = [0xec, 0xef, 0xf3];
const DISABLED_STROKE: Rgb = [0x9a, 0xa7, 0xb5];
const VOID_FILL: Rgb = [0xfb, 0xfb, 0xfc];
const VOID_STROKE: Rgb = [0xec, 0xef, 0xf2];
const DIVIDER: Rgb = [0xc8, 0xd2, 0xde];

/// The (fill, stroke) color pair for the grid position at `(row, col)`.
fn cell_colors(grid: &SeatingGrid, row: i32, col: i32) -> (Rgb, Rgb) {
    match grid.cell_at(row, col) {
        Some(cell) if cell.student.is_some() => (OCCUPIED_FILL, OCCUPIED_STROKE),
        Some(cell) if !cell.enabled => (DISABLED_FILL, DISABLED_STROKE),
        Some(_) => (EMPTY_FILL, EMPTY_STROKE),
        None => (VOID_FILL, VOID_STROKE),
    }
}

// ---------------------------------------------------------------------------
// PNG
// ---------------------------------------------------------------------------

/// Rasterize the plan as an RGB PNG.
///
/// The image is rendered at 2× density so names remain legible in documents,
/// messaging apps, and classroom projectors. Text uses the same discovered
/// system CJK font as PDF and is resolved at export time.
pub fn render_png(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    let cols = grid_cols(grid);
    let rows = grid_rows(grid);
    let width = ((PAD * 2.0 + cols as f64 * CELL_W) * PNG_RASTER_SCALE).ceil() as u32;
    let height = ((HEADER_H + PAD * 2.0 + rows as f64 * CELL_H) * PNG_RASTER_SCALE).ceil() as u32;
    if width == 0 || height == 0 {
        return Err("cannot render an empty grid to PNG".to_string());
    }
    // Fail with an error instead of allocating an unbounded raster for a
    // wide/tall grid (2x scale: a 1000x100 room would need ~8.5 GiB).
    if u64::from(width) * u64::from(height) * 3 > MAX_RASTER_BYTES {
        return Err(format!(
            "grid is too large to rasterize to PNG ({width}x{height} px exceeds \
             the {MAX_RASTER_BYTES}-byte buffer limit)"
        ));
    }

    let mut data = vec![0u8; width as usize * height as usize * 3];
    let mut canvas = Canvas::new(&mut data, width, height);
    canvas.fill(0, 0, width, height, WHITE);
    let divider_y = ((HEADER_H + 4.0) * PNG_RASTER_SCALE) as u32;
    canvas.fill(0, divider_y, width, 2, DIVIDER);

    let font = crate::fonts::load_cjk_font();
    if let Some(font) = &font {
        draw_text_in_rect(
            &mut canvas,
            font,
            &grid.title,
            PAD * PNG_RASTER_SCALE,
            6.0 * PNG_RASTER_SCALE,
            (f64::from(width) / PNG_RASTER_SCALE - PAD * 2.0) * PNG_RASTER_SCALE,
            26.0 * PNG_RASTER_SCALE,
            18.0 * PNG_RASTER_SCALE,
            (20, 20, 19),
        );
        draw_text_in_rect(
            &mut canvas,
            font,
            &grid.subtitle,
            PAD * PNG_RASTER_SCALE,
            31.0 * PNG_RASTER_SCALE,
            (f64::from(width) / PNG_RASTER_SCALE - PAD * 2.0) * PNG_RASTER_SCALE,
            16.0 * PNG_RASTER_SCALE,
            9.0 * PNG_RASTER_SCALE,
            (94, 93, 89),
        );
        draw_text_in_rect(
            &mut canvas,
            font,
            "讲台 / FRONT OF ROOM",
            PAD * PNG_RASTER_SCALE,
            47.0 * PNG_RASTER_SCALE,
            (f64::from(width) / PNG_RASTER_SCALE - PAD * 2.0) * PNG_RASTER_SCALE,
            15.0 * PNG_RASTER_SCALE,
            8.0 * PNG_RASTER_SCALE,
            (94, 93, 89),
        );
    }
    for row in grid.min_row..=grid.max_row {
        for col in grid.min_col..=grid.max_col {
            let (x, y) = cell_origin(grid, row, col);
            canvas.rect(
                (x + 4.0) * PNG_RASTER_SCALE,
                (y + 4.0) * PNG_RASTER_SCALE,
                RECT_W * PNG_RASTER_SCALE,
                RECT_H * PNG_RASTER_SCALE,
                cell_colors(grid, row, col),
                2,
            );
            if let Some(cell) = grid.cell_at(row, col) {
                if let Some(font) = &font {
                    let text_x = (x + 4.0) * PNG_RASTER_SCALE;
                    let text_y = (y + 4.0) * PNG_RASTER_SCALE;
                    let text_w = RECT_W * PNG_RASTER_SCALE;
                    let text_h = RECT_H * PNG_RASTER_SCALE;
                    if let Some(name) = &cell.student {
                        draw_text_in_rect(
                            &mut canvas,
                            font,
                            name,
                            text_x,
                            text_y + text_h * 0.08,
                            text_w,
                            text_h * 0.56,
                            14.0 * PNG_RASTER_SCALE,
                            (30, 34, 40),
                        );
                        draw_text_in_rect(
                            &mut canvas,
                            font,
                            &(cell.seat_index + 1).to_string(),
                            text_x,
                            text_y + text_h * 0.72,
                            text_w,
                            text_h * 0.2,
                            7.0 * PNG_RASTER_SCALE,
                            (94, 93, 89),
                        );
                    } else if cell.enabled {
                        draw_text_in_rect(
                            &mut canvas,
                            font,
                            "空座",
                            text_x,
                            text_y,
                            text_w,
                            text_h,
                            9.0 * PNG_RASTER_SCALE,
                            (154, 167, 181),
                        );
                    }
                }
            }
        }
    }

    let mut out = Vec::new();
    {
        // The encoder borrows `out`; drop the writer before returning it.
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("PNG header write failed: {error}"))?;
        writer
            .write_image_data(&data)
            .map_err(|error| format!("PNG data write failed: {error}"))?;
    }
    Ok(out)
}

const PNG_RASTER_SCALE: f64 = 2.0;

/// Upper bound for a PNG raster buffer (3 bytes per pixel at 2x density).
const MAX_RASTER_BYTES: u64 = 512 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
fn draw_text_in_rect(
    canvas: &mut Canvas<'_>,
    font: &fontdue::Font,
    text: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    max_font_px: f64,
    color: (u8, u8, u8),
) {
    if text.is_empty() {
        return;
    }
    let max_px = ((w - 4.0).max(2.0) / text.chars().count().max(1) as f64).min(max_font_px);
    let px_size = max_px.clamp(6.0, max_font_px).round() as f32;
    // Total advance width to center the string.
    let total_width: f64 = text
        .chars()
        .map(|ch| font.rasterize(ch, px_size).0.advance_width as f64)
        .sum();
    let mut cursor_x = x + (w - total_width) / 2.0;
    let baseline = y + h * 0.68;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, px_size);
        let draw_x = (cursor_x + metrics.xmin as f64).round() as u32;
        let draw_y = (baseline - metrics.height as f64 + metrics.ymin as f64).round() as u32;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let alpha = bitmap[row * metrics.width + col];
                if alpha > 0 {
                    canvas.blend_pixel(draw_x + col as u32, draw_y + row as u32, color, alpha);
                }
            }
        }
        cursor_x += metrics.advance_width as f64;
    }
}

/// `png` encoder owns the chunk/compression details).
struct Canvas<'a> {
    data: &'a mut [u8],
    width: u32,
    height: u32,
}

impl Canvas<'_> {
    fn new(data: &mut [u8], width: u32, height: u32) -> Canvas<'_> {
        Canvas {
            data,
            width,
            height,
        }
    }

    /// Blend an anti-aliased glyph pixel over the canvas (alpha 0..=255).
    fn blend_pixel(&mut self, x: u32, y: u32, color: (u8, u8, u8), alpha: u8) {
        if x >= self.width || y >= self.height || alpha == 0 {
            return;
        }
        let index = (y * self.width + x) as usize * 3;
        if alpha == 255 {
            self.data[index] = color.0;
            self.data[index + 1] = color.1;
            self.data[index + 2] = color.2;
            return;
        }
        let a = alpha as u32;
        let fg = [color.0 as u32, color.1 as u32, color.2 as u32];
        for (offset, channel) in fg.into_iter().enumerate() {
            let base = self.data[index + offset] as u32;
            self.data[index + offset] = ((channel * a + base * (255 - a)) / 255) as u8;
        }
    }

    /// Fill `[x, x+w) x [y, y+h)` with `rgb`, clipped to the image bounds.
    fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, rgb: Rgb) {
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);
        if x >= self.width || y >= self.height || x_end <= x || y_end <= y {
            return;
        }
        let row_len = self.width as usize * 3;
        for yy in y..y_end {
            let start = yy as usize * row_len + x as usize * 3;
            let end = yy as usize * row_len + x_end as usize * 3;
            for idx in (start..end).step_by(3) {
                self.data[idx] = rgb[0];
                self.data[idx + 1] = rgb[1];
                self.data[idx + 2] = rgb[2];
            }
        }
    }

    /// Draw a rectangle with a `border`-pixel border in the stroke color around
    /// an interior filled with the fill color.
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, colors: (Rgb, Rgb), border: u32) {
        let (fill, stroke) = colors;
        let x0 = x.round() as u32;
        let y0 = y.round() as u32;
        let rw = w.round() as u32;
        let rh = h.round() as u32;
        // Outer rect in the stroke color, then the inset area in the fill color.
        self.fill(x0, y0, rw, rh, stroke);
        if rw > border * 2 && rh > border * 2 {
            self.fill(
                x0 + border,
                y0 + border,
                rw - border * 2,
                rh - border * 2,
                fill,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// PDF
// ---------------------------------------------------------------------------

/// A4 portrait page size in points (PDF coordinates grow up and to the right).
/// Default printable margin in points (12mm ≈ 34pt), matching the print
/// layout spec (margins 12/14mm).
fn default_margin_pt() -> f64 {
    (12.0_f64 * 72.0 / 25.4).round()
}
/// Vertical space reserved for the title, subtitle, and front-of-room label.
const PDF_HEADER_SPACE: f64 = 100.0;

/// Page geometry for the PDF page-image renderer (app extension).
///
/// Defaults to A4 portrait at the natural fit-to-page scale. The export domain
/// module swaps in [`PdfLayout::landscape`] for `orientation: "landscape"` and
/// applies the frontend `page_scale` via [`PdfLayout::with_scale`] so the
/// `orientation`/`page_scale` fields of `ExportDraftRequest` map without
/// changing the default [`render_pdf`] behaviour.
/// Standard page sizes for document exports (plan §12.3 unification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    A4,
    A3,
    Letter,
}

impl PaperSize {
    /// Page dimensions in points (portrait order: width, height).
    pub fn points(self) -> (f64, f64) {
        match self {
            PaperSize::A4 => (595.0, 842.0),
            PaperSize::A3 => (842.0, 1191.0),
            PaperSize::Letter => (612.0, 792.0),
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "a4" => Ok(PaperSize::A4),
            "a3" => Ok(PaperSize::A3),
            "letter" => Ok(PaperSize::Letter),
            other => Err(format!(
                "unknown export paper_size '{other}' (expected a4, a3, or letter)"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PdfLayout {
    /// Page width in points.
    pub page_w: f64,
    /// Page height in points.
    pub page_h: f64,
    /// Extra multiplier on top of the automatic fit-to-page scale.
    pub scale_multiplier: f64,
    /// Printable margin in points (from `margin_mm`).
    pub margin_pt: f64,
    /// Discovered CJK font name for quality diagnostics and export metadata.
    /// Glyphs themselves are rasterized at export time (PD-D12 R2).
    pub font_name: Option<String>,
    /// Quality band of the discovered font (drives the export warning).
    pub font_quality: crate::fonts::FontQuality,
}

impl PdfLayout {
    /// A4 portrait with the default margin.
    pub fn portrait() -> Self {
        Self::from_paper(PaperSize::A4, false, default_margin_pt())
    }

    /// A4 landscape with the default margin.
    pub fn landscape() -> Self {
        Self::from_paper(PaperSize::A4, true, default_margin_pt())
    }

    /// Page geometry from paper size + orientation + margin (mm→pt).
    pub fn from_paper(paper: PaperSize, landscape: bool, margin_mm: f64) -> Self {
        let (mut w, mut h) = paper.points();
        if landscape {
            std::mem::swap(&mut w, &mut h);
        }
        let font = crate::fonts::find_system_cjk_font();
        let font_name =
            (font.quality != crate::fonts::FontQuality::None).then_some(font.pdf_name.clone());
        PdfLayout {
            page_w: w,
            page_h: h,
            scale_multiplier: 1.0,
            margin_pt: (margin_mm.clamp(5.0, 25.0) * 72.0 / 25.4).round(),
            font_name,
            font_quality: font.quality,
        }
    }

    /// Override font metadata (tests / explicit user selection).
    pub fn with_font(mut self, name: &str, quality: crate::fonts::FontQuality) -> Self {
        self.font_name = Some(name.to_string());
        self.font_quality = quality;
        self
    }

    /// Clamp a user `page_scale` into a sane range and apply it.
    pub fn with_scale(mut self, multiplier: f64) -> Self {
        self.scale_multiplier = multiplier.clamp(0.5, 2.0);
        self
    }
}

/// Render the plan as a single-page, viewer-independent PDF.
///
/// The page is rasterized with the system font at export time and stored as a
/// losslessly encoded image.  The previous system-font-reference path wrote
/// one font's glyph IDs without embedding that font; viewers that substituted
/// another font displayed dots, boxes, or unrelated letters.
pub fn render_pdf(grid: &SeatingGrid) -> String {
    render_pdf_with(
        grid,
        PdfLayout::from_paper(PaperSize::A4, false, default_margin_pt()),
    )
}

/// [`render_pdf`] with an explicit page geometry (orientation + scale).
pub fn render_pdf_with(grid: &SeatingGrid, layout: PdfLayout) -> String {
    let (image_width, image_height, rgb) = rasterize_pdf_page(grid, &layout);
    let (filter, compressed) = pdf_compress_image(&rgb);
    let encoded = ascii_hex(&compressed);
    let content = format!(
        "q\n{:.2} 0 0 {:.2} 0 0 cm\n/Im0 Do\nQ\n",
        layout.page_w, layout.page_h
    );

    let mut bodies = Vec::new();
    bodies.push("<< /Type /Catalog /Pages 2 0 R >>".to_string()); // obj 1
    bodies.push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()); // obj 2
    bodies.push(format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w} {page_h}] \
         /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>",
        page_w = layout.page_w,
        page_h = layout.page_h
    ));
    bodies.push(format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )); // obj 4
    bodies.push(format!(
        "<< /Type /XObject /Subtype /Image /Width {image_width} /Height {image_height} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 \
         /Filter [/ASCIIHexDecode {filter}] /Length {} >>\nstream\n{encoded}>\nendstream",
        encoded.len() + 2
    )); // obj 5

    let count = bodies.len() + 1;
    let mut out = String::with_capacity(4096 + content.len());
    out.push_str("%PDF-1.4\n%\u{e2}\u{e3}\u{cf}\u{d3}\n");
    let mut offsets = Vec::with_capacity(bodies.len());
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref_pos = out.len();
    out.push_str(&format!("xref\n0 {count}\n"));
    out.push_str("0000000000 65535 f \n");
    for offset in &offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"
    ));
    out
}

fn pdf_compress_image(data: &[u8]) -> (&'static str, Vec<u8>) {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    if encoder.write_all(data).is_ok() {
        if let Ok(compressed) = encoder.finish() {
            return ("/FlateDecode", compressed);
        }
    }
    ("/RunLengthDecode", pdf_run_length_encode(data))
}

const PDF_RASTER_SCALE: f64 = 2.0;

/// Rasterize the page at 144 DPI. The flat classroom palette compresses well
/// with FlateDecode while names stay crisp on paper and on screen.
fn rasterize_pdf_page(grid: &SeatingGrid, layout: &PdfLayout) -> (u32, u32, Vec<u8>) {
    let width = (layout.page_w * PDF_RASTER_SCALE).round() as u32;
    let height = (layout.page_h * PDF_RASTER_SCALE).round() as u32;
    let mut data = vec![0_u8; width as usize * height as usize * 3];
    let mut canvas = Canvas::new(&mut data, width, height);
    canvas.fill(0, 0, width, height, WHITE);

    let cols = grid_cols(grid) as f64;
    let rows = grid_rows(grid) as f64;
    let grid_w = cols * CELL_W;
    let grid_h = rows * CELL_H;
    let avail_w = layout.page_w - layout.margin_pt * 2.0;
    let avail_h = layout.page_h - PDF_HEADER_SPACE - layout.margin_pt * 0.5;
    let base_scale = (avail_w / grid_w).min(avail_h / grid_h).clamp(0.1, 2.0);
    let scale = (base_scale * layout.scale_multiplier).clamp(0.1, 2.0);
    let grid_x = layout.margin_pt;
    let grid_top = 112.0;

    let font = crate::fonts::load_cjk_font();
    if let Some(font) = &font {
        draw_text_in_rect(
            &mut canvas,
            font,
            &grid.title,
            layout.margin_pt * PDF_RASTER_SCALE,
            22.0 * PDF_RASTER_SCALE,
            (layout.page_w - layout.margin_pt * 2.0) * PDF_RASTER_SCALE,
            30.0 * PDF_RASTER_SCALE,
            20.0 * PDF_RASTER_SCALE,
            (20, 20, 19),
        );
        draw_text_in_rect(
            &mut canvas,
            font,
            &grid.subtitle,
            layout.margin_pt * PDF_RASTER_SCALE,
            54.0 * PDF_RASTER_SCALE,
            (layout.page_w - layout.margin_pt * 2.0) * PDF_RASTER_SCALE,
            20.0 * PDF_RASTER_SCALE,
            11.0 * PDF_RASTER_SCALE,
            (94, 93, 89),
        );
        draw_text_in_rect(
            &mut canvas,
            font,
            "讲台 / FRONT OF ROOM",
            grid_x * PDF_RASTER_SCALE,
            78.0 * PDF_RASTER_SCALE,
            grid_w * scale * PDF_RASTER_SCALE,
            18.0 * PDF_RASTER_SCALE,
            9.0 * PDF_RASTER_SCALE,
            (94, 93, 89),
        );
    }
    canvas.fill(
        (grid_x * PDF_RASTER_SCALE).round() as u32,
        (102.0 * PDF_RASTER_SCALE).round() as u32,
        (grid_w * scale * PDF_RASTER_SCALE).round() as u32,
        2,
        DIVIDER,
    );

    for row in grid.min_row..=grid.max_row {
        for col in grid.min_col..=grid.max_col {
            let col_offset = f64::from(col - grid.min_col);
            let row_offset = f64::from(row - grid.min_row);
            let inner_x = grid_x + col_offset * CELL_W * scale + 4.0 * scale;
            let inner_y = grid_top + row_offset * CELL_H * scale + 4.0 * scale;
            let inner_w = RECT_W * scale;
            let inner_h = RECT_H * scale;
            canvas.rect(
                inner_x * PDF_RASTER_SCALE,
                inner_y * PDF_RASTER_SCALE,
                inner_w * PDF_RASTER_SCALE,
                inner_h * PDF_RASTER_SCALE,
                cell_colors(grid, row, col),
                2,
            );

            let Some(cell) = grid.cell_at(row, col) else {
                continue;
            };
            let Some(font) = &font else {
                continue;
            };
            let x = inner_x * PDF_RASTER_SCALE;
            let y = inner_y * PDF_RASTER_SCALE;
            let w = inner_w * PDF_RASTER_SCALE;
            let h = inner_h * PDF_RASTER_SCALE;
            if let Some(name) = &cell.student {
                draw_text_in_rect(
                    &mut canvas,
                    font,
                    name,
                    x,
                    y + h * 0.10,
                    w,
                    h * 0.48,
                    13.0 * PDF_RASTER_SCALE,
                    (20, 20, 19),
                );
                if let Some(detail) = &cell.detail {
                    draw_text_in_rect(
                        &mut canvas,
                        font,
                        detail,
                        x,
                        y + h * 0.53,
                        w,
                        h * 0.25,
                        7.5 * PDF_RASTER_SCALE,
                        (94, 93, 89),
                    );
                }
                draw_text_in_rect(
                    &mut canvas,
                    font,
                    &(cell.seat_index + 1).to_string(),
                    x,
                    y + h * 0.78,
                    w,
                    h * 0.17,
                    7.0 * PDF_RASTER_SCALE,
                    (94, 93, 89),
                );
            } else if cell.enabled {
                draw_text_in_rect(
                    &mut canvas,
                    font,
                    "空座",
                    x,
                    y,
                    w,
                    h,
                    9.0 * PDF_RASTER_SCALE,
                    (154, 167, 181),
                );
            }
        }
    }
    (width, height, data)
}

/// PDF RunLengthEncode packets (the same packet layout as TIFF PackBits).
fn pdf_run_length_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 4);
    let mut index = 0;
    while index < data.len() {
        let mut repeated = 1_usize;
        while index + repeated < data.len()
            && data[index + repeated] == data[index]
            && repeated < 128
        {
            repeated += 1;
        }
        if repeated >= 3 {
            out.push((257 - repeated) as u8);
            out.push(data[index]);
            index += repeated;
            continue;
        }

        let literal_start = index;
        index += repeated;
        while index < data.len() && index - literal_start < 128 {
            let mut next_repeated = 1_usize;
            while index + next_repeated < data.len()
                && data[index + next_repeated] == data[index]
                && next_repeated < 128
            {
                next_repeated += 1;
            }
            if next_repeated >= 3 || index - literal_start + next_repeated > 128 {
                break;
            }
            index += next_repeated;
        }
        let literal_len = index - literal_start;
        out.push((literal_len - 1) as u8);
        out.extend_from_slice(&data[literal_start..index]);
    }
    out.push(128);
    out
}

fn ascii_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(data.len() * 2);
    for byte in data {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use seattrellis_core::models::{Layout, Seat, Student};

    fn sample_request() -> CoreSolveRequest {
        CoreSolveRequest {
            api_version: 2,
            student_count: 4,
            seat_positions: vec![
                [1.0, 1.0],
                [2.0, 1.0],
                [3.0, 1.0],
                [1.0, 2.0],
                [2.0, 2.0],
                [3.0, 2.0],
            ],
            edges: Vec::new(),
            fixed_seats: Vec::new(),
            must_be_adjacent: Vec::new(),
            cannot_be_adjacent: Vec::new(),
            min_distance: Vec::new(),
            seed: 0,
            time_limit_seconds: None,
            students: vec![
                Student {
                    key: "S1".into(),
                    display_name: Some("Alice".into()),
                    ..Student::default()
                },
                Student {
                    key: "S2".into(),
                    display_name: Some("Bob".into()),
                    ..Student::default()
                },
                Student {
                    key: "S3".into(),
                    display_name: None,
                    ..Student::default()
                },
                Student {
                    key: "S4".into(),
                    display_name: Some("张伟".into()),
                    ..Student::default()
                },
            ],
            student_scores: Vec::new(),
            rules: None,
            layout: None,
            history: None,
            pair_history: None,
        }
    }

    fn sample_response() -> CoreSolveResponse {
        CoreSolveResponse {
            api_version: 2,
            feasible: true,
            status: seattrellis_core::SolveStatus::Solved,
            assignment: vec![[0, 0], [1, 1], [2, 2], [3, 3]],
            attempts_used: 4,
            hard_constraints_satisfied: true,
            total_cost: Some(12.5),
        }
    }

    #[test]
    fn recovers_grid_from_positions_and_assignment() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        assert_eq!((grid.min_row, grid.max_row), (1, 2));
        assert_eq!((grid.min_col, grid.max_col), (1, 3));
        assert_eq!(grid.cells.len(), 6);

        // Student 0 -> seat 0 at (row 1, col 1).
        assert_eq!(
            grid.cell_at(1, 1).unwrap().student.as_deref(),
            Some("Alice")
        );
        assert_eq!(grid.cell_at(1, 2).unwrap().student.as_deref(), Some("Bob"));
        assert_eq!(grid.cell_at(1, 3).unwrap().student.as_deref(), Some("S3"));
        assert_eq!(grid.cell_at(2, 1).unwrap().student.as_deref(), Some("张伟"));
        // Seats 4 and 5 are unassigned.
        assert_eq!(grid.cell_at(2, 2).unwrap().student, None);
        assert_eq!(grid.cell_at(2, 3).unwrap().student, None);
    }

    #[test]
    fn sensitive_detail_follows_assigned_student_not_seat_index() {
        let mut request = sample_request();
        request.students[0].height_cm = Some(151.0);
        request.students[0].vision = Some("left".to_string());
        request.students[1].height_cm = Some(179.0);
        request.students[1].vision = Some("right".to_string());
        let response = CoreSolveResponse {
            assignment: vec![[0, 1], [1, 0], [2, 2], [3, 3]],
            ..sample_response()
        };

        let grid = SeatingGrid::build(&request, &response).unwrap();
        let first_seat = grid.cell_at(1, 1).unwrap();
        assert_eq!(first_seat.student.as_deref(), Some("Bob"));
        assert_eq!(first_seat.detail.as_deref(), Some("179 cm  vision right"));
        let second_seat = grid.cell_at(1, 2).unwrap();
        assert_eq!(second_seat.student.as_deref(), Some("Alice"));
        assert_eq!(second_seat.detail.as_deref(), Some("151 cm  vision left"));
    }

    #[test]
    fn svg_is_self_contained_and_self_closing() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let svg = render_svg(&grid);
        assert!(
            svg.starts_with("<svg "),
            "document opens with the <svg root"
        );
        assert!(svg.contains("viewBox"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(!svg.contains("<script"), "no scripts");
        assert!(!svg.contains("href="), "no external references");
        assert!(!svg.contains("<image"), "no external images");
        assert!(!svg.contains("url("), "no external fills");
        assert!(svg.contains("Alice"));
        assert!(svg.contains("张伟"), "CJK names survive as UTF-8");
    }

    #[test]
    fn svg_escapes_special_characters_in_names() {
        let mut request = sample_request();
        request.students[0].display_name = Some("A&B <C>\"'".into());
        let response = CoreSolveResponse {
            assignment: vec![[0, 0]],
            ..sample_response()
        };
        let grid = SeatingGrid::build(&request, &response).unwrap();
        let svg = render_svg(&grid);
        assert!(svg.contains("A&amp;B &lt;C&gt;&quot;&apos;"));
        assert!(
            !svg.contains("A&B <C>"),
            "raw special characters must not appear"
        );
    }

    #[test]
    fn svg_handles_infeasible_assignment() {
        let response = CoreSolveResponse {
            feasible: false,
            assignment: Vec::new(),
            hard_constraints_satisfied: false,
            total_cost: None,
            ..sample_response()
        };
        let grid = SeatingGrid::build(&sample_request(), &response).unwrap();
        let svg = render_svg(&grid);
        assert!(svg.contains("infeasible"), "subtitle notes infeasibility");
        assert!(svg.starts_with("<svg "));
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn html_is_self_contained_and_escapes_names() {
        let mut request = sample_request();
        request.students[0].display_name = Some("A&B <C>".into());
        let response = CoreSolveResponse {
            assignment: vec![[0, 0]],
            ..sample_response()
        };
        let grid = SeatingGrid::build(&request, &response).unwrap();
        let html = render_html(&grid);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<table"));
        assert!(html.contains("</table>"));
        assert!(!html.contains("<script"), "no scripts");
        assert!(html.contains("A&amp;B &lt;C&gt;"));
        assert!(
            !html.contains("A&B <C>"),
            "raw special characters must not appear"
        );
    }

    #[test]
    fn html_renders_empty_and_void_cells() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let html = render_html(&grid);
        assert!(html.contains("class=\"empty\""), "empty seats are marked");
        // Row 1 has 3 seats but max col is 3 == min col 1, so no void here;
        // use a wider request to exercise void cells.
        assert!(!html.contains("class=\"void\""));
    }

    #[test]
    fn html_renders_void_cells_for_sparse_rows() {
        let mut request = sample_request();
        request.seat_positions = vec![[1.0, 1.0], [2.0, 1.0], [4.0, 1.0]];
        request.student_count = 1;
        request.students.truncate(1);
        let response = CoreSolveResponse {
            assignment: vec![[0, 0]],
            ..sample_response()
        };
        let grid = SeatingGrid::build(&request, &response).unwrap();
        assert_eq!((grid.min_col, grid.max_col), (1, 4));
        let html = render_html(&grid);
        assert!(
            html.contains("class=\"void\""),
            "missing grid positions are void"
        );
        let svg = render_svg(&grid);
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn disabled_seats_render_as_unused() {
        let mut request = sample_request();
        let mut disabled = Seat::new("R2C3", 2, 3);
        disabled.enabled = false;
        request.layout = Some(Layout::new(vec![
            Seat::new("R1C1", 1, 1),
            Seat::new("R1C2", 1, 2),
            Seat::new("R1C3", 1, 3),
            Seat::new("R2C1", 2, 1),
            Seat::new("R2C2", 2, 2),
            disabled,
        ]));
        let grid = SeatingGrid::build(&request, &sample_response()).unwrap();
        assert!(!grid.cell_at(2, 3).unwrap().enabled);
        assert!(render_svg(&grid).contains("unused"));
        assert!(render_html(&grid).contains("unused"));
    }

    #[test]
    fn grid_rejects_empty_seat_positions() {
        let mut request = sample_request();
        request.seat_positions.clear();
        let error = SeatingGrid::build(&request, &sample_response()).unwrap_err();
        assert!(
            error.contains("seat_positions"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn build_rejects_pathological_grid_extent() {
        // Extreme-but-finite positions pass core validation (only finiteness
        // is checked) and saturate to the i32 extremes on rounding; the
        // resulting ~2^32-cell extent must be rejected instead of
        // overflowing i32 math and hanging every renderer.
        let mut request = sample_request();
        request.student_count = 2;
        request.students.truncate(2);
        request.seat_positions = vec![[1e300, 1e300], [-1e300, -1e300]];
        let response = CoreSolveResponse {
            assignment: vec![[0, 0], [1, 1]],
            ..sample_response()
        };
        let error = SeatingGrid::build(&request, &response).unwrap_err();
        assert!(error.contains("too large"), "unexpected error: {error}");

        // Same guard when the extremes come from the layout's row/col fields.
        let mut request = sample_request();
        request.student_count = 2;
        request.students.truncate(2);
        request.seat_positions = vec![[1.0, 1.0], [2.0, 1.0]];
        request.layout = Some(Layout::new(vec![
            Seat::new("a", i32::MAX, 1),
            Seat::new("b", i32::MIN, 1),
        ]));
        let error = SeatingGrid::build(&request, &response).unwrap_err();
        assert!(error.contains("too large"), "unexpected error: {error}");
    }

    #[test]
    fn png_rejects_oversized_raster_instead_of_allocating() {
        // A wide grid within the extent guard would need gigabytes of pixel
        // buffer at 2x density; render_png must fail cleanly, not allocate.
        let grid = SeatingGrid {
            title: "t".into(),
            subtitle: "s".into(),
            cells: Vec::new(),
            min_row: 1,
            max_row: 100,
            min_col: 1,
            max_col: 1000,
        };
        let error = render_png(&grid).unwrap_err();
        assert!(error.contains("too large"), "unexpected error: {error}");
    }

    #[test]
    fn escape_text_handles_specials_and_control_characters() {
        assert_eq!(escape_text("&<>\"'"), "&amp;&lt;&gt;&quot;&apos;");
        assert_eq!(
            escape_text("a\u{1}b\n"),
            "ab\n",
            "control chars dropped, LF kept"
        );
        assert_eq!(escape_text("张伟"), "张伟", "CJK passes through unchanged");
    }

    #[test]
    fn font_size_shrinks_for_long_names() {
        assert_eq!(name_font_size("Alice"), 13);
        assert_eq!(name_font_size("AliceandBob"), 10);
        assert_eq!(name_font_size("AveryLongStudentName"), 8);
    }

    #[test]
    fn png_magic_header_and_dimensions_match_grid() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let bytes = render_png(&grid).unwrap();

        // 8-byte PNG signature.
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
        // IHDR width/height are big-endian u32s at bytes 16..24.
        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let cols = grid_cols(&grid) as f64;
        let rows = grid_rows(&grid) as f64;
        assert_eq!(
            width,
            ((PAD * 2.0 + cols * CELL_W) * PNG_RASTER_SCALE).ceil() as u32
        );
        assert_eq!(
            height,
            ((HEADER_H + PAD * 2.0 + rows * CELL_H) * PNG_RASTER_SCALE).ceil() as u32
        );
        // Closes with an IEND chunk.
        let tail = &bytes[bytes.len() - 8..bytes.len() - 4];
        assert_eq!(tail, b"IEND", "last chunk must be IEND");
    }

    #[test]
    fn pdf_has_header_page_and_content_stream() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let pdf = render_pdf(&grid);
        assert!(pdf.starts_with("%PDF-1.4"));
        assert!(pdf.contains("/Type /Page"));
        assert!(pdf.contains("/Subtype /Image"));
        assert!(pdf.contains("/ASCIIHexDecode /FlateDecode"));
        assert!(pdf.contains("stream\n"));
        assert!(pdf.contains("endstream"));
        assert!(pdf.contains("startxref"));
        assert!(pdf.ends_with("%%EOF\n"));
        assert!(!pdf.contains("/Identity-H"));
        assert!(!pdf.contains("/CIDToGIDMap"));
    }

    #[test]
    fn pdf_does_not_delegate_glyph_mapping_to_the_viewer() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let mut layout = PdfLayout::portrait();
        layout.font_name = None;
        layout.font_quality = crate::fonts::FontQuality::None;
        let pdf = render_pdf_with(&grid, layout);

        assert!(pdf.contains("/Subtype /Image"));
        assert!(!pdf.contains("张伟"));
        assert!(!pdf.contains("/Type0"));
        assert!(!pdf.contains("/Encoding /Identity-H"));
    }

    #[test]
    fn pdf_run_length_encoding_handles_literals_and_repeats() {
        let input = b"abcccdefggggggghij";
        let encoded = pdf_run_length_encode(input);
        assert_eq!(encoded.last(), Some(&128));

        let mut decoded = Vec::new();
        let mut cursor = 0;
        while cursor < encoded.len() {
            let header = encoded[cursor];
            cursor += 1;
            match header {
                0..=127 => {
                    let len = header as usize + 1;
                    decoded.extend_from_slice(&encoded[cursor..cursor + len]);
                    cursor += len;
                }
                129..=255 => {
                    let len = 257 - header as usize;
                    decoded.extend(std::iter::repeat_n(encoded[cursor], len));
                    cursor += 1;
                }
                128 => break,
            }
        }
        assert_eq!(decoded, input);
    }

    #[test]
    fn pdf_xref_offsets_point_at_each_object() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let pdf = render_pdf(&grid);

        // Locate the xref table via startxref.
        let startxref = pdf.find("startxref").expect("startxref keyword");
        let rest = &pdf[startxref + "startxref".len()..];
        let xref_offset: usize = rest
            .trim_start()
            .lines()
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let xref = &pdf[xref_offset..];
        assert!(xref.starts_with("xref\n"), "xref table at reported offset");

        let mut lines = xref.lines();
        assert_eq!(lines.next().unwrap(), "xref");
        let counts = lines.next().unwrap();
        let mut counts = counts.split_whitespace();
        let first_obj: usize = counts.next().unwrap().parse().unwrap();
        let count: usize = counts.next().unwrap().parse().unwrap();
        assert_eq!((first_obj, count), (0, 6));

        // Entry 0 is the free list head; entries 1..=5 must point at objects.
        assert!(lines.next().unwrap().contains(" f "), "free head entry");
        for obj_num in 1..count {
            let entry = lines.next().expect("an xref entry per object");
            let offset: usize = entry.split_whitespace().next().unwrap().parse().unwrap();
            let head = format!("{obj_num} 0 obj");
            assert_eq!(
                &pdf[offset..offset + head.len()],
                head,
                "offset for object {obj_num}"
            );
        }
    }

    #[test]
    fn pdf_layout_honours_orientation_and_scale() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();

        let portrait = render_pdf_with(&grid, PdfLayout::portrait());
        assert!(portrait.contains("/MediaBox [0 0 595 842]"));
        assert!(!portrait.contains("/MediaBox [0 0 842 595]"));

        let landscape = render_pdf_with(&grid, PdfLayout::landscape());
        assert!(landscape.contains("/MediaBox [0 0 842 595]"));

        // A larger page_scale must still produce a structurally valid PDF.
        let scaled = render_pdf_with(&grid, PdfLayout::portrait().with_scale(1.5));
        assert!(scaled.starts_with("%PDF-1.4"));
        assert!(scaled.ends_with("%%EOF\n"));
    }

    // M5-A4 gates: the PNG renderer draws student names with the system
    // CJK font when a font file is available, and degrades to textless
    // output otherwise (no panic on fontless machines).

    fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(bytes);
        let mut reader = decoder.read_info().expect("png info");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("png frame");
        (info.width, info.height, buf)
    }

    #[test]
    fn png_renders_names_when_font_available() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let bytes = render_png(&grid).expect("png renders");
        let (width, height, data) = decode_png(&bytes);
        let font = crate::fonts::load_cjk_font();
        if font.is_none() {
            // Fontless machines (CI before fonts are installed) skip the
            // pixel assertion but must still produce a valid PNG.
            assert!(width > 0 && height > 0);
            return;
        }
        // First seat rectangle: center should contain dark text pixels
        // (name color 30,34,40) rather than only the seat background.
        let (x, y) = cell_origin(&grid, grid.min_row, grid.min_col);
        let start_x = ((x + 4.0) * PNG_RASTER_SCALE) as u32;
        let start_y = ((y + 4.0) * PNG_RASTER_SCALE) as u32;
        let mut dark = 0;
        for dy in 0..(RECT_H * PNG_RASTER_SCALE) as u32 {
            for dx in 0..(RECT_W * PNG_RASTER_SCALE) as u32 {
                let px = (start_y + dy) * width + (start_x + dx);
                let idx = px as usize * 3;
                if data[idx] < 90 && data[idx + 1] < 90 && data[idx + 2] < 110 {
                    dark += 1;
                }
            }
        }
        assert!(dark > 0, "name pixels must be drawn inside the first seat");
    }
}
