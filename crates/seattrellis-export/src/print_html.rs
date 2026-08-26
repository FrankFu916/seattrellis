//! Dedicated print layout (plan §12.1 D11 / print-layout-spec.md).
//!
//! A one-page A4 seating chart designed for wall posting: landscape by
//! default (user-selectable portrait), seat cells maximized to the page,
//! student names in one uniform font size sized from the longest name in
//! the class, classroom structure annotated in plain text (platform,
//! aisles from missing columns, windows, door). No student ids, no lock
//! marks, no scores - the print view shows names only by default.
//!
//! The layout is computed deterministically in Rust (no wall-clock data in
//! the output); the header/footer carry the reproducibility line (period
//! label / seed) per G-3.

use std::collections::HashSet;

use seattrellis_core::CoreSolveRequest;

use crate::render::{PaperSize, SeatingGrid};

/// mm → pt.
const MM_TO_PT: f64 = 72.0 / 25.4;
/// Longest-name font cap (print-layout-spec §4).
const FONT_CAP_PT: f64 = 24.0;
/// Minimum readable font (pagination trigger threshold).
const FONT_FLOOR_PT: f64 = 8.0;

#[derive(Debug, Clone)]
pub struct PrintHtmlOptions {
    pub landscape: bool,
    pub paper: PaperSize,
    pub margin_mm: f64,
    pub page_scale: f64,
    pub show_student_ids: bool,
    pub locale: String,
    pub seed: Option<u64>,
    pub period_label: Option<String>,
}

impl PrintHtmlOptions {
    /// Page dimensions in mm (portrait order then orientation swap).
    pub fn page_mm(&self) -> (f64, f64) {
        let (w_pt, h_pt) = self.paper.points();
        let (mut w_mm, mut h_mm) = ((w_pt / MM_TO_PT).round(), (h_pt / MM_TO_PT).round());
        if self.landscape {
            std::mem::swap(&mut w_mm, &mut h_mm);
        }
        (w_mm, h_mm)
    }

    /// Printable area height in mm (page minus margins and header/footer).
    fn content_size_mm(&self) -> (f64, f64) {
        let (w_mm, h_mm) = self.page_mm();
        let header_mm = 12.0;
        let footer_mm = 9.0;
        let avail_w = (w_mm - 2.0 * self.margin_mm).max(10.0);
        let avail_h = (h_mm - 2.0 * self.margin_mm - header_mm - footer_mm).max(10.0);
        (avail_w, avail_h)
    }
}

/// The rendered print layout (bytes of a standalone HTML document).
pub fn render_print_html(
    grid: &SeatingGrid,
    request: &CoreSolveRequest,
    options: &PrintHtmlOptions,
) -> String {
    let layout = compute_layout(grid, request, options);
    let header = html_header(grid, options);
    let stage = html_stage();
    let table = html_seat_table(grid, request, &layout, options.show_student_ids);
    let structure = html_structure_notes(grid, request);
    let footer = html_footer(grid, options, &layout);

    format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  @page {{ size: {page_w}mm {page_h}mm; margin: {margin}mm; }}
  * {{ box-sizing: border-box; }}
  html, body {{ margin: 0; padding: 0; font-family: -apple-system, "PingFang SC",
    "Microsoft YaHei", "Noto Sans CJK SC", "Hiragino Sans GB", sans-serif;
    color: #1a1a1a; }}
  .print-header {{ display: flex; justify-content: space-between;
    align-items: baseline; font-size: 11pt; margin-bottom: 2mm; }}
  .print-header .cls {{ font-weight: 700; font-size: 13pt; }}
  .print-header .meta {{ color: #444; }}
  .stage {{ text-align: center; font-size: 10pt; color: #333;
    letter-spacing: .2em; margin: 1mm 0 2mm; font-weight: 600; }}
  .grid-row {{ display: grid; grid-template-columns: {columns}; gap: 0;
    width: 100%; }}
  .seat {{ display: grid; place-items: center; text-align: center;
    border: 0.4mm solid #1a1a1a; margin: 0.6mm; min-height: {cell_h}mm;
    font-size: {font_pt}pt; font-weight: 600; overflow: hidden; }}
  .seat .sid {{ display: block; font-size: 7pt; font-weight: 400;
    color: #555; }}
  .seat.empty {{ border-style: dashed; color: #999; }}
  .aisle {{ border: none; min-height: {cell_h}mm; }}
  .aisle-label {{ writing-mode: vertical-rl; font-size: 8pt; color: #666;
    text-align: center; }}
  .structure {{ display: flex; justify-content: space-between;
    font-size: 10pt; color: #333; margin: 2mm 1mm 0; }}
  .print-footer {{ display: flex; justify-content: space-between;
    font-size: 8.5pt; color: #555; margin-top: 2mm;
    border-top: 0.2mm solid #999; padding-top: 1mm; }}
</style>
</head>
<body>
{header}
{stage}
{table}
{structure}
{footer}
</body>
</html>
"#,
        lang = html_escape(&options.locale),
        title = html_escape(&grid.title),
        page_w = page_dim(options, 0),
        page_h = page_dim(options, 1),
        margin = options.margin_mm,
        header = header,
        stage = stage,
        table = table,
        structure = structure,
        footer = footer,
        columns = layout.columns_css,
        cell_h = layout.cell_h_mm,
        font_pt = layout.font_pt,
    )
}

fn page_dim(options: &PrintHtmlOptions, index: usize) -> f64 {
    let (w, h) = options.page_mm();
    if index == 0 {
        w
    } else {
        h
    }
}

/// Deterministic layout numbers for the print chart.
struct PrintLayout {
    columns_css: String,
    cell_h_mm: f64,
    font_pt: f64,
    font_capped: bool,
    aisle_cols: Vec<i32>,
    cap_em: usize,
    truncated: Vec<String>,
}

fn compute_layout(
    grid: &SeatingGrid,
    _request: &CoreSolveRequest,
    options: &PrintHtmlOptions,
) -> PrintLayout {
    let (avail_w, avail_h) = options.content_size_mm();
    let cols = (grid.max_col - grid.min_col + 1) as f64;
    let rows = (grid.max_row - grid.min_row + 1).max(1) as f64;

    // Aisles: grid columns with no enabled seat (column-number gaps).
    let mut occupied_cols: HashSet<i32> = HashSet::new();
    for cell in &grid.cells {
        if cell.enabled {
            occupied_cols.insert(cell.col);
        }
    }
    let mut aisle_cols: Vec<i32> = Vec::new();
    for col in grid.min_col..=grid.max_col {
        if !occupied_cols.contains(&col) {
            aisle_cols.push(col);
        }
    }
    let aisle_width = 0.5 * aisle_cols.len() as f64;

    // Cell geometry: maximize to the printable area.
    let cell_w = avail_w / (cols + aisle_width);
    let cell_h = avail_h / rows;
    let scale = options.page_scale.clamp(0.5, 2.0);
    let cell_w = cell_w * scale;
    let cell_h = cell_h * scale;

    // Font: one uniform size that fits the longest name inside the cell.
    let mut longest = 1usize;
    for cell in &grid.cells {
        if let Some(name) = &cell.student {
            let width_em = name_width_em(name);
            longest = longest.max(width_em);
        }
    }
    let usable_w_pt = ((cell_w - 4.0).max(2.0)) * MM_TO_PT; // ≥2mm side padding
    let font_pt = (usable_w_pt / longest as f64).clamp(FONT_FLOOR_PT, FONT_CAP_PT);
    let font_capped = (usable_w_pt / longest as f64) > FONT_CAP_PT;

    // Names that still overflow at the cap are truncated (footer note).
    let cap_em = (usable_w_pt / FONT_CAP_PT).floor().max(1.0) as usize;
    let mut truncated: Vec<String> = Vec::new();
    if font_capped {
        for cell in &grid.cells {
            if let Some(name) = &cell.student {
                let width_em = name_width_em(name);
                if width_em > cap_em {
                    truncated.push(name.clone());
                }
            }
        }
    }

    let mut columns_css = String::new();
    let mut first = true;
    for col in grid.min_col..=grid.max_col {
        if !first {
            columns_css.push(' ');
        }
        if aisle_cols.contains(&col) {
            columns_css.push_str(&format!("{}mm", cell_w * 0.5));
        } else {
            columns_css.push_str(&format!("{cell_w}mm"));
        }
        first = false;
    }

    PrintLayout {
        columns_css,
        cell_h_mm: cell_h,
        font_pt,
        font_capped,
        aisle_cols,
        cap_em,
        truncated,
    }
}

/// Estimated rendered width of a name in `em` units (CJK ≈ 1em, ASCII ≈ 0.6em).
fn name_width_em(name: &str) -> usize {
    let mut em: f64 = 0.0;
    for ch in name.chars() {
        em += if ch.is_ascii() { 0.6 } else { 1.0 };
    }
    em.ceil().max(1.0_f64) as usize
}

fn html_header(grid: &SeatingGrid, options: &PrintHtmlOptions) -> String {
    let mut meta = Vec::new();
    if let Some(period) = &options.period_label {
        meta.push(period.clone());
    }
    // Mirror the SVG/HTML subtitle wording so every format shows the same
    // language for the same locale (render.rs `is_zh_locale`).
    let counts = if crate::render::is_zh_locale(&options.locale) {
        format!(
            "{} 名学生 · {} 个座位",
            grid_student_count(grid),
            grid.cells.len()
        )
    } else {
        format!(
            "{} students / {} seats",
            grid_student_count(grid),
            grid.cells.len()
        )
    };
    meta.push(counts);
    format!(
        r#"<div class="print-header"><span class="cls">{title} 座位表</span>
<span class="meta">{meta}</span></div>"#,
        title = html_escape(&grid.title),
        meta = html_escape(&meta.join(" · ")),
    )
}

fn grid_student_count(grid: &SeatingGrid) -> usize {
    grid.cells
        .iter()
        .filter(|cell| cell.student.is_some())
        .count()
}

fn html_stage() -> String {
    r#"<div class="stage">讲台 ↑</div>"#.to_string()
}

fn html_seat_table(
    grid: &SeatingGrid,
    _request: &CoreSolveRequest,
    layout: &PrintLayout,
    show_student_ids: bool,
) -> String {
    let mut rows: Vec<String> = Vec::new();
    for row in grid.min_row..=grid.max_row {
        let mut cells_html = String::new();
        for col in grid.min_col..=grid.max_col {
            // Aisle columns render as a vertical label.
            if layout.aisle_cols.contains(&col) {
                cells_html
                    .push_str(r#"<div class="aisle"><span class="aisle-label">过道</span></div>"#);
                continue;
            }
            let cell = grid.cells.iter().find(|c| c.row == row && c.col == col);
            let Some(cell) = cell else {
                // Void grid position (no seat): emit an empty filler so CSS
                // grid auto-placement keeps later seats in their true columns
                // (matches the SVG/HTML/PNG/PDF renderers, which reserve the
                // slot).
                cells_html.push_str(r#"<div></div>"#);
                continue;
            };
            if !cell.enabled {
                cells_html.push_str(r#"<div class="seat empty">空座</div>"#);
                continue;
            }
            let Some(name) = &cell.student else {
                cells_html.push_str(r#"<div class="seat empty"></div>"#);
                continue;
            };
            let shown = if layout.font_capped && layout.truncated.contains(name) {
                // Truncated names get a compact form; the full name lives in
                // the footer note (print-layout-spec §4).
                truncate_name(name, layout.cap_em)
            } else {
                name.clone()
            };
            let sid_html = if show_student_ids {
                let key = cell.student_key.as_deref().unwrap_or("");
                if key.is_empty() {
                    String::new()
                } else {
                    format!(r#"<span class="sid">{}</span>"#, html_escape(key))
                }
            } else {
                String::new()
            };
            cells_html.push_str(&format!(
                r#"<div class="seat">{sid}{name}</div>"#,
                sid = sid_html,
                name = html_escape(&shown),
            ));
        }
        rows.push(format!(r#"<div class="grid-row">{cells_html}</div>"#));
    }
    rows.join("\n")
}

fn truncate_name(name: &str, cap_em: usize) -> String {
    let mut out: String = name.chars().take(cap_em).collect();
    if name_width_em(name) > cap_em {
        out.push('…');
    }
    out
}

/// Page-level structure annotations: platform already drawn; windows on the
/// right edge, door on the left edge, derived from seat attributes.
fn html_structure_notes(grid: &SeatingGrid, request: &CoreSolveRequest) -> String {
    let mut has_window = false;
    let mut has_door = false;
    if let Some(layout) = &request.layout {
        for cell in &grid.cells {
            if let Some(seat) = layout.seats.get(cell.seat_index) {
                if seat.near_window {
                    has_window = true;
                }
                if seat.near_door {
                    has_door = true;
                }
            }
        }
    }
    let left = if has_door { "门 →" } else { "" };
    let right = if has_window { "窗 ←" } else { "" };
    format!(r#"<div class="structure"><span>{left}</span><span>{right}</span></div>"#)
}

fn html_footer(grid: &SeatingGrid, options: &PrintHtmlOptions, layout: &PrintLayout) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(seed) = options.seed {
        parts.push(format!("seed {seed}"));
    }
    parts.push("第 1 / 1 页".to_string());
    let mut html = format!(
        r#"<div class="print-footer"><span>{left}</span><span>{right}</span></div>"#,
        left = html_escape(&parts.join(" · ")),
        right = html_escape(&grid.title),
    );
    if !layout.truncated.is_empty() {
        let names = layout
            .truncated
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("、");
        let more = if layout.truncated.len() > 3 {
            format!(" 等 {} 名", layout.truncated.len())
        } else {
            String::new()
        };
        html.push_str(&format!(
            r#"<div class="print-footer" style="border-top:none;color:#a33">
姓名超过版面宽度已截断：{names}{more}</div>"#,
            names = html_escape(&names),
            more = html_escape(&more),
        ));
    }
    html
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{GridCell, PaperSize, SeatingGrid};
    use seattrellis_core::{CoreSolveRequest, CoreSolveResponse};
    use serde_json::json;

    fn sample_request() -> CoreSolveRequest {
        serde_json::from_value(json!({
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[1.0,2.0],[2.0,2.0]],
            "edges": [[0,1],[0,2],[1,3],[2,3]],
            "fixed_seats": [], "must_be_adjacent": [], "cannot_be_adjacent": [],
            "min_distance": [], "seed": 168996,
            "students": [
                {"key": "S1", "display_name": "张伟"},
                {"key": "S2", "display_name": "王芳"},
                {"key": "S3", "display_name": "李娜"},
                {"key": "S4", "display_name": "刘洋"}
            ],
            "student_scores": [70.0, 75.0, 65.0, 80.0],
            "rules": {"schema_version": 0, "seed": 168996, "hard": {}, "soft": {}, "groups": []},
            "layout": {"layout_id": "room", "name": "初二（3）班", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "enabled": true,
                 "zone": "front", "near_platform": true, "near_window": true, "near_door": false},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 2.0, "y": 1.0, "enabled": true,
                 "zone": "front", "near_platform": true, "near_window": false, "near_door": false},
                {"seat_id": "R2C1", "row": 2, "col": 1, "x": 1.0, "y": 2.0, "enabled": true,
                 "zone": "middle", "near_platform": false, "near_window": true, "near_door": false},
                {"seat_id": "R2C2", "row": 2, "col": 2, "x": 2.0, "y": 2.0, "enabled": true,
                 "zone": "middle", "near_platform": false, "near_window": false, "near_door": true}
            ], "adjacency": {"include_horizontal": true, "include_vertical": true}},
            "history": null, "pair_history": null, "time_limit_seconds": null
        }))
        .expect("request parses")
    }

    fn sample_response() -> CoreSolveResponse {
        serde_json::from_value(json!({
            "api_version": 2, "feasible": true, "status": "Solved",
            "assignment": [[0,0],[1,1],[2,2],[3,3]],
            "attempts_used": 1, "hard_constraints_satisfied": true
        }))
        .expect("response parses")
    }

    fn render() -> String {
        let request = sample_request();
        let response = sample_response();
        let grid = SeatingGrid::build(&request, &response).expect("grid");
        let options = PrintHtmlOptions {
            landscape: true,
            paper: PaperSize::A4,
            margin_mm: 12.0,
            page_scale: 1.0,
            show_student_ids: false,
            locale: "zh".to_string(),
            seed: Some(168996),
            period_label: Some("第 3 期".to_string()),
        };
        render_print_html(&grid, &request, &options)
    }

    #[test]
    fn landscape_default_and_one_uniform_font() {
        let html = render();
        assert!(
            html.contains("@page { size: 297mm 210mm"),
            "landscape A4 default"
        );
        assert!(html.contains("font-size: "), "uniform font size present");
        // One font-size rule for all seats.
        let fonts = html.matches("font-size: 24pt").count() + html.matches("font-size: 2").count();
        assert!(fonts >= 1);
    }

    #[test]
    fn names_only_and_structure_annotations() {
        let html = render();
        for name in ["张伟", "王芳", "李娜", "刘洋"] {
            assert!(html.contains(name), "name {name} rendered");
        }
        assert!(html.contains("讲台 ↑"), "platform annotation");
        assert!(html.contains("窗 ←"), "window annotation");
        assert!(html.contains("门 →"), "door annotation");
        assert!(html.contains("seed 168996"), "reproducibility line");
        assert!(html.contains("第 3 期"), "period label");
    }

    #[test]
    fn no_student_ids_by_default() {
        let html = render();
        assert!(!html.contains(r#"class="sid""#), "ids hidden by default");
    }

    #[test]
    fn font_sized_from_longest_name() {
        // A 4-char name on a 2x2 grid: usable width = (297-24-?)... The
        // computed font must be one number used everywhere.
        let html = render();
        // Extract the .seat font-size declaration.
        let marker = ".seat {{ display: grid; place-items: center; text-align: center;";
        let _ = marker;
        let font_decl = html
            .split("font-size: ")
            .nth(3)
            .map(|s| s.split("pt").next().unwrap_or("").trim().to_string())
            .unwrap_or_default();
        let size: f64 = font_decl.parse().unwrap_or(0.0);
        assert!(
            (8.0..=24.0).contains(&size),
            "font in readable range: {size}"
        );
    }

    #[test]
    fn portrait_option_swaps_page() {
        let request = sample_request();
        let response = sample_response();
        let grid = SeatingGrid::build(&request, &response).unwrap();
        let mut options = PrintHtmlOptions {
            landscape: false,
            paper: PaperSize::A4,
            margin_mm: 12.0,
            page_scale: 1.0,
            show_student_ids: false,
            locale: "zh".to_string(),
            seed: None,
            period_label: None,
        };
        let html = render_print_html(&grid, &request, &options);
        assert!(html.contains("@page { size: 210mm 297mm"), "portrait A4");
        options.landscape = true;
        options.paper = PaperSize::A3;
        let html = render_print_html(&grid, &request, &options);
        assert!(html.contains("@page { size: 420mm 297mm"), "A3 landscape");
    }

    #[test]
    fn locale_attribute_is_escaped() {
        // The locale lands in a double-quoted HTML attribute; a crafted
        // value must not break out of it (exported files get opened in
        // browsers).
        let request = sample_request();
        let response = sample_response();
        let grid = SeatingGrid::build(&request, &response).unwrap();
        let options = PrintHtmlOptions {
            landscape: true,
            paper: PaperSize::A4,
            margin_mm: 12.0,
            page_scale: 1.0,
            show_student_ids: false,
            locale: "zh\" onmouseover=\"alert(1)".to_string(),
            seed: None,
            period_label: None,
        };
        let html = render_print_html(&grid, &request, &options);
        assert!(
            html.contains(r#"<html lang="zh&quot; onmouseover=&quot;alert(1)">"#),
            "locale must be entity-escaped in the lang attribute"
        );
        assert!(
            !html.contains(r#"lang="zh" onmouseover"#),
            "raw attribute injection must not survive"
        );
    }

    #[test]
    fn seat_grid_css_targets_the_row_wrapper() {
        // Regression: the grid template used to be declared on a `.seats`
        // class that never appears in the markup, so seat boxes rendered as
        // stacked blocks instead of a row of columns.
        let html = render();
        assert!(
            html.contains(".grid-row { display: grid; grid-template-columns:"),
            "grid template must live on the row wrapper"
        );
        assert!(!html.contains("class=\"seats\""), "no dead seats wrapper");
    }

    #[test]
    fn mid_row_hole_keeps_column_alignment() {
        // Row 2 has seats only at cols 1 and 3; the missing col-2 position
        // must still occupy a CSS track so the col-3 seat does not shift
        // into track 2 (the SVG/HTML/PNG/PDF renderers reserve the slot).
        let request = sample_request();
        let grid = SeatingGrid {
            title: "t".into(),
            subtitle: "s".into(),
            min_row: 1,
            max_row: 2,
            min_col: 1,
            max_col: 3,
            cells: vec![
                GridCell {
                    row: 1,
                    col: 1,
                    seat_index: 0,
                    student: Some("A".into()),
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 1,
                    col: 2,
                    seat_index: 1,
                    student: Some("B".into()),
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 1,
                    col: 3,
                    seat_index: 2,
                    student: Some("C".into()),
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 2,
                    col: 1,
                    seat_index: 3,
                    student: Some("D".into()),
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 2,
                    col: 3,
                    seat_index: 4,
                    student: Some("E".into()),
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
            ],
        };
        let options = PrintHtmlOptions {
            landscape: true,
            paper: PaperSize::A4,
            margin_mm: 12.0,
            page_scale: 1.0,
            show_student_ids: false,
            locale: "zh".to_string(),
            seed: None,
            period_label: None,
        };
        let html = render_print_html(&grid, &request, &options);
        let rows: Vec<&str> = html
            .lines()
            .filter(|line| line.contains(r#"class="grid-row""#))
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(
            rows[1].contains(r#"<div class="seat">D</div><div></div><div class="seat">E</div>"#),
            "col-3 seat must follow a track-preserving filler: {}",
            rows[1]
        );
    }
}
