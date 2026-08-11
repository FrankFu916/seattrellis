//! Minimal, controlled OOXML writers for the Office export formats
//! (修订版 §5.6 / §11.6): XLSX, DOCX and PPTX.
//!
//! These are deliberately small hand-rolled writers instead of third-party
//! crates: the plan's §5.6 policy is "若第三方 crate 成熟度不足，宁可实现
//! 受控的最小 OOXML writer，也不要为方便引入不稳定或体积巨大的依赖".
//! Each writer emits a zip container with the standard package parts
//! (`[Content_Types].xml`, `_rels/.rels`, and the format's document parts)
//! so independent readers (openpyxl / python-docx / python-pptx) can reopen
//! the bytes — that independent-reader check is the §11.6 acceptance
//! criterion, not byte-parity with the Python exporters.
//!
//! Content semantics mirror the Python oracle:
//! - XLSX: a "Seating" grid sheet (title row + one cell per seat, disabled
//!   seats marked `seat_id\n--`) plus an "Assignments" sheet
//!   (student_key / student_name / seat_id).
//! - DOCX: centered title, a generated-at meta line, and a bordered seat
//!   grid table (display name, or the seat id when disabled/empty).
//! - PPTX: one 16:9 slide (screen16x9) with a title and one rounded-rect
//!   shape per seat carrying `seat_id` and the student label, so seats stay
//!   individually editable.
//!
//! The grid passed in is already privacy-filtered by the export domain
//! module (`anonymize_grid` / `filter_detail_grid`), so public exports never
//! carry real student names into the Office documents.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::render::SeatingGrid;

/// XML-escape a text run (OOXML text elements).
fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Strip control characters that are illegal in XML 1.0.
            '\u{0}'..='\u{8}' | '\u{B}'..='\u{C}' | '\u{E}'..='\u{1F}' => {}
            other => out.push(other),
        }
    }
    out
}

/// Build the zip container for one Office document.
fn package(entries: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, content) in entries {
        writer
            .start_file(*name, options)
            .map_err(|error| format!("OOXML package: could not start '{name}': {error}"))?;
        writer
            .write_all(content.as_bytes())
            .map_err(|error| format!("OOXML package: could not write '{name}': {error}"))?;
    }
    let cursor = writer
        .finish()
        .map_err(|error| format!("OOXML package: could not finish zip: {error}"))?;
    Ok(cursor.into_inner())
}

// ---------------------------------------------------------------------------
// XLSX
// ---------------------------------------------------------------------------

const XLSX_CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
</Types>"#;

const XLSX_ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;

const XLSX_WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Seating" sheetId="1" r:id="rId1"/>
    <sheet name="Assignments" sheetId="2" r:id="rId2"/>
  </sheets>
</workbook>"#;

const XLSX_WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

const XLSX_STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
  <fills count="1"><fill><patternFill patternType="none"/></fill></fills>
  <borders count="1"><border/></borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
  <cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>
  <cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
</styleSheet>"#;

/// Excel column name for a zero-based column index (A, B, ..., Z, AA, ...).
fn excel_column(index: usize) -> String {
    let mut value = index + 1;
    let mut name = String::new();
    while value > 0 {
        let remainder = (value - 1) % 26;
        name.insert(0, (b'A' + remainder as u8) as char);
        value = (value - 1) / 26;
    }
    name
}

/// One `<c>` cell carrying an inline string.
fn inline_string_cell(reference: &str, text: &str) -> String {
    format!(
        r#"<c r="{reference}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
        xml_escape(text)
    )
}

/// The "Seating" grid sheet: a title row followed by one cell per seat.
fn xlsx_seating_sheet(grid: &SeatingGrid) -> String {
    let mut sheet = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
    );
    sheet.push_str(&format!(
        r#"<row r="1">{}</row>"#,
        inline_string_cell("A1", &grid.title)
    ));
    for row in grid.min_row..=grid.max_row {
        let row_index = row - grid.min_row + 3;
        let mut row_xml = format!(r#"<row r="{row_index}">"#);
        for col in grid.min_col..=grid.max_col {
            let reference = format!("{}{row_index}", excel_column((col - grid.min_col) as usize));
            let cell = grid
                .cells
                .iter()
                .find(|cell| cell.row == row && cell.col == col);
            let text = match cell {
                None => String::new(),
                Some(cell) if !cell.enabled => format!("{}\n--", seat_id_for(grid, cell)),
                Some(cell) => match &cell.student {
                    Some(student) => format!("{}\n{student}", seat_id_for(grid, cell)),
                    None => format!("{}\n", seat_id_for(grid, cell)),
                },
            };
            row_xml.push_str(&inline_string_cell(&reference, &text));
        }
        row_xml.push_str("</row>");
        sheet.push_str(&row_xml);
    }
    sheet.push_str("</sheetData></worksheet>");
    sheet
}

/// The "Assignments" sheet: `student_key, student_name, seat_id` rows.
fn xlsx_assignments_sheet(grid: &SeatingGrid) -> String {
    let mut sheet = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
    );
    sheet.push_str(
        r#"<row r="1"><c r="A1" t="inlineStr"><is><t>student_key</t></is></c><c r="B1" t="inlineStr"><is><t>student_name</t></is></c><c r="C1" t="inlineStr"><is><t>seat_id</t></is></c></row>"#,
    );
    for (offset, cell) in grid.cells.iter().enumerate() {
        let (Some(key), Some(student)) = (&cell.student_key, &cell.student) else {
            continue;
        };
        let row_index = 2 + offset;
        let seat_id = seat_id_for(grid, cell);
        sheet.push_str(&format!(
            r#"<row r="{row_index}">{}{}{}</row>"#,
            inline_string_cell(&format!("A{row_index}"), key),
            inline_string_cell(&format!("B{row_index}"), student),
            inline_string_cell(&format!("C{row_index}"), &seat_id),
        ));
    }
    sheet.push_str("</sheetData></worksheet>");
    sheet
}

/// Render the seating grid as a minimal XLSX workbook (two sheets).
pub fn render_xlsx(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    package(&[
        ("[Content_Types].xml", XLSX_CONTENT_TYPES),
        ("_rels/.rels", XLSX_ROOT_RELS),
        ("xl/workbook.xml", XLSX_WORKBOOK),
        ("xl/_rels/workbook.xml.rels", XLSX_WORKBOOK_RELS),
        ("xl/worksheets/sheet1.xml", &xlsx_seating_sheet(grid)),
        ("xl/worksheets/sheet2.xml", &xlsx_assignments_sheet(grid)),
        ("xl/styles.xml", XLSX_STYLES),
    ])
}

// ---------------------------------------------------------------------------
// DOCX
// ---------------------------------------------------------------------------

const DOCX_CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const DOCX_ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

/// A bordered seat grid table (mirrors the oracle's `Table Grid` style).
fn docx_seat_table(grid: &SeatingGrid) -> String {
    let mut table = String::from(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>\
<w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/>\
<w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/>\
<w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/>\
<w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/>\
<w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/>\
<w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/>\
</w:tblBorders></w:tblPr>"#,
    );
    // python-docx requires the <w:tblGrid> column definition.
    table.push_str("<w:tblGrid>");
    for _ in grid.min_col..=grid.max_col {
        table.push_str(r#"<w:gridCol w:w="1800"/>"#);
    }
    table.push_str("</w:tblGrid>");
    for row in grid.min_row..=grid.max_row {
        table.push_str("<w:tr>");
        for col in grid.min_col..=grid.max_col {
            let cell = grid
                .cells
                .iter()
                .find(|cell| cell.row == row && cell.col == col);
            let text = match cell {
                None => String::new(),
                Some(cell) if !cell.enabled => seat_id_for(grid, cell),
                Some(cell) => match &cell.student {
                    Some(student) if !student.is_empty() => student.clone(),
                    _ => seat_id_for(grid, cell),
                },
            };
            table.push_str(&format!(
                r#"<w:tc><w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
                xml_escape(&text)
            ));
        }
        table.push_str("</w:tr>");
    }
    table.push_str("</w:tbl>");
    table
}

/// Render the seating grid as a minimal DOCX document (title + meta + table).
pub fn render_docx(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{title}</w:t></w:r></w:p>
    <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:sz w:val="18"/></w:rPr><w:t xml:space="preserve">{subtitle}</w:t></w:r></w:p>
    <w:p/>
    {table}
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>
  </w:body>
</w:document>"#,
        title = xml_escape(&grid.title),
        subtitle = xml_escape(&grid.subtitle),
        table = docx_seat_table(grid),
    );
    package(&[
        ("[Content_Types].xml", DOCX_CONTENT_TYPES),
        ("_rels/.rels", DOCX_ROOT_RELS),
        ("word/document.xml", &document),
    ])
}

// ---------------------------------------------------------------------------
// PPTX
// ---------------------------------------------------------------------------

const PPTX_CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>"#;

const PPTX_ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#;

const PPTX_PRESENTATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
  <p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst>
  <p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>
  <p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#;

const PPTX_PRESENTATION_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>"#;

const PPTX_SLIDE_MASTER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
  </p:spTree></p:cSld>
  <p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
  <p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
  <p:txStyles>
    <p:titleStyle><a:lvl1pPr/></p:titleStyle>
    <p:bodyStyle><a:lvl1pPr/></p:bodyStyle>
    <p:otherStyle><a:lvl1pPr/></p:otherStyle>
  </p:txStyles>
</p:sldMaster>"#;

const PPTX_SLIDE_MASTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#;

const PPTX_SLIDE_LAYOUT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1">
  <p:cSld name="Blank"><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
  </p:spTree></p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sldLayout>"#;

const PPTX_SLIDE_LAYOUT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#;

/// One slide shape: `name`, `bounds` (`x/y/cx/cy` in EMU), optional rounded
/// rectangle, and one text run per line.
fn pptx_shape(
    id: u32,
    name: &str,
    bounds: (i64, i64, i64, i64),
    rounded: bool,
    lines: &[&str],
) -> String {
    let (x, y, cx, cy) = bounds;
    let geometry = if rounded {
        r#"<a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom>"#
    } else {
        r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#
    };
    let paragraphs = lines
        .iter()
        .map(|line| {
            format!(
                r#"<a:p><a:r><a:rPr lang="en-US" sz="1400" b="0"/><a:t xml:space="preserve">{}</a:t></a:r></a:p>"#,
                xml_escape(line)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>{geometry}<a:solidFill><a:srgbClr val="EAF4FF"/></a:solidFill></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#,
        name = xml_escape(name)
    )
}

/// Render the seating grid as a minimal 16:9 PPTX deck with one slide.
pub fn render_pptx(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    // 16:9 slide in EMU: 12192000 x 6858000. The grid is letterboxed: 5% margin.
    let margin = 609_600_i64;
    let width = 12_192_000_i64 - 2 * margin;
    let height = 6_858_000_i64 - 2 * margin;
    let rows = (grid.max_row - grid.min_row + 1).max(1) as i64;
    let cols = (grid.max_col - grid.min_col + 1).max(1) as i64;
    let cell_w = width / cols;
    let cell_h = height / rows;

    let mut shapes = String::new();
    // Title shape at the top (shape id 2; seat shapes follow from 3).
    shapes.push_str(&pptx_shape(
        2,
        "Title",
        (margin, 0, width, 400_000),
        false,
        &[&grid.title],
    ));
    for (offset, cell) in grid.cells.iter().enumerate() {
        let shape_id = 3 + offset as u32;
        let x = margin + (cell.col as i64 - grid.min_col as i64) * cell_w;
        let y = margin / 2 + (cell.row as i64 - grid.min_row as i64) * cell_h;
        let mut lines = vec![seat_id_for(grid, cell)];
        if cell.enabled {
            if let Some(student) = &cell.student {
                lines.push(student.clone());
            }
        } else {
            lines.push("--".to_string());
        }
        let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
        shapes.push_str(&pptx_shape(
            shape_id,
            &seat_id_for(grid, cell),
            (x, y, cell_w - 10_000, cell_h - 10_000),
            true,
            &refs,
        ));
    }

    let slide = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree>
    <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
    <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
    {shapes}
  </p:spTree></p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>"#
    );

    package(&[
        ("[Content_Types].xml", PPTX_CONTENT_TYPES),
        ("_rels/.rels", PPTX_ROOT_RELS),
        ("ppt/presentation.xml", PPTX_PRESENTATION),
        ("ppt/_rels/presentation.xml.rels", PPTX_PRESENTATION_RELS),
        ("ppt/slideMasters/slideMaster1.xml", PPTX_SLIDE_MASTER),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            PPTX_SLIDE_MASTER_RELS,
        ),
        ("ppt/slideLayouts/slideLayout1.xml", PPTX_SLIDE_LAYOUT),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            PPTX_SLIDE_LAYOUT_RELS,
        ),
        ("ppt/slides/slide1.xml", &slide),
        ("ppt/slides/_rels/slide1.xml.rels", PPTX_SLIDE_RELS),
    ])
}

const PPTX_SLIDE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#;

/// Recover the seat id (`R{row}C{col}`, matching the SVG/HTML renderers).
fn seat_id_for(grid: &SeatingGrid, cell: &crate::render::GridCell) -> String {
    let _ = grid;
    format!("R{}C{}", cell.row, cell.col)
}

// ---------------------------------------------------------------------------
// Tests: independent structural validation (修订版 §11.6)
//
// The acceptance criterion for Office formats is that an *independent*
// reader can reopen the bytes. Here the zip container is unpacked and the
// XML parts are parsed with quick-xml (a different implementation than the
// writer); the Python-side harness additionally opens the same files with
// openpyxl / python-docx / python-pptx (see scripts/rust_python_diff.py
// `--exports`).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::GridCell;
    use quick_xml::Reader;

    fn sample_grid() -> SeatingGrid {
        SeatingGrid {
            title: "Class 8-3".to_string(),
            subtitle: "4 students / 6 seats / feasible".to_string(),
            min_row: 1,
            max_row: 2,
            min_col: 1,
            max_col: 3,
            cells: vec![
                GridCell {
                    row: 1,
                    col: 1,
                    seat_index: 0,
                    student: Some("Alice".to_string()),
                    student_key: Some("S1".to_string()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 1,
                    col: 2,
                    seat_index: 1,
                    student: Some("Bob".to_string()),
                    student_key: Some("S2".to_string()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 1,
                    col: 3,
                    seat_index: 2,
                    student: Some("Carol".to_string()),
                    student_key: Some("S3".to_string()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 2,
                    col: 1,
                    seat_index: 3,
                    student: None,
                    student_key: None,
                    detail: None,
                    enabled: false,
                },
                GridCell {
                    row: 2,
                    col: 2,
                    seat_index: 4,
                    student: None,
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    row: 2,
                    col: 3,
                    seat_index: 5,
                    student: None,
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
            ],
        }
    }

    fn unzip(bytes: &[u8]) -> std::collections::HashMap<String, String> {
        let reader = std::io::Cursor::new(bytes.to_vec());
        let mut archive = zip::ZipArchive::new(reader).expect("zip opens");
        let mut entries = std::collections::HashMap::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).expect("entry opens");
            let name = file.name().to_string();
            let mut content = String::new();
            std::io::Read::read_to_string(&mut file, &mut content).expect("entry reads");
            entries.insert(name, content);
        }
        entries
    }

    fn assert_well_formed_xml(content: &str, what: &str) {
        let mut reader = Reader::from_str(content);
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("{what} is not well-formed XML: {error}"),
            }
        }
    }

    #[test]
    fn xlsx_package_is_well_formed_and_carries_both_sheets() {
        let bytes = render_xlsx(&sample_grid()).expect("xlsx renders");
        let entries = unzip(&bytes);
        for part in [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
            "xl/styles.xml",
        ] {
            let content = entries
                .get(part)
                .unwrap_or_else(|| panic!("missing part {part}"));
            assert_well_formed_xml(content, part);
        }
        let seating = &entries["xl/worksheets/sheet1.xml"];
        assert!(seating.contains("Class 8-3"), "title cell");
        assert!(seating.contains("R1C1"), "seat id in grid");
        assert!(seating.contains("Alice"), "student name in grid");
        // Disabled seat marker (mirrors the oracle's `seat_id\n--`).
        assert!(seating.contains("R2C1&#10;--") || seating.contains("R2C1\n--"));
        let assignments = &entries["xl/worksheets/sheet2.xml"];
        assert!(assignments.contains("student_key"));
        assert!(assignments.contains("S1"));
        assert!(assignments.contains("Alice"));
        // Sheet names in the workbook part.
        assert!(entries["xl/workbook.xml"].contains("Seating"));
        assert!(entries["xl/workbook.xml"].contains("Assignments"));
    }

    #[test]
    fn docx_package_is_well_formed_and_carries_title_and_table() {
        let bytes = render_docx(&sample_grid()).expect("docx renders");
        let entries = unzip(&bytes);
        for part in ["[Content_Types].xml", "_rels/.rels", "word/document.xml"] {
            let content = entries
                .get(part)
                .unwrap_or_else(|| panic!("missing part {part}"));
            assert_well_formed_xml(content, part);
        }
        let document = &entries["word/document.xml"];
        assert!(document.contains("Class 8-3"), "title paragraph");
        assert!(document.contains("Alice"), "student in table cell");
        assert!(document.contains("R2C2"), "empty seat shows seat id");
        assert!(document.contains("<w:tbl>"), "grid table present");
    }

    #[test]
    fn pptx_package_is_well_formed_and_carries_editable_seat_shapes() {
        let bytes = render_pptx(&sample_grid()).expect("pptx renders");
        let entries = unzip(&bytes);
        for part in [
            "[Content_Types].xml",
            "_rels/.rels",
            "ppt/presentation.xml",
            "ppt/_rels/presentation.xml.rels",
            "ppt/slideMasters/slideMaster1.xml",
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            "ppt/slideLayouts/slideLayout1.xml",
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            "ppt/slides/slide1.xml",
            "ppt/slides/_rels/slide1.xml.rels",
        ] {
            let content = entries
                .get(part)
                .unwrap_or_else(|| panic!("missing part {part}"));
            assert_well_formed_xml(content, part);
        }
        let slide = &entries["ppt/slides/slide1.xml"];
        assert!(slide.contains("Class 8-3"), "title shape");
        assert!(slide.contains("Alice"), "student in seat shape");
        assert!(
            slide.contains(r#"prstGeom prst="roundRect""#),
            "rounded seat shape"
        );
        // 16:9 slide size (修订版 §5.6 单页 16:9).
        assert!(
            entries["ppt/presentation.xml"]
                .contains(r#"<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>"#),
            "slide size must be screen16x9"
        );
    }

    #[test]
    fn xlsx_escapes_xml_special_characters() {
        let mut grid = sample_grid();
        grid.cells[0].student = Some("A&B <C>".to_string());
        let bytes = render_xlsx(&grid).expect("xlsx renders");
        let entries = unzip(&bytes);
        let seating = &entries["xl/worksheets/sheet1.xml"];
        assert!(seating.contains("A&amp;B &lt;C&gt;"), "escaped text");
        assert!(
            !seating.contains("<A&B"),
            "raw special characters must not appear"
        );
        assert_well_formed_xml(seating, "sheet1.xml");
    }
}
