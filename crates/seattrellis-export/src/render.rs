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
    i64::from(grid.max_col - grid.min_col) + 1
}

fn grid_rows(grid: &SeatingGrid) -> i64 {
    i64::from(grid.max_row - grid.min_row) + 1
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
    out.push_str("  h1 { font-size: 18px; margin: 0 0 4px; text-align: center; }\n");
    out.push_str(
        "  p.sub { font-size: 12px; color: #5a6b7f; margin: 0 0 12px; text-align: center; }\n",
    );
    out.push_str(
        "  p.front { font-size: 11px; color: #8a97a6; margin: 0 0 6px; text-align: center; }\n",
    );
    out.push_str(
        "  table.seating { border-collapse: separate; border-spacing: 6px; margin: 0 auto; }\n",
    );
    out.push_str("  td { width: 92px; height: 54px; text-align: center; vertical-align: middle; border-radius: 7px; font-size: 12px; }\n");
    out.push_str("  td.seat { background: #e8f0fe; border: 1px solid #4a7fd4; }\n");
    out.push_str("  td.seat .name { font-weight: 600; }\n");
    out.push_str("  td.seat .detail { display: block; font-size: 9px; color: #7b8ea8; }\n");
    out.push_str(
        "  td.seat .num { display: block; font-size: 9px; color: #7b8ea8; margin-top: 2px; }\n",
    );
    out.push_str("  td.empty { background: #f7f8f9; border: 1px dashed #cfd8e2; color: #9aa7b5; font-size: 10px; }\n");
    out.push_str("  td.void { border: none; }\n");
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
/// The seat grid is drawn as solid colored rectangles with a thin border using
/// the same palette as the SVG renderer. No text is drawn: drawing legible
/// labels would require either embedding a font or carrying a hand-rolled
/// bitmap font, and the mandate here is a small binary over glyph fidelity.
/// The output is a standard PNG (IHDR/IDAT/IEND) readable by any image tool.
pub fn render_png(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    let cols = grid_cols(grid);
    let rows = grid_rows(grid);
    let width = (PAD * 2.0 + cols as f64 * CELL_W).ceil() as u32;
    let height = (HEADER_H + PAD * 2.0 + rows as f64 * CELL_H).ceil() as u32;
    if width == 0 || height == 0 {
        return Err("cannot render an empty grid to PNG".to_string());
    }

    let mut data = vec![0u8; width as usize * height as usize * 3];
    let mut canvas = Canvas::new(&mut data, width, height);
    canvas.fill(0, 0, width, height, WHITE);
    // Front-of-room divider under the (textless) header band.
    let divider_y = (HEADER_H + 4.0) as u32;
    canvas.fill(0, divider_y, width, 1, DIVIDER);

    for row in grid.min_row..=grid.max_row {
        for col in grid.min_col..=grid.max_col {
            let (x, y) = cell_origin(grid, row, col);
            canvas.rect(
                x + 4.0,
                y + 4.0,
                RECT_W,
                RECT_H,
                cell_colors(grid, row, col),
                2,
            );
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

/// A small RGB raster with bounds-clipping fill helpers (kept private; the
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

/// Page geometry for the hand-written PDF renderer (app extension).
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

#[derive(Debug, Clone, Copy)]
pub struct PdfLayout {
    /// Page width in points.
    pub page_w: f64,
    /// Page height in points.
    pub page_h: f64,
    /// Extra multiplier on top of the automatic fit-to-page scale.
    pub scale_multiplier: f64,
    /// Printable margin in points (from `margin_mm`).
    pub margin_pt: f64,
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
        PdfLayout {
            page_w: w,
            page_h: h,
            scale_multiplier: 1.0,
            margin_pt: (margin_mm.clamp(5.0, 25.0) * 72.0 / 25.4).round(),
        }
    }

    /// Clamp a user `page_scale` into a sane range and apply it.
    pub fn with_scale(mut self, multiplier: f64) -> Self {
        self.scale_multiplier = multiplier.clamp(0.5, 2.0);
        self
    }
}

/// Render the plan as a single-page, hand-written PDF.
///
/// The document is generated directly (catalog -> pages -> page -> content
/// stream -> Helvetica font, plus a correct xref table) rather than via
/// `printpdf` or similar, keeping the dependency tree — and the binary — tiny.
/// Text uses the standard-14 Helvetica, so ASCII labels render everywhere; a
/// non-ASCII label (e.g. a CJK name) falls back to a plain placeholder because
/// encoding it would require embedding a CID font, which is not worth the size.
pub fn render_pdf(grid: &SeatingGrid) -> String {
    render_pdf_with(grid, PdfLayout::from_paper(PaperSize::A4, false, default_margin_pt()))
}

/// [`render_pdf`] with an explicit page geometry (orientation + scale).
pub fn render_pdf_with(grid: &SeatingGrid, layout: PdfLayout) -> String {
    let content = build_pdf_content(grid, layout);

    let mut bodies: Vec<String> = Vec::new();
    bodies.push("<< /Type /Catalog /Pages 2 0 R >>".to_string()); // obj 1
    bodies.push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()); // obj 2
    bodies.push(format!(
        // obj 3
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w} {page_h}] \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        page_w = layout.page_w,
        page_h = layout.page_h
    ));
    bodies.push(format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )); // obj 4
    bodies.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()); // obj 5

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

/// The page content stream: white background, header text, front-of-room
/// label, and one colored rectangle (+ label) per seat.
fn build_pdf_content(grid: &SeatingGrid, layout: PdfLayout) -> String {
    let cols = grid_cols(grid) as f64;
    let rows = grid_rows(grid) as f64;
    let grid_w = cols * CELL_W;
    let grid_h = rows * CELL_H;

    // Scale the SVG geometry down (or up) to fit the printable area, then apply
    // the user page_scale multiplier on top.
    let avail_w = layout.page_w - layout.margin_pt * 2.0;
    let avail_h = layout.page_h - PDF_HEADER_SPACE - layout.margin_pt;
    let base_scale = (avail_w / grid_w).min(avail_h / grid_h).clamp(0.1, 2.0);
    let scale = (base_scale * layout.scale_multiplier).clamp(0.1, 2.0);

    let grid_x = layout.margin_pt;
    let grid_top = layout.page_h - PDF_HEADER_SPACE + 10.0;

    let mut ops = String::with_capacity(4096 + grid.cells.len() * 140);

    // White page background.
    ops.push_str(&format!(
        "q\n{} rg\n0 0 {} {} re f\nQ\n",
        pdf_rgb(WHITE),
        layout.page_w,
        layout.page_h
    ));

    // Title + subtitle (fall back to ASCII placeholders for non-Latin text).
    let title = pdf_text(&grid.title).unwrap_or("Seating Plan");
    ops.push_str(&text_op_centered(
        title,
        16.0,
        layout.page_w / 2.0,
        layout.page_h - 48.0,
    ));
    let subtitle = pdf_text(&grid.subtitle).unwrap_or("");
    ops.push_str(&text_op_centered(
        subtitle,
        11.0,
        layout.page_w / 2.0,
        layout.page_h - 66.0,
    ));

    // Front-of-room divider line and label above the grid.
    let front_y = grid_top - 8.0;
    ops.push_str(&format!(
        "{} RG\n0.8 w\n{grid_x:.2} {front_y:.2} m {:.2} {front_y:.2} l S\n",
        pdf_rgb(DIVIDER),
        grid_x + grid_w * scale
    ));
    ops.push_str(&text_op_centered(
        "front of room",
        9.0,
        grid_x + grid_w * scale / 2.0,
        front_y - 7.0,
    ));

    for row in grid.min_row..=grid.max_row {
        for col in grid.min_col..=grid.max_col {
            let (x, y) = cell_origin(grid, row, col);
            let rect_x = grid_x + (x - PAD) * scale;
            let offset = (y - HEADER_H - PAD) / CELL_H; // row index within the grid
            let cell_top = grid_top - offset * CELL_H * scale;
            let inner_x = rect_x + 4.0 * scale;
            let inner_w = RECT_W * scale;
            let inner_y = cell_top - CELL_H * scale + 4.0 * scale;
            let inner_h = RECT_H * scale;

            let (fill, stroke) = cell_colors(grid, row, col);
            ops.push_str(&format!(
                "q\n{} rg\n{inner_x:.2} {inner_y:.2} {inner_w:.2} {inner_h:.2} re f\n\
                 {} RG\n0.6 w\n{inner_x:.2} {inner_y:.2} {inner_w:.2} {inner_h:.2} re S\nQ\n",
                pdf_rgb(fill),
                pdf_rgb(stroke)
            ));

            let center_x = inner_x + inner_w / 2.0;
            let center_y = inner_y + inner_h / 2.0;
            if let Some(cell) = grid.cell_at(row, col) {
                if let Some(name) = &cell.student {
                    if let Some(label) = pdf_text(name) {
                        let size = (name_font_size(name) as f64 * scale).clamp(6.0, 12.0);
                        ops.push_str(&text_op_centered(
                            label,
                            size,
                            center_x,
                            center_y - size * 0.35,
                        ));
                    }
                    if let Some(detail) = cell.detail.as_deref().and_then(pdf_text) {
                        // ASCII detail line (height/vision) under the name.
                        ops.push_str(&text_op_centered(
                            detail,
                            7.0,
                            center_x,
                            center_y + 9.0 * scale,
                        ));
                    }
                    let num = (cell.seat_index + 1).to_string();
                    ops.push_str(&text_op_centered(&num, 7.0, center_x, inner_y + 9.0));
                } else {
                    let label = if cell.enabled { "empty" } else { "unused" };
                    ops.push_str(&text_op_centered(label, 8.0, center_x, center_y - 2.8));
                }
            }
        }
    }
    ops
}

/// A color as a PDF nonstroking/stroking operand (`r g b`).
fn pdf_rgb(rgb: Rgb) -> String {
    format!(
        "{:.3} {:.3} {:.3}",
        rgb[0] as f64 / 255.0,
        rgb[1] as f64 / 255.0,
        rgb[2] as f64 / 255.0
    )
}

/// Escape text for a PDF literal string. Non-ASCII characters (e.g. CJK)
/// become `?` — the standard-14 Helvetica cannot encode them.
fn escape_pdf_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\x20'..='\x7e' => out.push(ch),
            _ => out.push('?'),
        }
    }
    out
}

/// `Some(text)` when `text` is pure printable ASCII (safe for Helvetica);
/// `None` when it contains non-ASCII characters.
fn pdf_text(text: &str) -> Option<&str> {
    if text.chars().all(|ch| matches!(ch, '\x20'..='\x7e')) {
        Some(text)
    } else {
        None
    }
}

/// A left-aligned text run: `BT /F1 <size> Tf 1 0 0 1 <x> <y> Tm (text) Tj ET`.
fn text_op(text: &str, size: f64, x: f64, y: f64) -> String {
    format!(
        "BT /F1 {size:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm ({}) Tj ET\n",
        escape_pdf_literal(text)
    )
}

/// Center `text` horizontally at `center_x` with its baseline at `baseline_y`.
fn text_op_centered(text: &str, size: f64, center_x: f64, baseline_y: f64) -> String {
    let width = approx_text_width(text, size);
    text_op(text, size, center_x - width / 2.0, baseline_y)
}

/// Rough Helvetica advance widths so centered text lands close to the mark.
fn approx_text_width(text: &str, size: f64) -> f64 {
    let mut units = 0.0;
    for ch in text.chars() {
        let factor = match ch {
            // Narrow glyphs first so the broad ranges below don't shadow them.
            'i' | 'l' | 'I' | 'j' | 't' | 'f' | '\'' | '.' | ',' | ':' | ';' => 0.25,
            ' ' => 0.28,
            'A'..='Z' => 0.72,
            '0'..='9' | 'a'..='z' => 0.5,
            _ => 0.42,
        };
        units += factor;
    }
    units * size
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
        assert_eq!(width, (PAD * 2.0 + cols * CELL_W).ceil() as u32);
        assert_eq!(height, (HEADER_H + PAD * 2.0 + rows * CELL_H).ceil() as u32);
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
        assert!(pdf.contains("/BaseFont /Helvetica"));
        assert!(pdf.contains("stream\n"));
        assert!(pdf.contains("endstream"));
        assert!(pdf.contains("startxref"));
        assert!(pdf.ends_with("%%EOF\n"));
        // Labels survive as PDF literal text.
        assert!(pdf.contains("(Alice)"));
        assert!(pdf.contains("(S3)"));
        assert!(pdf.contains("(3)"), "seat numbers render next to names");
    }

    #[test]
    fn pdf_cjk_names_fall_back_to_ascii() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let pdf = render_pdf(&grid);
        // The non-ASCII name must not appear raw in the literal string.
        assert!(!pdf.contains("张伟"));
        // The cell still renders its seat number instead.
        assert!(pdf.contains("(4)"));
        // An ASCII name does appear.
        assert!(pdf.contains("(Bob)"));
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
        assert_eq!((first_obj, count), (0, 6), "expected 0..6 table entries");

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
}
