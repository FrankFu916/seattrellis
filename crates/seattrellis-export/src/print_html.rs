//! Compatibility entry point for the printable HTML format. The page uses the
//! same scene as PDF/SVG/PNG instead of a separate CSS sizing algorithm.
use crate::render::{PaperSize, PdfLayout, SeatingGrid};
use crate::scene::{Element, Rect, TextAlign, MUTED};
use seattrellis_core::CoreSolveRequest;

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
    pub fn page_mm(&self) -> (f64, f64) {
        let (w, h) = self.paper.points();
        let dimensions = (w * 25.4 / 72.0, h * 25.4 / 72.0);
        if self.landscape {
            (dimensions.1, dimensions.0)
        } else {
            dimensions
        }
    }
}

pub fn render_print_html(
    grid: &SeatingGrid,
    _request: &CoreSolveRequest,
    options: &PrintHtmlOptions,
) -> String {
    let page = PdfLayout::from_paper(options.paper, options.landscape, options.margin_mm)
        .with_scale(options.page_scale);
    let mut grid = grid.clone();
    if !options.show_student_ids {
        for cell in &mut grid.cells {
            cell.student_key = None;
        }
    }
    let mut scene = crate::scene::build_scene(&grid, &page, &options.locale);
    let metadata = [
        options.period_label.clone(),
        options.seed.map(|seed| format!("seed {seed}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    if !metadata.is_empty() {
        scene.elements.push(Element::Text {
            bounds: Rect {
                x: page.page_w / 2.0,
                y: page.page_h - page.margin_pt - 10.0,
                w: page.page_w / 2.0 - page.margin_pt,
                h: 10.0,
            },
            text: metadata,
            font_size: 7.0,
            color: MUTED,
            bold: false,
            align: TextAlign::Left,
        });
    }
    crate::render::render_scene_html(&scene, &grid.title, &options.locale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::GridCell;
    fn grid() -> SeatingGrid {
        SeatingGrid {
            title: "Class <A>".into(),
            subtitle: "1 students / 2 seats / feasible".into(),
            min_row: 0,
            max_row: 0,
            min_col: 0,
            max_col: 2,
            cells: vec![
                GridCell {
                    seat_id: "Window".into(),
                    row: 0,
                    col: 0,
                    seat_index: 0,
                    student: Some("Alice".into()),
                    student_key: Some("S001".into()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    seat_id: "Door".into(),
                    row: 0,
                    col: 2,
                    seat_index: 1,
                    student: None,
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
            ],
        }
    }
    fn options() -> PrintHtmlOptions {
        PrintHtmlOptions {
            landscape: true,
            paper: PaperSize::A4,
            margin_mm: 12.0,
            page_scale: 1.0,
            show_student_ids: false,
            locale: "en".into(),
            seed: Some(42),
            period_label: None,
        }
    }
    fn request() -> CoreSolveRequest {
        serde_json::from_value(
            serde_json::json!({"api_version":2,"student_count":1,"seat_positions":[[0,0],[2,0]]}),
        )
        .unwrap()
    }
    #[test]
    fn print_html_embeds_the_shared_scene_without_independent_grid_sizing() {
        let html = render_print_html(&grid(), &request(), &options());
        assert!(html.contains("size:842pt 595pt;margin:0"));
        assert!(html.contains("<svg "));
        assert!(html.contains("Window"));
        assert!(html.contains("Door"));
        assert!(html.contains("Class &lt;A&gt;"));
        assert!(html.contains("seed 42"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("S001"));
        assert!(!html.contains("grid-template-columns"));
    }
    #[test]
    fn ids_are_opt_in_and_orientation_follows_paper_geometry() {
        let mut opts = options();
        opts.show_student_ids = true;
        opts.landscape = false;
        opts.paper = PaperSize::A3;
        let html = render_print_html(&grid(), &request(), &opts);
        assert!(html.contains("S001"));
        assert!(html.contains("size:842pt 1191pt"));
        let (w, h) = opts.page_mm();
        assert!(w < h);
    }
    #[test]
    fn legacy_scale_above_one_cannot_push_seats_outside_page() {
        let mut opts = options();
        let normal = render_print_html(&grid(), &request(), &opts);
        opts.page_scale = 2.0;
        assert_eq!(normal, render_print_html(&grid(), &request(), &opts));
    }
}
