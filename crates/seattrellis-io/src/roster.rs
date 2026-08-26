//! Roster import domain: bounded CSV/XLSX parsing, column-mapping suggestions,
//! and conflict-aware update previews.
//!
//! This is the self-contained Rust port of the Python roster pipeline
//! (`src/seattrellis/io/roster_table.py`, `application/roster_mapping.py`,
//! `application/roster_update.py`, and `api/rosters.py`). It exposes:
//!
//! * [`parse_roster_csv`] — a bounded, hand-rolled UTF-8 CSV reader plus
//!   automatic field-mapping suggestions. No third-party CSV dependency.
//! * [`parse_roster_xlsx`] — a bounded reader for the first worksheet of an
//!   OOXML workbook (`.xlsx`/`.xlsm`) built on the existing `zip` dependency,
//!   producing the same draft shape as the CSV reader.
//! * [`preview_roster_update`] — an incremental or full-replacement difference
//!   preview with the same identity rules and conflict codes as the Python
//!   implementation.
//! * [`RosterDraftStore`] — an in-memory, TTL-bounded draft map plus a global
//!   process-wide instance, so a loopback HTTP server can wire up drafts with
//!   a few JSON in/out helpers ([`upload_draft_json`], [`upload_draft_file_json`],
//!   [`preview_update_json`]).
//!
//! JSON shapes match `clients/web/src/api/types.ts` (`snake_case`):
//! `RosterDraftResponse` and `RosterUpdatePreviewResponse` are produced
//! field-for-field. Student records in responses are serialized with the
//! canonical `student_id` / `height_cm` fields; the request side additionally
//! accepts the frontend's camelCase `id` / `heightCm` aliases.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::ZipArchive;

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// Maximum accepted roster file size, in bytes (20 MiB).
pub const MAX_ROSTER_FILE_BYTES: usize = 20 * 1024 * 1024;
/// Maximum number of data rows parsed from one roster file.
pub const MAX_ROSTER_ROWS: usize = 10_000;
/// Maximum number of columns allowed in a roster file.
pub const MAX_ROSTER_COLUMNS: usize = 256;
/// Number of data rows included in a draft preview.
const PREVIEW_ROW_COUNT: usize = 5;

/// Default number of drafts kept in memory before the oldest is evicted.
const DEFAULT_MAX_DRAFTS: usize = 10;
/// Default draft lifetime before it is treated as expired, in seconds.
const DEFAULT_TTL_SECONDS: u64 = 2 * 60 * 60;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One canonical roster field that can be mapped to a source column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RosterField {
    StudentId,
    Name,
    Gender,
    HeightCm,
    Score,
    Vision,
    Tags,
    Needs,
    Notes,
}

impl RosterField {
    /// All roster fields in the canonical suggestion order.
    pub const ALL: [RosterField; 9] = [
        RosterField::StudentId,
        RosterField::Name,
        RosterField::Gender,
        RosterField::HeightCm,
        RosterField::Score,
        RosterField::Vision,
        RosterField::Tags,
        RosterField::Needs,
        RosterField::Notes,
    ];

    /// The stable `snake_case` wire name (also used for error messages).
    pub fn as_str(self) -> &'static str {
        match self {
            RosterField::StudentId => "student_id",
            RosterField::Name => "name",
            RosterField::Gender => "gender",
            RosterField::HeightCm => "height_cm",
            RosterField::Score => "score",
            RosterField::Vision => "vision",
            RosterField::Tags => "tags",
            RosterField::Needs => "needs",
            RosterField::Notes => "notes",
        }
    }
}

impl fmt::Display for RosterField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A physical source column, identified by position rather than header text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterColumnItem {
    pub index: usize,
    pub header: String,
}

/// One preview row of raw cell values (`string | number | boolean | null`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterPreviewRow {
    pub row_number: usize,
    pub cells: Vec<Value>,
}

/// An assignment of one canonical field to one source column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterMappingItem {
    pub field: RosterField,
    pub column_index: usize,
}

/// A stable, UI-friendly explanation of a mapping decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterMappingIssueItem {
    pub code: String,
    pub message: String,
    pub field: Option<RosterField>,
    pub column_indices: Vec<usize>,
}

/// The `RosterDraftResponse` wire shape from `types.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterDraftResponse {
    pub draft_id: String,
    pub source_format: String,
    pub headerless: bool,
    pub row_count: usize,
    pub column_count: usize,
    pub columns: Vec<RosterColumnItem>,
    pub preview_rows: Vec<RosterPreviewRow>,
    pub suggested_mapping: Vec<RosterMappingItem>,
    pub mapping_issues: Vec<RosterMappingIssueItem>,
}

/// A student record. Serialized with canonical `snake_case` fields; accepts the
/// frontend's camelCase `id` / `heightCm` aliases when deserializing the
/// `current_students` array of a preview request.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct Student {
    #[serde(alias = "id")]
    pub student_id: Option<String>,
    pub name: Option<String>,
    pub gender: Option<String>,
    #[serde(alias = "heightCm")]
    pub height_cm: Option<f64>,
    pub score: Option<f64>,
    pub vision: Option<VisionValue>,
    pub tags: Vec<String>,
    pub needs: Vec<String>,
    pub notes: Option<String>,
    pub attributes: HashMap<String, Value>,
}

/// `vision` is `string | number | null` on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VisionValue {
    Num(f64),
    Str(String),
}

/// The `RosterUpdatePreviewRequest` wire shape from `types.ts`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterUpdatePreviewRequest {
    pub mapping: Vec<RosterMappingItem>,
    #[serde(default)]
    pub current_students: Vec<Student>,
    #[serde(default)]
    pub current_revision: u32,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub updated_fields: Option<Vec<String>>,
}

fn default_mode() -> String {
    "incremental".to_string()
}

/// One visible field difference for an existing student.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterFieldChangeItem {
    pub field: String,
    pub before: Value,
    pub after: Value,
}

/// One row in an import difference preview.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterChangeItem {
    pub action: String,
    pub match_method: String,
    pub before: Option<Student>,
    pub after: Option<Student>,
    pub field_changes: Vec<RosterFieldChangeItem>,
    pub incoming_index: Option<usize>,
    pub existing_index: Option<usize>,
}

/// An identity ambiguity that must be resolved before applying an import.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterConflictItem {
    pub code: String,
    pub message: String,
    pub incoming_index: Option<usize>,
    pub existing_indices: Vec<usize>,
}

/// Deterministic per-action counts (`Record<string, number>` in `types.ts`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ActionCounts {
    pub add: usize,
    pub update: usize,
    pub unchanged: usize,
    pub remove: usize,
    pub conflict: usize,
}

/// The `RosterUpdatePreviewResponse` wire shape from `types.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterUpdatePreviewResponse {
    pub draft_id: String,
    pub base_revision: u32,
    pub mode: String,
    pub can_apply: bool,
    pub action_counts: ActionCounts,
    pub changes: Vec<RosterChangeItem>,
    pub conflicts: Vec<RosterConflictItem>,
    pub resulting_students: Option<Vec<Student>>,
}

// ---------------------------------------------------------------------------
// Stored draft
// ---------------------------------------------------------------------------

/// One raw roster data row with its source row number.
#[derive(Debug, Clone, PartialEq)]
pub struct RosterRow {
    pub row_number: usize,
    pub cells: Vec<String>,
}

/// A parsed, stored roster draft with its mapping suggestions.
#[derive(Debug, Clone)]
pub struct RosterDraft {
    pub draft_id: String,
    pub source_format: &'static str,
    pub headerless: bool,
    pub columns: Vec<RosterColumnItem>,
    /// Raw data rows (trimmed cells), including a promoted header row for
    /// headerless uploads.
    pub rows: Vec<RosterRow>,
    pub suggested_mapping: Vec<RosterMappingItem>,
    pub mapping_issues: Vec<RosterMappingIssueItem>,
}

impl RosterDraft {
    /// Number of data rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of source columns.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// The first [`PREVIEW_ROW_COUNT`] rows as JSON preview cells.
    pub fn preview_rows(&self) -> Vec<RosterPreviewRow> {
        self.rows
            .iter()
            .take(PREVIEW_ROW_COUNT)
            .map(|row| {
                let cells = self
                    .columns
                    .iter()
                    .map(|column| match row.cells.get(column.index) {
                        Some(cell) => Value::String(cell.clone()),
                        None => Value::Null,
                    })
                    .collect();
                RosterPreviewRow {
                    row_number: row.row_number,
                    cells,
                }
            })
            .collect()
    }

    /// Project this draft into the `RosterDraftResponse` wire shape.
    pub fn to_response(&self) -> RosterDraftResponse {
        RosterDraftResponse {
            draft_id: self.draft_id.clone(),
            source_format: self.source_format.to_string(),
            headerless: self.headerless,
            row_count: self.row_count(),
            column_count: self.column_count(),
            columns: self.columns.clone(),
            preview_rows: self.preview_rows(),
            suggested_mapping: self.suggested_mapping.clone(),
            mapping_issues: self.mapping_issues.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// CSV parsing
// ---------------------------------------------------------------------------

/// Parse roster-shaped CSV `bytes` (UTF-8, comma-separated) into a stored
/// draft with mapping suggestions.
///
/// Bounds are applied before and during parsing: the byte size is checked
/// first, then row and column counts. Only UTF-8 input is accepted. A leading
/// UTF-8 BOM is stripped. Fields are trimmed of surrounding whitespace, quotes
/// (with `""` escapes) and embedded newlines are honored, and blank trailing
/// records are not produced.
pub fn parse_roster_csv(bytes: &[u8]) -> Result<RosterDraft, String> {
    if bytes.len() > MAX_ROSTER_FILE_BYTES {
        return Err(format!(
            "Roster file is {} bytes; the limit is {} bytes.",
            bytes.len(),
            MAX_ROSTER_FILE_BYTES
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|error| format!("Roster CSV must be UTF-8 text: {error}"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let records = parse_csv(text)?;
    draft_from_records(records, "csv")
}

/// Build a stored draft from a table of raw cell strings (header row first),
/// shared by the CSV and XLSX readers so mapping, bounds, and headerless
/// detection behave identically for both formats. Rows are renumbered
/// sequentially with the header at row 1 (`row_number: index + 2`).
fn draft_from_records(
    records: Vec<Vec<String>>,
    source_format: &'static str,
) -> Result<RosterDraft, String> {
    if records.is_empty() {
        return Err("Roster is empty; a header row is required.".to_string());
    }
    let raw_headers = &records[0];
    if raw_headers.is_empty() {
        return Err("Roster has no columns in its header row.".to_string());
    }
    if raw_headers.len() > MAX_ROSTER_COLUMNS {
        return Err(format!(
            "Roster has {} columns; the limit is {}.",
            raw_headers.len(),
            MAX_ROSTER_COLUMNS
        ));
    }
    let headers: Vec<String> = raw_headers
        .iter()
        .map(|cell| cell.trim().to_string())
        .collect();

    let mut raw_rows: Vec<RosterRow> = Vec::new();
    for (index, record) in records.iter().skip(1).enumerate() {
        if raw_rows.len() >= MAX_ROSTER_ROWS {
            return Err(format!(
                "Roster has more than the allowed {MAX_ROSTER_ROWS} data rows."
            ));
        }
        if record.len() > MAX_ROSTER_COLUMNS {
            return Err(format!(
                "Roster has {} columns; the limit is {}.",
                record.len(),
                MAX_ROSTER_COLUMNS
            ));
        }
        if record.len() > headers.len() {
            return Err(format!(
                "Roster row {} has {} cells but the header has only {} columns.",
                index + 2,
                record.len(),
                headers.len()
            ));
        }
        let cells: Vec<String> = record.iter().map(|cell| cell.trim().to_string()).collect();
        raw_rows.push(RosterRow {
            row_number: index + 2,
            cells,
        });
    }

    let first_data = raw_rows.first();
    let headerless = match first_data {
        Some(first) => {
            !has_header_hints(&headers)
                && looks_like_record(&headers)
                && looks_like_record(&first.cells)
        }
        None => false,
    };

    let columns: Vec<RosterColumnItem> = if headerless {
        headers
            .iter()
            .enumerate()
            .map(|(index, _)| RosterColumnItem {
                index,
                header: format!("Column {}", index + 1),
            })
            .collect()
    } else {
        headers
            .iter()
            .enumerate()
            .map(|(index, header)| RosterColumnItem {
                index,
                header: header.clone(),
            })
            .collect()
    };

    let rows = if headerless {
        // The original header is promoted into row 1 so its values stay
        // previewable and positionally aligned with the generated columns.
        let mut promoted = vec![RosterRow {
            row_number: 1,
            cells: headers,
        }];
        promoted.extend(raw_rows);
        if promoted.len() > MAX_ROSTER_ROWS {
            return Err(format!(
                "Roster has more than the allowed {MAX_ROSTER_ROWS} data rows."
            ));
        }
        promoted
    } else {
        raw_rows
    };

    let (assignments, mapping_issues) = suggest_mapping(&columns, &rows, headerless);
    let suggested_mapping = assignments
        .into_iter()
        .map(|(field, column_index)| RosterMappingItem {
            field,
            column_index,
        })
        .collect();

    Ok(RosterDraft {
        draft_id: new_draft_id(),
        source_format,
        headerless,
        columns,
        rows,
        suggested_mapping,
        mapping_issues,
    })
}

/// A minimal, strict-ish CSV state machine over `&str`.
///
/// - Quoted fields span newlines and embedded commas; `""` is an escaped quote.
/// - After a closing quote only whitespace, a comma, or a newline may follow.
/// - `\r\n` and lone `\r` are treated as row terminators.
/// - A file ending in a newline does not produce an extra empty row.
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut field_started = false;
    let mut just_closed = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                    just_closed = true;
                }
            } else {
                field.push(ch);
            }
            continue;
        }

        if just_closed {
            match ch {
                ',' => {
                    record.push(std::mem::take(&mut field));
                    field_started = false;
                    just_closed = false;
                }
                '\r' | '\n' => {
                    if ch == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    field_started = false;
                    just_closed = false;
                }
                ' ' | '\t' => {}
                _ => {
                    return Err(format!("invalid character {ch:?} after a quoted CSV field"));
                }
            }
            continue;
        }

        match ch {
            '"' if !field_started => {
                in_quotes = true;
                field_started = true;
            }
            '"' => field.push('"'),
            ',' => {
                record.push(std::mem::take(&mut field));
                field_started = false;
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                field_started = false;
            }
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                field_started = false;
            }
            _ => {
                field.push(ch);
                field_started = true;
            }
        }
    }

    if in_quotes {
        return Err("unterminated quoted CSV field".to_string());
    }
    if field_started || !record.is_empty() || !field.is_empty() {
        record.push(std::mem::take(&mut field));
        records.push(std::mem::take(&mut record));
    }
    Ok(records)
}

// ---------------------------------------------------------------------------
// XLSX parsing
// ---------------------------------------------------------------------------

/// Package entry holding the workbook structure.
const WORKBOOK_ENTRY: &str = "xl/workbook.xml";
/// Package entry resolving workbook sheet `r:id`s to worksheet parts.
const WORKBOOK_RELS_ENTRY: &str = "xl/_rels/workbook.xml.rels";
/// Conventional first-worksheet entry used when the workbook metadata is
/// unreadable or missing.
const DEFAULT_SHEET_ENTRY: &str = "xl/worksheets/sheet1.xml";
/// Package entry holding shared strings, when present.
const SHARED_STRINGS_ENTRY: &str = "xl/sharedStrings.xml";

/// Parse roster-shaped XLSX/XLSM `bytes` (the first worksheet of an OOXML
/// workbook) into a stored draft with mapping suggestions.
///
/// The same bounds as the CSV reader apply: the container is capped at
/// [`MAX_ROSTER_FILE_BYTES`], each decompressed XML part at the same limit,
/// and data rows/columns at [`MAX_ROSTER_ROWS`] / [`MAX_ROSTER_COLUMNS`].
/// Shared (`t="s"`), inline (`t="inlineStr"`), formula-result (`t="str"`)
/// boolean (`t="b"`) and untyped numeric cells are supported; formulas are
/// only read through their cached result and rejected otherwise.
pub fn parse_roster_xlsx(bytes: &[u8]) -> Result<RosterDraft, String> {
    if bytes.len() > MAX_ROSTER_FILE_BYTES {
        return Err(format!(
            "Roster file is {} bytes; the limit is {} bytes.",
            bytes.len(),
            MAX_ROSTER_FILE_BYTES
        ));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Roster XLSX is not a valid OOXML archive: {error}"))?;
    let sheet_path = resolve_first_sheet_path(&mut archive)?;
    let shared_strings = load_shared_strings(&mut archive)?;
    let sheet_xml = read_xlsx_entry(&mut archive, &sheet_path)?;
    let records = parse_worksheet_records(&sheet_xml, &shared_strings)?;
    draft_from_records(records, "xlsx")
}

/// Read one required package entry as UTF-8 text, capped at
/// [`MAX_ROSTER_FILE_BYTES`] of *decompressed* output so a compressed bomb
/// cannot expand unboundedly in memory.
fn read_xlsx_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    match try_read_xlsx_entry(archive, name)? {
        Some(text) => Ok(text),
        None => Err(format!("Roster XLSX is missing the required entry {name}.")),
    }
}

/// Read one optional package entry; `None` when it does not exist.
fn try_read_xlsx_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<String>, String> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("Could not read Roster XLSX entry {name}: {error}")),
    };
    let limit = MAX_ROSTER_FILE_BYTES as u64;
    if file.size() > limit {
        return Err(format!(
            "Roster XLSX entry {name} is {} bytes; the limit is {} bytes.",
            file.size(),
            MAX_ROSTER_FILE_BYTES
        ));
    }
    // The declared size can lie; enforce the cap on real bytes too.
    let mut limited = file.take(limit.saturating_add(1));
    let mut buffer = Vec::new();
    limited
        .read_to_end(&mut buffer)
        .map_err(|error| format!("Could not read Roster XLSX entry {name}: {error}"))?;
    if buffer.len() > MAX_ROSTER_FILE_BYTES {
        return Err(format!(
            "Roster XLSX entry {name} expands to more than the allowed \
             {MAX_ROSTER_FILE_BYTES} bytes."
        ));
    }
    String::from_utf8(buffer)
        .map(Some)
        .map_err(|error| format!("Roster XLSX entry {name} is not valid UTF-8: {error}"))
}

/// Locate the package path of the first worksheet: follow the first
/// `<sheet>`'s relationship id through `xl/_rels/workbook.xml.rels`, fall back
/// to the first worksheet-typed relationship, then to `sheet1.xml`.
fn resolve_first_sheet_path(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<String, String> {
    let workbook = read_xlsx_entry(archive, WORKBOOK_ENTRY)?;
    let rels = try_read_xlsx_entry(archive, WORKBOOK_RELS_ENTRY)?.unwrap_or_default();

    for candidate in sheet_entry_candidates(&workbook, &rels) {
        if archive.by_name(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    if archive.by_name(DEFAULT_SHEET_ENTRY).is_ok() {
        return Ok(DEFAULT_SHEET_ENTRY.to_string());
    }
    Err(
        "Roster XLSX does not contain a readable worksheet; expected \
         xl/worksheets/sheet1.xml."
            .to_string(),
    )
}

/// Ordered worksheet-part candidates derived from workbook/relationship
/// metadata (external `http(s)` targets are ignored).
fn sheet_entry_candidates(workbook: &str, rels: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let relationship_id = xml_elements(workbook, "sheet")
        .first()
        .map(|element| &workbook[element.open_tag.0..element.open_tag.1])
        .and_then(|tag| xml_attr(tag, "id"));

    let mut push_relationship = |tag: &str| {
        if let Some(target) = xml_attr(tag, "target") {
            if let Some(path) = relationship_target(&target) {
                candidates.push(path);
                return true;
            }
        }
        false
    };

    if let Some(relationship_id) = relationship_id.as_deref() {
        for element in xml_elements(rels, "Relationship") {
            let tag = &rels[element.open_tag.0..element.open_tag.1];
            if xml_attr(tag, "id").as_deref() == Some(relationship_id) && push_relationship(tag) {
                return candidates;
            }
        }
    }
    for element in xml_elements(rels, "Relationship") {
        let tag = &rels[element.open_tag.0..element.open_tag.1];
        let is_worksheet = xml_attr(tag, "type")
            .map(|relationship_type| relationship_type.ends_with("/worksheet"))
            .unwrap_or(false);
        if is_worksheet && push_relationship(tag) {
            return candidates;
        }
    }
    candidates
}

/// Resolve a relationship target to a package-absolute entry path relative to
/// the `xl/` part directory. Returns `None` for unsupported external targets.
fn relationship_target(target: &str) -> Option<String> {
    let joined = if let Some(absolute) = target.strip_prefix('/') {
        absolute.to_string()
    } else if target.contains("://") || target.starts_with("//") {
        return None;
    } else {
        format!("xl/{target}")
    };
    Some(
        joined
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Load the shared-string table (empty when the part is absent).
fn load_shared_strings(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<String>, String> {
    match try_read_xlsx_entry(archive, SHARED_STRINGS_ENTRY)? {
        Some(xml) => Ok(xml_elements(&xml, "si")
            .iter()
            .map(|element| xml_concat_t_text(&xml[element.inner.0..element.inner.1]))
            .collect()),
        None => Ok(Vec::new()),
    }
}

/// Convert worksheet XML into raw records (header row first). Empty rows —
/// including the ghost rows spreadsheet apps append — are skipped, gaps in the
/// cell grid become empty cells, and row/column bounds are enforced.
fn parse_worksheet_records(
    sheet_xml: &str,
    shared_strings: &[String],
) -> Result<Vec<Vec<String>>, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut data_rows = 0usize;
    for (row_index, element) in xml_elements(sheet_xml, "row").iter().enumerate() {
        let row_inner = &sheet_xml[element.inner.0..element.inner.1];
        let mut cells: Vec<String> = Vec::new();
        let mut cursor_column = 0usize;
        for cell in xml_elements(row_inner, "c") {
            let cell_tag = &row_inner[cell.open_tag.0..cell.open_tag.1];
            let column_index = match xml_attr(cell_tag, "r").as_deref().map(parse_cell_ref) {
                Some(Some((_, column))) => column - 1,
                Some(None) => {
                    return Err(format!(
                        "Roster XLSX has an invalid cell reference in sheet row {}.",
                        row_index + 1
                    ));
                }
                // Cells without an explicit reference fill positionally.
                None => cursor_column,
            };
            if column_index >= MAX_ROSTER_COLUMNS {
                return Err(format!(
                    "Roster has more than the allowed {MAX_ROSTER_COLUMNS} columns."
                ));
            }
            while cursor_column < column_index {
                cells.push(String::new());
                cursor_column += 1;
            }
            cursor_column = column_index + 1;
            cells.push(extract_cell_value(
                &row_inner[cell.inner.0..cell.inner.1],
                xml_attr(cell_tag, "t").as_deref(),
                shared_strings,
                row_index + 1,
                column_index + 1,
            )?);
        }
        while cells.last().map(|cell| cell.trim().is_empty()) == Some(true) {
            cells.pop();
        }
        if cells.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        if !records.is_empty() {
            data_rows += 1;
            if data_rows > MAX_ROSTER_ROWS {
                return Err(format!(
                    "Roster has more than the allowed {MAX_ROSTER_ROWS} data rows."
                ));
            }
        }
        records.push(cells);
    }
    Ok(records)
}

/// Resolve one worksheet cell to its display text.
///
/// Formula cells are read through their cached result only; a formula without
/// a cached value is rejected with a clear error instead of guessed at.
fn extract_cell_value(
    inner: &str,
    type_attr: Option<&str>,
    shared_strings: &[String],
    row_number: usize,
    column_number: usize,
) -> Result<String, String> {
    let position = format!("row {row_number}, column {column_number}");
    let cached = xml_first_element_text(inner, "v");
    match type_attr {
        Some("s") => {
            let text = cached.ok_or_else(|| {
                format!("Roster XLSX cell {position} is a shared-string cell without a value.")
            })?;
            let index: usize = text.trim().parse().map_err(|_| {
                format!("Roster XLSX cell {position} has an invalid shared-string index {text:?}.")
            })?;
            shared_strings.get(index).cloned().ok_or_else(|| {
                format!("Roster XLSX shared-string index {index} ({position}) is out of range.")
            })
        }
        Some("inlineStr") => Ok(xml_concat_t_text(inner)),
        Some("str") => cached.ok_or_else(|| {
            format!(
                "Roster XLSX cell {position} holds a formula string without a cached value; \
                 save the workbook with calculated values before importing."
            )
        }),
        Some("b") => match cached.as_deref().map(str::trim) {
            Some("1") => Ok("TRUE".to_string()),
            Some("0") => Ok("FALSE".to_string()),
            Some(other) => Err(format!(
                "Roster XLSX cell {position} has an unsupported boolean value {other:?}."
            )),
            None => Err(format!(
                "Roster XLSX cell {position} is a boolean cell without a cached value."
            )),
        },
        // `t="n"` is the standard numeric cell type written by Excel and
        // openpyxl alike; it carries the value in `<v>` exactly like an
        // untyped cell, so it takes the same path.
        Some("n") | None => match cached {
            Some(value) => Ok(value),
            None => {
                if xml_elements(inner, "f").is_empty() {
                    Ok(String::new())
                } else {
                    Err(format!(
                        "Roster XLSX cell {position} contains a formula without a cached \
                         value; open and save the workbook so values are stored before \
                         importing."
                    ))
                }
            }
        },
        Some(other) => Err(format!(
            "Roster XLSX cell {position} uses unsupported cell type {other:?}."
        )),
    }
}

/// Parse an `A1`-style cell reference into 1-based `(row, column)` numbers.
fn parse_cell_ref(reference: &str) -> Option<(usize, usize)> {
    let bytes = reference.trim().as_bytes();
    let mut index = 0;
    let mut column = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
        column = column
            .checked_mul(26)?
            .checked_add((bytes[index].to_ascii_uppercase() - b'A') as usize + 1)?;
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let mut row = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        row = row
            .checked_mul(10)?
            .checked_add((bytes[index] - b'0') as usize)?;
        index += 1;
    }
    if index != bytes.len() || row == 0 {
        return None;
    }
    Some((row, column))
}

// ---------------------------------------------------------------------------
// Minimal XML scanning helpers (OOXML worksheets are machine-generated; no
// general XML parser dependency is warranted here)
// ---------------------------------------------------------------------------

/// One XML element occurrence inside a scanned region: byte ranges of the open
/// tag (inclusive of `<name … >`) and of the inner content.
#[derive(Debug, Clone, Copy)]
struct XmlElement {
    open_tag: (usize, usize),
    inner: (usize, usize),
}

/// Find every `<name …>…</name>` occurrence (self-closing elements yield an
/// empty inner range). Names cannot nest for the OOXML subset used here.
fn xml_elements(xml: &str, name: &str) -> Vec<XmlElement> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut elements = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = xml[cursor..].find(&open).map(|position| position + cursor) {
        cursor = found + open.len();
        // Reject longer names that merely share a prefix (`<sheetData` vs
        // `<sheet`): the next char must end or continue a real open tag.
        match xml[cursor..].chars().next() {
            Some(ch) if ch == '>' || ch == '/' || ch.is_ascii_whitespace() => {}
            _ => continue,
        }
        let Some(open_end) = xml[cursor..].find('>').map(|position| position + cursor) else {
            break;
        };
        if xml[cursor..open_end].ends_with('/') {
            elements.push(XmlElement {
                open_tag: (found, open_end + 1),
                inner: (open_end + 1, open_end + 1),
            });
            continue;
        }
        let Some(close_start) = xml[open_end..].find(&close).map(|p| p + open_end) else {
            continue;
        };
        elements.push(XmlElement {
            open_tag: (found, open_end + 1),
            inner: (open_end + 1, close_start),
        });
        cursor = close_start + close.len();
    }
    elements
}

/// Read one attribute from an open-tag fragment, matching local attribute
/// names case-insensitively (namespace prefixes like `r:id` are stripped).
fn xml_attr(tag: &str, wanted: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let len = bytes.len();
    let mut index = 0usize;
    while index < len
        && !bytes[index].is_ascii_whitespace()
        && bytes[index] != b'>'
        && bytes[index] != b'/'
    {
        index += 1;
    }
    loop {
        while index < len && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= len || bytes[index] == b'>' || bytes[index] == b'/' {
            return None;
        }
        let name_start = index;
        while index < len
            && bytes[index] != b'='
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'>'
            && bytes[index] != b'/'
        {
            index += 1;
        }
        let name = &tag[name_start..index];
        while index < len && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= len || bytes[index] != b'=' {
            // Attribute without a value (invalid XML); keep scanning.
            continue;
        }
        index += 1;
        while index < len && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let quote = if index < len { bytes[index] } else { b' ' };
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        index += 1;
        let value_start = index;
        while index < len && bytes[index] != quote {
            index += 1;
        }
        if index >= len {
            return None;
        }
        let value = &tag[value_start..index];
        index += 1;
        let local = name.rsplit_once(':').map_or(name, |(_, local)| local);
        if local.eq_ignore_ascii_case(wanted) {
            return Some(xml_unescape(value));
        }
    }
}

/// Concatenate every `<t>` text under `xml`, preserving order and whitespace.
fn xml_concat_t_text(xml: &str) -> String {
    let mut out = String::new();
    for element in xml_elements(xml, "t") {
        out.push_str(&xml_unescape(&xml[element.inner.0..element.inner.1]));
    }
    out
}

/// Inner text of the first `<tag>` element under `xml`, if any.
fn xml_first_element_text(xml: &str, tag: &str) -> Option<String> {
    xml_elements(xml, tag)
        .first()
        .map(|element| xml_unescape(&xml[element.inner.0..element.inner.1]))
}

/// Unescape the five predefined XML entities plus decimal/hex character refs.
/// Unknown escapes are kept verbatim.
fn xml_unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        out.push_str(&rest[..position]);
        let tail = &rest[position..];
        let Some(semi) = tail.find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..semi];
        let replacement = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|dec| dec.parse::<u32>().ok())
                })
                .and_then(char::from_u32),
        };
        match replacement {
            Some(ch) => out.push(ch),
            None => out.push_str(&tail[..semi + 1]),
        }
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Header normalization and mapping suggestions
// ---------------------------------------------------------------------------

/// Normalize header text only; cell values are never normalized here.
///
/// Lowercases and keeps only letters and numbers, so `student id`,
/// `student_id`, and `身高(cm)` all normalize deterministically.
pub fn normalize_header(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn aliases_for(field: RosterField) -> &'static [&'static str] {
    match field {
        RosterField::StudentId => &["student_id", "id", "sid", "学号", "学生编号", "编号"],
        RosterField::Name => &["name", "姓名", "学生姓名"],
        RosterField::Gender => &["gender", "sex", "性别"],
        RosterField::HeightCm => &["height_cm", "height", "身高", "身高cm", "身高_cm"],
        RosterField::Score => &["score", "成绩", "总分", "分数"],
        RosterField::Vision => &["vision", "视力"],
        RosterField::Notes => &["notes", "note", "备注", "说明"],
        RosterField::Tags => &["tags", "tag", "标签"],
        RosterField::Needs => &["needs", "need", "特殊需求", "需求"],
    }
}

/// The field name plus every alias, normalized and de-duplicated.
fn normalized_aliases(field: RosterField) -> Vec<String> {
    let mut out = vec![normalize_header(field.as_str())];
    for alias in aliases_for(field) {
        let normalized = normalize_header(alias);
        if !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    out
}

/// Headers that prove a first row is really a header (used to reject
/// headerless detection for normal uploads).
const HEADER_HINTS: &[&str] = &[
    "id",
    "sid",
    "student",
    "studentid",
    "studentnumber",
    "studentname",
    "name",
    "fullname",
    "phone",
    "mobile",
    "phonenumber",
    "gender",
    "sex",
    "height",
    "heightcm",
    "score",
    "grade",
    "vision",
    "needs",
    "notes",
    "tags",
    "姓名",
    "学生姓名",
    "学号",
    "学生编号",
    "编号",
    "电话",
    "手机号",
    "性别",
    "身高",
    "成绩",
    "总分",
    "视力",
    "特殊需求",
    "备注",
    "标签",
];

fn has_header_hints(headers: &[String]) -> bool {
    headers
        .iter()
        .map(|header| normalize_header(header))
        .any(|normalized| HEADER_HINTS.contains(&normalized.as_str()))
}

/// True when any non-empty value is a 4+ digit number or contains CJK text.
fn looks_like_record(values: &[String]) -> bool {
    values.iter().any(|value| {
        let text = value.trim();
        if text.is_empty() {
            return false;
        }
        text.chars().all(|ch| ch.is_ascii_digit()) && text.chars().count() >= 4
            || text.chars().any(is_cjk)
    })
}

fn is_cjk(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}

/// `[A-Za-z]*\d{4,}` full match, mirroring the Python identifier heuristic.
fn looks_like_identifier(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() && chars[index].is_ascii_alphabetic() {
        index += 1;
    }
    let mut digits = 0;
    while index + digits < chars.len() && chars[index + digits].is_ascii_digit() {
        digits += 1;
    }
    index + digits == chars.len() && digits >= 4
}

/// A plausible person name: non-empty, no digits, every char alphabetic or CJK.
fn looks_like_person_name(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || looks_like_identifier(text) {
        return false;
    }
    text.chars().all(|ch| ch.is_alphabetic() || is_cjk(ch))
}

/// Conservatively assign fields to uniquely identifiable columns. Ambiguous
/// headers become `ambiguous_header` issues; a missing identity yields a
/// `missing_identity` issue. Headerless uploads get name/ID suggestions from
/// value shapes.
fn suggest_mapping(
    columns: &[RosterColumnItem],
    rows: &[RosterRow],
    headerless: bool,
) -> (Vec<(RosterField, usize)>, Vec<RosterMappingIssueItem>) {
    let mut assignments: Vec<(RosterField, usize)> = Vec::new();
    let mut issues: Vec<RosterMappingIssueItem> = Vec::new();

    for field in RosterField::ALL {
        let aliases = normalized_aliases(field);
        let matches: Vec<usize> = columns
            .iter()
            .filter(|column| {
                let normalized = normalize_header(&column.header);
                aliases
                    .iter()
                    .any(|alias| alias.as_str() == normalized.as_str())
            })
            .map(|column| column.index)
            .collect();
        match matches.len().cmp(&1) {
            std::cmp::Ordering::Equal => assignments.push((field, matches[0])),
            std::cmp::Ordering::Greater => {
                issues.push(RosterMappingIssueItem {
                    code: "ambiguous_header".to_string(),
                    message: format!(
                        "More than one column looks like {field}; choose one column explicitly."
                    ),
                    field: Some(field),
                    column_indices: matches,
                });
            }
            std::cmp::Ordering::Less => {}
        }
    }

    if headerless {
        add_headerless_identity_suggestions(columns, rows, &mut assignments);
    }

    let has_identity = assignments
        .iter()
        .any(|(field, _)| matches!(field, RosterField::StudentId | RosterField::Name));
    if !has_identity {
        issues.push(RosterMappingIssueItem {
            code: "missing_identity".to_string(),
            message: "Map at least one Student ID or Name column.".to_string(),
            field: None,
            column_indices: Vec::new(),
        });
    }

    (assignments, issues)
}

fn add_headerless_identity_suggestions(
    columns: &[RosterColumnItem],
    rows: &[RosterRow],
    assignments: &mut Vec<(RosterField, usize)>,
) {
    let assigned_fields: HashSet<RosterField> =
        assignments.iter().map(|(field, _)| *field).collect();
    let assigned_columns: HashSet<usize> = assignments.iter().map(|(_, column)| *column).collect();

    let mut candidates: Vec<(usize, f64, f64)> = Vec::new();
    for column in columns {
        if assigned_columns.contains(&column.index) {
            continue;
        }
        let non_empty: Vec<String> = rows
            .iter()
            .take(20)
            .filter_map(|row| row.cells.get(column.index))
            .map(|cell| cell.trim().to_string())
            .filter(|cell| !cell.is_empty())
            .collect();
        if non_empty.is_empty() {
            continue;
        }
        let count = non_empty.len() as f64;
        let identifier_ratio = non_empty
            .iter()
            .filter(|value| looks_like_identifier(value))
            .count() as f64
            / count;
        let name_ratio = non_empty
            .iter()
            .filter(|value| looks_like_person_name(value))
            .count() as f64
            / count;
        candidates.push((column.index, identifier_ratio, name_ratio));
    }

    if !assigned_fields.contains(&RosterField::StudentId) {
        if let Some((index, _)) = best_candidate(&candidates, 0.6, &HashSet::new(), |c| c.1) {
            assignments.push((RosterField::StudentId, index));
        }
    }
    if !assigned_fields.contains(&RosterField::Name) {
        let used: HashSet<usize> = assignments.iter().map(|(_, column)| *column).collect();
        if let Some((index, _)) = best_candidate(&candidates, 0.6, &used, |c| c.2) {
            assignments.push((RosterField::Name, index));
        }
    }
}

/// Highest-scoring column (then lowest index) meeting the `threshold`, while
/// not already used.
fn best_candidate(
    candidates: &[(usize, f64, f64)],
    threshold: f64,
    used: &HashSet<usize>,
    score: impl Fn(&(usize, f64, f64)) -> f64,
) -> Option<(usize, f64)> {
    candidates
        .iter()
        .filter(|candidate| score(candidate) >= threshold && !used.contains(&candidate.0))
        .max_by(|left, right| {
            score(right)
                .partial_cmp(&score(left))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.0.cmp(&right.0))
        })
        .map(|candidate| (candidate.0, score(candidate)))
}

// ---------------------------------------------------------------------------
// Draft store
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct StoredDraft {
    draft: RosterDraft,
    touched_at: Instant,
}

/// An in-memory, TTL-bounded map of uploaded roster drafts. Evicts expired
/// drafts on access and the oldest draft when over capacity.
#[derive(Debug)]
pub struct RosterDraftStore {
    drafts: HashMap<String, StoredDraft>,
    max_drafts: usize,
    ttl: Duration,
}

impl RosterDraftStore {
    /// A store with the default capacity (10 drafts) and 2-hour TTL.
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_DRAFTS, DEFAULT_TTL_SECONDS)
    }

    /// A store with explicit bounds. Both bounds are clamped to at least 1.
    pub fn with_limits(max_drafts: usize, ttl_seconds: u64) -> Self {
        RosterDraftStore {
            drafts: HashMap::new(),
            max_drafts: max_drafts.max(1),
            ttl: Duration::from_secs(ttl_seconds.max(1)),
        }
    }

    /// Store a parsed draft and return its response.
    pub fn create(&mut self, draft: RosterDraft) -> RosterDraftResponse {
        self.prune();
        while self.drafts.len() >= self.max_drafts {
            let oldest = self
                .drafts
                .values()
                .min_by_key(|stored| stored.touched_at)
                .map(|stored| stored.draft.draft_id.clone());
            match oldest {
                Some(draft_id) => {
                    self.drafts.remove(&draft_id);
                }
                None => break,
            }
        }
        let response = draft.to_response();
        let draft_id = response.draft_id.clone();
        self.drafts.insert(
            draft_id,
            StoredDraft {
                draft,
                touched_at: Instant::now(),
            },
        );
        response
    }

    /// Return the current response for a draft, refreshing its TTL.
    pub fn state(&mut self, draft_id: &str) -> Result<RosterDraftResponse, String> {
        let draft = self.get(draft_id)?;
        Ok(draft.to_response())
    }

    /// Clone a stored draft by id, refreshing its TTL.
    pub fn get(&mut self, draft_id: &str) -> Result<RosterDraft, String> {
        self.prune();
        let cleaned = draft_id.trim();
        match self.drafts.get_mut(cleaned) {
            Some(stored) => {
                stored.touched_at = Instant::now();
                Ok(stored.draft.clone())
            }
            None => Err(format!(
                "roster draft {cleaned:?} was not found (it may have expired)"
            )),
        }
    }

    /// Build an update preview for a stored draft by id.
    pub fn preview(
        &mut self,
        draft_id: &str,
        request: &RosterUpdatePreviewRequest,
    ) -> Result<RosterUpdatePreviewResponse, String> {
        let draft = self.get(draft_id)?;
        let preview = preview_roster_update(
            &draft,
            &request.mapping,
            &request.current_students,
            request.current_revision,
            &request.mode,
            request.updated_fields.as_deref(),
        )?;
        Ok(preview.to_response(draft_id))
    }

    /// Remove a draft immediately. Returns whether it existed.
    pub fn delete(&mut self, draft_id: &str) -> bool {
        self.drafts.remove(draft_id.trim()).is_some()
    }

    /// Drop every stored draft.
    pub fn clear(&mut self) {
        self.drafts.clear();
    }

    /// Number of drafts currently stored.
    pub fn len(&self) -> usize {
        self.drafts.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.drafts.is_empty()
    }

    fn prune(&mut self) {
        let now = Instant::now();
        let expired: Vec<String> = self
            .drafts
            .iter()
            .filter(|(_, stored)| now.duration_since(stored.touched_at) > self.ttl)
            .map(|(draft_id, _)| draft_id.clone())
            .collect();
        for draft_id in expired {
            self.drafts.remove(&draft_id);
        }
    }
}

impl Default for RosterDraftStore {
    fn default() -> Self {
        Self::new()
    }
}

fn global_store() -> &'static Mutex<RosterDraftStore> {
    static STORE: OnceLock<Mutex<RosterDraftStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(RosterDraftStore::new()))
}

/// MIME type used by Excel workbook uploads (`.xlsx` / `.xlsm`).
const XLSX_CONTENT_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// Whether the supplied metadata identifies an Excel workbook upload.
pub fn looks_like_xlsx(filename: Option<&str>, content_type: Option<&str>) -> bool {
    let by_type = content_type
        .map(|value| value.trim().eq_ignore_ascii_case(XLSX_CONTENT_TYPE))
        .unwrap_or(false);
    let by_extension = filename.map(|name| {
        let lower = name.trim().to_ascii_lowercase();
        lower.ends_with(".xlsx") || lower.ends_with(".xlsm")
    });
    by_type || by_extension.unwrap_or(false)
}

/// Upload roster bytes and return the `RosterDraftResponse` JSON, routing
/// `.xlsx`/`.xlsm` uploads (by filename, content type, or zip magic bytes) to
/// the XLSX reader and everything else to the CSV reader. Uses the
/// process-wide draft store.
pub fn upload_draft_json(bytes: &[u8]) -> Result<String, String> {
    upload_draft_file_json(bytes, None, None)
}

/// [`upload_draft_json`] with source metadata from an upload form. When
/// `filename` / `content_type` identify a spreadsheet the bytes are parsed as
/// XLSX; otherwise they parse as CSV.
pub fn upload_draft_file_json(
    bytes: &[u8],
    filename: Option<&str>,
    content_type: Option<&str>,
) -> Result<String, String> {
    let is_xlsx = looks_like_xlsx(filename, content_type) || bytes.starts_with(b"PK\x03\x04");
    let draft = if is_xlsx {
        parse_roster_xlsx(bytes)?
    } else {
        parse_roster_csv(bytes)?
    };
    let response = global_store()
        .lock()
        .map_err(|_| "roster draft store lock is poisoned".to_string())?
        .create(draft);
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize roster draft: {error}"))
}

/// Return the stored `RosterDraftResponse` JSON for a draft id.
pub fn get_draft_json(draft_id: &str) -> Result<String, String> {
    let response = global_store()
        .lock()
        .map_err(|_| "roster draft store lock is poisoned".to_string())?
        .state(draft_id)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize roster draft: {error}"))
}

/// Build a preview from a `RosterUpdatePreviewRequest` JSON body against a
/// stored draft, returning `RosterUpdatePreviewResponse` JSON.
pub fn preview_update_json(draft_id: &str, body: &str) -> Result<String, String> {
    let request: RosterUpdatePreviewRequest = serde_json::from_str(body)
        .map_err(|error| format!("request body is not a valid roster update preview: {error}"))?;
    let response = global_store()
        .lock()
        .map_err(|_| "roster draft store lock is poisoned".to_string())?
        .preview(draft_id, &request)?;
    serde_json::to_string(&response)
        .map_err(|error| format!("could not serialize roster preview: {error}"))
}

/// Delete a stored draft by id. Returns whether it existed.
pub fn delete_draft(draft_id: &str) -> bool {
    global_store()
        .lock()
        .map(|mut store| store.delete(draft_id))
        .unwrap_or(false)
}

static DRAFT_SEQ: AtomicU64 = AtomicU64::new(0);

fn new_draft_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = DRAFT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{seq:x}")
}

// ---------------------------------------------------------------------------
// Update preview
// ---------------------------------------------------------------------------

/// A complete, immutable update plan (the structured result of a preview).
#[derive(Debug, Clone)]
pub struct RosterUpdatePreview {
    pub mode: String,
    pub base_revision: u32,
    pub changes: Vec<RosterChangeItem>,
    pub conflicts: Vec<RosterConflictItem>,
    pub resulting_students: Option<Vec<Student>>,
    pub updated_fields: Vec<String>,
    pub can_apply: bool,
}

impl RosterUpdatePreview {
    /// Project this preview into the `RosterUpdatePreviewResponse` wire shape.
    pub fn to_response(&self, draft_id: &str) -> RosterUpdatePreviewResponse {
        let mut counts = ActionCounts::default();
        for change in &self.changes {
            match change.action.as_str() {
                "add" => counts.add += 1,
                "update" => counts.update += 1,
                "unchanged" => counts.unchanged += 1,
                "remove" => counts.remove += 1,
                "conflict" => counts.conflict += 1,
                _ => {}
            }
        }
        RosterUpdatePreviewResponse {
            draft_id: draft_id.to_string(),
            base_revision: self.base_revision,
            mode: self.mode.clone(),
            can_apply: self.can_apply,
            action_counts: counts,
            changes: self.changes.clone(),
            conflicts: self.conflicts.clone(),
            resulting_students: self.resulting_students.clone(),
        }
    }
}

/// Normalize a student name for exact matching. Whitespace is collapsed but
/// every remaining character must match exactly.
pub fn normalize_student_name(value: &Option<String>) -> Option<String> {
    let text = value.as_deref()?.trim();
    if text.is_empty() {
        return None;
    }
    let collapsed = text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

const STUDENT_FIELDS: &[&str] = &[
    "student_id",
    "name",
    "gender",
    "height_cm",
    "score",
    "vision",
    "tags",
    "needs",
    "notes",
    "attributes",
];

/// Validate `updated_fields`: `None` means every student field, unknown names
/// are rejected, and the result keeps the canonical field order.
fn normalize_updated_fields(values: Option<&[String]>) -> Result<Vec<String>, String> {
    match values {
        None => Ok(STUDENT_FIELDS
            .iter()
            .map(|field| field.to_string())
            .collect()),
        Some(requested) => {
            for value in requested {
                if !STUDENT_FIELDS.contains(&value.as_str()) {
                    return Err(format!("Unknown student update fields: {value}"));
                }
            }
            let mut out = Vec::new();
            for field in STUDENT_FIELDS {
                if requested.iter().any(|value| value.as_str() == *field) {
                    out.push(field.to_string());
                }
            }
            Ok(out)
        }
    }
}

/// Validate a manual mapping: unique fields, unique columns, in-bounds
/// column indices, and at least one identity field. Returns ordered
/// `(field, column_index)` pairs.
fn validate_mapping(
    mapping: &[RosterMappingItem],
    column_count: usize,
) -> Result<Vec<(RosterField, usize)>, String> {
    let mut fields: Vec<RosterField> = Vec::new();
    let mut columns: Vec<usize> = Vec::new();
    for item in mapping {
        if fields.contains(&item.field) {
            return Err(format!(
                "Roster fields mapped more than once: {}",
                item.field
            ));
        }
        if columns.contains(&item.column_index) {
            return Err(format!(
                "Source columns mapped more than once: {}",
                item.column_index
            ));
        }
        if item.column_index >= column_count {
            return Err(format!(
                "Mapped column {} is outside this {column_count}-column roster.",
                item.column_index
            ));
        }
        fields.push(item.field);
        columns.push(item.column_index);
    }
    let has_identity = fields
        .iter()
        .any(|field| matches!(field, RosterField::StudentId | RosterField::Name));
    if !has_identity {
        return Err("Map at least one of student_id or name".to_string());
    }
    Ok(fields.into_iter().zip(columns).collect())
}

/// Convert a raw data row into a student record under a mapping.
fn build_student(
    row: &RosterRow,
    assignments: &[(RosterField, usize)],
) -> Result<Option<Student>, String> {
    let mut student = Student::default();
    let mut has_data = false;
    let mut name_mapped = false;
    let mut name_value: Option<String> = None;

    for (field, column) in assignments {
        let raw = row.cells.get(*column).map(String::as_str).unwrap_or("");
        match field {
            RosterField::StudentId
            | RosterField::Name
            | RosterField::Gender
            | RosterField::Notes => {
                let value = clean_text(raw);
                match field {
                    RosterField::StudentId => {
                        if value.is_some() {
                            has_data = true;
                        }
                        student.student_id = value;
                    }
                    RosterField::Name => {
                        name_mapped = true;
                        if value.is_some() {
                            has_data = true;
                        }
                        name_value = value.clone();
                        student.name = value;
                    }
                    RosterField::Gender => {
                        if value.is_some() {
                            has_data = true;
                        }
                        student.gender = value;
                    }
                    _ => {
                        if value.is_some() {
                            has_data = true;
                        }
                        student.notes = value;
                    }
                }
            }
            RosterField::HeightCm | RosterField::Score => {
                let text = raw.trim();
                if text.is_empty() {
                    continue;
                }
                let number = text
                    .parse::<f64>()
                    .ok()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| {
                        format!(
                            "Row {}: column \"{}\" must be a number, got \"{text}\".",
                            row.row_number,
                            field.as_str()
                        )
                    })?;
                if *field == RosterField::HeightCm && number <= 0.0 {
                    return Err(format!(
                        "Row {}: column \"height_cm\" must be a positive number, got \"{text}\".",
                        row.row_number
                    ));
                }
                if *field == RosterField::Score {
                    student.score = Some(number);
                } else {
                    student.height_cm = Some(number);
                }
                has_data = true;
            }
            RosterField::Vision => {
                if let Some(value) = clean_text(raw) {
                    student.vision = Some(VisionValue::Str(value));
                    has_data = true;
                }
            }
            RosterField::Tags | RosterField::Needs => {
                let items = split_list(raw);
                if !items.is_empty() {
                    has_data = true;
                    if *field == RosterField::Tags {
                        student.tags = items;
                    } else {
                        student.needs = items;
                    }
                }
            }
        }
    }

    if !has_data {
        return Ok(None);
    }
    if name_mapped && name_value.is_none() {
        return Err(format!(
            "Row {}: column \"name\" cannot be empty.",
            row.row_number
        ));
    }
    if student.student_id.is_none() && student.name.is_none() {
        return Err(format!(
            "Row {}: at least one of column \"student_id\" or \"name\" is required.",
            row.row_number
        ));
    }
    Ok(Some(student))
}

/// Project every data row under a mapping into incoming student records.
fn students_from_draft(
    draft: &RosterDraft,
    assignments: &[(RosterField, usize)],
) -> Result<Vec<Student>, String> {
    let mut students = Vec::new();
    for row in &draft.rows {
        if let Some(student) = build_student(row, assignments)? {
            students.push(student);
        }
    }
    Ok(students)
}

/// Parse a roster CSV into student records using the automatic header
/// mapping (project workflows / CLI project-solve).
pub fn parse_roster_students(bytes: &[u8]) -> Result<Vec<Student>, String> {
    let draft = parse_roster_csv(bytes)?;
    let assignments = validate_mapping(&draft.suggested_mapping, draft.column_count())?;
    students_from_draft(&draft, &assignments)
}

/// Build an incremental or full-replacement difference preview for a draft.
///
/// Identity resolution follows a strict order:
///
/// 1. exact `student_id`;
/// 2. a unique exact normalized name when no ID match exists;
/// 3. otherwise a new student or an explicit conflict.
///
/// An incoming ID is never silently used to replace a different existing ID,
/// even when the names match; that becomes a reviewable `student_id_name_mismatch`
/// conflict instead of corrupting the stable identity used by seat history.
pub fn preview_roster_update(
    draft: &RosterDraft,
    mapping: &[RosterMappingItem],
    current_students: &[Student],
    current_revision: u32,
    mode: &str,
    updated_fields: Option<&[String]>,
) -> Result<RosterUpdatePreview, String> {
    let resolved_mode = match mode {
        "incremental" => "incremental",
        "replace" | "overwrite" | "full" => "replace",
        _ => {
            return Err(format!(
                "mode must be incremental, replace, or overwrite; got {mode:?}"
            ));
        }
    };
    let fields = normalize_updated_fields(updated_fields)?;
    let assignments = validate_mapping(mapping, draft.column_count())?;
    let imported = students_from_draft(draft, &assignments)?;

    let existing_ids = index_values(current_students, |student| student.student_id.clone());
    let existing_names = index_values(current_students, |student| {
        normalize_student_name(&student.name)
    });
    let incoming_ids = index_values(&imported, |student| student.student_id.clone());
    let incoming_name_only = index_values(&imported, |student| {
        if student.student_id.is_none() {
            normalize_student_name(&student.name)
        } else {
            None
        }
    });

    let mut conflicts: Vec<RosterConflictItem> = Vec::new();
    let mut changes: Vec<RosterChangeItem> = Vec::new();
    let mut blocked_incoming: HashSet<usize> = HashSet::new();

    for (student_id, indices) in sorted_duplicates(&existing_ids) {
        conflicts.push(RosterConflictItem {
            code: "duplicate_existing_student_id".to_string(),
            message: format!(
                "The current roster contains student_id {student_id:?} more than once. \
                 Resolve it before importing."
            ),
            incoming_index: None,
            existing_indices: indices,
        });
    }
    for (student_id, indices) in sorted_duplicates(&incoming_ids) {
        blocked_incoming.extend(indices.iter().copied());
        conflicts.push(RosterConflictItem {
            code: "duplicate_incoming_student_id".to_string(),
            message: format!("The import contains student_id {student_id:?} more than once."),
            incoming_index: indices.first().copied(),
            existing_indices: Vec::new(),
        });
    }
    for (name, indices) in sorted_duplicates(&incoming_name_only) {
        blocked_incoming.extend(indices.iter().copied());
        conflicts.push(RosterConflictItem {
            code: "duplicate_incoming_name".to_string(),
            message: format!(
                "The import contains the name {name:?} more than once without student IDs."
            ),
            incoming_index: indices.first().copied(),
            existing_indices: Vec::new(),
        });
    }

    let mut matched_existing: HashSet<usize> = HashSet::new();
    let mut replacements: HashMap<usize, Student> = HashMap::new();
    let mut additions: Vec<Student> = Vec::new();

    for (incoming_index, student) in imported.iter().enumerate() {
        if blocked_incoming.contains(&incoming_index) {
            changes.push(conflict_change(student, incoming_index, None, None, None));
            continue;
        }

        let (matched_index, method, conflict) = match_student(
            student,
            current_students,
            &existing_ids,
            &existing_names,
            incoming_index,
        );

        if let Some(conflict) = &conflict {
            conflicts.push(conflict.clone());
            changes.push(conflict_change(
                student,
                incoming_index,
                None,
                None,
                Some(conflict),
            ));
            continue;
        }

        let Some(matched_index) = matched_index else {
            additions.push(student.clone());
            changes.push(RosterChangeItem {
                action: "add".to_string(),
                match_method: "new".to_string(),
                before: None,
                after: Some(student.clone()),
                field_changes: Vec::new(),
                incoming_index: Some(incoming_index),
                existing_index: None,
            });
            continue;
        };

        if matched_existing.contains(&matched_index) {
            let conflict = RosterConflictItem {
                code: "existing_student_matched_twice".to_string(),
                message: "Two imported rows resolve to the same current student. Add or correct \
                     student IDs before applying."
                    .to_string(),
                incoming_index: Some(incoming_index),
                existing_indices: vec![matched_index],
            };
            conflicts.push(conflict.clone());
            changes.push(conflict_change(
                student,
                incoming_index,
                Some(matched_index),
                Some(&current_students[matched_index]),
                Some(&conflict),
            ));
            continue;
        }

        matched_existing.insert(matched_index);
        let previous = &current_students[matched_index];
        let replacement = if resolved_mode == "replace" {
            student.clone()
        } else {
            merge_student(previous, student, &fields, method)
        };
        replacements.insert(matched_index, replacement.clone());
        let field_changes = field_changes(previous, &replacement);
        let action = if field_changes.is_empty() {
            "unchanged"
        } else {
            "update"
        };
        changes.push(RosterChangeItem {
            action: action.to_string(),
            match_method: method.to_string(),
            before: Some(previous.clone()),
            after: Some(replacement),
            field_changes,
            incoming_index: Some(incoming_index),
            existing_index: Some(matched_index),
        });
    }

    if resolved_mode == "replace" {
        for (existing_index, student) in current_students.iter().enumerate() {
            if !matched_existing.contains(&existing_index) {
                changes.push(RosterChangeItem {
                    action: "remove".to_string(),
                    match_method: "new".to_string(),
                    before: Some(student.clone()),
                    after: None,
                    field_changes: Vec::new(),
                    incoming_index: None,
                    existing_index: Some(existing_index),
                });
            }
        }
    }

    let mut resulting_students = if !conflicts.is_empty() {
        None
    } else if resolved_mode == "replace" {
        Some(imported.clone())
    } else {
        let mut merged: Vec<Student> = current_students
            .iter()
            .enumerate()
            .map(|(index, student)| {
                replacements
                    .get(&index)
                    .cloned()
                    .unwrap_or_else(|| student.clone())
            })
            .collect();
        merged.extend(additions);
        Some(merged)
    };

    if let Some(ref students) = resulting_students {
        let duplicates = duplicate_student_keys(students);
        if !duplicates.is_empty() {
            conflicts.push(RosterConflictItem {
                code: "duplicate_resulting_identifier".to_string(),
                message: format!(
                    "The imported roster would create duplicate student identifiers: {}.",
                    duplicates.join(", ")
                ),
                incoming_index: None,
                existing_indices: Vec::new(),
            });
            resulting_students = None;
        }
    }

    Ok(RosterUpdatePreview {
        mode: resolved_mode.to_string(),
        base_revision: current_revision,
        can_apply: conflicts.is_empty() && resulting_students.is_some(),
        changes,
        conflicts,
        resulting_students,
        updated_fields: fields,
    })
}

fn conflict_change(
    student: &Student,
    incoming_index: usize,
    existing_index: Option<usize>,
    before: Option<&Student>,
    conflict: Option<&RosterConflictItem>,
) -> RosterChangeItem {
    let match_method = match &conflict {
        Some(conflict) => match conflict.code.as_str() {
            "ambiguous_student_id" => "student_id",
            "ambiguous_name" | "student_id_name_mismatch" => "name",
            _ => "new",
        },
        None => "new",
    };
    RosterChangeItem {
        action: "conflict".to_string(),
        match_method: match_method.to_string(),
        before: before.cloned(),
        after: Some(student.clone()),
        field_changes: Vec::new(),
        incoming_index: Some(incoming_index),
        existing_index,
    }
}

/// Match one incoming student against the current roster.
///
/// Returns `(matched_index, match_method, conflict)`; exactly one of the index
/// and the conflict is `Some`.
fn match_student(
    incoming: &Student,
    existing: &[Student],
    existing_ids: &HashMap<String, Vec<usize>>,
    existing_names: &HashMap<String, Vec<usize>>,
    incoming_index: usize,
) -> (Option<usize>, &'static str, Option<RosterConflictItem>) {
    if let Some(student_id) = &incoming.student_id {
        if let Some(id_matches) = existing_ids.get(student_id) {
            if id_matches.len() == 1 {
                return (Some(id_matches[0]), "student_id", None);
            }
            if id_matches.len() > 1 {
                return (
                    None,
                    "student_id",
                    Some(RosterConflictItem {
                        code: "ambiguous_student_id".to_string(),
                        message: format!(
                            "student_id {student_id:?} matches more than one current student."
                        ),
                        incoming_index: Some(incoming_index),
                        existing_indices: id_matches.clone(),
                    }),
                );
            }
        }
    }

    let Some(normalized_name) = normalize_student_name(&incoming.name) else {
        return (None, "new", None);
    };
    let Some(name_matches) = existing_names.get(&normalized_name) else {
        return (None, "new", None);
    };
    if name_matches.is_empty() {
        return (None, "new", None);
    }
    if name_matches.len() > 1 {
        return (
            None,
            "name",
            Some(RosterConflictItem {
                code: "ambiguous_name".to_string(),
                message: format!(
                    "Name {:?} matches more than one current student. Use a student ID.",
                    incoming.name
                ),
                incoming_index: Some(incoming_index),
                existing_indices: name_matches.clone(),
            }),
        );
    }

    let existing_index = name_matches[0];
    let current_id = existing[existing_index].student_id.clone();
    if let (Some(incoming_id), Some(current_id)) = (&incoming.student_id, current_id) {
        if incoming_id != &current_id {
            return (
                None,
                "name",
                Some(RosterConflictItem {
                    code: "student_id_name_mismatch".to_string(),
                    message: format!(
                        "Name {:?} already belongs to student_id {current_id:?}, not \
                         {incoming_id:?}.",
                        incoming.name
                    ),
                    incoming_index: Some(incoming_index),
                    existing_indices: vec![existing_index],
                }),
            );
        }
    }
    (Some(existing_index), "name", None)
}

/// Merge an incoming student into an existing one, honoring `updated_fields`.
/// An empty incoming ID never erases the stable key used by history.
fn merge_student(before: &Student, incoming: &Student, fields: &[String], method: &str) -> Student {
    let mut merged = before.clone();
    for field in fields {
        match field.as_str() {
            "student_id" => {
                if let Some(student_id) = &incoming.student_id {
                    merged.student_id = Some(student_id.clone());
                }
            }
            "name" => merged.name = incoming.name.clone(),
            "gender" => merged.gender = incoming.gender.clone(),
            "height_cm" => merged.height_cm = incoming.height_cm,
            "score" => merged.score = incoming.score,
            "vision" => merged.vision = incoming.vision.clone(),
            "tags" => merged.tags = incoming.tags.clone(),
            "needs" => merged.needs = incoming.needs.clone(),
            "notes" => merged.notes = incoming.notes.clone(),
            "attributes" => merged.attributes = incoming.attributes.clone(),
            _ => {}
        }
    }
    // When a row matched a name-only record, preserve the newly supplied
    // stable ID even if an adapter omitted `updated_fields` accidentally.
    if method == "name" && before.student_id.is_none() && incoming.student_id.is_some() {
        merged.student_id = incoming.student_id.clone();
    }
    merged
}

/// Visible field differences between two student records, in canonical order.
fn field_changes(before: &Student, after: &Student) -> Vec<RosterFieldChangeItem> {
    STUDENT_FIELDS
        .iter()
        .filter_map(|field| {
            let before_value = student_field_json(before, field);
            let after_value = student_field_json(after, field);
            if before_value == after_value {
                None
            } else {
                Some(RosterFieldChangeItem {
                    field: field.to_string(),
                    before: before_value,
                    after: after_value,
                })
            }
        })
        .collect()
}

fn student_field_json(student: &Student, field: &str) -> Value {
    match field {
        "student_id" => to_value(&student.student_id),
        "name" => to_value(&student.name),
        "gender" => to_value(&student.gender),
        "height_cm" => to_value(&student.height_cm),
        "score" => to_value(&student.score),
        "vision" => to_value(&student.vision),
        "tags" => to_value(&student.tags),
        "needs" => to_value(&student.needs),
        "notes" => to_value(&student.notes),
        "attributes" => to_value(&student.attributes),
        _ => Value::Null,
    }
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Duplicate stable keys (`student_id or name`) in a candidate roster, sorted.
fn duplicate_student_keys(students: &[Student]) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for student in students {
        let key = student
            .student_id
            .clone()
            .or_else(|| student.name.clone())
            .unwrap_or_default();
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut duplicates: Vec<String> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(key, _)| key)
        .collect();
    duplicates.sort();
    duplicates
}

fn index_values(
    students: &[Student],
    getter: impl Fn(&Student) -> Option<String>,
) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, student) in students.iter().enumerate() {
        if let Some(value) = getter(student) {
            map.entry(value).or_default().push(index);
        }
    }
    map
}

/// Duplicate values (more than one index) from a grouped map, key-sorted.
fn sorted_duplicates(groups: &HashMap<String, Vec<usize>>) -> Vec<(String, Vec<usize>)> {
    let mut duplicates: Vec<(String, Vec<usize>)> = groups
        .iter()
        .filter(|(_, indices)| indices.len() > 1)
        .map(|(key, indices)| (key.clone(), indices.clone()))
        .collect();
    duplicates.sort_by(|left, right| left.0.cmp(&right.0));
    duplicates
}

fn clean_text(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Split a cell into list items on common spreadsheet separators.
fn split_list(value: &str) -> Vec<String> {
    value
        .split([';', '；', ',', '，', '、', '|'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(field: RosterField, column_index: usize) -> RosterMappingItem {
        RosterMappingItem {
            field,
            column_index,
        }
    }

    fn id_name_score_mapping() -> Vec<RosterMappingItem> {
        vec![
            mapping(RosterField::StudentId, 0),
            mapping(RosterField::Name, 1),
            mapping(RosterField::Score, 2),
        ]
    }

    fn student_with_id_name_score(student_id: &str, name: &str, score: Option<f64>) -> Student {
        Student {
            student_id: Some(student_id.to_string()),
            name: Some(name.to_string()),
            score,
            ..Student::default()
        }
    }

    #[test]
    fn parse_basic_csv_yields_table_shape() {
        let draft = parse_roster_csv(b"name,id,score\nAlice,S1,93\nBob,S2,81\n").unwrap();
        assert_eq!(draft.source_format, "csv");
        assert!(!draft.headerless);
        assert_eq!(draft.row_count(), 2);
        assert_eq!(draft.column_count(), 3);
        assert_eq!(draft.columns[0].header, "name");
        assert_eq!(draft.rows[0].row_number, 2);
        assert_eq!(draft.rows[0].cells, vec!["Alice", "S1", "93"]);
        assert_eq!(draft.rows[1].cells, vec!["Bob", "S2", "81"]);
        assert_eq!(draft.preview_rows().len(), 2);
    }

    #[test]
    fn parse_csv_handles_quotes_newlines_and_escaped_quotes() {
        let csv = "name,notes\n\"Zhao, Wei\",\"line1\nline2 \"\"quoted\"\"\"\nThird,plain\n";
        let draft = parse_roster_csv(csv.as_bytes()).unwrap();
        assert_eq!(draft.row_count(), 2);
        assert_eq!(draft.rows[0].cells[0], "Zhao, Wei");
        assert_eq!(draft.rows[0].cells[1], "line1\nline2 \"quoted\"");
        assert_eq!(draft.rows[1].cells[0], "Third");
        assert_eq!(draft.rows[1].cells[1], "plain");
    }

    #[test]
    fn parse_csv_empty_file_is_error() {
        let error = parse_roster_csv(b"").unwrap_err();
        assert!(error.contains("empty"), "unexpected error: {error}");
        assert!(parse_roster_csv(b"\xff\xfe not utf8").is_err());
    }

    #[test]
    fn parse_csv_strips_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"name,id\nAlice,S1\n");
        let draft = parse_roster_csv(&bytes).unwrap();
        assert_eq!(draft.columns[0].header, "name");
        assert_eq!(draft.rows[0].cells, vec!["Alice", "S1"]);
    }

    #[test]
    fn parse_csv_trims_surrounding_whitespace() {
        let draft = parse_roster_csv(b" id , name \n S1 , Alice \n").unwrap();
        assert_eq!(draft.columns[0].header, "id");
        assert_eq!(draft.rows[0].cells, vec!["S1", "Alice"]);
    }

    #[test]
    fn suggest_mapping_recognizes_chinese_headers() {
        let draft =
            parse_roster_csv("学号,姓名,总分\n001,小明,91\n002,小红,88\n".as_bytes()).unwrap();
        let suggested: Vec<(String, usize)> = draft
            .suggested_mapping
            .iter()
            .map(|item| (item.field.as_str().to_string(), item.column_index))
            .collect();
        assert_eq!(
            suggested,
            vec![
                ("student_id".to_string(), 0),
                ("name".to_string(), 1),
                ("score".to_string(), 2),
            ]
        );
        assert!(draft.mapping_issues.is_empty());
    }

    #[test]
    fn suggest_mapping_recognizes_english_headers() {
        let draft = parse_roster_csv(
            b"student_id,name,gender,height_cm,score,vision\nS1,Alice,F,160,90,0.8\n",
        )
        .unwrap();
        let suggested: HashMap<String, usize> = draft
            .suggested_mapping
            .iter()
            .map(|item| (item.field.as_str().to_string(), item.column_index))
            .collect();
        assert_eq!(suggested.get("student_id"), Some(&0));
        assert_eq!(suggested.get("name"), Some(&1));
        assert_eq!(suggested.get("gender"), Some(&2));
        assert_eq!(suggested.get("height_cm"), Some(&3));
        assert_eq!(suggested.get("score"), Some(&4));
        assert_eq!(suggested.get("vision"), Some(&5));
    }

    #[test]
    fn suggest_mapping_reports_ambiguous_headers() {
        let draft = parse_roster_csv("id,学号\n1,2\n".as_bytes()).unwrap();
        let issue = draft
            .mapping_issues
            .iter()
            .find(|issue| issue.code == "ambiguous_header");
        let issue = issue.expect("expected an ambiguous_header issue");
        assert_eq!(issue.field, Some(RosterField::StudentId));
        assert_eq!(issue.column_indices, vec![0, 1]);
        assert!(draft
            .suggested_mapping
            .iter()
            .all(|item| item.field != RosterField::StudentId));
    }

    #[test]
    fn suggest_mapping_reports_missing_identity() {
        let draft = parse_roster_csv("score\n91\n".as_bytes()).unwrap();
        assert!(draft
            .mapping_issues
            .iter()
            .any(|issue| issue.code == "missing_identity" && issue.field.is_none()));
    }

    #[test]
    fn headerless_uploads_get_generated_columns_and_identity_suggestions() {
        let draft = parse_roster_csv("小林,18513806422\n小周,18513806423\n".as_bytes()).unwrap();
        assert!(draft.headerless);
        assert_eq!(draft.row_count(), 2);
        assert_eq!(draft.columns[0].header, "Column 1");
        assert_eq!(draft.columns[1].header, "Column 2");
        let suggested: HashMap<String, usize> = draft
            .suggested_mapping
            .iter()
            .map(|item| (item.field.as_str().to_string(), item.column_index))
            .collect();
        assert_eq!(suggested.get("name"), Some(&0));
        assert_eq!(suggested.get("student_id"), Some(&1));
    }

    #[test]
    fn incremental_preview_counts_updates_and_adds() {
        let draft = parse_roster_csv(b"id,name,score\nS1,Alice,93\nS2,Bob,81\n").unwrap();
        let current = vec![
            Student {
                notes: Some("keep".to_string()),
                ..student_with_id_name_score("S1", "Alice", Some(70.0))
            },
            student_with_id_name_score("S3", "Cara", Some(80.0)),
        ];
        let updated_fields = vec![
            "student_id".to_string(),
            "name".to_string(),
            "score".to_string(),
        ];
        let preview = preview_roster_update(
            &draft,
            &id_name_score_mapping(),
            &current,
            4,
            "incremental",
            Some(&updated_fields),
        )
        .unwrap();

        assert!(preview.can_apply);
        assert!(preview.conflicts.is_empty());
        assert_eq!(
            preview
                .changes
                .iter()
                .filter(|change| change.action == "update")
                .count(),
            1
        );
        assert_eq!(
            preview
                .changes
                .iter()
                .filter(|change| change.action == "add")
                .count(),
            1
        );
        assert_eq!(
            preview
                .changes
                .iter()
                .filter(|change| change.action == "remove")
                .count(),
            0
        );
        let resulting = preview.resulting_students.as_ref().unwrap();
        assert_eq!(resulting.len(), 3);
        // Notes are not in updated_fields, so the current value survives.
        assert_eq!(resulting[0].notes.as_deref(), Some("keep"));
        assert_eq!(resulting[0].score, Some(93.0));
        assert_eq!(resulting[2].student_id.as_deref(), Some("S2"));
    }

    #[test]
    fn replace_preview_removes_unmatched_and_reorders() {
        let draft = parse_roster_csv(b"id,name,score\nS1,Alice,93\nS2,Bob,81\n").unwrap();
        let current = vec![
            student_with_id_name_score("S1", "Alice", Some(70.0)),
            student_with_id_name_score("S3", "Cara", Some(80.0)),
        ];
        let preview = preview_roster_update(
            &draft,
            &id_name_score_mapping(),
            &current,
            4,
            "replace",
            None,
        )
        .unwrap();

        assert!(preview.can_apply);
        assert_eq!(
            preview
                .changes
                .iter()
                .filter(|change| change.action == "remove")
                .count(),
            1
        );
        let ids: Vec<Option<&String>> = preview
            .resulting_students
            .as_ref()
            .unwrap()
            .iter()
            .map(|student| student.student_id.as_ref())
            .collect();
        assert_eq!(ids, vec![Some(&"S1".to_string()), Some(&"S2".to_string())]);
    }

    #[test]
    fn preview_reports_student_id_name_mismatch_conflict() {
        let draft = parse_roster_csv(b"id,name\nS9,Alice\n").unwrap();
        let current = vec![student_with_id_name_score("S1", "Alice", None)];
        let mapping = vec![
            mapping(RosterField::StudentId, 0),
            mapping(RosterField::Name, 1),
        ];
        let preview =
            preview_roster_update(&draft, &mapping, &current, 0, "incremental", None).unwrap();

        assert!(!preview.can_apply);
        assert!(preview.resulting_students.is_none());
        assert!(preview
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "student_id_name_mismatch"));
    }

    #[test]
    fn preview_reports_duplicate_incoming_ids() {
        let draft = parse_roster_csv(b"id,name\nS1,Alice\nS1,Bob\n").unwrap();
        let mapping = vec![
            mapping(RosterField::StudentId, 0),
            mapping(RosterField::Name, 1),
        ];
        let preview = preview_roster_update(&draft, &mapping, &[], 0, "incremental", None).unwrap();

        assert!(!preview.can_apply);
        assert!(preview
            .conflicts
            .iter()
            .any(|conflict| conflict.code == "duplicate_incoming_student_id"));
        assert_eq!(
            preview
                .changes
                .iter()
                .filter(|change| change.action == "conflict")
                .count(),
            2
        );
    }

    #[test]
    fn preview_accepts_camel_case_current_students() {
        // The frontend sends the camelCase Student shape for current_students.
        let request: RosterUpdatePreviewRequest = serde_json::from_value(serde_json::json!({
            "mapping": [
                {"field": "student_id", "column_index": 0},
                {"field": "name", "column_index": 1},
            ],
            "mode": "incremental",
            "current_revision": 3,
            "current_students": [
                {"id": "S1", "name": "Alice", "heightCm": 160},
            ],
        }))
        .unwrap();
        assert_eq!(
            request.current_students[0].student_id.as_deref(),
            Some("S1")
        );
        assert_eq!(request.current_students[0].height_cm, Some(160.0));
        assert_eq!(request.current_revision, 3);
    }

    #[test]
    fn preview_rejects_non_numeric_score() {
        let draft = parse_roster_csv(b"id,score\nS1,abc\n").unwrap();
        let mapping = vec![
            mapping(RosterField::StudentId, 0),
            mapping(RosterField::Score, 1),
        ];
        let error =
            preview_roster_update(&draft, &mapping, &[], 0, "incremental", None).unwrap_err();
        assert!(
            error.contains("must be a number"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn draft_store_evicts_oldest_when_over_capacity() {
        let mut store = RosterDraftStore::with_limits(1, 3600);
        let first = store.create(parse_roster_csv(b"id,name\nS1,Alice\n").unwrap());
        let second = store.create(parse_roster_csv(b"id,name\nS2,Bob\n").unwrap());
        assert!(store.get(&first.draft_id).is_err());
        assert!(store.get(&second.draft_id).is_ok());
        assert!(store.delete(&second.draft_id));
        assert!(!store.delete(&second.draft_id));
    }

    #[test]
    fn parse_csv_enforces_row_limit() {
        let mut csv = String::from("id,name\n");
        for index in 0..=MAX_ROSTER_ROWS {
            csv.push_str(&format!("{index},student{index}\n"));
        }
        let error = parse_roster_csv(csv.as_bytes()).unwrap_err();
        assert!(error.contains("data rows"), "unexpected error: {error}");
    }

    #[test]
    fn json_helpers_roundtrip_upload_get_preview_delete() {
        let draft_json = upload_draft_json(b"id,name,score\nS1,Alice,93\nS2,Bob,81\n").unwrap();
        let draft: Value = serde_json::from_str(&draft_json).unwrap();
        let draft_id = draft["draft_id"].as_str().unwrap().to_string();
        assert_eq!(draft["source_format"], "csv");
        assert_eq!(draft["row_count"], 2);

        let fetched: Value = serde_json::from_str(&get_draft_json(&draft_id).unwrap()).unwrap();
        assert_eq!(fetched["draft_id"], draft_id);

        // The request body mirrors what the React workbench sends, including
        // camelCase student keys for current_students.
        let request = serde_json::json!({
            "mapping": [
                {"field": "student_id", "column_index": 0},
                {"field": "name", "column_index": 1},
                {"field": "score", "column_index": 2},
            ],
            "mode": "incremental",
            "current_revision": 5,
            "current_students": [
                {"id": "S1", "name": "Alice", "score": 70},
            ],
            "updated_fields": ["student_id", "name", "score"],
        });
        let preview_json = preview_update_json(&draft_id, &request.to_string()).unwrap();
        let preview: Value = serde_json::from_str(&preview_json).unwrap();
        assert_eq!(preview["base_revision"], 5);
        assert_eq!(preview["can_apply"], true);
        assert_eq!(preview["action_counts"]["update"], 1);
        assert_eq!(preview["action_counts"]["add"], 1);
        assert_eq!(preview["resulting_students"][0]["student_id"], "S1");
        assert_eq!(preview["resulting_students"][0]["score"], 93.0);

        assert!(delete_draft(&draft_id));
        assert!(!delete_draft(&draft_id));
    }

    #[test]
    fn draft_response_json_matches_contract_shape() {
        let draft =
            parse_roster_csv("姓名,学号,总分\n小林,001,91\n小周,002,88\n".as_bytes()).unwrap();
        let response = draft.to_response();
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["source_format"], "csv");
        assert_eq!(value["headerless"], false);
        assert_eq!(value["row_count"], 2);
        assert_eq!(value["column_count"], 3);
        assert_eq!(value["columns"][0]["header"], "姓名");
        assert!(value.get("preview_rows").is_some());
        assert!(value.get("suggested_mapping").is_some());
        assert_eq!(value["preview_rows"][0]["cells"][0], "小林");
    }

    #[test]
    fn preview_response_json_matches_contract_shape() {
        let draft = parse_roster_csv(b"id,name,score\nS1,Alice,93\n").unwrap();
        let current = vec![student_with_id_name_score("S1", "Alice", Some(70.0))];
        let preview = preview_roster_update(
            &draft,
            &id_name_score_mapping(),
            &current,
            7,
            "incremental",
            None,
        )
        .unwrap();
        let value = serde_json::to_value(preview.to_response("draft-1")).unwrap();
        assert_eq!(value["draft_id"], "draft-1");
        assert_eq!(value["base_revision"], 7);
        assert_eq!(value["mode"], "incremental");
        assert_eq!(value["can_apply"], true);
        assert_eq!(value["action_counts"]["update"], 1);
        assert_eq!(value["action_counts"]["unchanged"], 0);
        assert_eq!(value["changes"].as_array().unwrap().len(), 1);
        // resulting students are snake_case on the wire.
        let resulting = &value["resulting_students"][0];
        assert_eq!(resulting["student_id"], "S1");
        assert_eq!(resulting["score"], 93.0);
        assert!(resulting.get("height_cm").is_some());
        assert!(resulting.get("tags").is_some());
    }

    // -----------------------------------------------------------------------
    // XLSX import
    // -----------------------------------------------------------------------

    /// Build a minimal in-memory XLSX container from raw package parts.
    fn build_xlsx(parts: &[(&str, String)]) -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;
        use zip::CompressionMethod;
        use zip::ZipWriter;

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, content) in parts {
                writer.start_file(*name, options).unwrap();
                writer.write_all(content.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn workbook_xml(sheet_ref: bool) -> String {
        let sheet = if sheet_ref { r#" r:id="rId1""# } else { "" };
        format!(
            "<?xml version=\"1.0\"?>\
             <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
             xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
             <sheets><sheet name=\"Sheet1\" sheetId=\"1\"{sheet}/></sheets></workbook>"
        )
    }

    fn workbook_rels_xml() -> String {
        "<?xml version=\"1.0\"?>\
         <Relationships \
         xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" \
         Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" \
         Target=\"worksheets/sheet1.xml\"/></Relationships>"
            .to_string()
    }

    /// A workbook whose first sheet mixes every supported cell type, plus a
    /// ghost trailing row that must be skipped.
    fn mixed_type_xlsx() -> Vec<u8> {
        let shared_strings = "<?xml version=\"1.0\"?><sst \
             xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
             <si><t>学号</t></si><si><t>姓名</t></si><si><t>总分</t></si>\
             <si><t>001</t></si><si><t xml:space=\"preserve\">小明</t></si>\
             <si><t>小红&amp;小刚</t></si></sst>"
            .to_string();
        let sheet = "<?xml version=\"1.0\"?><worksheet \
             xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
             <sheetData>\
             <row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c>\
             <c r=\"C1\" t=\"s\"><v>2</v></c></row>\
             <row r=\"2\"><c r=\"A2\" t=\"s\"><v>3</v></c><c r=\"B2\" t=\"s\"><v>4</v></c>\
             <c r=\"C2\"><v>91</v></c></row>\
             <row r=\"3\"><c r=\"A3\" t=\"inlineStr\"><is><t>002</t></is></c>\
             <c r=\"B3\" t=\"s\"><v>5</v></c><c r=\"C3\" t=\"str\">\
             <f>CONCAT(A3,B3)</f><v>88.5</v></c></row>\
             <row r=\"4\"><c r=\"A4\"/></row>\
             </sheetData></worksheet>"
            .to_string();
        build_xlsx(&[
            ("xl/workbook.xml", workbook_xml(true)),
            ("xl/_rels/workbook.xml.rels", workbook_rels_xml()),
            ("xl/sharedStrings.xml", shared_strings),
            ("xl/worksheets/sheet1.xml", sheet),
        ])
    }

    #[test]
    fn parse_xlsx_mixed_cell_types_and_skips_ghost_rows() {
        let xlsx = mixed_type_xlsx();
        let draft = parse_roster_xlsx(&xlsx).unwrap();

        assert_eq!(draft.source_format, "xlsx");
        assert!(!draft.headerless);
        assert_eq!(draft.column_count(), 3);
        // The ghost row 4 is dropped; data rows keep spreadsheet numbering.
        assert_eq!(draft.row_count(), 2);
        assert_eq!(draft.columns[0].header, "学号");
        // Leading zeros survive via the shared-string text cell.
        assert_eq!(
            draft.rows[0].cells,
            vec!["001".to_string(), "小明".to_string(), "91".to_string()]
        );
        assert_eq!(
            draft.rows[1].cells,
            vec![
                "002".to_string(),
                "小红&小刚".to_string(),
                "88.5".to_string()
            ]
        );
        assert_eq!(draft.rows[0].row_number, 2);
        assert_eq!(draft.rows[1].row_number, 3);

        // Chinese headers map exactly like the CSV path.
        let suggested: Vec<(String, usize)> = draft
            .suggested_mapping
            .iter()
            .map(|item| (item.field.as_str().to_string(), item.column_index))
            .collect();
        assert_eq!(
            suggested,
            vec![
                ("student_id".to_string(), 0),
                ("name".to_string(), 1),
                ("score".to_string(), 2),
            ]
        );
    }

    #[test]
    fn parse_xlsx_preserves_text_ids_and_boolean_cells() {
        let sheet = "<?xml version=\"1.0\"?><worksheet \
             xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
             <sheetData>\
             <row r=\"1\"><c r=\"A1\"><t/></c><c r=\"B1\" t=\"s\"><v>0</v></c></row>\
             <row r=\"2\"><c r=\"A2\" t=\"b\"><v>1</v></c><c r=\"B2\"><v>7</v></c></row>\
             </sheetData></worksheet>"
            .to_string();
        let xlsx = build_xlsx(&[
            (
                "xl/workbook.xml",
                workbook_xml(false), // no r:id: falls back to the typed rel
            ),
            ("xl/_rels/workbook.xml.rels", workbook_rels_xml()),
            (
                "xl/sharedStrings.xml",
                "<sst xmlns=\"x\"><si><t>视力</t></si></sst>".to_string(),
            ),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let draft = parse_roster_xlsx(&xlsx).unwrap();
        assert_eq!(
            draft.rows[0].cells,
            vec!["TRUE".to_string(), "7".to_string()]
        );
        assert_eq!(draft.columns[1].header, "视力");
    }

    #[test]
    fn parse_xlsx_formula_without_cached_value_is_rejected() {
        let sheet = "<?xml version=\"1.0\"?><worksheet \
             xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
             <sheetData>\
             <row r=\"1\"><c r=\"A1\"><t/></c></row>\
             <row r=\"2\"><c r=\"A2\"><f>SUM(1,2)</f></c></row>\
             </sheetData></worksheet>"
            .to_string();
        let xlsx = build_xlsx(&[
            ("xl/workbook.xml", workbook_xml(true)),
            ("xl/_rels/workbook.xml.rels", workbook_rels_xml()),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let error = parse_roster_xlsx(&xlsx).unwrap_err();
        assert!(
            error.contains("formula without a cached value"),
            "unexpected error: {error}"
        );
        assert!(error.contains("row 2"), "unexpected error: {error}");
    }

    #[test]
    fn upload_pipeline_accepts_xlsx_and_previews_updates() {
        let draft_json =
            upload_draft_file_json(&mixed_type_xlsx(), Some("roster.xlsx"), None).unwrap();
        let draft: Value = serde_json::from_str(&draft_json).unwrap();
        assert_eq!(draft["source_format"], "xlsx");
        let draft_id = draft["draft_id"].as_str().unwrap().to_string();

        let request = serde_json::json!({
            "mapping": [
                {"field": "student_id", "column_index": 0},
                {"field": "name", "column_index": 1},
                {"field": "score", "column_index": 2},
            ],
            "mode": "incremental",
        });
        let preview_json = preview_update_json(&draft_id, &request.to_string()).unwrap();
        let preview: Value = serde_json::from_str(&preview_json).unwrap();
        assert_eq!(preview["action_counts"]["add"], 2);
        assert_eq!(preview["resulting_students"][0]["name"], "小明");

        // Content type alone also routes to the XLSX reader.
        let by_type = upload_draft_file_json(
            &mixed_type_xlsx(),
            None,
            Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&by_type).unwrap();
        assert_eq!(value["source_format"], "xlsx");
    }

    #[test]
    fn parse_xlsx_reports_corrupt_or_incomplete_workbooks() {
        // Not a zip container at all.
        let error = parse_roster_xlsx(b"definitely not a zip archive").unwrap_err();
        assert!(
            error.contains("not a valid OOXML archive"),
            "unexpected error: {error}"
        );

        // A valid zip without any OOXML parts.
        let empty_zip = build_xlsx(&[("random.txt", "hello".to_string())]);
        let error = parse_roster_xlsx(&empty_zip).unwrap_err();
        assert!(
            error.contains("missing the required entry xl/workbook.xml"),
            "unexpected error: {error}"
        );

        // Workbook metadata present but no worksheet part anywhere.
        let no_sheet = build_xlsx(&[
            ("xl/workbook.xml", workbook_xml(true)),
            ("xl/_rels/workbook.xml.rels", workbook_rels_xml()),
        ]);
        let error = parse_roster_xlsx(&no_sheet).unwrap_err();
        assert!(
            error.contains("does not contain a readable worksheet"),
            "unexpected error: {error}"
        );

        // An empty first sheet.
        let empty_sheet = build_xlsx(&[
            ("xl/workbook.xml", workbook_xml(true)),
            ("xl/_rels/workbook.xml.rels", workbook_rels_xml()),
            (
                "xl/worksheets/sheet1.xml",
                "<?xml version=\"1.0\"?><worksheet xmlns=\"x\"><sheetData/></worksheet>"
                    .to_string(),
            ),
        ]);
        let error = parse_roster_xlsx(&empty_sheet).unwrap_err();
        assert!(error.contains("empty"), "unexpected error: {error}");
    }

    #[test]
    fn parse_xlsx_enforces_decompressed_size_limit() {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;
        use zip::CompressionMethod;
        use zip::ZipWriter;

        // A tiny compressed entry that expands past MAX_ROSTER_FILE_BYTES.
        // Its declared header size is then forged down so only the real-byte
        // cap (Take + counted read) can catch it, like a crafted bomb would.
        let huge = format!("<v>{}</v>", "A".repeat(MAX_ROSTER_FILE_BYTES + 1024));
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file("xl/workbook.xml", options).unwrap();
            writer.write_all(workbook_xml(true).as_bytes()).unwrap();
            writer
                .start_file("xl/_rels/workbook.xml.rels", options)
                .unwrap();
            writer.write_all(workbook_rels_xml().as_bytes()).unwrap();
            writer
                .start_file("xl/worksheets/sheet1.xml", options)
                .unwrap();
            writer.write_all(huge.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        let mut bytes = cursor.into_inner();
        assert!(bytes.len() < 1024 * 1024, "fixture should compress well");
        forge_declared_size(&mut bytes, "xl/worksheets/sheet1.xml", huge.len(), 64);

        let error = parse_roster_xlsx(&bytes).unwrap_err();
        assert!(
            error.contains("more than the allowed"),
            "unexpected error: {error}"
        );
    }

    /// Overwrite the uncompressed-size fields (central directory + local file
    /// header) of entry `name` with `declared`, simulating a lying zip bomb
    /// header. Test-only; asserts the layout it patches still matches.
    fn forge_declared_size(bytes: &mut [u8], name: &str, real_size: usize, declared: u32) {
        let needle = name.as_bytes();
        let mut index = 0usize;
        while index + 46 <= bytes.len() {
            if &bytes[index..index + 4] == b"PK\x01\x02" && bytes[index + 46..].starts_with(needle)
            {
                let cd_size = &mut bytes[index + 24..index + 28];
                assert_eq!(
                    u32::from_le_bytes(cd_size.try_into().unwrap()),
                    real_size as u32,
                    "fixture layout changed: CD size mismatch"
                );
                cd_size.copy_from_slice(&declared.to_le_bytes());
                // This writer's central-directory records keep the local
                // header offset at +42.
                let local_offset =
                    u32::from_le_bytes(bytes[index + 42..index + 46].try_into().unwrap()) as usize;
                debug_assert_eq!(
                    &bytes[local_offset + 30..local_offset + 30 + needle.len()],
                    needle,
                    "fixture layout changed: local header name mismatch"
                );
                let lfh_size = &mut bytes[local_offset + 22..local_offset + 26];
                assert_eq!(
                    u32::from_le_bytes(lfh_size.try_into().unwrap()),
                    real_size as u32,
                    "fixture layout changed: LFH size mismatch"
                );
                lfh_size.copy_from_slice(&declared.to_le_bytes());
                return;
            }
            index += 1;
        }
        panic!("central directory entry {name} not found");
    }

    #[test]
    fn parse_cell_refs_cover_multi_letter_columns() {
        assert_eq!(parse_cell_ref("A1"), Some((1, 1)));
        assert_eq!(parse_cell_ref("Z9"), Some((9, 26)));
        assert_eq!(parse_cell_ref("AA12"), Some((12, 27)));
        assert_eq!(parse_cell_ref("abc"), None);
        assert_eq!(parse_cell_ref(""), None);
        assert_eq!(parse_cell_ref("0A"), None);
    }
}
