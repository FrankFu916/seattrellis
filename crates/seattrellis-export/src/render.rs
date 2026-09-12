//! Shared seating model and scene-backed visual export writers.

use std::collections::{HashMap, HashSet};
use std::io::Write;

use seattrellis_core::{CoreSolveRequest, CoreSolveResponse};

// ---------------------------------------------------------------------------
// Grid model
// ---------------------------------------------------------------------------

/// One seat in the recovered grid, plus the student seated there (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct GridCell {
    /// Authoritative layout identifier, not a synthesized row/column label.
    pub seat_id: String,
    pub row: i32,
    pub col: i32,
    pub seat_index: usize,
    pub student: Option<String>,
    /// Optional per-student detail line (height / vision), rendered under
    /// the name when the privacy options ask for it (C.8).
    pub detail: Option<String>,
    pub enabled: bool,
    /// The seated student's key (identifier), when the request carries one.
    /// Shown by all renderers only when the caller opts in to identifiers.
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
        let mut occupied_coordinates = HashSet::with_capacity(seat_count);
        let mut min_row = i32::MAX;
        let mut max_row = i32::MIN;
        let mut min_col = i32::MAX;
        let mut max_col = i32::MIN;
        for (seat_index, position) in request.seat_positions.iter().enumerate() {
            let (row, col, enabled) = seat_row_col(request, seat_index, *position)?;
            if !occupied_coordinates.insert((row, col)) {
                return Err(format!(
                    "multiple seats map to row {row}, column {col}; give each seat a distinct layout row/column before exporting"
                ));
            }
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            min_col = min_col.min(col);
            max_col = max_col.max(col);
            cells.push(GridCell {
                seat_id: request
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.seats.get(seat_index))
                    .map(|seat| seat.seat_id.clone())
                    .unwrap_or_else(|| format!("R{row}C{col}")),
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

pub(crate) fn is_zh_locale(locale: &str) -> bool {
    !matches!(locale.trim().to_ascii_lowercase().as_str(), "en")
}

pub fn render_svg(grid: &SeatingGrid, locale: &str) -> String {
    render_svg_with(grid, &PdfLayout::portrait(), locale)
}

pub fn render_svg_with(grid: &SeatingGrid, page: &PdfLayout, locale: &str) -> String {
    render_scene_svg(&crate::scene::build_scene(grid, page, locale))
}

/// Standalone, script-free SVG, using exactly the same coordinates as the
/// raster and presentation writers. Local clipping paths cannot load resources.
pub fn render_scene_svg(scene: &crate::scene::ChartScene) -> String {
    use crate::scene::{Element, TextAlign};
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n",
        scene.width, scene.height, scene.width, scene.height
    );
    for (index, element) in scene.elements.iter().enumerate() {
        match element {
            Element::Box {
                bounds: b,
                radius,
                fill,
                stroke,
                stroke_width,
                dashed,
            } => {
                out.push_str(&format!(
                    "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"{:.3}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{:.3}\"{}/>\n",
                    b.x,b.y,b.w,b.h,radius,hex_color(*fill),hex_color(*stroke),stroke_width,
                    if *dashed { " stroke-dasharray=\"3 2\"" } else { "" }
                ));
            }
            Element::Text {
                bounds: b,
                text,
                font_size,
                color,
                bold,
                align,
            } => {
                let x = if *align == TextAlign::Center {
                    b.x + b.w / 2.0
                } else {
                    b.x
                };
                let baseline = text_baseline(*b, *font_size);
                let outline = crate::fonts::svg_text_outline(text, *font_size);
                out.push_str(&format!(
                    "<defs><clipPath id=\"text-{index}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\"/></clipPath></defs>\n\
                     <text x=\"{x:.3}\" y=\"{baseline:.3}\" font-family=\"'PingFang SC','Microsoft YaHei','Noto Sans CJK SC','Heiti SC',Arial,sans-serif\" font-size=\"{font_size:.3}\" font-weight=\"{}\" text-anchor=\"{}\" fill=\"{}\" clip-path=\"url(#text-{index})\"{}>{}</text>\n",
                    b.x,b.y,b.w,b.h,if *bold { 600 } else { 400 },
                    if *align == TextAlign::Center { "middle" } else { "start" },
                    hex_color(*color), if outline.is_some() { " opacity=\"0\"" } else { "" },escape_text(text)
                ));
                if let Some(path) = outline {
                    let origin = if *align == TextAlign::Center {
                        x - crate::scene::text_width(text, *font_size) / 2.0
                    } else {
                        x
                    };
                    out.push_str(&format!(
                        "<g aria-hidden=\"true\" clip-path=\"url(#text-{index})\"><path transform=\"translate({origin:.3} {baseline:.3}) scale(1 -1)\" fill=\"{}\"{} d=\"{path}\"/></g>\n",
                        hex_color(*color), if *bold { format!(" stroke=\"{}\" stroke-width=\"0.1\" stroke-linejoin=\"round\"",hex_color(*color)) } else { String::new() }
                    ));
                }
            }
        }
    }
    out.push_str("</svg>\n");
    out
}

fn hex_color(color: crate::scene::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

pub fn render_html(grid: &SeatingGrid, locale: &str) -> String {
    let scene = crate::scene::build_scene(grid, &PdfLayout::portrait(), locale);
    render_scene_html(&scene, &grid.title, locale)
}

/// The web page embeds the scene rather than running a second CSS table layout.
/// Printing has zero browser page margin: the scene already contains margins.
pub fn render_scene_html(scene: &crate::scene::ChartScene, title: &str, locale: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>\n\
         <style>@page{{size:{}pt {}pt;margin:0}}*{{box-sizing:border-box}}html,body{{margin:0;padding:0}}body{{background:#e9ece5}}.paper{{width:{}pt;margin:0 auto;background:white}}.paper>svg{{display:block;width:100%;height:auto}}@media print{{html,body{{background:white;width:{}pt;height:{}pt}}.paper{{margin:0;break-inside:avoid;page-break-inside:avoid}}}}</style></head>\n\
         <body><main class=\"paper\" aria-label=\"{}\">{}</main></body></html>\n",
        escape_text(locale),escape_text(title),scene.width,scene.height,scene.width,
        scene.width,scene.height,escape_text(title),render_scene_svg(scene)
    )
}

const RASTER_SCALE: f64 = 3.0;
const MAX_RASTER_BYTES: u64 = 256 * 1024 * 1024;

pub fn render_png(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    render_png_with(grid, &PdfLayout::portrait(), "zh")
}

pub fn render_png_with(
    grid: &SeatingGrid,
    page: &PdfLayout,
    locale: &str,
) -> Result<Vec<u8>, String> {
    let scene = crate::scene::build_scene(grid, page, locale);
    render_scene_png(&scene)
}

pub fn render_scene_png(scene: &crate::scene::ChartScene) -> Result<Vec<u8>, String> {
    validate_raster_page(scene)?;
    let (width, height, data) = rasterize_scene(scene, RASTER_SCALE);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header write failed: {e}"))?;
        writer
            .write_image_data(&data)
            .map_err(|e| format!("PNG data write failed: {e}"))?;
    }
    Ok(out)
}

fn validate_raster_page(scene: &crate::scene::ChartScene) -> Result<(), String> {
    if !scene.width.is_finite()
        || !scene.height.is_finite()
        || scene.width <= 0.0
        || scene.height <= 0.0
        || scene.width * scene.height * RASTER_SCALE * RASTER_SCALE * 3.0 > MAX_RASTER_BYTES as f64
    {
        return Err("page is too large to rasterize".into());
    }
    Ok(())
}

fn default_margin_mm() -> f64 {
    12.0
}

pub fn render_pdf(grid: &SeatingGrid) -> String {
    render_pdf_with(grid, PdfLayout::portrait())
}

pub fn render_pdf_with(grid: &SeatingGrid, page: PdfLayout) -> String {
    render_pdf_locale_with(grid, &page, "zh")
}

pub fn render_pdf_locale_with(grid: &SeatingGrid, page: &PdfLayout, locale: &str) -> String {
    render_scene_pdf(&crate::scene::build_scene(grid, page, locale))
        .expect("supported PDF page geometry")
}

/// Convert a top-origin line box to a font baseline using font ascent/descent.
/// The old renderer added ymin to the glyph origin, moving descenders upward
/// and other glyphs downward independently within the same line.
fn text_baseline(bounds: crate::scene::Rect, size: f64) -> f64 {
    if let Some(metrics) =
        crate::fonts::load_cjk_font().and_then(|f| f.horizontal_line_metrics(size as f32))
    {
        bounds.y
            + (bounds.h - f64::from(metrics.ascent - metrics.descent)) / 2.0
            + f64::from(metrics.ascent)
    } else {
        bounds.y + (bounds.h - size) / 2.0 + size * 0.82
    }
}

fn rasterize_scene(scene: &crate::scene::ChartScene, density: f64) -> (u32, u32, Vec<u8>) {
    use crate::scene::{Element, Rect, TextAlign};
    let width = (scene.width * density).ceil() as u32;
    let height = (scene.height * density).ceil() as u32;
    let mut data = vec![255u8; width as usize * height as usize * 3];
    let mut canvas = Canvas {
        data: &mut data,
        width,
        height,
    };
    for element in &scene.elements {
        match element {
            Element::Box {
                bounds,
                radius,
                fill,
                stroke,
                stroke_width,
                dashed,
            } => {
                let b = Rect {
                    x: bounds.x * density,
                    y: bounds.y * density,
                    w: bounds.w * density,
                    h: bounds.h * density,
                };
                canvas.rounded_rect(
                    b,
                    *radius * density,
                    *fill,
                    *stroke,
                    *stroke_width * density,
                    *dashed,
                );
            }
            Element::Text {
                bounds,
                text,
                font_size,
                color,
                bold,
                align,
            } => {
                let Some(font) = crate::fonts::load_cjk_font() else {
                    continue;
                };
                let size = (*font_size * density) as f32;
                let text_w: f64 = text
                    .chars()
                    .map(|c| f64::from(font.metrics(c, size).advance_width))
                    .sum();
                let mut cursor_x = bounds.x * density
                    + if *align == TextAlign::Center {
                        (bounds.w * density - text_w) / 2.0
                    } else {
                        0.0
                    };
                let baseline = text_baseline(*bounds, *font_size) * density;
                let clip = Rect {
                    x: bounds.x * density,
                    y: bounds.y * density,
                    w: bounds.w * density,
                    h: bounds.h * density,
                };
                for ch in text.chars() {
                    let (metrics, bitmap) = font.rasterize(ch, size);
                    let x = (cursor_x + f64::from(metrics.xmin)).round() as i64;
                    let y =
                        (baseline - metrics.height as f64 - f64::from(metrics.ymin)).round() as i64;
                    for row in 0..metrics.height {
                        for col in 0..metrics.width {
                            let px = x + col as i64;
                            let py = y + row as i64;
                            if (px as f64) >= clip.x
                                && (px as f64) < clip.x + clip.w
                                && (py as f64) >= clip.y
                                && (py as f64) < clip.y + clip.h
                            {
                                canvas.blend(px, py, *color, bitmap[row * metrics.width + col]);
                                if *bold {
                                    canvas.blend(
                                        px + 1,
                                        py,
                                        *color,
                                        bitmap[row * metrics.width + col] / 3,
                                    );
                                }
                            }
                        }
                    }
                    cursor_x += f64::from(metrics.advance_width);
                }
            }
        }
    }
    (width, height, data)
}

struct Canvas<'a> {
    data: &'a mut [u8],
    width: u32,
    height: u32,
}
impl Canvas<'_> {
    fn blend(&mut self, x: i64, y: i64, color: crate::scene::Color, alpha: u8) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) || alpha == 0
        {
            return;
        }
        let offset = (y as usize * self.width as usize + x as usize) * 3;
        for (channel, value) in color.iter().enumerate() {
            self.data[offset + channel] = ((u32::from(*value) * u32::from(alpha)
                + u32::from(self.data[offset + channel]) * (255 - u32::from(alpha)))
                / 255) as u8;
        }
    }
    fn rounded_rect(
        &mut self,
        b: crate::scene::Rect,
        radius: f64,
        fill: crate::scene::Color,
        stroke: crate::scene::Color,
        border: f64,
        dashed: bool,
    ) {
        let radius = radius.min(b.w / 2.0).min(b.h / 2.0).max(0.0);
        let inside = |x: f64, y: f64, inset: f64| {
            let left = b.x + inset;
            let top = b.y + inset;
            let right = b.x + b.w - inset;
            let bottom = b.y + b.h - inset;
            if x < left || x > right || y < top || y > bottom {
                return false;
            }
            let r = (radius - inset).max(0.0);
            let cx = x.clamp(left + r, right - r);
            let cy = y.clamp(top + r, bottom - r);
            (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r + 0.001
        };
        for y in
            (b.y.floor().max(0.0) as i64)..((b.y + b.h).ceil().min(f64::from(self.height)) as i64)
        {
            for x in (b.x.floor().max(0.0) as i64)
                ..((b.x + b.w).ceil().min(f64::from(self.width)) as i64)
            {
                if inside(x as f64 + 0.5, y as f64 + 0.5, 0.0) {
                    let is_border = !inside(x as f64 + 0.5, y as f64 + 0.5, border);
                    let color = if is_border && (!dashed || ((x + y) / 6) % 2 == 0) {
                        stroke
                    } else {
                        fill
                    };
                    self.blend(x, y, color, 255);
                }
            }
        }
    }
}

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
        Self::from_paper(PaperSize::A4, false, default_margin_mm())
    }

    /// A4 landscape with the default margin.
    pub fn landscape() -> Self {
        Self::from_paper(PaperSize::A4, true, default_margin_mm())
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

pub fn render_scene_pdf(scene: &crate::scene::ChartScene) -> Result<String, String> {
    validate_raster_page(scene)?;
    let (image_width, image_height, rgb) = rasterize_scene(scene, RASTER_SCALE);
    let layout = scene;
    let (filter, compressed) = pdf_compress_image(&rgb);
    let encoded = ascii_hex(&compressed);
    let content = format!(
        "q\n{:.2} 0 0 {:.2} 0 0 cm\n/Im0 Do\nQ\n",
        layout.width, layout.height
    );

    let mut bodies = Vec::new();
    bodies.push("<< /Type /Catalog /Pages 2 0 R >>".to_string()); // obj 1
    bodies.push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()); // obj 2
    bodies.push(format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w} {page_h}] \
         /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>",
        page_w = layout.width,
        page_h = layout.height
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
    Ok(out)
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
        let svg = render_svg(&grid, "en");
        assert!(
            svg.starts_with("<svg "),
            "document opens with the <svg root"
        );
        assert!(svg.contains("viewBox"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(!svg.contains("<script"), "no scripts");
        assert!(!svg.contains("href="), "no external references");
        assert!(!svg.contains("<image"), "no external images");
        assert!(!svg.contains("url(http"), "no external fills");
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
        let svg = render_svg(&grid, "en");
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
        let svg = render_svg(&grid, "en");
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
        let html = render_html(&grid, "en");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<svg "));
        assert!(html.contains("</svg>"));
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
        let html = render_html(&grid, "en");
        assert!(html.contains(">Empty<"), "empty seats are marked");
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
        let html = render_html(&grid, "en");
        assert!(!html.contains("R1C3"), "missing grid positions are void");
        let svg = render_svg(&grid, "en");
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
        assert!(render_svg(&grid, "en").contains("Unavailable"));
        assert!(render_html(&grid, "en").contains("Unavailable"));
    }

    // V2/V3: the zh locale must render the same Chinese wording as PNG/PDF
    // and print-html ("空座" / front-of-room / subtitle), not English labels.

    #[test]
    fn svg_localizes_labels_for_zh_and_keeps_en_unchanged() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let svg = render_svg(&grid, "zh");
        assert!(svg.contains(">空座<"), "zh empty-seat label: {svg}");
        assert!(svg.contains("教室前方"), "zh front-of-room label");
        assert!(svg.contains("4 名学生 · 6 个座位"), "zh subtitle: {svg}");
        assert!(!svg.contains(">Empty<"), "no English empty label: {svg}");
        assert!(
            !svg.contains("FRONT OF ROOM"),
            "no English front label: {svg}"
        );
        assert!(!svg.contains("students / "), "no English subtitle: {svg}");

        let en = render_svg(&grid, "en");
        assert!(en.contains(">Empty<"));
        assert!(en.contains("FRONT OF ROOM"));
        assert!(en.contains("4 students · 6 seats"));
        assert!(!en.contains("空座"));
    }

    #[test]
    fn html_localizes_labels_for_zh_and_keeps_en_unchanged() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let html = render_html(&grid, "zh");
        assert!(html.contains(">空座<"), "zh empty-seat cell: {html}");
        assert!(html.contains("教室前方"), "zh front-of-room label");
        assert!(html.contains("4 名学生 · 6 个座位"), "zh subtitle");
        assert!(
            !html.contains(">Empty<"),
            "no rendered English empty label (CSS class names excluded): {html}"
        );
        assert!(!html.contains(">front of room<"));

        let en = render_html(&grid, "en");
        assert!(en.contains(">Empty<"));
        assert!(en.contains("FRONT OF ROOM"));
        assert!(en.contains("4 students · 6 seats"));
    }

    #[test]
    fn zh_subtitle_reports_infeasibility() {
        let response = CoreSolveResponse {
            feasible: false,
            assignment: Vec::new(),
            hard_constraints_satisfied: false,
            total_cost: None,
            ..sample_response()
        };
        let grid = SeatingGrid::build(&sample_request(), &response).unwrap();
        let svg = render_svg(&grid, "zh");
        assert!(
            svg.contains("0 名学生 · 6 个座位 · 不可行"),
            "zh infeasible verdict: {svg}"
        );
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
    fn png_rejects_oversized_pages_instead_of_allocating() {
        let scene = crate::scene::ChartScene {
            width: 100_000.0,
            height: 100_000.0,
            elements: Vec::new(),
            warnings: Vec::new(),
        };
        assert!(render_scene_png(&scene).unwrap_err().contains("too large"));
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
    fn typography_fits_measured_width_and_multiple_lines() {
        let (short, _) = crate::scene::fit_text("Alice", 100.0, 32.0, 18.0, 2);
        let (long, lines) =
            crate::scene::fit_text("Alexandra Montgomery-Johnson", 100.0, 32.0, 18.0, 2);
        assert!(long < short);
        assert!(lines.len() <= 2);
        for line in lines {
            assert!(crate::scene::text_width(&line, long) <= 100.1);
        }
    }

    #[test]
    fn png_magic_header_and_dimensions_match_the_shared_paper_scene() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let bytes = render_png(&grid).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            (595.0 * RASTER_SCALE) as u32
        );
        assert_eq!(
            u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            (842.0 * RASTER_SCALE) as u32
        );
        assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
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
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("png info");
        let output_size = reader
            .output_buffer_size()
            .expect("png output buffer size fits usize");
        let mut buf = vec![0u8; output_size];
        let info = reader.next_frame(&mut buf).expect("png frame");
        (info.width, info.height, buf)
    }

    #[test]
    fn png_renders_names_inside_their_scene_text_boxes() {
        let grid = SeatingGrid::build(&sample_request(), &sample_response()).unwrap();
        let bytes = render_png(&grid).unwrap();
        let (width, height, data) = decode_png(&bytes);
        assert!(width > 0 && height > 0);
        if crate::fonts::load_cjk_font().is_none() {
            return;
        }
        let scene = crate::scene::build_scene(&grid, &PdfLayout::portrait(), "zh");
        let bounds = scene
            .elements
            .iter()
            .find_map(|element| match element {
                crate::scene::Element::Text { bounds, text, .. } if text == "Alice" => {
                    Some(*bounds)
                }
                _ => None,
            })
            .unwrap();
        let mut dark = 0;
        for y in (bounds.y * RASTER_SCALE) as u32..((bounds.y + bounds.h) * RASTER_SCALE) as u32 {
            for x in (bounds.x * RASTER_SCALE) as u32..((bounds.x + bounds.w) * RASTER_SCALE) as u32
            {
                let offset = (y * width + x) as usize * 3;
                if data[offset] < 100 && data[offset + 1] < 100 && data[offset + 2] < 100 {
                    dark += 1;
                }
            }
        }
        assert!(dark > 20, "name ink must be inside the shared text box");
    }
}
