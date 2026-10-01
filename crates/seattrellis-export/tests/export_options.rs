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

fn text_parts(bytes: &[u8], format: &str) -> Vec<String> {
    use std::io::Read;
    if matches!(format, "docx" | "xlsx" | "pptx") {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .filter_map(|index| {
                let mut part = archive.by_index(index).unwrap();
                if !part.name().ends_with(".xml") && !part.name().ends_with(".rels") {
                    return None;
                }
                let mut text = String::new();
                part.read_to_string(&mut text).unwrap();
                Some(text)
            })
            .collect()
    } else {
        vec![String::from_utf8(bytes.to_vec()).unwrap()]
    }
}

#[test]
fn every_format_keeps_assignments_on_the_enabled_layout_seats() {
    let mut request: serde_json::Value = serde_json::from_str(&base_request(serde_json::json!({
        "locale": "en", "privacy": {"show_height": false, "show_vision": false}
    })))
    .unwrap();
    request["request"]["student_count"] = serde_json::json!(2);
    request["request"]["students"] = serde_json::json!([
        {"key": "s0", "display_name": "Alice"},
        {"key": "s1", "display_name": "Bob"}
    ]);
    request["request"]["student_scores"] = serde_json::json!([]);
    request["request"]["seat_positions"] = serde_json::json!([[1.0, 1.0], [3.0, 1.0]]);
    request["request"]["edges"] = serde_json::json!([]);
    request["request"]["layout"]["seats"] = serde_json::json!([
        {"seat_id":"A0", "row":7, "col":4, "x":0.0, "y":1.0, "enabled":false},
        {"seat_id":"A1", "row":2, "col":5, "x":1.0, "y":1.0, "enabled":true},
        {"seat_id":"A2", "row":7, "col":6, "x":2.0, "y":1.0, "enabled":false},
        {"seat_id":"A3", "row":4, "col":9, "x":3.0, "y":1.0, "enabled":true}
    ]);
    request["response"]["assignment"] = serde_json::json!([[0, 0], [1, 1]]);
    for format in [
        "svg",
        "html",
        "print-html",
        "docx",
        "xlsx",
        "pptx",
        "png",
        "pdf",
    ] {
        request["format"] = serde_json::json!(format);
        let (preview, _) =
            seattrellis_export::export::export_preview_with_warnings(&request.to_string()).unwrap();
        let preview = String::from_utf8(preview).unwrap();
        assert!(
            preview.contains("Alice") && preview.contains("Bob"),
            "{format}"
        );
        assert!(preview.contains("A1") && preview.contains("A3"), "{format}");
        let bytes = export_plan(&request.to_string()).unwrap();
        if format == "png" {
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        } else if format == "pdf" {
            assert!(bytes.starts_with(b"%PDF-"));
        } else {
            let text = text_parts(&bytes, format).join("\n");
            assert!(text.contains("Alice") && text.contains("Bob"), "{format}");
            assert!(text.contains("A1") && text.contains("A3"), "{format}");
            if matches!(format, "html" | "print-html") {
                assert!(
                    text.contains("<td>s0</td><td>Alice</td><td>A1</td>"),
                    "{format}"
                );
                assert!(
                    text.contains("<td>s1</td><td>Bob</td><td>A3</td>"),
                    "{format}"
                );
            }
        }
    }
}

#[test]
fn forbidden_xml_characters_are_removed_from_every_export_field_with_a_private_safe_warning() {
    let mut request: serde_json::Value =
        serde_json::from_str(&base_request(serde_json::json!({}))).unwrap();
    request["title"] = serde_json::json!("Title\u{fffe}\u{ffff}");
    request["request"]["students"][0]["display_name"] =
        serde_json::json!("PrivateName\u{fffe}\u{ffff}");
    request["request"]["students"][0]["vision"] = serde_json::json!("PrivateVision\u{ffff}");
    request["request"]["students"][0]["key"] = serde_json::json!("PrivateKey\u{fffe}");
    request["request"]["layout"]["seats"][0]["seat_id"] = serde_json::json!("Seat\u{ffff}");
    for format in ["svg", "html", "print-html", "docx", "xlsx", "pptx"] {
        request["format"] = serde_json::json!(format);
        let (bytes, warnings) =
            seattrellis_export::export::export_plan_with_warnings(&request.to_string()).unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("characters were removed")),
            "{format}: {warnings:?}"
        );
        assert!(warnings.iter().all(|warning| !warning.contains("Private")));
        for text in text_parts(&bytes, format) {
            assert!(
                !text.contains('\u{fffe}') && !text.contains('\u{ffff}'),
                "{format}"
            );
            if !matches!(format, "html" | "print-html") {
                let mut reader = quick_xml::Reader::from_str(&text);
                loop {
                    if matches!(
                        reader
                            .read_event()
                            .expect("independent XML reader accepts exported part"),
                        quick_xml::events::Event::Eof
                    ) {
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn id_only_records_have_localized_neutral_labels_when_ids_are_off() {
    let mut request: serde_json::Value = serde_json::from_str(&base_request(
        serde_json::json!({"show_student_ids": false}),
    ))
    .unwrap();
    let identifier = "IDENTIFIER_ONLY_012345";
    request["request"]["students"][0]["key"] = serde_json::json!(identifier);
    request["request"]["students"][0]["display_name"] = serde_json::Value::Null;
    for (locale, label) in [("en", "Student 1"), ("zh", "学生 1")] {
        request["locale"] = serde_json::json!(locale);
        for format in [
            "svg",
            "html",
            "print-html",
            "docx",
            "xlsx",
            "pptx",
            "png",
            "pdf",
        ] {
            request["format"] = serde_json::json!(format);
            let (preview, _) =
                seattrellis_export::export::export_preview_with_warnings(&request.to_string())
                    .unwrap();
            let preview = String::from_utf8(preview).unwrap();
            assert!(preview.contains(label), "{locale}/{format}");
            assert!(!preview.contains(identifier), "{locale}/{format}");
            if !matches!(format, "png" | "pdf") {
                let bytes = export_plan(&request.to_string()).unwrap();
                let text = text_parts(&bytes, format).join("\n");
                assert!(!text.contains(identifier), "{locale}/{format}");
                assert!(text.contains(label), "{locale}/{format}");
            }
        }
    }
    request["show_student_ids"] = serde_json::json!(true);
    request["format"] = serde_json::json!("xlsx");
    assert!(
        text_parts(&export_plan(&request.to_string()).unwrap(), "xlsx")
            .join("\n")
            .contains(identifier)
    );
}

#[test]
fn public_sanitization_warnings_do_not_inspect_hidden_student_values() {
    let mut request: serde_json::Value = serde_json::from_str(&base_request(
        serde_json::json!({"template": "public", "format": "xlsx"}),
    ))
    .unwrap();
    request["request"]["students"][0]["display_name"] = serde_json::json!("Private\u{ffff}");
    request["request"]["students"][0]["key"] = serde_json::json!("Identifier\u{fffe}");
    request["request"]["students"][0]["vision"] = serde_json::json!("Vision\u{ffff}");
    let (bytes, warnings) =
        seattrellis_export::export::export_plan_with_warnings(&request.to_string()).unwrap();
    assert!(!warnings
        .iter()
        .any(|warning| warning.contains("characters were removed")));
    let text = text_parts(&bytes, "xlsx").join("\n");
    assert!(!text.contains("Private") && !text.contains("Identifier") && !text.contains("Vision"));
}

#[test]
fn html_complete_assignments_preserve_long_values_and_the_public_privacy_boundary() {
    let mut request: serde_json::Value = serde_json::from_str(&base_request(
        serde_json::json!({"format": "html", "locale": "en"}),
    ))
    .unwrap();
    let name = "Long <private> student & name ".repeat(20);
    request["request"]["students"][0]["display_name"] = serde_json::json!(name);
    let html = String::from_utf8(export_plan(&request.to_string()).unwrap()).unwrap();
    assert!(html.contains("<details class=\"assignment-values\">"));
    assert!(html.contains("<th scope=\"col\">Student name</th>"));
    assert!(html.contains(&"Long &lt;private&gt; student &amp; name ".repeat(20)));
    assert!(html.contains("@media print{.assignment-values{display:none}}"));
    request["template"] = serde_json::json!("public");
    let html = String::from_utf8(export_plan(&request.to_string()).unwrap()).unwrap();
    assert!(!html.contains("private") && !html.contains("150 cm") && !html.contains(">s0<"));
    assert!(html.contains("student 01"));
}

fn classroom_request(count: usize, landscape: bool, show_ids: bool, margin_mm: f64) -> String {
    let mut request: serde_json::Value = serde_json::from_str(&base_request(serde_json::json!({
        "format": "docx", "orientation": if landscape { "landscape" } else { "portrait" },
        "show_student_ids": show_ids, "margin_mm": margin_mm,
        "privacy": {"show_height": false, "show_vision": false}
    })))
    .unwrap();
    request["request"]["student_count"] = serde_json::json!(count);
    request["request"]["seat_positions"] = serde_json::json!((0..count)
        .map(|index| [index % 6, index / 6])
        .collect::<Vec<_>>());
    request["request"]["students"] = serde_json::json!((0..count).map(|index| serde_json::json!({"key":format!("ID{index:02}"),"display_name":format!("学生{index:02}")})).collect::<Vec<_>>());
    request["request"]["student_scores"] = serde_json::json!([]);
    request["request"]["edges"] = serde_json::json!([]);
    request["request"]["layout"] = serde_json::Value::Null;
    request["response"]["assignment"] =
        serde_json::json!((0..count).map(|index| [index, index]).collect::<Vec<_>>());
    request["response"]["total_cost"] = serde_json::json!(0.0);
    request.to_string()
}

/// Independent reader acceptance, deliberately separate from XML geometry tests.
/// Requires LibreOffice and Poppler plus CJK fonts on PATH, no product runtime.
#[test]
#[ignore = "requires LibreOffice/Poppler; run the Word reader acceptance gate explicitly"]
fn word_reader_keeps_ordinary_40_and_60_student_charts_on_one_page() {
    use std::process::Command;
    let tmp = tempfile::tempdir().unwrap();
    for (index, (count, landscape, show_ids, margin)) in [
        (40, true, false, 12.0),
        (40, false, true, 12.0),
        (60, true, false, 12.0),
        (60, true, true, 12.0),
        (60, false, false, 12.0),
        (60, false, true, 12.0),
        (60, true, true, 25.0),
    ]
    .into_iter()
    .enumerate()
    {
        let request = classroom_request(count, landscape, show_ids, margin);
        let docx = tmp.path().join(format!("chart-{index}.docx"));
        std::fs::write(&docx, export_plan(&request).unwrap()).unwrap();
        let conversion = Command::new("soffice")
            .arg(format!(
                "-env:UserInstallation=file://{}",
                tmp.path().join("lo-profile").display()
            ))
            .args(["--headless", "--convert-to", "pdf", "--outdir"])
            .arg(tmp.path())
            .arg(&docx)
            .env("XDG_CACHE_HOME", tmp.path().join("lo-cache"))
            .output()
            .expect("LibreOffice must be installed for this explicit reader gate");
        assert!(
            conversion.status.success(),
            "{}",
            String::from_utf8_lossy(&conversion.stderr)
        );
        let pdf = docx.with_extension("pdf");
        let info = Command::new("pdfinfo").arg(&pdf).output().unwrap();
        assert!(info.status.success());
        let info = String::from_utf8(info.stdout).unwrap();
        let pages = info
            .lines()
            .find(|line| line.starts_with("Pages:"))
            .unwrap()
            .split_whitespace()
            .last()
            .unwrap();
        assert_eq!(
            pages, "1",
            "count={count} landscape={landscape} ids={show_ids} margin={margin}\n{info}"
        );
        let text = Command::new("pdftotext")
            .args(["-layout"])
            .arg(&pdf)
            .arg("-")
            .output()
            .unwrap();
        assert!(text.status.success());
        let text = String::from_utf8(text.stdout).unwrap();
        // Readers may separate CJK and Latin runs during PDF extraction.
        let normalized: String = text
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        for student in 0..count {
            assert!(
                normalized.contains(&format!("学生{student:02}")),
                "student {student} was clipped or lost: {text}"
            );
        }
    }
}

#[test]
fn every_format_rejects_seats_that_collapse_to_one_rendered_coordinate() {
    let mut request: serde_json::Value =
        serde_json::from_str(&base_request(serde_json::json!({}))).unwrap();
    request["request"]["layout"] = serde_json::Value::Null;
    request["request"]["seat_positions"][0] = serde_json::json!([0.1, 0.0]);
    request["request"]["seat_positions"][1] = serde_json::json!([0.2, 0.0]);
    for format in [
        "svg",
        "png",
        "pdf",
        "html",
        "print-html",
        "docx",
        "pptx",
        "xlsx",
    ] {
        request["format"] = serde_json::json!(format);
        let error = export_plan(&request.to_string()).unwrap_err();
        assert!(error.contains("multiple seats map"), "{format}: {error}");
        let error = seattrellis_export::export::export_preview_with_warnings(&request.to_string())
            .unwrap_err();
        assert!(
            error.contains("multiple seats map"),
            "{format} preview: {error}"
        );
    }
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
        doc.contains(r#"<w:pgSz w:w="16840" w:h="11900"/>"#),
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
        doc.contains(r#"<w:pgSz w:w="11900" w:h="16840"/>"#),
        "portrait pgSz must keep A4 portrait"
    );
}

#[test]
fn docx_export_honours_paper_and_margin_options() {
    let bytes = export_plan(&base_request(serde_json::json!({
        "format": "docx", "orientation": "landscape", "paper_size": "a3", "margin_mm": 20.0
    })))
    .expect("a3 docx exports");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut doc = String::new();
    use std::io::Read;
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut doc)
        .unwrap();
    assert!(
        doc.contains(r#"<w:pgSz w:w="23820" w:h="16840"/>"#),
        "A3 landscape must not silently export A4"
    );
    assert!(
        doc.contains(r#"<w:pgMar w:top="1140" w:right="1140" w:bottom="1140" w:left="1140""#),
        "20 mm margins must be used on all edges"
    );
    assert!(doc.contains(r#"<w:tblLayout w:type="fixed"/>"#));
}
