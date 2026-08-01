//! SVG / HTML rendering of a solved seating plan.
//!
//! The seat grid is recovered from the problem: each `seat_positions` entry
//! becomes a grid cell at `(row = round(y), col = round(x))` — or, when the
//! problem carries a `layout`, its authoritative `row`/`col`/`enabled` are
//! used instead. The solve response's `assignment` fills each occupied seat
//! with a student label.
//!
//! Both renderers are fully self-contained: no `<script>`, no external
//! references, no embedded fonts — just plain shapes/text with a generic
//! `sans-serif` family so Chinese names render with whatever the system has.

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
    pub enabled: bool,
}

/// The full classroom grid recovered from a problem + solved assignment.
#[derive(Debug)]
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

        // Map seat -> student label from the assignment. Stray indices are
        // ignored rather than crashing the renderer.
        let mut student_by_seat: HashMap<usize, String> = HashMap::new();
        for [student_index, seat_index] in &response.assignment {
            if *student_index >= request.student_count || *seat_index >= seat_count {
                continue;
            }
            student_by_seat.insert(*seat_index, student_label(request, *student_index));
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
            if response.feasible { "feasible" } else { "infeasible" }
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
        self.cells.iter().find(|cell| cell.row == row && cell.col == col)
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
fn student_label(request: &CoreSolveRequest, index: usize) -> String {
    if !request.students.is_empty() {
        if let Some(student) = request.students.get(index) {
            let name = student
                .display_name
                .as_deref()
                .or_else(|| Some(student.key.as_str()))
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
    out.push_str("  p.sub { font-size: 12px; color: #5a6b7f; margin: 0 0 12px; text-align: center; }\n");
    out.push_str("  p.front { font-size: 11px; color: #8a97a6; margin: 0 0 6px; text-align: center; }\n");
    out.push_str("  table.seating { border-collapse: separate; border-spacing: 6px; margin: 0 auto; }\n");
    out.push_str("  td { width: 92px; height: 54px; text-align: center; vertical-align: middle; border-radius: 7px; font-size: 12px; }\n");
    out.push_str("  td.seat { background: #e8f0fe; border: 1px solid #4a7fd4; }\n");
    out.push_str("  td.seat .name { font-weight: 600; }\n");
    out.push_str("  td.seat .num { display: block; font-size: 9px; color: #7b8ea8; margin-top: 2px; }\n");
    out.push_str("  td.empty { background: #f7f8f9; border: 1px dashed #cfd8e2; color: #9aa7b5; font-size: 10px; }\n");
    out.push_str("  td.void { border: none; }\n");
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str(&format!("<h1>{}</h1>\n", escape_text(&grid.title)));
    out.push_str(&format!("<p class=\"sub\">{}</p>\n", escape_text(&grid.subtitle)));
    out.push_str("<p class=\"front\">front of room</p>\n");
    out.push_str("<table class=\"seating\">\n");

    for row in grid.min_row..=grid.max_row {
        out.push_str("  <tr>\n");
        for col in grid.min_col..=grid.max_col {
            match grid.cell_at(row, col) {
                Some(cell) => match &cell.student {
                    Some(name) => {
                        out.push_str(&format!(
                            "    <td class=\"seat\"><span class=\"name\">{}</span><span class=\"num\">{}</span></td>\n",
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
            students: vec![
                Student { key: "S1".into(), display_name: Some("Alice".into()), ..Student::default() },
                Student { key: "S2".into(), display_name: Some("Bob".into()), ..Student::default() },
                Student { key: "S3".into(), display_name: None, ..Student::default() },
                Student { key: "S4".into(), display_name: Some("张伟".into()), ..Student::default() },
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
        assert_eq!(grid.cell_at(1, 1).unwrap().student.as_deref(), Some("Alice"));
        assert_eq!(grid.cell_at(1, 2).unwrap().student.as_deref(), Some("Bob"));
        assert_eq!(grid.cell_at(1, 3).unwrap().student.as_deref(), Some("S3"));
        assert_eq!(grid.cell_at(2, 1).unwrap().student.as_deref(), Some("张伟"));
        // Seats 4 and 5 are unassigned.
        assert_eq!(grid.cell_at(2, 2).unwrap().student, None);
        assert_eq!(grid.cell_at(2, 3).unwrap().student, None);
    }

    #[test]
    fn svg_is_self_contained_and_self_closing() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let svg = render_svg(&grid);
        assert!(svg.starts_with("<svg "), "document opens with the <svg root");
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
        let response = CoreSolveResponse { assignment: vec![[0, 0]], ..sample_response() };
        let grid = SeatingGrid::build(&request, &response).unwrap();
        let svg = render_svg(&grid);
        assert!(svg.contains("A&amp;B &lt;C&gt;&quot;&apos;"));
        assert!(!svg.contains("A&B <C>"), "raw special characters must not appear");
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
        let response = CoreSolveResponse { assignment: vec![[0, 0]], ..sample_response() };
        let grid = SeatingGrid::build(&request, &response).unwrap();
        let html = render_html(&grid);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<table"));
        assert!(html.contains("</table>"));
        assert!(!html.contains("<script"), "no scripts");
        assert!(html.contains("A&amp;B &lt;C&gt;"));
        assert!(!html.contains("A&B <C>"), "raw special characters must not appear");
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
        let response = CoreSolveResponse { assignment: vec![[0, 0]], ..sample_response() };
        let grid = SeatingGrid::build(&request, &response).unwrap();
        assert_eq!((grid.min_col, grid.max_col), (1, 4));
        let html = render_html(&grid);
        assert!(html.contains("class=\"void\""), "missing grid positions are void");
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
        assert!(error.contains("seat_positions"), "unexpected error: {error}");
    }

    #[test]
    fn escape_text_handles_specials_and_control_characters() {
        assert_eq!(escape_text("&<>\"'"), "&amp;&lt;&gt;&quot;&apos;");
        assert_eq!(escape_text("a\u{1}b\n"), "ab\n", "control chars dropped, LF kept");
        assert_eq!(escape_text("张伟"), "张伟", "CJK passes through unchanged");
    }

    #[test]
    fn font_size_shrinks_for_long_names() {
        assert_eq!(name_font_size("Alice"), 13);
        assert_eq!(name_font_size("AliceandBob"), 10);
        assert_eq!(name_font_size("AveryLongStudentName"), 8);
    }
}
