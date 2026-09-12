//! Controlled OOXML writers for editable seating exports.
//!
//! XLSX has a styled, printable seating sheet and a separate assignment data
//! sheet. DOCX uses an explicitly sized table with real gutters and paper-aware
//! geometry. PPTX maps the shared print scene to editable shapes on one 16:9
//! slide, keeping the same proportions, text hierarchy and positions as PDF.
//! Office formats declare regular sans-serif Latin and East Asian families;
//! the reader may substitute installed fonts because these files do not embed
//! proprietary system fonts.
//!
//! The grid passed in is already privacy-filtered by the export domain
//! module (`anonymize_grid` / `filter_detail_grid`), so public exports never
//! carry real student names into the Office documents.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::render::{GridCell, PdfLayout, SeatingGrid};

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
  <fonts count="5">
    <font><sz val="11"/><color rgb="FF232522"/><name val="Microsoft YaHei"/><family val="2"/><charset val="134"/></font>
    <font><b/><sz val="20"/><color rgb="FF232522"/><name val="Microsoft YaHei"/><family val="2"/><charset val="134"/></font>
    <font><sz val="9"/><color rgb="FF676B63"/><name val="Microsoft YaHei"/><family val="2"/><charset val="134"/></font>
    <font><b/><sz val="12"/><color rgb="FF232522"/><name val="Microsoft YaHei"/><family val="2"/><charset val="134"/></font>
    <font><sz val="10"/><color rgb="FF928C82"/><name val="Microsoft YaHei"/><family val="2"/><charset val="134"/></font>
  </fonts>
  <fills count="5"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FFF6F7F2"/><bgColor indexed="64"/></patternFill></fill><fill><patternFill patternType="solid"><fgColor rgb="FFEAEBE7"/><bgColor indexed="64"/></patternFill></fill><fill><patternFill patternType="solid"><fgColor rgb="FFFFFFFF"/><bgColor indexed="64"/></patternFill></fill></fills>
  <borders count="3"><border/><border><left style="thin"><color rgb="FFB2B8AB"/></left><right style="thin"><color rgb="FFB2B8AB"/></right><top style="thin"><color rgb="FFB2B8AB"/></top><bottom style="thin"><color rgb="FFB2B8AB"/></bottom></border><border><left style="dashed"><color rgb="FFB2B8AB"/></left><right style="dashed"><color rgb="FFB2B8AB"/></right><top style="dashed"><color rgb="FFB2B8AB"/></top><bottom style="dashed"><color rgb="FFB2B8AB"/></bottom></border></borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
  <cellXfs count="8">
    <xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
    <xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment horizontal="center" vertical="center" shrinkToFit="1"/></xf>
    <xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="3" fillId="2" borderId="1" xfId="0" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="4" fillId="3" borderId="1" xfId="0" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="2" fillId="4" borderId="2" xfId="0" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="3" fillId="2" borderId="1" xfId="0" applyAlignment="1"><alignment vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="0" fillId="0" borderId="1" xfId="0" applyAlignment="1"><alignment vertical="center" wrapText="1"/></xf>
  </cellXfs>
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

/// Excel's hard limit for one cell's string value, in characters (R1):
/// longer inline strings produce a workbook that independent readers
/// (openpyxl / Excel) refuse to open, so values are truncated instead.
const MAX_CELL_CHARS: usize = 32_767;

const XLSX_TRUNCATION_WARNING: &str = "Some spreadsheet cell values were shortened to Excel's 32767-character limit. Check the source roster for complete values.";
const DOCX_TRUNCATION_WARNING: &str = "Some text in the Word document was shortened to fit. Use a larger paper size or the editable spreadsheet to see complete values.";
const DOCX_SMALL_NAMES_WARNING: &str = "Some student names in the Word document are smaller than 8 pt. Use a larger paper size or landscape orientation for a more readable chart.";

fn warn_once(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|existing| existing == warning) {
        warnings.push(warning.to_string());
    }
}

/// Clamp a cell value to [`MAX_CELL_CHARS`] characters, marking the cut with
/// an ellipsis so the truncation is visible rather than silent.
fn truncate_cell_text(text: &str) -> std::borrow::Cow<'_, str> {
    if text.chars().count() <= MAX_CELL_CHARS {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut truncated: String = text.chars().take(MAX_CELL_CHARS - 1).collect();
    truncated.push('…');
    std::borrow::Cow::Owned(truncated)
}

/// One `<c>` cell carrying an inline string.
fn styled_string_cell(
    reference: &str,
    text: &str,
    style: u8,
    warnings: &mut Vec<String>,
) -> String {
    let value = truncate_cell_text(text);
    if matches!(value, std::borrow::Cow::Owned(_)) {
        warn_once(warnings, XLSX_TRUNCATION_WARNING);
    }
    format!(
        r#"<c r="{reference}" s="{style}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
        xml_escape(&value)
    )
}

fn front_label(locale: &str) -> &'static str {
    if crate::render::is_zh_locale(locale) {
        "讲台 · 教室前方"
    } else {
        "FRONT OF ROOM"
    }
}

fn chart_summary(grid: &SeatingGrid, locale: &str) -> String {
    let students = grid
        .cells
        .iter()
        .filter(|cell| cell.student.is_some())
        .count();
    if crate::render::is_zh_locale(locale) {
        format!("{students} 名学生 · {} 个座位", grid.cells.len())
    } else {
        format!("{students} students · {} seats", grid.cells.len())
    }
}

fn seat_label<'a>(cell: &'a GridCell, locale: &str) -> &'a str {
    if !cell.enabled {
        if crate::render::is_zh_locale(locale) {
            "停用"
        } else {
            "Unavailable"
        }
    } else {
        cell.student
            .as_deref()
            .unwrap_or(if crate::render::is_zh_locale(locale) {
                "空座"
            } else {
                "Empty"
            })
    }
}

fn seat_metadata(cell: &GridCell) -> String {
    [cell.student_key.as_deref(), cell.detail.as_deref()]
        .into_iter()
        .flatten()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn xlsx_seat_cell(
    reference: &str,
    cell: &GridCell,
    locale: &str,
    style: u8,
    warnings: &mut Vec<String>,
) -> String {
    // Visual cells are previews; the complete (Excel-limit-clamped) values are
    // also available as plain data on Assignments. Bound the card's text so an
    // unusually long imported label cannot become an invisible wall of text.
    let label = seat_label(cell, locale);
    let name_size = crate::scene::fit_text(label, 104.0, 32.0, 12.0, 2).0;
    let name_size = name_size.max(5.0);
    let metadata = if cell.enabled {
        seat_metadata(cell)
    } else {
        String::new()
    };
    let lines = [
        (format!("{}\n", cell.seat_id), 8.0, "676B63"),
        (
            label.to_string(),
            name_size,
            if cell.student.is_some() {
                "232522"
            } else {
                "676B63"
            },
        ),
        (
            if metadata.is_empty() {
                String::new()
            } else {
                format!("\n\n{metadata}")
            },
            8.0,
            "676B63",
        ),
    ];
    let mut output = format!(r#"<c r="{reference}" s="{style}" t="inlineStr"><is>"#);
    let mut remaining = MAX_CELL_CHARS;
    for (text, size, color) in lines {
        if text.chars().count() > remaining {
            warn_once(warnings, XLSX_TRUNCATION_WARNING);
        }
        let value: String = text.chars().take(remaining).collect();
        remaining -= value.chars().count();
        if !value.is_empty() {
            output.push_str(&format!(r#"<r><rPr><rFont val="Microsoft YaHei"/><sz val="{size:.1}"/><color rgb="FF{color}"/><charset val="134"/></rPr><t xml:space="preserve">{}</t></r>"#, xml_escape(&value)));
        }
    }
    output.push_str("</is></c>");
    output
}

/// The visual sheet is deliberately a spreadsheet, with individually editable
/// seat cells and narrow spacer rows/columns. Missing coordinates stay blank.
fn xlsx_seating_sheet(grid: &SeatingGrid, locale: &str, warnings: &mut Vec<String>) -> String {
    let cols = (grid.max_col - grid.min_col + 1).max(1) as usize;
    let rows = (grid.max_row - grid.min_row + 1).max(1) as usize;
    let last_col = excel_column(cols * 2 - 2);
    let last_row = rows * 2 + 3;
    let mut sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetPr><pageSetUpPr fitToPage="1"/></sheetPr><dimension ref="A1:{last_col}{last_row}"/><sheetViews><sheetView showGridLines="0" workbookViewId="0"/></sheetViews><sheetFormatPr defaultRowHeight="18"/><cols>"#,
    );
    for column in 0..cols * 2 - 1 {
        let index = column + 1;
        let width = if column % 2 == 0 { 17.0 } else { 2.4 };
        sheet.push_str(&format!(
            r#"<col min="{index}" max="{index}" width="{width}" customWidth="1"/>"#
        ));
    }
    sheet.push_str("</cols><sheetData>");
    sheet.push_str(&format!(
        r#"<row r="1" ht="36" customHeight="1">{}</row><row r="2" ht="24" customHeight="1">{}</row><row r="3" ht="24" customHeight="1">{}</row><row r="4" ht="10" customHeight="1"/>"#,
        styled_string_cell("A1", &grid.title, 1, warnings),
        styled_string_cell("A2", &chart_summary(grid, locale), 2, warnings),
        styled_string_cell("A3", front_label(locale), 2, warnings),
    ));
    for row in grid.min_row..=grid.max_row {
        let row_index = (row - grid.min_row) * 2 + 5;
        let mut row_xml = format!(r#"<row r="{row_index}" ht="64" customHeight="1">"#);
        for col in grid.min_col..=grid.max_col {
            let reference = format!(
                "{}{row_index}",
                excel_column((col - grid.min_col) as usize * 2)
            );
            let Some(cell) = grid.cell_at(row, col) else {
                continue;
            };
            let style = if !cell.enabled {
                4
            } else if cell.student.is_none() {
                5
            } else {
                3
            };
            row_xml.push_str(&xlsx_seat_cell(&reference, cell, locale, style, warnings));
        }
        row_xml.push_str("</row>");
        sheet.push_str(&row_xml);
        if row < grid.max_row {
            sheet.push_str(&format!(
                r#"<row r="{}" ht="8" customHeight="1"/>"#,
                row_index + 1
            ));
        }
    }
    sheet.push_str("</sheetData>");
    if cols > 1 {
        sheet.push_str(&format!(r#"<mergeCells count="3"><mergeCell ref="A1:{last_col}1"/><mergeCell ref="A2:{last_col}2"/><mergeCell ref="A3:{last_col}3"/></mergeCells>"#));
    }
    sheet.push_str(r#"<printOptions horizontalCentered="1"/><pageMargins left="0.4" right="0.4" top="0.4" bottom="0.4" header="0.2" footer="0.2"/><pageSetup paperSize="9" orientation="landscape" fitToWidth="1" fitToHeight="1"/></worksheet>"#);
    sheet
}

/// Assignment data remains complete even when student identifiers are hidden.
fn xlsx_assignments_sheet(grid: &SeatingGrid, warnings: &mut Vec<String>) -> String {
    let mut sheet = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetPr><pageSetUpPr fitToPage="1"/></sheetPr><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><cols><col min="1" max="1" width="22" customWidth="1"/><col min="2" max="2" width="28" customWidth="1"/><col min="3" max="3" width="20" customWidth="1"/><col min="4" max="4" width="32" customWidth="1"/></cols><sheetData>"#,
    );
    sheet.push_str(r#"<row r="1" ht="28" customHeight="1">"#);
    for (index, label) in ["student_key", "student_name", "seat_id", "details"]
        .iter()
        .enumerate()
    {
        sheet.push_str(&styled_string_cell(
            &format!("{}1", excel_column(index)),
            label,
            6,
            warnings,
        ));
    }
    sheet.push_str("</row>");
    // Only seated cells become rows; the row numbers stay contiguous so
    // independent readers (openpyxl) see exactly one row per assignment.
    let seated: Vec<&crate::render::GridCell> = grid
        .cells
        .iter()
        .filter(|cell| cell.enabled && cell.student.is_some())
        .collect();
    for (row_index, cell) in seated.iter().enumerate() {
        let (key, student) = (
            cell.student_key.as_deref().unwrap_or_default(),
            cell.student.as_ref().expect("filtered"),
        );
        let row_index = 2 + row_index;
        let seat_id = &cell.seat_id;
        sheet.push_str(&format!(
            r#"<row r="{row_index}" ht="24" customHeight="1">{}{}{}{}</row>"#,
            styled_string_cell(&format!("A{row_index}"), key, 7, warnings),
            styled_string_cell(&format!("B{row_index}"), student, 7, warnings),
            styled_string_cell(&format!("C{row_index}"), seat_id, 7, warnings),
            styled_string_cell(
                &format!("D{row_index}"),
                cell.detail.as_deref().unwrap_or_default(),
                7,
                warnings,
            ),
        ));
    }
    sheet.push_str(&format!(
        "</sheetData><autoFilter ref=\"A1:D{}\"/><pageMargins left=\"0.4\" right=\"0.4\" top=\"0.4\" bottom=\"0.4\" header=\"0.2\" footer=\"0.2\"/><pageSetup paperSize=\"9\" orientation=\"portrait\" fitToWidth=\"1\" fitToHeight=\"0\"/></worksheet>",
        seated.len() + 1
    ));
    sheet
}

/// Render the seating grid as a minimal XLSX workbook (two sheets).
pub fn render_xlsx(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    render_xlsx_with(grid, "zh")
}

pub fn render_xlsx_with(grid: &SeatingGrid, locale: &str) -> Result<Vec<u8>, String> {
    render_xlsx_with_warnings(grid, locale).map(|(bytes, _)| bytes)
}

/// Spreadsheet warnings describe actual cell truncation, not the paper-based
/// chart preview. Values that merely exceed a visual card remain on Assignments.
pub fn render_xlsx_with_warnings(
    grid: &SeatingGrid,
    locale: &str,
) -> Result<(Vec<u8>, Vec<String>), String> {
    // A spacer column separates adjacent seats; never emit a workbook outside
    // Excel's 16,384-column limit, even for a crafted single-row layout.
    if i64::from(grid.max_col) - i64::from(grid.min_col) + 1 > 8192 {
        return Err("XLSX seating chart is wider than 8192 columns; choose another format or a smaller layout".to_string());
    }
    let last_col = excel_column((grid.max_col - grid.min_col).max(0) as usize * 2);
    let last_row = (grid.max_row - grid.min_row).max(0) as usize * 2 + 5;
    let workbook = XLSX_WORKBOOK.replace("</workbook>", &format!(r#"<definedNames><definedName name="_xlnm.Print_Area" localSheetId="0">'Seating'!$A$1:${last_col}${last_row}</definedName></definedNames></workbook>"#));
    let mut warnings = Vec::new();
    let seating = xlsx_seating_sheet(grid, locale, &mut warnings);
    let assignments = xlsx_assignments_sheet(grid, &mut warnings);
    let bytes = package(&[
        ("[Content_Types].xml", XLSX_CONTENT_TYPES),
        ("_rels/.rels", XLSX_ROOT_RELS),
        ("xl/workbook.xml", &workbook),
        ("xl/_rels/workbook.xml.rels", XLSX_WORKBOOK_RELS),
        ("xl/worksheets/sheet1.xml", &seating),
        ("xl/worksheets/sheet2.xml", &assignments),
        ("xl/styles.xml", XLSX_STYLES),
    ])?;
    Ok((bytes, warnings))
}

// ---------------------------------------------------------------------------
// DOCX
// ---------------------------------------------------------------------------

const DOCX_CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/>
</Types>"#;

const DOCX_ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCX_DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/>
</Relationships>"#;

const DOCX_STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault><w:rPr><w:rFonts w:ascii="Aptos" w:hAnsi="Aptos" w:eastAsia="Microsoft YaHei" w:cs="Arial"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="en-US" w:eastAsia="zh-CN"/></w:rPr></w:rPrDefault>
    <w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
</w:styles>"#;

const DOCX_FONT_TABLE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:font w:name="Aptos"><w:family w:val="swiss"/><w:pitch w:val="variable"/></w:font>
  <w:font w:name="Microsoft YaHei"><w:charset w:val="86"/><w:family w:val="swiss"/><w:pitch w:val="variable"/></w:font>
</w:fonts>"#;

fn docx_paragraph(text: &str, size: f64, color: &str, bold: bool, align: &str) -> String {
    let half_points = (size * 2.0).round().max(2.0) as u32;
    let line_twips = (size * 25.0).ceil().max(20.0) as u32;
    let weight = if bold { "<w:b/>" } else { "" };
    format!(
        r#"<w:p><w:pPr><w:jc w:val="{align}"/><w:spacing w:before="0" w:after="0" w:line="{line_twips}" w:lineRule="exact"/><w:rPr><w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/></w:rPr></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Aptos" w:hAnsi="Aptos" w:eastAsia="Microsoft YaHei" w:cs="Arial"/><w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/><w:color w:val="{color}"/>{weight}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn docx_spacer(width: i64) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:line="20" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:sz w:val="2"/></w:rPr><w:t/></w:r></w:p></w:tc>"#
    )
}

/// Compare the actual fitted lines, ignoring layout-only whitespace changes.
/// Checking only for a final ellipsis misses truncation when the source already
/// ends in one. Never include the source text in a quality diagnostic.
fn record_docx_fit(
    source: &str,
    lines: &[String],
    size: f64,
    student_name: bool,
    warnings: &mut Vec<String>,
) {
    if source
        .chars()
        .filter(|character| !character.is_whitespace())
        .ne(lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|character| !character.is_whitespace()))
    {
        warn_once(warnings, DOCX_TRUNCATION_WARNING);
    }
    // IDs and optional metadata intentionally have a smaller secondary scale.
    // The readability threshold applies to the names teachers need to read.
    if student_name && !source.trim().is_empty() && size < 8.0 {
        warn_once(warnings, DOCX_SMALL_NAMES_WARNING);
    }
}

/// A fixed-width table with explicit gutters. A missing seat is not a bordered
/// cell, and no seat row can split across pages. Geometry uses the actual paper.
fn docx_seat_table(
    grid: &SeatingGrid,
    layout: &PdfLayout,
    locale: &str,
    warnings: &mut Vec<String>,
) -> String {
    let cols = i64::from(grid.max_col - grid.min_col + 1).max(1);
    let rows = i64::from(grid.max_row - grid.min_row + 1).max(1);
    let available_w = ((layout.page_w - 2.0 * layout.margin_pt) * 20.0)
        .round()
        .max(1.0) as i64;
    let available_h = ((layout.page_h - 2.0 * layout.margin_pt - 100.0) * 20.0).max(1.0);
    let scale = layout.scale_multiplier.clamp(0.5, 1.0);
    let unit_w = available_w as f64 / (cols as f64 * 134.0 - 18.0);
    let unit_h = available_h / (rows as f64 * 88.0 - 18.0);
    let unit = unit_w.min(unit_h) * scale;
    let seat_w = (116.0 * unit).round().max(1.0) as i64;
    let gap = (18.0 * unit).round().max(1.0) as i64;
    let seat_h = (70.0 * unit).round().max(1.0) as i64;
    let table_w = seat_w * cols + gap * (cols - 1);
    // A long name only affects its own seat, never every name in the class.
    let name_cap = (18.0 * unit / 20.0).min(24.0);
    let small_size = (8.0 * unit / 20.0).clamp(1.0, 9.0);
    let text_width = (seat_w as f64 / 20.0 - 8.0).max(1.0);
    let mut table = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="{table_w}" w:type="dxa"/><w:jc w:val="center"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="40" w:type="dxa"/><w:left w:w="60" w:type="dxa"/><w:bottom w:w="40" w:type="dxa"/><w:right w:w="60" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid>"#
    );
    for index in 0..cols * 2 - 1 {
        let width = if index % 2 == 0 { seat_w } else { gap };
        table.push_str(&format!(r#"<w:gridCol w:w="{width}"/>"#));
    }
    table.push_str("</w:tblGrid>");
    for row in grid.min_row..=grid.max_row {
        table.push_str(&format!(
            r#"<w:tr><w:trPr><w:cantSplit/><w:trHeight w:val="{seat_h}" w:hRule="exact"/></w:trPr>"#
        ));
        for col in grid.min_col..=grid.max_col {
            if col > grid.min_col {
                table.push_str(&docx_spacer(gap));
            }
            let Some(cell) = grid.cell_at(row, col) else {
                table.push_str(&docx_spacer(seat_w));
                continue;
            };
            let fill = if !cell.enabled { "EAEBE7" } else { "F6F7F2" };
            let border = if cell.student.is_none() {
                "dashed"
            } else {
                "single"
            };
            table.push_str(&format!(
                r#"<w:tc><w:tcPr><w:tcW w:w="{seat_w}" w:type="dxa"/><w:tcBorders>"#
            ));
            for side in ["top", "left", "bottom", "right"] {
                table.push_str(&format!(
                    r#"<w:{side} w:val="{border}" w:sz="5" w:color="B2B8AB"/>"#
                ));
            }
            table.push_str(&format!(r#"</w:tcBorders><w:shd w:val="clear" w:fill="{fill}"/><w:vAlign w:val="center"/></w:tcPr>"#));
            let (id_size, id_lines) =
                crate::scene::fit_text(&cell.seat_id, text_width, small_size * 1.5, small_size, 1);
            record_docx_fit(&cell.seat_id, &id_lines, id_size, false, warnings);
            for line in id_lines {
                table.push_str(&docx_paragraph(&line, id_size, "676B63", false, "left"));
            }
            let (name_size, lines) = crate::scene::fit_text(
                seat_label(cell, locale),
                text_width,
                seat_h as f64 / 20.0 * 0.45,
                name_cap,
                2,
            );
            record_docx_fit(
                seat_label(cell, locale),
                &lines,
                name_size,
                cell.enabled && cell.student.is_some(),
                warnings,
            );
            for line in lines {
                table.push_str(&docx_paragraph(&line, name_size, "232522", false, "center"));
            }
            let metadata = if cell.enabled {
                seat_metadata(cell)
            } else {
                String::new()
            };
            if !metadata.is_empty() {
                let (size, lines) = crate::scene::fit_text(
                    &metadata,
                    (seat_w as f64 / 20.0 - 8.0).max(1.0),
                    small_size * 1.5,
                    small_size,
                    1,
                );
                record_docx_fit(&metadata, &lines, size, false, warnings);
                for line in lines {
                    table.push_str(&docx_paragraph(&line, size, "676B63", false, "center"));
                }
            }
            table.push_str("</w:tc>");
        }
        table.push_str("</w:tr>");
        if row < grid.max_row {
            table.push_str(&format!(r#"<w:tr><w:trPr><w:cantSplit/><w:trHeight w:val="{gap}" w:hRule="exact"/></w:trPr>"#));
            for index in 0..cols * 2 - 1 {
                table.push_str(&docx_spacer(if index % 2 == 0 { seat_w } else { gap }));
            }
            table.push_str("</w:tr>");
        }
    }
    table.push_str("</w:tbl>");
    table
}

/// Render the seating grid as a minimal DOCX document (title + meta + table).
pub fn render_docx(grid: &SeatingGrid, landscape: bool) -> Result<Vec<u8>, String> {
    let mut layout = if landscape {
        PdfLayout::landscape()
    } else {
        PdfLayout::portrait()
    };
    // Preserve the public wrapper's historical exact A4 dimensions in twips.
    (layout.page_w, layout.page_h) = if landscape {
        (841.9, 595.3)
    } else {
        (595.3, 841.9)
    };
    render_docx_with(grid, &layout, "zh")
}

pub fn render_docx_with(
    grid: &SeatingGrid,
    layout: &PdfLayout,
    locale: &str,
) -> Result<Vec<u8>, String> {
    render_docx_with_warnings(grid, layout, locale).map(|(bytes, _)| bytes)
}

/// Report fits performed by the actual Word table/page layout, which differs
/// from the SVG scene used to preview its seating arrangement.
pub fn render_docx_with_warnings(
    grid: &SeatingGrid,
    layout: &PdfLayout,
    locale: &str,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut warnings = Vec::new();
    let doc_w = (layout.page_w * 20.0).round() as i64;
    let doc_h = (layout.page_h * 20.0).round() as i64;
    let margin = (layout.margin_pt * 20.0).round() as i64;
    let (title_size, title_lines) = crate::scene::fit_text(
        &grid.title,
        (layout.page_w - 2.0 * layout.margin_pt).max(1.0),
        32.0,
        22.0,
        1,
    );
    record_docx_fit(&grid.title, &title_lines, title_size, false, &mut warnings);
    let title = title_lines
        .iter()
        .map(|line| docx_paragraph(line, title_size, "232522", true, "left"))
        .collect::<String>();
    let subtitle = docx_paragraph(&chart_summary(grid, locale), 10.0, "676B63", false, "left");
    let front = docx_paragraph(front_label(locale), 9.0, "676B63", false, "center");
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {title}{subtitle}<w:p><w:pPr><w:spacing w:line="120" w:lineRule="exact"/></w:pPr></w:p>{front}<w:p><w:pPr><w:spacing w:line="120" w:lineRule="exact"/></w:pPr></w:p>
    {table}
    <w:sectPr><w:pgSz w:w="{doc_w}" w:h="{doc_h}"/><w:pgMar w:top="{margin}" w:right="{margin}" w:bottom="{margin}" w:left="{margin}" w:header="360" w:footer="360" w:gutter="0"/></w:sectPr>
  </w:body>
</w:document>"#,
        table = docx_seat_table(grid, layout, locale, &mut warnings),
    );
    let bytes = package(&[
        ("[Content_Types].xml", DOCX_CONTENT_TYPES),
        ("_rels/.rels", DOCX_ROOT_RELS),
        ("word/document.xml", &document),
        ("word/_rels/document.xml.rels", DOCX_DOCUMENT_RELS),
        ("word/styles.xml", DOCX_STYLES),
        ("word/fontTable.xml", DOCX_FONT_TABLE),
    ])?;
    Ok((bytes, warnings))
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
  <Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
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
  <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
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
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
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

fn color_hex(color: crate::scene::Color) -> String {
    format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

fn pptx_bounds(bounds: crate::scene::Rect) -> String {
    let x = (bounds.x * 12_700.0).round() as i64;
    let y = (bounds.y * 12_700.0).round() as i64;
    let cx = (bounds.w * 12_700.0).round().max(0.0) as i64;
    let cy = (bounds.h * 12_700.0).round().max(0.0) as i64;
    format!(r#"<a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>"#)
}

/// Every scene element becomes an editable shape. Text has zero insets and
/// fixed, pre-fitted font sizes: Office must not shrink or reflow the chart.
fn pptx_element(id: u32, element: &crate::scene::Element, locale: &str) -> String {
    use crate::scene::{Element, TextAlign};
    let (properties, text, name) = match element {
        Element::Box {
            bounds,
            radius,
            fill,
            stroke,
            stroke_width,
            dashed,
        } => {
            let adjustment = (radius / bounds.w.min(bounds.h).max(0.01) * 100_000.0)
                .round()
                .clamp(0.0, 50_000.0) as u32;
            let line_width = (stroke_width * 12_700.0).round() as u32;
            let dash = if *dashed { "dash" } else { "solid" };
            (
                format!(
                    r#"{}<a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val {adjustment}"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:ln w="{line_width}"><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:prstDash val="{dash}"/></a:ln>"#,
                    pptx_bounds(*bounds),
                    color_hex(*fill),
                    color_hex(*stroke)
                ),
                String::new(),
                format!("Seat card {id}"),
            )
        }
        Element::Text {
            bounds,
            text,
            font_size,
            color,
            bold,
            align,
        } => {
            let size = (font_size * 100.0).round().clamp(100.0, 400_000.0) as u32;
            let weight = u8::from(*bold);
            let alignment = if *align == TextAlign::Center {
                "ctr"
            } else {
                "l"
            };
            let lang = if crate::render::is_zh_locale(locale) {
                "zh-CN"
            } else {
                "en-US"
            };
            let rpr = format!(r#"lang="{lang}" altLang="zh-CN" sz="{size}" b="{weight}""#);
            let runs = format!(
                r#"<a:r><a:rPr {rpr}><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:latin typeface="Aptos"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface="Arial"/></a:rPr><a:t xml:space="preserve">{}</a:t></a:r>"#,
                color_hex(*color),
                xml_escape(text)
            );
            let body = format!(
                r#"<p:txBody><a:bodyPr wrap="none" lIns="0" tIns="0" rIns="0" bIns="0" anchor="ctr"><a:noAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr algn="{alignment}"><a:spcBef><a:spcPts val="0"/></a:spcBef><a:spcAft><a:spcPts val="0"/></a:spcAft></a:pPr>{runs}<a:endParaRPr {rpr}/></a:p></p:txBody>"#
            );
            (
                format!(
                    r#"{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln>"#,
                    pptx_bounds(*bounds)
                ),
                body,
                text.clone(),
            )
        }
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{properties}</p:spPr>{text}</p:sp>"#,
        xml_escape(&name)
    )
}

const PPTX_THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="SeatTrellis"><a:themeElements>
<a:clrScheme name="SeatTrellis"><a:dk1><a:srgbClr val="232522"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="676B63"/></a:dk2><a:lt2><a:srgbClr val="F6F7F2"/></a:lt2><a:accent1><a:srgbClr val="6C795F"/></a:accent1><a:accent2><a:srgbClr val="B2B8AB"/></a:accent2><a:accent3><a:srgbClr val="A7B8C0"/></a:accent3><a:accent4><a:srgbClr val="C1B397"/></a:accent4><a:accent5><a:srgbClr val="A5A8C1"/></a:accent5><a:accent6><a:srgbClr val="BCA7A4"/></a:accent6><a:hlink><a:srgbClr val="356C89"/></a:hlink><a:folHlink><a:srgbClr val="6B577E"/></a:folHlink></a:clrScheme>
<a:fontScheme name="SeatTrellis"><a:majorFont><a:latin typeface="Aptos"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface="Microsoft YaHei"/><a:cs typeface="Arial"/></a:minorFont></a:fontScheme>
<a:fmtScheme name="SeatTrellis"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>
</a:themeElements></a:theme>"#;

/// Render the seating grid as a minimal 16:9 PPTX deck with one slide.
pub fn render_pptx(grid: &SeatingGrid) -> Result<Vec<u8>, String> {
    render_pptx_with(grid, "zh")
}

pub fn render_pptx_with(grid: &SeatingGrid, locale: &str) -> Result<Vec<u8>, String> {
    let mut layout = PdfLayout::landscape();
    layout.page_w = 960.0;
    layout.page_h = 540.0;
    layout.margin_pt = 24.0;
    let scene = crate::scene::build_scene(grid, &layout, locale);
    let shapes = scene
        .elements
        .iter()
        .enumerate()
        .map(|(index, element)| pptx_element(index as u32 + 2, element, locale))
        .collect::<String>();

    let slide = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>
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
        ("ppt/theme/theme1.xml", PPTX_THEME),
    ])
}

const PPTX_SLIDE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#;

// ---------------------------------------------------------------------------
// Tests: independent structural validation (revised plan §11.6)
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
                    seat_id: "R1C1".to_string(),
                    row: 1,
                    col: 1,
                    seat_index: 0,
                    student: Some("Alice".to_string()),
                    student_key: Some("S1".to_string()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    seat_id: "R1C2".to_string(),
                    row: 1,
                    col: 2,
                    seat_index: 1,
                    student: Some("Bob".to_string()),
                    student_key: Some("S2".to_string()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    seat_id: "R1C3".to_string(),
                    row: 1,
                    col: 3,
                    seat_index: 2,
                    student: Some("Carol".to_string()),
                    student_key: Some("S3".to_string()),
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    seat_id: "R2C1".to_string(),
                    row: 2,
                    col: 1,
                    seat_index: 3,
                    student: None,
                    student_key: None,
                    detail: None,
                    enabled: false,
                },
                GridCell {
                    seat_id: "R2C2".to_string(),
                    row: 2,
                    col: 2,
                    seat_index: 4,
                    student: None,
                    student_key: None,
                    detail: None,
                    enabled: true,
                },
                GridCell {
                    seat_id: "R2C3".to_string(),
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

    /// The inline-string values written by this module (`<is><t …>text</t>`),
    /// recovered for cell-length assertions. Only the writer's own tag
    /// vocabulary is scanned, so a plain prefix search on `<t` is exact here.
    fn inline_cell_texts(xml: &str) -> Vec<String> {
        xml.split("</t>")
            .filter_map(|chunk| {
                let start = chunk.rfind("<t")?;
                let open_end = chunk[start..].find('>')? + start + 1;
                Some(chunk[open_end..].to_string())
            })
            .collect()
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
        assert!(seating.contains("R2C1\n"));
        assert!(seating.contains("停用"));
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
        let bytes = render_docx(&sample_grid(), false).expect("docx renders");
        let entries = unzip(&bytes);
        for part in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/_rels/document.xml.rels",
            "word/styles.xml",
            "word/fontTable.xml",
        ] {
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
        // 16:9 slide size (revised plan §5.6 single-page 16:9).
        assert!(
            entries["ppt/presentation.xml"]
                .contains(r#"<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>"#),
            "slide size must be screen16x9"
        );
    }

    #[test]
    fn docx_fits_long_values_locally_without_shrinking_other_names() {
        let mut grid = sample_grid();
        let original = unzip(&render_docx(&grid, true).unwrap());
        let alice = original["word/document.xml"]
            .split("</w:p>")
            .find(|p| p.contains(">Alice</w:t>"))
            .unwrap();
        grid.cells[1].student = Some("Long Name ".repeat(40));
        grid.title = "很长的班级标题".repeat(28);
        grid.cells[1].seat_id = "seat-identifier-".repeat(40);
        let changed = unzip(&render_docx(&grid, true).unwrap());
        let document = &changed["word/document.xml"];
        assert!(
            document.contains(alice),
            "another name must not change Alice's paragraph"
        );
        assert!(
            !document.contains(&grid.title),
            "the title uses the fitted line, not its overflowing original"
        );
        assert!(
            !document.contains(&grid.cells[1].seat_id),
            "seat identifiers are fitted too"
        );
        assert!(document.contains('…'));
        assert_well_formed_xml(document, "word/document.xml");
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

    #[test]
    fn xlsx_truncates_over_limit_cells_and_stays_well_formed() {
        // A 40_000-character name exceeds Excel's 32_767-char cell limit;
        // the raw value would corrupt the workbook for independent readers.
        let long_name = "名".repeat(40_000);
        let mut grid = sample_grid();
        grid.cells[0].student = Some(long_name.clone());
        grid.cells[0].student_key = Some("S1".to_string());

        let entries = unzip(&render_xlsx(&grid).expect("xlsx renders"));
        for part in ["xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml"] {
            assert_well_formed_xml(&entries[part], part);
            assert!(
                !entries[part].contains(&long_name),
                "{part} must not carry the untruncated value"
            );
            for text in inline_cell_texts(&entries[part]) {
                assert!(
                    text.chars().count() <= 32_767,
                    "{part} carries an over-limit cell ({})",
                    text.chars().count()
                );
            }
        }
        // The cut is marked with an ellipsis and keeps the head of the name.
        let expected_head: String = "名".repeat(32_766) + "…";
        let assignments = &entries["xl/worksheets/sheet2.xml"];
        assert!(
            inline_cell_texts(assignments).contains(&expected_head),
            "the truncated name (… suffix, 32767 chars) survives in the Assignments sheet"
        );
    }

    #[test]
    fn pptx_shape_extents_never_go_negative() {
        // With more than ~1100 columns the 10_000 EMU gap exceeds the cell
        // width; the shape extent must clamp to zero instead of emitting a
        // negative a:ext (which would corrupt the slide).
        let mut grid = sample_grid();
        grid.max_col = 1100;
        grid.min_col = 1;
        grid.cells.clear();
        for col in 1..=1100 {
            grid.cells.push(GridCell {
                seat_id: format!("seat-{col}"),
                row: 1,
                col,
                seat_index: (col - 1) as usize,
                student: None,
                student_key: None,
                detail: None,
                enabled: true,
            });
        }
        let bytes = render_pptx(&grid).expect("pptx renders");
        let entries = unzip(&bytes);
        let slide = &entries["ppt/slides/slide1.xml"];
        assert!(
            !slide.contains("cx=\"-") && !slide.contains("cy=\"-"),
            "negative shape extents must be clamped"
        );
        assert_well_formed_xml(slide, "slide1.xml");
    }

    #[test]
    fn office_formats_preserve_cjk_names_and_declare_east_asian_text() {
        let mut grid = sample_grid();
        grid.cells[0].student = Some("林晓雨".to_string());

        let xlsx = unzip(&render_xlsx(&grid).expect("xlsx renders"));
        assert!(xlsx["xl/worksheets/sheet1.xml"].contains("林晓雨"));
        assert!(xlsx["xl/worksheets/sheet2.xml"].contains("林晓雨"));
        assert!(xlsx["xl/styles.xml"].contains(r#"charset val="134""#));

        let docx = unzip(&render_docx(&grid, true).expect("docx renders"));
        let document = &docx["word/document.xml"];
        assert!(document.contains("林晓雨"));
        assert!(document.contains(r#"w:eastAsia="Microsoft YaHei""#));
        assert!(
            !document.contains("/>\\"),
            "no literal slash escapes in OOXML"
        );

        let pptx = unzip(&render_pptx(&grid).expect("pptx renders"));
        let slide = &pptx["ppt/slides/slide1.xml"];
        assert!(slide.contains("林晓雨"));
        assert!(slide.contains(r#"<a:ea typeface="Microsoft YaHei"/>"#));
        assert!(slide.contains(r#"lang="zh-CN""#));
    }

    fn attributes(xml: &str, tag: &str) -> Vec<std::collections::HashMap<String, String>> {
        let mut reader = Reader::from_str(xml);
        let mut found = Vec::new();
        loop {
            match reader.read_event().expect("valid XML") {
                quick_xml::events::Event::Start(event) | quick_xml::events::Event::Empty(event)
                    if event.name().as_ref() == tag =>
                {
                    found.push(
                        event
                            .attributes()
                            .map(|attribute| {
                                let attribute = attribute.expect("valid attribute");
                                (
                                    attribute.key.as_ref().to_string(),
                                    attribute
                                        .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                                        .expect("text attribute")
                                        .into_owned(),
                                )
                            })
                            .collect(),
                    );
                }
                quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
        found
    }

    #[test]
    fn xlsx_visual_sheet_has_explicit_geometry_and_blank_aisles() {
        let mut grid = sample_grid();
        grid.cells.retain(|cell| cell.col != 2);
        grid.cells[0].seat_id = "window-A".into();
        let entries = unzip(&render_xlsx_with(&grid, "en").unwrap());
        let sheet = &entries["xl/worksheets/sheet1.xml"];
        let cols = attributes(sheet, "col");
        assert_eq!(cols.len(), 5, "three seat coordinates with narrow gaps");
        assert_eq!(cols[0]["width"], "17");
        assert_eq!(cols[1]["width"], "2.4");
        assert!(attributes(sheet, "row")
            .iter()
            .any(|row| row.get("r").is_some_and(|value| value == "5")
                && row.get("ht").is_some_and(|value| value == "64")));
        assert!(
            !attributes(sheet, "c").iter().any(|cell| cell["r"] == "C5"),
            "missing seat column remains unbordered"
        );
        assert!(sheet.contains("window-A"));
        assert!(
            !sheet.contains("R1C1"),
            "do not regenerate the authoritative seat id"
        );
        assert!(sheet.contains("FRONT OF ROOM"));
        assert!(entries["xl/styles.xml"].contains(r#"wrapText="1""#));
        assert!(entries["xl/workbook.xml"].contains("'Seating'!$A$1:$E$7"));
        assert!(sheet.contains(r#"fitToWidth="1" fitToHeight="1""#));
    }

    #[test]
    fn xlsx_assignments_are_not_lost_when_student_ids_are_hidden() {
        let mut grid = sample_grid();
        for cell in &mut grid.cells {
            cell.student_key = None;
        }
        let entries = unzip(&render_xlsx(&grid).unwrap());
        let assignments = &entries["xl/worksheets/sheet2.xml"];
        for name in ["Alice", "Bob", "Carol"] {
            assert!(assignments.contains(name));
        }
        assert_eq!(attributes(assignments, "row").len(), 4);
        assert!(assignments.contains("R1C1"));
        assert!(!assignments.contains(">S1<"));
    }

    #[test]
    fn xlsx_rejects_a_formatted_sheet_beyond_excels_column_limit() {
        let mut grid = sample_grid();
        grid.max_col = 8193;
        assert!(render_xlsx(&grid).unwrap_err().contains("8192 columns"));
    }

    #[test]
    fn xlsx_warnings_follow_real_cell_limits_not_visual_name_fitting() {
        let mut grid = sample_grid();
        let long_name = "LongName".repeat(80);
        grid.cells[0].student = Some(long_name.clone());
        let (bytes, warnings) = render_xlsx_with_warnings(&grid, "en").unwrap();
        assert!(
            warnings.is_empty(),
            "complete values must not warn: {warnings:?}"
        );
        let entries = unzip(&bytes);
        assert!(entries["xl/worksheets/sheet2.xml"].contains(&long_name));

        let over_limit = "名".repeat(MAX_CELL_CHARS + 1);
        grid.cells[0].student = Some(over_limit.clone());
        grid.title = over_limit.clone();
        let (bytes, warnings) = render_xlsx_with_warnings(&grid, "zh").unwrap();
        assert_eq!(warnings, vec![XLSX_TRUNCATION_WARNING]);
        let entries = unzip(&bytes);
        assert!(!entries["xl/worksheets/sheet2.xml"].contains(&over_limit));
        assert!(entries["xl/worksheets/sheet2.xml"].contains('…'));
        assert!(
            !warnings[0].contains('名'),
            "warnings must not copy private values"
        );
    }

    #[test]
    fn xlsx_warns_when_combined_rich_text_exceeds_one_cell_limit() {
        let mut grid = sample_grid();
        // Each individual field fits Excel; the rendered card's combined
        // seat ID, name and metadata still exceeds a single cell's limit.
        grid.cells[0].seat_id = "s".repeat(MAX_CELL_CHARS - 2);
        let (bytes, warnings) = render_xlsx_with_warnings(&grid, "en").unwrap();
        assert_eq!(warnings, vec![XLSX_TRUNCATION_WARNING]);
        let assignments = &unzip(&bytes)["xl/worksheets/sheet2.xml"];
        assert!(assignments.contains(&grid.cells[0].seat_id));
        assert!(assignments.contains("Alice"));
    }

    #[test]
    fn docx_reports_actual_title_and_name_fits_without_private_text() {
        let mut grid = sample_grid();
        let (_, normal_warnings) =
            render_docx_with_warnings(&grid, &PdfLayout::portrait(), "en").unwrap();
        assert!(normal_warnings.is_empty(), "{normal_warnings:?}");
        grid.title = "标题".repeat(100);
        let (bytes, title_warnings) =
            render_docx_with_warnings(&grid, &PdfLayout::portrait(), "zh").unwrap();
        assert_eq!(title_warnings, vec![DOCX_TRUNCATION_WARNING]);
        assert!(!unzip(&bytes)["word/document.xml"].contains(&grid.title));

        grid.cells[0].student = Some(format!("{}…", "PrivateName".repeat(80)));
        let (bytes, warnings) =
            render_docx_with_warnings(&grid, &PdfLayout::portrait(), "en").unwrap();
        assert_eq!(
            warnings,
            vec![DOCX_TRUNCATION_WARNING, DOCX_SMALL_NAMES_WARNING]
        );
        let document = &unzip(&bytes)["word/document.xml"];
        assert!(document.contains('…'));
        assert!(!document.contains(grid.cells[0].student.as_ref().unwrap()));
        assert!(warnings
            .iter()
            .all(|warning| !warning.contains("PrivateName")));
    }

    #[test]
    fn docx_fit_diagnostics_ignore_whitespace_but_detect_existing_ellipsis() {
        let mut warnings = Vec::new();
        record_docx_fit(
            "Alice  Bob\nCarol",
            &["Alice Bob".into(), "Carol".into()],
            12.0,
            true,
            &mut warnings,
        );
        assert!(warnings.is_empty());
        record_docx_fit("Alice Bob…", &["Alice…".into()], 6.0, true, &mut warnings);
        assert_eq!(
            warnings,
            vec![DOCX_TRUNCATION_WARNING, DOCX_SMALL_NAMES_WARNING]
        );
    }

    #[test]
    fn docx_respects_paper_margins_and_fits_the_table_to_the_page() {
        let grid = sample_grid();
        for (paper, landscape, margin) in [
            (crate::render::PaperSize::A4, false, 12.0),
            (crate::render::PaperSize::A3, true, 20.0),
            (crate::render::PaperSize::Letter, true, 10.0),
        ] {
            let layout = PdfLayout::from_paper(paper, landscape, margin);
            let entries = unzip(&render_docx_with(&grid, &layout, "en").unwrap());
            let doc = &entries["word/document.xml"];
            let page = &attributes(doc, "w:pgSz")[0];
            assert_eq!(
                page["w:w"].parse::<i64>().unwrap(),
                (layout.page_w * 20.0).round() as i64
            );
            assert_eq!(
                page["w:h"].parse::<i64>().unwrap(),
                (layout.page_h * 20.0).round() as i64
            );
            assert_eq!(
                attributes(doc, "w:pgMar")[0]["w:left"]
                    .parse::<i64>()
                    .unwrap(),
                (layout.margin_pt * 20.0).round() as i64
            );
            let widths: Vec<i64> = attributes(doc, "w:gridCol")
                .iter()
                .map(|column| column["w:w"].parse().unwrap())
                .collect();
            assert_eq!(widths.len(), 5);
            assert!(
                widths.iter().sum::<i64>()
                    <= ((layout.page_w - layout.margin_pt * 2.0) * 20.0).round() as i64 + 5
            );
            assert!(widths[1] < widths[0]);
            assert!(doc.contains(r#"<w:tblLayout w:type="fixed"/>"#));
            assert_eq!(attributes(doc, "w:cantSplit").len(), 3);
            assert!(
                attributes(doc, "w:trHeight")
                    .iter()
                    .all(|row| row["w:hRule"] == "exact"),
                "Office must not expand pre-fitted seat rows into another page"
            );
            assert!(doc.contains("FRONT OF ROOM"));
            assert!(
                doc.contains(r#"<w:sz w:val="44"/>"#),
                "title is explicitly 22 pt"
            );
        }
    }

    #[test]
    fn office_exports_preserve_custom_ids_details_and_disabled_state() {
        let mut grid = sample_grid();
        grid.cells[0].seat_id = "窗边-A".into();
        grid.cells[0].detail = Some("165 cm · vision 4.8".into());
        for (entries, part) in [
            (
                unzip(&render_xlsx_with(&grid, "en").unwrap()),
                "xl/worksheets/sheet1.xml",
            ),
            (
                unzip(&render_docx_with(&grid, &PdfLayout::landscape(), "en").unwrap()),
                "word/document.xml",
            ),
            (
                unzip(&render_pptx_with(&grid, "en").unwrap()),
                "ppt/slides/slide1.xml",
            ),
        ] {
            let xml = &entries[part];
            assert!(xml.contains("窗边-A"), "{part} must retain the real id");
            assert!(!xml.contains("R1C1"));
            assert!(
                xml.contains("165 cm"),
                "{part} must render filtered details"
            );
            assert!(
                xml.contains("Unavailable"),
                "{part} must distinguish disabled seats"
            );
        }
    }

    #[test]
    fn pptx_follows_scene_coordinates_and_keeps_text_fixed() {
        let grid = sample_grid();
        let entries = unzip(&render_pptx_with(&grid, "en").unwrap());
        let slide = &entries["ppt/slides/slide1.xml"];
        let mut layout = PdfLayout::landscape();
        layout.page_w = 960.0;
        layout.page_h = 540.0;
        layout.margin_pt = 24.0;
        let scene = crate::scene::build_scene(&grid, &layout, "en");
        let offsets = attributes(slide, "a:off");
        let extents = attributes(slide, "a:ext");
        assert_eq!(
            offsets.len(),
            scene.elements.len() + 1,
            "one transform for each scene element plus the root group"
        );
        for (index, element) in scene.elements.iter().enumerate() {
            let bounds = match element {
                crate::scene::Element::Box { bounds, .. }
                | crate::scene::Element::Text { bounds, .. } => bounds,
            };
            let offset = &offsets[index + 1];
            let extent = &extents[index + 1];
            assert_eq!(
                offset["x"].parse::<i64>().unwrap(),
                (bounds.x * 12_700.0).round() as i64
            );
            assert_eq!(
                offset["y"].parse::<i64>().unwrap(),
                (bounds.y * 12_700.0).round() as i64
            );
            assert_eq!(
                extent["cx"].parse::<i64>().unwrap(),
                (bounds.w * 12_700.0).round() as i64
            );
            assert_eq!(
                extent["cy"].parse::<i64>().unwrap(),
                (bounds.h * 12_700.0).round() as i64
            );
        }
        let text_count = scene
            .elements
            .iter()
            .filter(|element| matches!(element, crate::scene::Element::Text { .. }))
            .count();
        assert_eq!(attributes(slide, "a:noAutofit").len(), text_count);
        assert!(attributes(slide, "a:bodyPr")
            .iter()
            .all(|body| body["lIns"] == "0" && body["tIns"] == "0" && body["wrap"] == "none"));
        assert!(attributes(slide, "a:rPr")
            .iter()
            .any(|properties| properties["sz"] == "2200"));
        assert!(entries.contains_key("ppt/theme/theme1.xml"));
        assert_well_formed_xml(&entries["ppt/theme/theme1.xml"], "theme");
    }
}
