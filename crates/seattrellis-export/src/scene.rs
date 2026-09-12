//! One print-space scene for visual exports and their previews. Coordinates and
//! font sizes are in points; no renderer independently stretches the seat grid.

use crate::render::{is_zh_locale, PdfLayout, SeatingGrid};

pub type Color = [u8; 3];
pub const INK: Color = [35, 37, 34];
pub const MUTED: Color = [103, 107, 99];
pub const SEAT_FILL: Color = [246, 247, 242];
pub const SEAT_BORDER: Color = [178, 184, 171];
const SMALL_NAMES_WARNING: &str = "Some names are smaller than 8 pt. Use a larger paper size or landscape orientation for a more readable chart.";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
}

#[derive(Debug, Clone)]
pub enum Element {
    Box {
        bounds: Rect,
        radius: f64,
        fill: Color,
        stroke: Color,
        stroke_width: f64,
        dashed: bool,
    },
    /// Each element is a single pre-fitted line. All writers use this same
    /// box, size and alignment instead of applying their own auto-fit rules.
    Text {
        bounds: Rect,
        text: String,
        font_size: f64,
        color: Color,
        bold: bool,
        align: TextAlign,
    },
}

#[derive(Debug, Clone)]
pub struct ChartScene {
    pub width: f64,
    pub height: f64,
    pub elements: Vec<Element>,
    pub warnings: Vec<String>,
}

/// Natural desk proportions match the workbench canvas (116 × 70, 18-point
/// spacing). Missing row/column coordinates remain actual whitespace.
pub fn build_scene(grid: &SeatingGrid, page: &PdfLayout, locale: &str) -> ChartScene {
    let mut scene = ChartScene {
        width: page.page_w,
        height: page.page_h,
        elements: Vec::new(),
        warnings: Vec::new(),
    };
    let margin = page.margin_pt;
    let width = (page.page_w - 2.0 * margin).max(1.0);
    let zh = is_zh_locale(locale);
    scene.text(
        &grid.title,
        Rect {
            x: margin,
            y: margin,
            w: width,
            h: 30.0,
        },
        22.0,
        INK,
        true,
        TextAlign::Left,
        1,
    );
    let mut subtitle = if zh {
        format!(
            "{} 名学生 · {} 个座位",
            grid.cells.iter().filter(|c| c.student.is_some()).count(),
            grid.cells.len()
        )
    } else {
        format!(
            "{} students · {} seats",
            grid.cells.iter().filter(|c| c.student.is_some()).count(),
            grid.cells.len()
        )
    };
    if grid.subtitle.ends_with("infeasible") {
        subtitle.push_str(if zh {
            " · 不可行"
        } else {
            " · infeasible"
        });
    }
    scene.text(
        &subtitle,
        Rect {
            x: margin,
            y: margin + 34.0,
            w: width,
            h: 16.0,
        },
        10.0,
        MUTED,
        false,
        TextAlign::Left,
        1,
    );
    let cols = (i64::from(grid.max_col) - i64::from(grid.min_col) + 1).max(1) as f64;
    let rows = (i64::from(grid.max_row) - i64::from(grid.min_row) + 1).max(1) as f64;
    let natural_w = cols * 134.0 - 18.0;
    let natural_h = rows * 88.0 - 18.0;
    let available_h = (page.page_h - 2.0 * margin - 112.0).max(1.0);
    // Legacy scale values > 1 must never enlarge content past the page edge.
    let scale =
        (width / natural_w).min(available_h / natural_h) * page.scale_multiplier.clamp(0.5, 1.0);
    let grid_x = margin + (width - natural_w * scale) / 2.0;
    let grid_y = margin + 92.0 + (available_h - natural_h * scale) / 2.0;
    let stage_w = (180.0 * scale).max(90.0).min(width.min(180.0));
    // Keep the front marker attached to the room, including a wide room on
    // portrait paper where vertical centering otherwise leaves a huge gap.
    let stage_y = grid_y - 30.0;
    scene.elements.push(Element::Box {
        bounds: Rect {
            x: page.page_w / 2.0 - stage_w / 2.0,
            y: stage_y,
            w: stage_w,
            h: 20.0,
        },
        radius: 5.0,
        fill: [239, 241, 234],
        stroke: SEAT_BORDER,
        stroke_width: 0.7,
        dashed: false,
    });
    scene.text(
        if zh {
            "讲台 · 教室前方"
        } else {
            "FRONT OF ROOM"
        },
        Rect {
            x: page.page_w / 2.0 - stage_w / 2.0,
            y: stage_y,
            w: stage_w,
            h: 20.0,
        },
        9.0,
        MUTED,
        false,
        TextAlign::Center,
        1,
    );
    // Normal names share a geometric type scale. An unusually long name can
    // wrap/shrink locally; it must not make every other student's name tiny.
    let name_box = Rect {
        x: 0.0,
        y: 0.0,
        w: 100.0 * scale,
        h: 31.0 * scale,
    };
    let name_size = (18.0 * scale).min(24.0);
    for cell in &grid.cells {
        let bounds = Rect {
            x: grid_x + f64::from(cell.col - grid.min_col) * 134.0 * scale,
            y: grid_y + f64::from(cell.row - grid.min_row) * 88.0 * scale,
            w: 116.0 * scale,
            h: 70.0 * scale,
        };
        scene.elements.push(Element::Box {
            bounds,
            radius: 7.0 * scale,
            fill: if cell.enabled {
                SEAT_FILL
            } else {
                [234, 235, 231]
            },
            stroke: SEAT_BORDER,
            stroke_width: (0.8 * scale).clamp(0.4, 1.2),
            dashed: cell.student.is_none(),
        });
        scene.text(
            &cell.seat_id,
            Rect {
                x: bounds.x + 8.0 * scale,
                y: bounds.y + 5.0 * scale,
                w: bounds.w - 16.0 * scale,
                h: 12.0 * scale,
            },
            (8.0 * scale).min(10.0),
            MUTED,
            false,
            TextAlign::Left,
            1,
        );
        let label = cell.student.as_deref().unwrap_or(if !cell.enabled {
            if zh {
                "停用"
            } else {
                "Unavailable"
            }
        } else if zh {
            "空座"
        } else {
            "Empty"
        });
        let name_start = scene.elements.len();
        scene.text(
            label,
            Rect {
                x: bounds.x + 8.0 * scale,
                y: bounds.y + 20.0 * scale,
                w: name_box.w,
                h: name_box.h,
            },
            if cell.student.is_some() {
                name_size
            } else {
                (11.0 * scale).min(16.0)
            },
            if cell.student.is_some() { INK } else { MUTED },
            false,
            TextAlign::Center,
            2,
        );
        if cell.student.is_some()
            && scene.elements[name_start..].iter().any(
                |element| matches!(element, Element::Text { font_size, .. } if *font_size < 8.0),
            )
            && !scene
                .warnings
                .iter()
                .any(|warning| warning == SMALL_NAMES_WARNING)
        {
            scene.warnings.push(SMALL_NAMES_WARNING.into());
        }
        let detail = [cell.student_key.as_deref(), cell.detail.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        scene.text(
            &detail,
            Rect {
                x: bounds.x + 7.0 * scale,
                y: bounds.y + 54.0 * scale,
                w: bounds.w - 14.0 * scale,
                h: 11.0 * scale,
            },
            (8.0 * scale).min(10.0),
            MUTED,
            false,
            TextAlign::Center,
            1,
        );
    }
    scene.text(
        "SeatTrellis",
        Rect {
            x: margin,
            y: page.page_h - margin - 10.0,
            w: width,
            h: 10.0,
        },
        7.0,
        MUTED,
        false,
        TextAlign::Left,
        1,
    );
    scene
}

impl ChartScene {
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        text: &str,
        bounds: Rect,
        cap: f64,
        color: Color,
        bold: bool,
        align: TextAlign,
        max_lines: usize,
    ) {
        if text.is_empty() {
            return;
        }
        let (size, lines) = fit_text(text, bounds.w, bounds.h, cap, max_lines);
        if text.chars().filter(|ch| !ch.is_whitespace()).ne(lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|ch| !ch.is_whitespace()))
        {
            let warning = "Some text was shortened to fit. Use a larger paper size or the editable spreadsheet to see complete values.".to_string();
            if !self.warnings.contains(&warning) {
                self.warnings.push(warning);
            }
        }
        let line_h = size * 1.2;
        let top = bounds.y + (bounds.h - line_h * lines.len() as f64) / 2.0;
        for (index, line) in lines.into_iter().enumerate() {
            self.elements.push(Element::Text {
                bounds: Rect {
                    x: bounds.x,
                    y: top + index as f64 * line_h,
                    w: bounds.w,
                    h: line_h,
                },
                text: line,
                font_size: size,
                color,
                bold,
                align,
            });
        }
    }
}

/// Font advance metrics keep Latin names from being shrunk as if every glyph
/// were full-width. A conservative em estimate supports vector-only hosts.
pub fn text_width(text: &str, size: f64) -> f64 {
    if let Some(font) = crate::fonts::load_cjk_font() {
        text.chars()
            .map(|ch| f64::from(font.metrics(ch, size as f32).advance_width))
            .sum()
    } else {
        text.chars()
            .map(|ch| if ch.is_ascii() { 0.62 } else { 1.0 })
            .sum::<f64>()
            * size
    }
}

pub fn fit_text(
    text: &str,
    width: f64,
    height: f64,
    cap: f64,
    max_lines: usize,
) -> (f64, Vec<String>) {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut size = cap.min(height / 1.2).max(0.1);
    let floor = size.min(6.0);
    for _ in 0..24 {
        let lines = wrap_text(&clean, width.max(0.1), size);
        if lines.len() <= max_lines
            && lines.len() as f64 * size * 1.2 <= height + 0.01
            && lines
                .iter()
                .all(|line| text_width(line, size) <= width + 0.001)
        {
            return (size, lines);
        }
        if size <= floor {
            break;
        }
        size = (size * 0.9).max(floor);
    }
    let allowed = max_lines.min((height / (size * 1.2)).floor().max(1.0) as usize);
    let mut lines = wrap_text(&clean, width.max(0.1), size);
    lines.truncate(allowed);
    if let Some(last) = lines.last_mut() {
        while !last.is_empty() && text_width(&format!("{last}…"), size) > width {
            last.pop();
        }
        last.push('…');
    }
    (size, lines)
}

fn wrap_text(text: &str, width: f64, size: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut line_width = 0.0;
    for ch in text.chars() {
        let w = text_width(&ch.to_string(), size);
        if !line.is_empty() && line_width + w > width {
            // Prefer natural word boundaries for Latin names; CJK and a single
            // overlong word still break at characters rather than overflow.
            if let Some(split) = line.rfind([' ', '-']) {
                let split = if line.as_bytes()[split] == b'-' {
                    split + 1
                } else {
                    split
                };
                let remainder = line[split..].trim_start().to_string();
                let first = line[..split].trim_end().to_string();
                if !first.is_empty() {
                    lines.push(first);
                    line = remainder;
                    line_width = text_width(&line, size);
                }
            }
            if !line.is_empty() && line_width + w > width {
                lines.push(std::mem::take(&mut line));
                line_width = 0.0;
            }
        }
        if line.is_empty() && ch.is_whitespace() {
            continue;
        }
        line.push(ch);
        line_width += w;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::GridCell;
    fn grid() -> SeatingGrid {
        SeatingGrid {
            title: "A very long but valid classroom name".into(),
            subtitle: "feasible".into(),
            min_row: 0,
            max_row: 4,
            min_col: 0,
            max_col: 6,
            cells: vec![
                GridCell {
                    seat_id: "Window-17".into(),
                    row: 0,
                    col: 0,
                    seat_index: 0,
                    student: Some("Alexandra Montgomery-Johnson".into()),
                    student_key: Some("S001".into()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    seat_id: "Door".into(),
                    row: 4,
                    col: 6,
                    seat_index: 1,
                    student: Some("李明".into()),
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
            ],
        }
    }
    #[test]
    fn scene_preserves_desk_proportions_real_ids_and_sparse_gaps() {
        let scene = build_scene(&grid(), &PdfLayout::landscape(), "zh");
        let boxes: Vec<_> = scene
            .elements
            .iter()
            .filter_map(|e| {
                if let Element::Box { bounds, .. } = e {
                    Some(bounds)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(boxes.len(), 3, "only the podium and actual seats are drawn");
        assert!((boxes[1].w / boxes[1].h - 116.0 / 70.0).abs() < 0.0001);
        assert!(boxes[2].x > boxes[1].x + boxes[1].w * 5.0);
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e,Element::Text{text,..} if text=="Window-17")));
    }
    #[test]
    fn all_elements_fit_the_page_even_at_legacy_double_scale() {
        for landscape in [true, false] {
            let page = PdfLayout::from_paper(crate::render::PaperSize::A4, landscape, 25.0)
                .with_scale(2.0);
            let scene = build_scene(&grid(), &page, "en");
            for element in scene.elements {
                let bounds = match element {
                    Element::Box { bounds, .. } | Element::Text { bounds, .. } => bounds,
                };
                assert!(bounds.x >= page.margin_pt - 0.01 && bounds.y >= page.margin_pt - 0.01);
                assert!(bounds.x + bounds.w <= page.page_w - page.margin_pt + 0.01);
                assert!(bounds.y + bounds.h <= page.page_h - page.margin_pt + 0.01);
            }
        }
    }
    #[test]
    fn names_wrap_at_words_without_making_other_names_tiny() {
        let (_, lines) = fit_text("Alexandra Montgomery", 110.0, 32.0, 15.0, 2);
        assert_eq!(lines, vec!["Alexandra", "Montgomery"]);
        let normal = build_scene(&grid(), &PdfLayout::landscape(), "zh");
        let mut changed = grid();
        changed.cells[0].student = Some("Verylongname".repeat(40));
        let pathological = build_scene(&changed, &PdfLayout::landscape(), "zh");
        let size = |scene: &ChartScene| {
            scene
                .elements
                .iter()
                .find_map(|e| match e {
                    Element::Text {
                        text, font_size, ..
                    } if text == "李明" => Some(*font_size),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(size(&normal), size(&pathological));
        assert!(pathological
            .warnings
            .iter()
            .any(|w| w.contains("shortened")));
        assert!(pathological
            .warnings
            .iter()
            .any(|w| w == SMALL_NAMES_WARNING));
        changed.cells[0].student.as_mut().unwrap().push('…');
        assert!(
            build_scene(&changed, &PdfLayout::landscape(), "zh")
                .warnings
                .iter()
                .any(|w| w.contains("shortened")),
            "an ellipsis in source text must not suppress a real truncation warning"
        );
    }
}
