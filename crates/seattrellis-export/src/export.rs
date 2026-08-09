//! Export domain module: turns a solved plan into SVG / HTML / PNG / PDF bytes.
//!
//! This is the app-side counterpart of the workbench's "export" flow. The
//! frontend sends an `ExportDraftRequest` (see `clients/web/src/api/types.ts`);
//! this module parses that shape, recovers the renderable seat grid from the
//! solved plan ([`SeatingGrid::build`] in `seattrellis_export::render`, a mirror of the CLI's
//! renderer), and dispatches to the matching render function.
//!
//! # Entry point
//!
//! [`export_plan`] accepts a single JSON object that carries the
//! `ExportDraftRequest` fields **plus** the solved plan, so the loopback server
//! can forward everything it already has in one shot:
//!
//! ```json
//! {
//!   "draft_id": "draft-1",
//!   "format": "svg",
//!   "template": "teacher",
//!   "privacy": {
//!     "hide_scores": false,
//!     "hide_notes": false,
//!     "hide_special_needs": false,
//!     "anonymize": false,
//!     "show_height": false,
//!     "show_vision": false
//!   },
//!   "orientation": "portrait",
//!   "page_scale": 1.0,
//!   "locale": "zh",
//!   "show_student_ids": true,
//!   "request":  { ...CoreSolveRequest },
//!   "response": { ...CoreSolveResponse }
//! }
//! ```
//!
//! # Template / privacy mapping (v1)
//!
//! - `template: "teacher"` / `"report"` render the real student labels
//!   (display name, else key, else "Student N" — same as the CLI).
//! - `template: "public"` — or any template with `privacy.anonymize` — renders a
//!   placeholder in every occupied seat instead of a name ("学生"/"student",
//!   following `locale`).
//! - `privacy` fields beyond `anonymize` (`hide_scores`, `hide_notes`, ...) are
//!   accepted for contract compatibility; the native renderers do not carry
//!   scores/notes yet, so there is nothing extra to hide in v1.
//! - `orientation` is decided by grid geometry for SVG/HTML/PNG (the raster and
//!   vector documents size to the seat grid); for PDF it swaps the A4 page
//!   between portrait and landscape.
//! - `page_scale` applies to the PDF fit-to-page scale (clamped to 0.5–2.0);
//!   it is inert for SVG/HTML/PNG.
//! - `show_student_ids` is accepted for contract compatibility; the teacher
//!   template already renders the student identifier (name-or-key) in v1.
//!
//! The module never panics: every failure is returned as a `String` error that
//! identifies the offending field, so the server can surface a coarse 400.

use seattrellis_core::{CoreSolveRequest, CoreSolveResponse};
use serde::Deserialize;

use crate::render::{
    render_html, render_pdf_with, render_png, render_svg, GridCell, PdfLayout, SeatingGrid,
};

// ---------------------------------------------------------------------------
// Format / template / orientation enums
// ---------------------------------------------------------------------------

/// The four export formats the app can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Html,
    Png,
    Pdf,
}

impl ExportFormat {
    /// Case-insensitive parse of the frontend `format` string.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "svg" => Ok(ExportFormat::Svg),
            "html" | "htm" => Ok(ExportFormat::Html),
            "png" => Ok(ExportFormat::Png),
            "pdf" => Ok(ExportFormat::Pdf),
            other => Err(format!(
                "unknown export format '{other}' (expected svg, html, png or pdf)"
            )),
        }
    }

    /// HTTP `Content-Type` for this format (useful when the server responds).
    pub fn mime(self) -> &'static str {
        match self {
            ExportFormat::Svg => "image/svg+xml",
            ExportFormat::Html => "text/html; charset=utf-8",
            ExportFormat::Png => "image/png",
            ExportFormat::Pdf => "application/pdf",
        }
    }

    /// Default file extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Html => "html",
            ExportFormat::Png => "png",
            ExportFormat::Pdf => "pdf",
        }
    }
}

/// The frontend `ExportDraftRequest.template` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTemplate {
    Public,
    Teacher,
    Report,
}

impl ExportTemplate {
    /// Case-insensitive parse of the frontend `template` string.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "public" => Ok(ExportTemplate::Public),
            "teacher" => Ok(ExportTemplate::Teacher),
            "report" => Ok(ExportTemplate::Report),
            other => Err(format!(
                "unknown export template '{other}' (expected public, teacher or report)"
            )),
        }
    }
}

/// Page orientation for document-style exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportOrientation {
    Portrait,
    Landscape,
}

impl ExportOrientation {
    /// Case-insensitive parse of the frontend `orientation` string.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "portrait" => Ok(ExportOrientation::Portrait),
            "landscape" => Ok(ExportOrientation::Landscape),
            other => Err(format!(
                "unknown export orientation '{other}' (expected portrait or landscape)"
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Request types (frontend `ExportDraftRequest` shape)
// ---------------------------------------------------------------------------

/// `ExportDraftRequest.privacy` (all booleans optional).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportPrivacyOptions {
    #[serde(default)]
    pub hide_scores: bool,
    #[serde(default)]
    pub hide_notes: bool,
    #[serde(default)]
    pub hide_special_needs: bool,
    #[serde(default)]
    pub anonymize: bool,
    #[serde(default)]
    pub show_height: bool,
    #[serde(default)]
    pub show_vision: bool,
}

fn default_template() -> String {
    "public".to_string()
}

fn default_orientation() -> String {
    "portrait".to_string()
}

fn default_page_scale() -> f64 {
    1.0
}

fn default_locale() -> String {
    "zh".to_string()
}

/// The combined export request: `ExportDraftRequest` fields plus the solved
/// plan the server already holds (`request` + `response`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportRequest {
    #[serde(default)]
    pub draft_id: String,
    /// `svg` | `html` | `png` | `pdf` (required).
    pub format: String,
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default)]
    pub privacy: ExportPrivacyOptions,
    #[serde(default = "default_orientation")]
    pub orientation: String,
    #[serde(default = "default_page_scale")]
    pub page_scale: f64,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub show_student_ids: bool,
    /// The `CoreSolveRequest` that produced the plan.
    pub request: CoreSolveRequest,
    /// The `CoreSolveResponse` (assignment) to render.
    pub response: CoreSolveResponse,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Produce the export bytes for a combined export request (see the module doc
/// for the JSON shape). Never panics; errors are descriptive `String`s.
pub fn export_plan(request_json: &str) -> Result<Vec<u8>, String> {
    let request = parse_export_request(request_json)?;
    render_export(&request)
}

/// Parse just the `format` field so the server can set a `Content-Type`
/// without running the full export. Fails on invalid JSON or a missing/invalid
/// `format`.
pub fn format_of(request_json: &str) -> Result<ExportFormat, String> {
    let value: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|error| format!("export request is not valid JSON: {error}"))?;
    let format = value
        .get("format")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "export request is missing a string 'format' field".to_string())?;
    ExportFormat::parse(format)
}

/// Dispatch an already-parsed export request to the matching renderer.
pub fn render_export(request: &ExportRequest) -> Result<Vec<u8>, String> {
    let format = ExportFormat::parse(&request.format)?;
    let template = ExportTemplate::parse(&request.template)?;
    let orientation = ExportOrientation::parse(&request.orientation)?;
    let page_scale = validate_page_scale(request.page_scale)?;

    let grid = SeatingGrid::build(&request.request, &request.response)?;

    // Privacy options (C.8): the public template — or explicit anonymization
    // — hides every student name; the detail line (height/vision) only shows
    // when the matching show_* option is set. hide_scores / hide_notes /
    // hide_special_needs have nothing to hide in this renderer: it never
    // draws scores, notes or needs, so those exports cannot leak them.
    let hide_names = template == ExportTemplate::Public || request.privacy.anonymize;
    let grid = if hide_names {
        anonymize_grid(&grid, &request.locale)
    } else {
        filter_detail_grid(
            &grid,
            request.privacy.show_height,
            request.privacy.show_vision,
        )
    };

    match format {
        ExportFormat::Svg => Ok(render_svg(&grid).into_bytes()),
        ExportFormat::Html => Ok(render_html(&grid).into_bytes()),
        ExportFormat::Png => render_png(&grid),
        ExportFormat::Pdf => {
            let layout = match orientation {
                ExportOrientation::Portrait => PdfLayout::portrait(),
                ExportOrientation::Landscape => PdfLayout::landscape(),
            };
            Ok(render_pdf_with(&grid, layout.with_scale(page_scale)).into_bytes())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_export_request(request_json: &str) -> Result<ExportRequest, String> {
    if request_json.trim().is_empty() {
        return Err("export request body is empty".to_string());
    }
    let value: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|error| format!("export request is not valid JSON: {error}"))?;
    serde_json::from_value(value).map_err(|error| {
        format!("export request is missing required fields (format, request, response): {error}")
    })
}

fn validate_page_scale(raw: f64) -> Result<f64, String> {
    if !raw.is_finite() || raw <= 0.0 {
        return Err(format!(
            "invalid page_scale '{raw}' (expected a positive number)"
        ));
    }
    Ok(raw)
}

/// Keep only the detail line parts the privacy options allow: the raw grid
/// carries height + vision; `show_height`/`show_vision` decide what survives.
fn filter_detail_grid(grid: &SeatingGrid, show_height: bool, show_vision: bool) -> SeatingGrid {
    if show_height && show_vision {
        return grid.clone();
    }
    let mut filtered = SeatingGrid {
        title: grid.title.clone(),
        subtitle: grid.subtitle.clone(),
        min_row: grid.min_row,
        max_row: grid.max_row,
        min_col: grid.min_col,
        max_col: grid.max_col,
        cells: Vec::with_capacity(grid.cells.len()),
    };
    for cell in &grid.cells {
        let detail = match &cell.detail {
            None => None,
            Some(detail) if show_height && !show_vision => detail
                .split("  ")
                .find(|part| part.ends_with(" cm"))
                .map(str::to_string),
            Some(detail) if show_vision && !show_height => detail
                .split("  ")
                .find(|part| part.starts_with("vision"))
                .map(str::to_string),
            Some(_) => None,
        };
        filtered.cells.push(GridCell {
            row: cell.row,
            col: cell.col,
            seat_index: cell.seat_index,
            student: cell.student.clone(),
            detail,
            enabled: cell.enabled,
        });
    }
    filtered
}

/// Copy of the grid with every occupied seat's label replaced by a locale-aware
/// placeholder. The renderers draw whatever labels the grid carries, so this is
/// the single place public/anonymized exports differ from teacher exports.
fn anonymize_grid(grid: &SeatingGrid, locale: &str) -> SeatingGrid {
    let placeholder = match locale.trim().to_ascii_lowercase().as_str() {
        "en" => "student",
        _ => "学生",
    };
    SeatingGrid {
        title: grid.title.clone(),
        subtitle: grid.subtitle.clone(),
        min_row: grid.min_row,
        max_row: grid.max_row,
        min_col: grid.min_col,
        max_col: grid.max_col,
        cells: grid
            .cells
            .iter()
            .map(|cell| GridCell {
                row: cell.row,
                col: cell.col,
                seat_index: cell.seat_index,
                student: cell.student.as_ref().map(|_| placeholder.to_string()),
                // Anonymized exports never carry detail lines either.
                detail: None,
                enabled: cell.enabled,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but valid `CoreSolveRequest`, matching the CLI test fixture.
    fn sample_request_json() -> serde_json::Value {
        serde_json::json!({
            "api_version": 2,
            "student_count": 4,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0],[1.0,2.0],[2.0,2.0],[3.0,2.0]],
            "students": [
                { "key": "S1", "display_name": "Alice" },
                { "key": "S2", "display_name": "Bob" },
                { "key": "S3" },
                { "key": "S4", "display_name": "张伟" }
            ]
        })
    }

    fn sample_response_json() -> serde_json::Value {
        serde_json::json!({
            "api_version": 2,
            "feasible": true,
            "assignment": [[0,0],[1,1],[2,2],[3,3]],
            "attempts_used": 4,
            "hard_constraints_satisfied": true,
            "total_cost": 12.5
        })
    }

    /// Build a combined export request body, overriding a couple of fields.
    fn export_body(format: &str, template: &str) -> serde_json::Value {
        serde_json::json!({
            "draft_id": "draft-1",
            "format": format,
            "template": template,
            "privacy": {
                "hide_scores": false,
                "hide_notes": false,
                "hide_special_needs": false,
                "anonymize": false,
                "show_height": false,
                "show_vision": false
            },
            "orientation": "portrait",
            "page_scale": 1.0,
            "locale": "zh",
            "show_student_ids": true,
            "request": sample_request_json(),
            "response": sample_response_json()
        })
    }

    fn body_string(value: &serde_json::Value) -> String {
        serde_json::to_string(value).expect("test body serializes")
    }

    fn export_ok(value: &serde_json::Value) -> Vec<u8> {
        export_plan(&body_string(value)).expect("export should succeed")
    }

    #[test]
    fn svg_export_produces_valid_svg() {
        let bytes = export_ok(&export_body("svg", "teacher"));
        let svg = String::from_utf8(bytes).unwrap();
        assert!(svg.starts_with("<svg "), "SVG must open with the <svg root");
        assert!(svg.contains("viewBox"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn html_export_produces_valid_html() {
        let bytes = export_ok(&export_body("html", "teacher"));
        let html = String::from_utf8(bytes).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"), "HTML document type");
        assert!(html.contains("<table"));
        assert!(html.contains("</table>"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn png_export_has_png_magic_and_iend() {
        let bytes = export_ok(&export_body("png", "teacher"));
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
        let tail = &bytes[bytes.len() - 8..bytes.len() - 4];
        assert_eq!(tail, b"IEND", "last chunk must be IEND");
    }

    #[test]
    fn pdf_export_has_pdf_magic_and_eof() {
        let bytes = export_ok(&export_body("pdf", "teacher"));
        let pdf = String::from_utf8(bytes).unwrap();
        assert!(pdf.starts_with("%PDF-1.4"), "PDF header");
        assert!(
            pdf.contains("/MediaBox [0 0 595 842]"),
            "A4 portrait by default"
        );
        assert!(pdf.ends_with("%%EOF\n"), "PDF trailer");
        assert!(pdf.contains("/BaseFont /Helvetica"));
    }

    #[test]
    fn teacher_template_shows_names_public_hides_them() {
        let teacher = String::from_utf8(export_ok(&export_body("svg", "teacher"))).unwrap();
        assert!(
            teacher.contains("Alice"),
            "teacher export shows the student name"
        );
        assert!(teacher.contains("Bob"));
        assert!(teacher.contains("张伟"), "CJK names survive in SVG");

        let public = String::from_utf8(export_ok(&export_body("svg", "public"))).unwrap();
        assert!(
            !public.contains("Alice"),
            "public export must not leak names"
        );
        assert!(!public.contains("Bob"));
        assert!(!public.contains("张伟"));
        assert!(
            public.contains("学生"),
            "public export shows the zh placeholder"
        );
    }

    #[test]
    fn privacy_anonymize_hides_names_even_for_teacher() {
        let mut body = export_body("svg", "teacher");
        body["privacy"]["anonymize"] = serde_json::Value::Bool(true);
        let svg = String::from_utf8(export_ok(&body)).unwrap();
        assert!(
            !svg.contains("Alice"),
            "anonymize must hide names regardless of template"
        );
        assert!(svg.contains("学生"));
    }

    #[test]
    fn public_template_uses_english_placeholder_for_en_locale() {
        let mut body = export_body("svg", "public");
        body["locale"] = serde_json::Value::String("en".into());
        let svg = String::from_utf8(export_ok(&body)).unwrap();
        assert!(!svg.contains("Alice"));
        assert!(
            svg.contains(">student<"),
            "en placeholder is lowercase 'student'"
        );
    }

    #[test]
    fn invalid_format_is_rejected() {
        let body = export_body("bmp", "teacher");
        let error = export_plan(&body_string(&body)).unwrap_err();
        assert!(
            error.contains("unknown export format 'bmp'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn invalid_template_is_rejected() {
        let mut body = export_body("svg", "teacher");
        body["template"] = serde_json::Value::String("admin".into());
        let error = export_plan(&body_string(&body)).unwrap_err();
        assert!(
            error.contains("unknown export template 'admin'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn invalid_orientation_is_rejected() {
        let mut body = export_body("pdf", "teacher");
        body["orientation"] = serde_json::Value::String("square".into());
        let error = export_plan(&body_string(&body)).unwrap_err();
        assert!(
            error.contains("unknown export orientation 'square'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn invalid_page_scale_is_rejected() {
        let mut body = export_body("svg", "teacher");
        body["page_scale"] = serde_json::Value::from(0.0);
        let error = export_plan(&body_string(&body)).unwrap_err();
        assert!(
            error.contains("invalid page_scale"),
            "unexpected error: {error}"
        );

        body["page_scale"] = serde_json::Value::from(-2.0);
        let error = export_plan(&body_string(&body)).unwrap_err();
        assert!(
            error.contains("invalid page_scale"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn malformed_json_is_rejected() {
        let error = export_plan("not json at all").unwrap_err();
        assert!(
            error.contains("not valid JSON"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn missing_plan_is_rejected() {
        let mut body = export_body("svg", "teacher");
        body.as_object_mut().unwrap().remove("request");
        body.as_object_mut().unwrap().remove("response");
        let error = export_plan(&body_string(&body)).unwrap_err();
        assert!(
            error.contains("missing required fields"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn pdf_landscape_swaps_a4_page() {
        let mut body = export_body("pdf", "teacher");
        body["orientation"] = serde_json::Value::String("landscape".into());
        let pdf = String::from_utf8(export_ok(&body)).unwrap();
        assert!(pdf.starts_with("%PDF-1.4"));
        assert!(pdf.contains("/MediaBox [0 0 842 595]"), "A4 landscape");
        assert!(pdf.ends_with("%%EOF\n"));
    }

    #[test]
    fn pdf_page_scale_produces_valid_output() {
        let mut body = export_body("pdf", "teacher");
        body["page_scale"] = serde_json::Value::from(1.5);
        let pdf = String::from_utf8(export_ok(&body)).unwrap();
        assert!(pdf.starts_with("%PDF-1.4"));
        assert!(pdf.ends_with("%%EOF\n"));
    }

    #[test]
    fn format_of_reports_format_and_mime() {
        let format = format_of(&body_string(&export_body("png", "teacher"))).unwrap();
        assert_eq!(format, ExportFormat::Png);
        assert_eq!(format.mime(), "image/png");
        assert_eq!(format.extension(), "png");

        assert!(format_of(&body_string(&export_body("gif", "teacher"))).is_err());
        assert!(format_of("{}").is_err());
        assert!(format_of("").is_err());
    }

    #[test]
    fn export_case_insensitive_format_and_template() {
        let body = export_body("SVG", "TEACHER");
        let bytes = export_plan(&body_string(&body)).unwrap();
        let svg = String::from_utf8(bytes).unwrap();
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("Alice"));
    }
    #[test]
    fn detail_line_follows_show_height_and_show_vision() {
        let request_json = r#"{
            "format": "svg",
            "template": "teacher",
            "privacy": {
                "hide_scores": false, "hide_notes": false, "hide_special_needs": false,
                "anonymize": false, "show_height": true, "show_vision": true
            },
            "orientation": "portrait", "page_scale": 1.0, "locale": "zh", "show_student_ids": true,
            "request": {
                "api_version": 2, "student_count": 1,
                "seat_positions": [[0.0, 0.0]],
                "students": [{"key": "S1", "display_name": "Alice", "height_cm": 160.0, "vision": "0.8"}]
            },
            "response": {
                "api_version": 2, "feasible": true, "assignment": [[0, 0]],
                "attempts_used": 1, "hard_constraints_satisfied": true
            }
        }"#;
        // Both options on: the SVG carries the detail line.
        let svg =
            String::from_utf8(render_export(&parse_export_request(request_json).unwrap()).unwrap())
                .unwrap();
        assert!(svg.contains("160 cm"), "height detail missing: {svg}");
        assert!(svg.contains("vision 0.8"), "vision detail missing: {svg}");

        // Only height: the vision part is filtered out.
        let height_only = request_json.replace("\"show_vision\": true", "\"show_vision\": false");
        let svg =
            String::from_utf8(render_export(&parse_export_request(&height_only).unwrap()).unwrap())
                .unwrap();
        assert!(svg.contains("160 cm"), "height detail missing: {svg}");
        assert!(!svg.contains("vision"), "vision must be filtered: {svg}");

        // Anonymized: neither the name nor the detail survives.
        let anonymized = request_json.replace("\"anonymize\": false", "\"anonymize\": true");
        let svg =
            String::from_utf8(render_export(&parse_export_request(&anonymized).unwrap()).unwrap())
                .unwrap();
        assert!(!svg.contains("Alice"), "name must be hidden: {svg}");
        assert!(!svg.contains("160 cm"), "detail must be hidden: {svg}");
    }
}
