//! Export option unification gates (plan §12.3 / M5-A1): paper_size,
//! margin_mm and orientation must take effect on the formats that support
//! them, and option rejection rules must stay strict.

use seattrellis_export::export::export_plan;
use seattrellis_export::render::{PaperSize, PdfLayout};

fn base_request(extra: serde_json::Value) -> String {
    let mut doc = serde_json::json!({
        "draft_id": "opt-test",
        "format": "pdf",
        "template": "teacher",
        "privacy": {"hide_scores": true, "hide_notes": true, "hide_special_needs": true,
                    "anonymize": false, "show_height": true, "show_vision": true},
        "orientation": "landscape",
        "page_scale": 1.0,
        "locale": "zh",
        "show_student_ids": true,
        "request": {"api_version": 2, "student_count": 4,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[1.0,2.0],[2.0,2.0]],
            "edges": [[0,1],[0,2],[1,3],[2,3]],
            "fixed_seats": [], "must_be_adjacent": [], "cannot_be_adjacent": [], "min_distance": [],
            "seed": 7,
            "students": [
                {"key": "s0", "display_name": "学生0", "height_cm": 150.0, "score": 70.0},
                {"key": "s1", "display_name": "学生1", "height_cm": 160.0, "score": 75.0},
                {"key": "s2", "display_name": "学生2", "height_cm": 140.0, "score": 65.0},
                {"key": "s3", "display_name": "学生3", "height_cm": 170.0, "score": 80.0}
            ],
            "student_scores": [70.0, 75.0, 65.0, 80.0],
            "rules": {"schema_version": 0, "seed": 7, "hard": {}, "soft": {}, "groups": []},
            "layout": {"layout_id": "opt", "name": "opt", "seats": [
                {"seat_id": "R1C1", "row": 1, "col": 1, "x": 1.0, "y": 1.0, "enabled": true, "zone": "front"},
                {"seat_id": "R1C2", "row": 1, "col": 2, "x": 2.0, "y": 1.0, "enabled": true, "zone": "front"},
                {"seat_id": "R2C1", "row": 2, "col": 1, "x": 1.0, "y": 2.0, "enabled": true, "zone": "middle"},
                {"seat_id": "R2C2", "row": 2, "col": 2, "x": 2.0, "y": 2.0, "enabled": true, "zone": "middle"}
            ], "adjacency": {"include_horizontal": true, "include_vertical": true}},
            "history": null, "pair_history": null, "time_limit_seconds": null
        },
        "response": {"api_version": 2, "feasible": true, "status": "Solved",
            "assignment": [[0,0],[1,1],[2,2],[3,3]], "attempts_used": 1,
            "hard_constraints_satisfied": true}
    });
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            doc[k] = v.clone();
        }
    }
    doc.to_string()
}

#[test]
fn paper_sizes_have_correct_point_dimensions() {
    assert_eq!(PaperSize::A4.points(), (595.0, 842.0));
    assert_eq!(PaperSize::A3.points(), (842.0, 1191.0));
    assert_eq!(PaperSize::Letter.points(), (612.0, 792.0));
}

#[test]
fn pdf_layout_applies_paper_orientation_and_margin() {
    let a4_landscape = PdfLayout::from_paper(PaperSize::A4, true, 12.0);
    assert_eq!(a4_landscape.page_w, 842.0);
    assert_eq!(a4_landscape.page_h, 595.0);
    assert_eq!(a4_landscape.margin_pt, 34.0); // 12mm

    let a3_portrait = PdfLayout::from_paper(PaperSize::A3, false, 20.0);
    assert_eq!(a3_portrait.page_w, 842.0);
    assert_eq!(a3_portrait.page_h, 1191.0);
    assert_eq!(a3_portrait.margin_pt, 57.0); // 20mm

    // margin clamp 5..25mm
    assert_eq!(
        PdfLayout::from_paper(PaperSize::A4, false, 3.0).margin_pt,
        14.0
    );
    assert_eq!(
        PdfLayout::from_paper(PaperSize::A4, false, 40.0).margin_pt,
        71.0
    );
}

#[test]
fn pdf_export_accepts_paper_size_and_margin_options() {
    // A3 landscape PDF must render without error and carry the A3 MediaBox.
    let bytes = export_plan(&base_request(serde_json::json!({
        "paper_size": "a3", "margin_mm": 15.0
    })))
    .expect("a3 pdf exports");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("/MediaBox [0 0 1191 842]"),
        "A3 landscape MediaBox"
    );
}

#[test]
fn pdf_export_rejects_unknown_paper_size() {
    let error = export_plan(&base_request(serde_json::json!({"paper_size": "legal"})))
        .expect_err("unknown paper size must be rejected");
    assert!(error.contains("paper_size"), "{error}");
}

#[test]
fn pdf_export_rejects_non_positive_margin() {
    let error = export_plan(&base_request(serde_json::json!({"margin_mm": -1.0})))
        .expect_err("non-positive margin must be rejected");
    assert!(error.contains("margin_mm"), "{error}");
}

#[test]
fn docx_landscape_swaps_page_dimensions() {
    let bytes = export_plan(&base_request(serde_json::json!({
        "format": "docx", "orientation": "landscape"
    })))
    .expect("landscape docx exports");
    // OOXML zip: extract word/document.xml and check the pgSz swap.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut doc = String::new();
    use std::io::Read;
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut doc)
        .unwrap();
    assert!(
        doc.contains(r#"<w:pgSz w:w="16838" w:h="11906"/>"#),
        "landscape pgSz must swap width/height"
    );
    let portrait = export_plan(&base_request(serde_json::json!({
        "format": "docx", "orientation": "portrait"
    })))
    .expect("portrait docx exports");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&portrait)).unwrap();
    let mut doc = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut doc)
        .unwrap();
    assert!(
        doc.contains(r#"<w:pgSz w:w="11906" w:h="16838"/>"#),
        "portrait pgSz must keep A4 portrait"
    );
}
