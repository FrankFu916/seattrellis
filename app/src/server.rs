//! Loopback-only HTTP backend for the SeatTrellis desktop app.
//!
//! Serves the compiled React workbench (`web_static/`) and exposes the native
//! endpoints the workbench's teacher flow needs end-to-end: roster upload &
//! preview, class generation (which also creates an editable draft), the
//! command-driven seating editor, export, and the static catalogs. The server
//! is deliberately dependency-free beyond `seattrellis_core` and `serde_json`:
//! a hand-rolled, minimal HTTP/1.1 server keeps the release binary small and
//! the surface auditable.
//!
//! Security posture (from-zero standards):
//! - Binds loopback only (`127.0.0.1`); never exposes a LAN address.
//! - No CORS headers are ever emitted; clients must already be same-origin.
//! - Static files are confined to the configured web root; `..` traversal and
//!   percent-encoded escapes are rejected, and canonical paths are re-checked.
//! - Errors are coarse (`404 not found`) and never leak internal paths.
//! - No unwrap/expect on the request path; all failures become HTTP errors.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use crate::editing::{self, EditorDraftStore, EditorSeatSpec};
use seattrellis_core::CoreSolveRequest;

/// Status-code -> reason text table for the responses we emit.
const STATUS_TEXT: &[(u16, &str)] = &[
    (100, "Continue"),
    (200, "OK"),
    (204, "No Content"),
    (400, "Bad Request"),
    (404, "Not Found"),
    (405, "Method Not Allowed"),
    (409, "Conflict"),
    (411, "Length Required"),
    (413, "Payload Too Large"),
    (422, "Unprocessable Entity"),
    (500, "Internal Server Error"),
    (501, "Not Implemented"),
];

/// Maximum size of the request head (request line + headers), in bytes.
const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Maximum accepted request body size, in bytes (64 MiB).
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Idle read/write timeout per connection.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

/// Compiled React workbench location resolved at build time. Used as a
/// fallback so the binary serves assets regardless of the launch directory.
const BUILTIN_WEB_STATIC: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../src/seattrellis/web_static");

/// Original solve request bodies, keyed by editor draft id. The export route
/// needs the request that produced a draft so it can reconstruct the full
/// renderable plan (request + current assignment) after edits.
type SolveRequestStore = Mutex<HashMap<String, Value>>;

/// Errors surfaced by [`resolve_web_root`] and [`Server::bind`].
#[derive(Debug)]
pub enum ServerError {
    /// TCP listener could not be bound (e.g. port already in use).
    Bind(io::Error),
    /// No usable workbench build was found.
    MissingWebRoot(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::Bind(e) => write!(f, "could not bind TCP listener: {e}"),
            ServerError::MissingWebRoot(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ServerError {}

/// Validated settings for the local backend.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Loopback address to bind. Only `127.0.0.1`/`::1` are appropriate.
    pub host: IpAddr,
    /// TCP port to bind.
    pub port: u16,
    /// Directory containing `index.html` plus static assets.
    pub web_root: PathBuf,
}

impl ServerConfig {
    pub fn new(port: u16, web_root: PathBuf) -> Self {
        ServerConfig {
            host: IpAddr::from([127, 0, 0, 1]),
            port,
            web_root,
        }
    }
}

/// The running backend: a bound loopback listener plus the web root and the
/// in-process stores shared across connection threads.
pub struct Server {
    listener: TcpListener,
    local_addr: SocketAddr,
    web_root: Arc<PathBuf>,
    editor_store: Arc<EditorDraftStore>,
    solve_requests: Arc<SolveRequestStore>,
}

impl Server {
    /// Bind the loopback listener. Fails if the address/port is unavailable.
    pub fn bind(config: &ServerConfig) -> Result<Server, ServerError> {
        let addr = SocketAddr::new(config.host, config.port);
        let listener = TcpListener::bind(addr).map_err(ServerError::Bind)?;
        let local_addr = listener.local_addr().map_err(ServerError::Bind)?;
        Ok(Server {
            listener,
            local_addr,
            web_root: Arc::new(config.web_root.clone()),
            editor_store: Arc::new(editing::new_draft_store()),
            solve_requests: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// The actual bound address (useful when port 0 auto-assigns).
    pub fn addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Accept connections forever, handling each on its own thread.
    pub fn serve(&self) -> io::Result<()> {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let web_root = Arc::clone(&self.web_root);
                    let editor_store = Arc::clone(&self.editor_store);
                    let solve_requests = Arc::clone(&self.solve_requests);
                    let _ = thread::Builder::new()
                        .name("seattrellis-conn".to_string())
                        .spawn(move || {
                            handle_connection(stream, web_root, editor_store, solve_requests);
                        });
                }
                Err(error) => eprintln!("[seattrellis] accept error: {error}"),
            }
        }
        Ok(())
    }
}

/// Locate a complete workbench build (`index.html` present) from, in order:
/// 1. the `SEATTRELLIS_WEB_STATIC` env var,
/// 2. the launch working directory (`src/seattrellis/web_static` or a
///    `../src/...` when launched from the app crate),
/// 3. the compile-time path baked into the binary.
pub fn resolve_web_root() -> Result<PathBuf, ServerError> {
    let candidates = [
        std::env::var_os("SEATTRELLIS_WEB_STATIC").map(PathBuf::from),
        Some(PathBuf::from("src/seattrellis/web_static")),
        Some(PathBuf::from("../src/seattrellis/web_static")),
        Some(PathBuf::from(BUILTIN_WEB_STATIC)),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Ok(resolved) = candidate.canonicalize() {
            if resolved.join("index.html").is_file() {
                return Ok(resolved);
            }
        }
    }

    Err(ServerError::MissingWebRoot(format!(
        "no workbench build found under SEATTRELLIS_WEB_STATIC, the launch \
         directory, or the built-in path {BUILTIN_WEB_STATIC:?}; build the \
         React frontend first (index.html must exist in web_static/)"
    )))
}

// ---------------------------------------------------------------------------
// Connection handling
// ---------------------------------------------------------------------------

/// Read one request from a client and respond with one response, then close.
/// Uses a `Connection: close` model: one request per connection keeps the
/// hand-rolled parser simple and is entirely adequate for a single-user app.
fn handle_connection(
    stream: TcpStream,
    web_root: Arc<PathBuf>,
    editor_store: Arc<EditorDraftStore>,
    solve_requests: Arc<SolveRequestStore>,
) {
    let _ = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
    let _ = stream.set_nodelay(true);

    // Split the stream so we can emit `100 Continue` while still reading.
    let mut write_stream = match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);

    let request = match read_request(&mut reader, &mut write_stream) {
        Ok(Some(request)) => request,
        Ok(None) => return, // clean EOF (client closed)
        Err(status) => {
            let response = plain_response(status, "bad request");
            let _ = write_response(&mut write_stream, &response);
            return;
        }
    };

    let response = route(&request, &web_root, &editor_store, &solve_requests);
    let _ = write_response(&mut write_stream, &response);
}

/// A parsed HTTP/1.1 request (head + body). The head's `Content-Length` /
/// `Expect` / `Transfer-Encoding` headers are consumed during parsing.
struct Request {
    method: String,
    path: String,
    /// The request's `Content-Type` header, if any (needed for multipart).
    content_type: Option<String>,
    body: Vec<u8>,
}

/// Parse a request head + body from the reader. Returns `None` on clean EOF.
/// On malformed input returns the HTTP status that should be replied with.
fn read_request(
    reader: &mut BufReader<TcpStream>,
    write_stream: &mut TcpStream,
) -> Result<Option<Request>, u16> {
    let mut line = String::new();
    let mut head_bytes = 0usize;

    // Request line, skipping stray blank lines (e.g. keep-alive remnants).
    let request_line;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|_| 400u16)?;
        if n == 0 {
            return Ok(None);
        }
        head_bytes += line.len();
        if head_bytes > MAX_HEAD_BYTES {
            return Err(413u16);
        }
        let trimmed = trim_line(&line);
        if !trimmed.is_empty() {
            request_line = trimmed.to_string();
            break;
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or(400u16)?.to_ascii_uppercase();
    let path = parts.next().ok_or(400u16)?.to_string();
    let _version = parts.next().ok_or(400u16)?.to_string();
    if parts.next().is_some() {
        return Err(400u16);
    }

    // Headers until the blank line.
    let mut headers: HashMap<String, String> = HashMap::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|_| 400u16)?;
        head_bytes += line.len();
        if head_bytes > MAX_HEAD_BYTES {
            return Err(413u16);
        }
        if n == 0 {
            return Err(400u16); // EOF mid-head
        }
        let trimmed = trim_line(&line);
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    // Honour `Expect: 100-continue` so curl (and any large POST) works.
    if headers
        .get("expect")
        .is_some_and(|value| value.to_ascii_lowercase().contains("100-continue"))
    {
        let _ = write_stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
        let _ = write_stream.flush();
    }

    let body = if method == "POST" {
        let transfer_encoding = headers
            .get("transfer-encoding")
            .map(|value| value.to_ascii_lowercase());
        if transfer_encoding.is_some_and(|value| value != "identity") {
            return Err(501u16);
        }
        let content_length = match headers.get("content-length") {
            Some(value) => value.parse::<usize>().map_err(|_| 400u16)?,
            None => 0,
        };
        if content_length > MAX_BODY_BYTES {
            return Err(411u16);
        }
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).map_err(|_| 400u16)?;
        }
        body
    } else {
        Vec::new()
    };

    Ok(Some(Request {
        method,
        path,
        content_type: headers.get("content-type").cloned(),
        body,
    }))
}

fn trim_line(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

// ---------------------------------------------------------------------------
// Routing and handlers
// ---------------------------------------------------------------------------

/// A minimal response: status code, optional content type, optional
/// `Content-Disposition`, and the raw body.
struct Response {
    status: u16,
    content_type: Option<&'static str>,
    content_disposition: Option<String>,
    body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: serde_json::Value) -> Response {
        let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
        Response {
            status,
            content_type: Some("application/json; charset=utf-8"),
            content_disposition: None,
            body,
        }
    }

    fn text(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Response {
        Response {
            status,
            content_type: Some(content_type),
            content_disposition: None,
            body: body.into(),
        }
    }
}

fn plain_response(status: u16, message: &str) -> Response {
    Response::text(status, "text/plain; charset=utf-8", message.to_string())
}

fn json_error(status: u16, message: &str) -> Response {
    Response::json(status, json!({ "error": message }))
}

/// A structured error with a machine-readable `code` and a human `message`,
/// mirroring the `plan_not_found` / `invalid_class_draft` style responses.
fn code_json_error(status: u16, code: &str, message: &str) -> Response {
    Response::json(status, json!({ "error": code, "message": message }))
}

/// Split a request path (query string already stripped) into segments.
fn path_segments(path: &str) -> Vec<&str> {
    path.trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Dispatch a parsed request to the matching handler.
fn route(
    request: &Request,
    web_root: &Path,
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Response {
    // Split the query string off for routing: the raw path decides the route,
    // and the query is handed to handlers that read it (e.g. `projects/recent`).
    let (path, query) = match request.path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (&request.path[..], None),
    };
    let segments = path_segments(path);

    match (request.method.as_str(), segments.as_slice()) {
        ("GET", ["api", "v1", "health"]) => health_response(),
        ("GET", ["api", "v1", "catalogs"]) => catalogs_response(),
        ("POST", ["api", "v1", "classes", "generate"])
        | ("POST", ["api", "v1", "solve"]) => {
            generate_response(&request.body, editor_store, solve_requests)
        }
        ("POST", ["api", "v1", "rosters", "drafts"]) => {
            roster_upload_response(&request.body, request.content_type.as_deref())
        }
        ("GET", ["api", "v1", "rosters", "drafts", draft_id]) => {
            roster_get_response(draft_id)
        }
        ("POST", ["api", "v1", "rosters", "drafts", draft_id, "preview"]) => {
            roster_preview_response(draft_id, &request.body)
        }
        ("DELETE", ["api", "v1", "rosters", "drafts", draft_id]) => {
            roster_delete_response(draft_id)
        }
        ("GET", ["api", "v1", "editing", "drafts", draft_id]) => {
            editing_fetch_response(draft_id, editor_store)
        }
        ("POST", ["api", "v1", "editing", "drafts", draft_id, "commands"]) => {
            editing_command_response(draft_id, &request.body, editor_store)
        }
        ("POST", ["api", "v1", "exports"]) => {
            export_response(&request.body, editor_store, solve_requests)
        }
        ("POST", ["api", "v1", "layouts", "drafts"]) => {
            layout_create_response(&request.body)
        }
        ("GET", ["api", "v1", "layouts", "drafts", draft_id]) => {
            layout_get_response(draft_id)
        }
        ("POST", ["api", "v1", "layouts", "drafts", draft_id, "commands"]) => {
            layout_command_response(draft_id, &request.body)
        }
        ("GET", ["api", "v1", "layouts", "drafts", draft_id, "compiled"]) => {
            layout_compiled_response(draft_id)
        }
        ("DELETE", ["api", "v1", "layouts", "drafts", draft_id]) => {
            layout_delete_response(draft_id)
        }
        ("GET", ["api", "v1", "projects", "recent"]) => {
            projects_recent_response(query)
        }
        ("POST", ["api", "v1", "projects", "history"]) => {
            project_history_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "privacy"]) => {
            project_privacy_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "bundle"]) => {
            project_bundle_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "restore"]) => {
            project_restore_response(&request.body, request.content_type.as_deref())
        }
        ("POST", ["api", "v1", "projects", "migration", "preview"]) => {
            migration_preview_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "migration", "apply"]) => {
            migration_apply_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "migration", "reference-checks"]) => {
            migration_reference_checks_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "migration", "batch", "preview"]) => {
            migration_batch_preview_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "migration", "batch", "apply"]) => {
            migration_batch_apply_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "migration", "restore"]) => {
            migration_restore_response(&request.body)
        }
        ("GET", []) | ("GET", ["index.html"]) => index_response(web_root),
        ("GET", _) if path.starts_with("/api/") => json_error(404, "not found"),
        ("GET", _) => static_response(web_root, path),
        ("POST", _) => json_error(404, "not found"),
        _ => plain_response(405, "method not allowed"),
    }
}

fn health_response() -> Response {
    Response::json(
        200,
        json!({
            "status": "ok",
            "service": "seattrellis",
            "api_version": "1",
        }),
    )
}

/// `GET /api/v1/catalogs`: static bilingual teacher catalogs, matching the
/// workbench `CatalogResponse` contract. Only lists export formats the native
/// renderer can actually produce.
fn catalogs_response() -> Response {
    Response::json(
        200,
        json!({
            "roomTemplates": [
                {
                    "id": "standard-30",
                    "name": localized("30 座教室", "30-seat classroom"),
                    "description": localized(
                        "5 排 × 6 座，中央过道，适合小班。",
                        "5 rows of 6 seats with a center aisle for a smaller class."
                    ),
                    "rows": 5,
                    "columns": 6,
                },
                {
                    "id": "standard-48",
                    "name": localized("48 座教室", "48-seat classroom"),
                    "description": localized(
                        "6 排 × 8 座，中央过道，适合常规班级。",
                        "6 rows of 8 seats with a center aisle for a typical class."
                    ),
                    "rows": 6,
                    "columns": 8,
                },
                {
                    "id": "standard-60",
                    "name": localized("60 座教室", "60-seat classroom"),
                    "description": localized(
                        "6 排 × 10 座，中央过道，适合大班。",
                        "6 rows of 10 seats with a center aisle for a larger class."
                    ),
                    "rows": 6,
                    "columns": 10,
                },
            ],
            "teacherGoals": [
                {
                    "id": "daily-rotation",
                    "name": localized("日常轮换", "Daily rotation"),
                    "description": localized(
                        "兼顾视力和身高需求，减少近期重复邻座，并适度轮换位置。",
                        "Balance vision and height needs, vary recent neighbors, and rotate seats for everyday classroom use."
                    ),
                },
                {
                    "id": "quick-shuffle",
                    "name": localized("快速打乱", "Quick shuffle"),
                    "description": localized(
                        "不依赖成绩或历史记录，快速生成一组中性的随机座位方案。",
                        "Create a neutral shuffle without relying on scores or saved history."
                    ),
                },
                {
                    "id": "fair-shuffle",
                    "name": localized("公平轮换", "Fair shuffle"),
                    "description": localized(
                        "优先参考历史座位，让每名学生逐步获得不同的位置和邻座。",
                        "Use seating history to give each student a wider range of positions and neighbors over time."
                    ),
                },
                {
                    "id": "peer-support",
                    "name": localized("邻座互助", "Peer support"),
                    "description": localized(
                        "让成绩层次不同的学生在邻座范围内适度混合。",
                        "Mix students from different score ranges across neighboring seats."
                    ),
                },
            ],
            "exportFormats": [
                {
                    "id": "svg",
                    "name": localized("SVG 矢量图", "SVG image"),
                    "description": localized("矢量格式，方便继续编辑。", "Vector image that stays easy to edit."),
                },
                {
                    "id": "html",
                    "name": localized("网页版", "HTML"),
                    "description": localized("适合在浏览器中查看。", "Open in any browser."),
                },
                {
                    "id": "png",
                    "name": localized("PNG 图片", "PNG image"),
                    "description": localized("适合截图和分享。", "A simple image for sharing."),
                },
                {
                    "id": "pdf",
                    "name": localized("PDF", "PDF"),
                    "description": localized("适合打印或分发。", "Best for printing and sharing."),
                },
                {
                    "id": "print-html",
                    "name": localized("打印版", "Print sheet"),
                    "description": localized("适合 A4 打印或存为 PDF。", "Designed for A4 printing or saving as PDF."),
                },
            ],
        }),
    )
}

fn localized(zh: &str, en: &str) -> Value {
    json!({ "zh-CN": zh, "en": en })
}

/// `POST /api/v1/classes/generate` (and `/api/v1/solve`): run the native
/// cost-ranked greedy solver over the request body, then open an editable
/// draft for the recommended plan so the workbench can adjust and export it.
///
/// Two request shapes are accepted:
///
/// 1. The raw `CoreSolveRequest` (`api_version` / `student_count` /
///    `seat_positions` / ...), used by tests and advanced clients.
/// 2. The React workbench's `GenerateClassRequest`
///    (`draft.students` + `draft.room.template_id` + `draft.goal.goal_id`),
///    detected by the presence of `draft.room.template_id` and expanded into
///    a `CoreSolveRequest` via [`crate::room_templates::room_template_grid`]
///    and [`crate::goal_rules::goal_rules`] before solving.
///
/// Returns the frontend `GenerateClassResponse` shape (`class_name`, `goal`,
/// `warnings`, `recommended_candidate_id`, `candidates`, `editor`). When the
/// solver reports the plan infeasible, the response is `409 plan_not_found`;
/// an unknown room template or goal on the frontend path is `422`.
fn generate_response(
    body: &[u8],
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Response {
    if body.is_empty() {
        return json_error(400, "empty request body");
    }
    let raw_request: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return json_error(400, "request body is not valid JSON"),
    };

    // Expand the workbench's GenerateClassRequest into the core solve shape;
    // anything without a `draft.room.template_id` is already a CoreSolveRequest.
    let (core_request, goal_id) = if is_frontend_class_request(&raw_request) {
        let goal_id = raw_request
            .pointer("/draft/goal/goal_id")
            .and_then(Value::as_str)
            .unwrap_or("daily-rotation")
            .to_string();
        match frontend_class_request_to_core(&raw_request) {
            Ok(value) => (value, goal_id),
            Err(response) => return response,
        }
    } else {
        (raw_request.clone(), "daily-rotation".to_string())
    };

    let request: CoreSolveRequest = match serde_json::from_value(core_request.clone()) {
        Ok(request) => request,
        Err(_) => return json_error(400, "request body is not a valid solve problem"),
    };

    let response = match seattrellis_core::solve_problem(&request) {
        Ok(response) => response,
        // Domain messages (capacity, unsupported api_version, ...) are fine to
        // return verbatim; the JSON parse errors above are kept coarse.
        Err(message) => return json_error(400, &message),
    };
    if !response.feasible {
        return Response::json(
            409,
            json!({
                "error": "plan_not_found",
                "message": "No seating plan was found with the current room and rules.",
            }),
        );
    }

    // Open an editable draft mirroring the recommended plan.
    let keys: Vec<String> = student_keys(&request);
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let seats = seat_specs(&request);
    let seat_ids: Vec<String> = (0..request.seat_positions.len())
        .map(|index| seat_id_for_index(&request, index))
        .collect();
    let assignment: Vec<(&str, &str)> = response
        .assignment
        .iter()
        .filter(|[student, seat]| *student < key_refs.len() && *seat < seat_ids.len())
        .map(|[student, seat]| (key_refs[*student], seat_ids[*seat].as_str()))
        .collect();

    let draft_id = new_draft_id();
    let editor = match editing::create_draft(
        editor_store,
        draft_id.clone(),
        Some(draft_id.clone()),
        &key_refs,
        seats,
        &assignment,
    ) {
        Ok(state) => state,
        Err(message) => return json_error(500, &message),
    };

    // Remember the (core-shaped) request that produced this draft so export
    // can rebuild the full plan (request + current assignment) after edits.
    match solve_requests.lock() {
        Ok(mut guard) => {
            guard.insert(draft_id.clone(), core_request);
        }
        Err(_) => return json_error(500, "solve request store is poisoned"),
    }

    let class_name = request
        .layout
        .as_ref()
        .map(|layout| layout.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Classroom".to_string());
    let total_score = response.total_cost.unwrap_or(0.0);

    Response::json(
        200,
        json!({
            "class_name": class_name,
            "goal": {
                "goal_id": goal_id,
                "title": "日常轮换",
                "description": "兼顾视力和身高需求，减少近期重复邻座，并适度轮换位置。",
                "preset_name": null,
            },
            "warnings": [],
            "recommended_candidate_id": draft_id,
            "candidates": [{
                "candidate_id": draft_id,
                "recommended": true,
                "total_score": total_score,
            }],
            "editor": editor,
        }),
    )
}

/// `true` when the body is a React workbench `GenerateClassRequest`, i.e. it
/// carries a `draft` object whose `room.template_id` names a room template.
fn is_frontend_class_request(value: &Value) -> bool {
    value
        .pointer("/draft/room/template_id")
        .and_then(Value::as_str)
        .map(|template_id| !template_id.is_empty())
        .unwrap_or(false)
}

/// Adapt a React `GenerateClassRequest` (`draft.students` +
/// `draft.room.template_id` + `draft.goal.goal_id`) into the core
/// `CoreSolveRequest` JSON document, expanding the room template grid and the
/// goal rule-set and mapping each student record onto the core `Student`
/// shape (`key`/`display_name`/`score`/`height_cm`/`vision`/`tags`/`needs`).
///
/// Returns a `422` response naming the missing piece when the draft is
/// malformed (`invalid_class_draft`), the room template is unknown
/// (`room_not_found`) or the goal is unknown (`unknown_goal`).
fn frontend_class_request_to_core(value: &Value) -> Result<Value, Response> {
    let draft = value
        .get("draft")
        .and_then(Value::as_object)
        .ok_or_else(|| code_json_error(422, "invalid_class_draft", "missing 'draft' object"))?;
    let template_id = draft
        .get("room")
        .and_then(Value::as_object)
        .and_then(|room| room.get("template_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            code_json_error(422, "invalid_class_draft", "missing draft.room.template_id")
        })?;
    let goal_id = draft
        .get("goal")
        .and_then(Value::as_object)
        .and_then(|goal| goal.get("goal_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| code_json_error(422, "invalid_class_draft", "missing draft.goal.goal_id"))?;

    let grid = match crate::room_templates::room_template_grid(template_id) {
        Ok(grid) => grid,
        Err(message) => return Err(code_json_error(422, "room_not_found", &message)),
    };
    let rules = match crate::goal_rules::goal_rules(goal_id) {
        Ok(rules) => rules,
        Err(message) => return Err(code_json_error(422, "unknown_goal", &message)),
    };

    let students: Vec<Value> = draft
        .get("students")
        .and_then(Value::as_array)
        .map(|students| students.iter().map(core_student_value).collect())
        .unwrap_or_default();

    let options = value.get("options").and_then(Value::as_object);
    let seed = options
        .and_then(|options| options.get("seed"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SEED);

    // The solver, the editor-draft builder and the renderer all assume
    // `layout.seats[index]` aligns one-to-one with `seat_positions[index]`.
    // The grid's full layout also carries disabled aisle cells, so hand the
    // request the *enabled* seats in layout order (which `room_templates`
    // guarantees are exactly `seat_positions`, in order). The physical aisle
    // gap is preserved anyway because `seat_positions` skip the aisle column.
    let layout = crate::room_templates::Layout {
        layout_id: grid.layout.layout_id.clone(),
        name: grid.layout.name.clone(),
        seats: grid
            .layout
            .enabled_seats()
            .iter()
            .map(|seat| (*seat).clone())
            .collect(),
        adjacency: grid.layout.adjacency.clone(),
    };

    Ok(json!({
        "api_version": 2,
        "student_count": students.len(),
        "seat_positions": grid.seat_positions.clone(),
        "edges": grid.edges.clone(),
        "layout": layout,
        "rules": rules,
        "students": students,
        "seed": seed,
    }))
}

/// Default solve seed when the frontend sends no `options.seed` (matches the
/// rule-set default in `goal_rules.rs` / the core `RuleSet` model).
const DEFAULT_SEED: u64 = 42;

/// Map one React `draft.students` entry onto the core `Student` JSON shape.
/// Absent or `null` fields are omitted so they deserialize to the core
/// defaults; `vision` follows the core convention of storing its string
/// rendering (`0.8` -> `"0.8"`, `"poor"` -> `"poor"`).
fn core_student_value(student: &Value) -> Value {
    let mut result = serde_json::Map::new();
    // The core `key` mirrors Python's `student_id or name or ""`.
    let student_id = student.get("student_id").and_then(Value::as_str).unwrap_or("");
    let name = student.get("name").and_then(Value::as_str).unwrap_or("");
    let key = if !student_id.is_empty() { student_id } else { name };
    if !key.is_empty() {
        result.insert("key".to_string(), json!(key));
    }
    if !name.is_empty() {
        result.insert("display_name".to_string(), json!(name));
    }
    if let Some(score) = student.get("score").and_then(Value::as_f64) {
        result.insert("score".to_string(), json!(score));
    }
    if let Some(height_cm) = student.get("height_cm").and_then(Value::as_f64) {
        result.insert("height_cm".to_string(), json!(height_cm));
    }
    if let Some(vision) = student.get("vision") {
        match vision {
            Value::String(text) if !text.is_empty() => {
                result.insert("vision".to_string(), json!(text));
            }
            Value::Number(number) => {
                result.insert("vision".to_string(), json!(number.to_string()));
            }
            _ => {}
        }
    }
    if let Some(tags) = student.get("tags").and_then(Value::as_array) {
        result.insert("tags".to_string(), json!(tags));
    }
    if let Some(needs) = student.get("needs").and_then(Value::as_array) {
        result.insert("needs".to_string(), json!(needs));
    }
    Value::Object(result)
}

/// Student keys for an editor draft: the solve request's `students` `key`,
/// falling back to `student-N` for placeholder/padded students.
fn student_keys(request: &CoreSolveRequest) -> Vec<String> {
    (0..request.student_count)
        .map(|index| {
            request
                .students
                .get(index)
                .map(|student| student.key.trim())
                .filter(|key| !key.is_empty())
                .map(|key| key.to_string())
                .unwrap_or_else(|| format!("student-{}", index + 1))
        })
        .collect()
}

/// Seat specs for an editor draft: prefer the layout's authoritative
/// row/col/enabled per seat; otherwise derive grid coordinates from the raw
/// `seat_positions` (mirrors `render::seat_row_col`).
fn seat_specs(request: &CoreSolveRequest) -> Vec<EditorSeatSpec> {
    request
        .seat_positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let (row, col, enabled) = match request.layout.as_ref() {
                Some(layout) => match layout.seats.get(index) {
                    Some(seat) => (seat.row, seat.col, seat.enabled),
                    None => fallback_coordinates(position),
                },
                None => fallback_coordinates(position),
            };
            EditorSeatSpec {
                seat_id: seat_id_for_index(request, index),
                row,
                col,
                enabled,
            }
        })
        .collect()
}

/// The seat id the editor draft uses for a seat index: the layout's `seat_id`
/// when present, else `seat-N`.
fn seat_id_for_index(request: &CoreSolveRequest, index: usize) -> String {
    request
        .layout
        .as_ref()
        .and_then(|layout| layout.seats.get(index))
        .map(|seat| seat.seat_id.clone())
        .unwrap_or_else(|| format!("seat-{}", index + 1))
}

fn fallback_coordinates(position: &[f64; 2]) -> (i32, i32, bool) {
    (position[1].round() as i32, position[0].round() as i32, true)
}

/// `POST /api/v1/rosters/drafts`: parse a multipart `file` field and store the
/// parsed roster, returning the `RosterDraftResponse`.
fn roster_upload_response(body: &[u8], content_type: Option<&str>) -> Response {
    let Some(content_type) = content_type else {
        return json_error(400, "multipart/form-data upload expected");
    };
    let Some(boundary) = multipart_boundary(content_type) else {
        return json_error(400, "multipart/form-data boundary is missing");
    };
    let fields = match parse_multipart(body, &boundary) {
        Ok(fields) => fields,
        Err(message) => return json_error(422, &message),
    };
    let Some(file_bytes) = fields.get("file") else {
        return json_error(422, "upload is missing a 'file' field");
    };
    if file_bytes.is_empty() {
        return json_error(422, "uploaded roster file is empty");
    }
    match crate::roster::upload_draft_json(file_bytes) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => json_error(422, &message),
    }
}

fn roster_get_response(draft_id: &str) -> Response {
    match crate::roster::get_draft_json(draft_id) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(_) => json_error(404, "roster draft was not found"),
    }
}

fn roster_preview_response(draft_id: &str, body: &[u8]) -> Response {
    let body_str = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return json_error(400, "request body is not valid UTF-8"),
    };
    match crate::roster::preview_update_json(draft_id, body_str) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("not found") => {
            json_error(404, "roster draft was not found")
        }
        Err(message) => json_error(400, &message),
    }
}

fn roster_delete_response(draft_id: &str) -> Response {
    if crate::roster::delete_draft(draft_id) {
        Response {
            status: 204,
            content_type: None,
            content_disposition: None,
            body: Vec::new(),
        }
    } else {
        json_error(404, "roster draft was not found")
    }
}

fn editing_fetch_response(draft_id: &str, editor_store: &EditorDraftStore) -> Response {
    match editing::fetch_state(editor_store, draft_id) {
        Ok(state) => Response::json(200, serde_json::to_value(state).unwrap_or(json!({}))),
        Err(_) => json_error(404, "editor draft was not found"),
    }
}

/// `POST /api/v1/editing/drafts/{id}/commands`: apply a versioned editor
/// command. Maps domain errors to 400 (bad command), 404 (unknown draft), or
/// 409 (stale revision / protocol / duplicate / wrong-target conflicts).
fn editing_command_response(
    draft_id: &str,
    body: &[u8],
    editor_store: &EditorDraftStore,
) -> Response {
    let envelope: editing::EditorCommandEnvelope = match serde_json::from_slice(body) {
        Ok(envelope) => envelope,
        Err(_) => {
            return json_error(400, "command body is not a valid editor command envelope");
        }
    };
    if envelope.draft_id != draft_id {
        return json_error(409, "The editor command targets a different draft.");
    }
    match editing::apply_command_in_store(editor_store, &envelope) {
        Ok(state) => Response::json(200, serde_json::to_value(state).unwrap_or(json!({}))),
        Err(message) => {
            let status = if message.contains("unknown editor draft") {
                404
            } else if message.contains("stale")
                || message.contains("protocol version")
                || message.contains("already been applied")
                || message.contains("different draft")
                || message.contains("command kind")
                || message.contains("command_id")
            {
                409
            } else {
                400
            };
            json_error(status, &message)
        }
    }
}

/// `POST /api/v1/exports`: take the frontend `ExportDraftRequest`, fold in the
/// stored solve request and the draft's current assignment, and render bytes
/// with the matching `Content-Type` and an attachment filename.
fn export_response(
    body: &[u8],
    editor_store: &EditorDraftStore,
    solve_requests: &SolveRequestStore,
) -> Response {
    if body.is_empty() {
        return json_error(400, "empty request body");
    }
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return json_error(400, "export request is not valid JSON"),
    };
    let draft_id = value
        .get("draft_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if draft_id.is_empty() {
        return json_error(400, "export request is missing a 'draft_id'");
    }

    let request_value = match solve_requests.lock() {
        Ok(guard) => match guard.get(draft_id) {
            Some(value) => value.clone(),
            None => return json_error(404, "editor draft was not found"),
        },
        Err(_) => return json_error(500, "solve request store is poisoned"),
    };
    // Fetching the state also validates that the draft still exists.
    let state = match editing::fetch_state(editor_store, draft_id) {
        Ok(state) => state,
        Err(_) => return json_error(404, "editor draft was not found"),
    };
    let response_value = export_response_value(&request_value, &state);

    let mut export_json = value;
    if let Some(object) = export_json.as_object_mut() {
        // `print-html` renders the same native HTML sheet as `html`.
        let format = object.get("format").and_then(Value::as_str).unwrap_or("");
        if format.eq_ignore_ascii_case("print-html") {
            object.insert("format".to_string(), json!("html"));
        }
        object.insert("request".to_string(), request_value);
        object.insert("response".to_string(), response_value);
    }
    let export_string = export_json.to_string();

    let format = match crate::export::format_of(&export_string) {
        Ok(format) => format,
        Err(message) => return json_error(400, &message),
    };
    let bytes = match crate::export::export_plan(&export_string) {
        Ok(bytes) => bytes,
        Err(message) => return json_error(400, &message),
    };

    let filename = format!("seat-plan.{}", format.extension());
    Response {
        status: 200,
        content_type: Some(format.mime()),
        content_disposition: Some(format!("attachment; filename=\"{filename}\"")),
        body: bytes,
    }
}

/// Reconstruct the `CoreSolveResponse`-shaped JSON for export from the current
/// editor state, so exports reflect manual adjustments, not the original solve.
fn export_response_value(request_value: &Value, state: &editing::EditorState) -> Value {
    // student key -> index, using the same fallback keys as the draft builder.
    let students = request_value.get("students").and_then(Value::as_array);
    let student_count = request_value
        .get("student_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut student_index: HashMap<String, usize> = HashMap::new();
    for index in 0..student_count {
        let key = students
            .and_then(|list| list.get(index))
            .and_then(|student| student.get("key"))
            .and_then(Value::as_str)
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| format!("student-{}", index + 1));
        student_index.insert(key, index);
    }

    // seat_id -> index, using the same seat ids as the draft builder.
    let layout_seats = request_value
        .get("layout")
        .and_then(|layout| layout.get("seats"))
        .and_then(Value::as_array);
    let seat_count = request_value
        .get("seat_positions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let mut seat_index: HashMap<String, usize> = HashMap::new();
    for index in 0..seat_count {
        let seat_id = layout_seats
            .and_then(|list| list.get(index))
            .and_then(|seat| seat.get("seat_id"))
            .and_then(Value::as_str)
            .map(|seat_id| seat_id.to_string())
            .unwrap_or_else(|| format!("seat-{}", index + 1));
        seat_index.insert(seat_id, index);
    }

    let mut assignment: Vec<[usize; 2]> = Vec::new();
    for student in &state.students {
        if let Some(seat_id) = &student.seat_id {
            if let (Some(&student_idx), Some(&seat_idx)) =
                (student_index.get(&student.student_key), seat_index.get(seat_id))
            {
                assignment.push([student_idx, seat_idx]);
            }
        }
    }

    json!({
        "api_version": 2,
        "feasible": true,
        "assignment": assignment,
        "attempts_used": 1,
        "hard_constraints_satisfied": true,
        "total_cost": null,
    })
}

// ---------------------------------------------------------------------------
// Layout routes
// ---------------------------------------------------------------------------

/// `POST /api/v1/layouts/drafts`: create a layout draft from a
/// `CreateLayoutDraftRequest` JSON document and return the initial
/// `LayoutStateResponse`. Domain validation failures (missing name, multiple
/// sources, unknown template, oversized grid) are 422.
fn layout_create_response(body: &[u8]) -> Response {
    if body.is_empty() {
        return json_error(400, "empty request body");
    }
    let body_str = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return json_error(400, "request body is not valid UTF-8"),
    };
    match crate::layouts::create_layout_draft_json(body_str) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("poisoned") => json_error(500, &message),
        Err(message) => json_error(422, &message),
    }
}

/// `GET /api/v1/layouts/drafts/{id}`: fetch the current layout state.
fn layout_get_response(draft_id: &str) -> Response {
    match crate::layouts::get_layout_state_json(draft_id) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("poisoned") => json_error(500, &message),
        Err(_) => json_error(404, "layout draft was not found"),
    }
}

/// `POST /api/v1/layouts/drafts/{id}/commands`: dispatch a layout command.
/// Maps domain errors to 400 (bad command), 404 (unknown draft), or 409
/// (stale revision / duplicate / wrong-target conflicts).
fn layout_command_response(draft_id: &str, body: &[u8]) -> Response {
    let body_str = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return json_error(400, "command body is not valid UTF-8"),
    };
    match crate::layouts::dispatch_layout_command_json(draft_id, body_str) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => {
            let status = if message.contains("poisoned") {
                500
            } else if message.contains("unknown layout draft") {
                404
            } else if message.contains("different draft")
                || message.contains("already been applied")
                || message.contains("stale revision")
                || message.contains("Unsupported layout command action")
            {
                409
            } else {
                400
            };
            json_error(status, &message)
        }
    }
}

/// `GET /api/v1/layouts/drafts/{id}/compiled`: compile the draft into the
/// strict solver layout. Unknown drafts are 404; a draft that cannot compile
/// (e.g. no seats left) is 422.
fn layout_compiled_response(draft_id: &str) -> Response {
    match crate::layouts::compile_layout_draft_json(draft_id) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("poisoned") => json_error(500, &message),
        Err(message) if message.contains("unknown layout draft") => json_error(404, &message),
        Err(message) => json_error(422, &message),
    }
}

/// `DELETE /api/v1/layouts/drafts/{id}`: remove a layout draft (204), or 404
/// when it never existed.
fn layout_delete_response(draft_id: &str) -> Response {
    if crate::layouts::delete_layout_draft(draft_id) {
        Response {
            status: 204,
            content_type: None,
            content_disposition: None,
            body: Vec::new(),
        }
    } else {
        json_error(404, "layout draft was not found")
    }
}

// ---------------------------------------------------------------------------
// Project routes
// ---------------------------------------------------------------------------

/// `GET /api/v1/projects/recent?root=..&limit=..`: list recent project files
/// under `root` (default `.`), capped at `limit` (default 20, 1..=100).
fn projects_recent_response(query: Option<&str>) -> Response {
    let params = parse_query(query.unwrap_or(""));
    let root = params
        .get("root")
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    let limit = match params.get("limit") {
        None => 20,
        Some(raw) => match raw.parse::<usize>() {
            Ok(value) => value,
            Err(_) => {
                return json_error(422, "The project list limit must be between 1 and 100.")
            }
        },
    };
    match crate::projects::list_projects_json(&root, limit) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => json_error(422, &message),
    }
}

/// Parse a URL query string into its percent-decoded key/value pairs.
fn parse_query(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        let key = percent_decode(key).unwrap_or_else(|_| key.to_string());
        let value = percent_decode(value).unwrap_or_else(|_| value.to_string());
        params.insert(key, value);
    }
    params
}

/// `POST /api/v1/projects/history`: return the history and outputs listing for
/// a project file (`{project_path, include_outputs?}`).
fn project_history_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    project_result_response(crate::projects::project_history_json(&project_path))
}

/// `POST /api/v1/projects/privacy`: scan a project for sensitive fields
/// (`{project_path, include_outputs?}`).
fn project_privacy_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    project_result_response(crate::projects::project_privacy_json(&project_path))
}

/// `POST /api/v1/projects/bundle`: pack a project into a self-contained
/// `.seattrellis.zip` byte stream for download. A successful pack is recorded
/// as a recently-opened project.
fn project_bundle_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    match crate::projects::pack_project_json(&project_path) {
        Ok(bytes) => {
            record_recent(&project_path);
            let filename = crate::projects::default_bundle_name(&project_path)
                .unwrap_or_else(|_| "project.seattrellis.zip".to_string());
            Response {
                status: 200,
                content_type: Some("application/zip"),
                content_disposition: Some(format!("attachment; filename=\"{filename}\"")),
                body: bytes,
            }
        }
        Err(message) => project_result_response(Err(message)),
    }
}

/// `POST /api/v1/projects/restore`: restore a project from a multipart
/// `bundle` upload into `output_dir`, honoring the optional `overwrite` flag.
/// A successful restore is recorded as a recently-opened project.
fn project_restore_response(body: &[u8], content_type: Option<&str>) -> Response {
    let Some(content_type) = content_type else {
        return json_error(400, "multipart/form-data upload expected");
    };
    let Some(boundary) = multipart_boundary(content_type) else {
        return json_error(400, "multipart/form-data boundary is missing");
    };
    let fields = match parse_multipart(body, &boundary) {
        Ok(fields) => fields,
        Err(message) => return json_error(422, &message),
    };
    let Some(bundle) = fields.get("bundle") else {
        return json_error(422, "upload is missing a 'bundle' field");
    };
    if bundle.is_empty() {
        return json_error(422, "uploaded project bundle is empty");
    }
    let output_dir = match fields.get("output_dir") {
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => {
                return json_error(422, "Choose a destination folder for the restored project.")
            }
        },
        None => {
            return json_error(422, "Choose a destination folder for the restored project.")
        }
    };
    let output_dir = resolve_request_path(&output_dir);
    let overwrite = fields
        .get("overwrite")
        .map(|bytes| std::str::from_utf8(bytes).unwrap_or("false"))
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);

    match crate::projects::restore_project_bundle(bundle, &output_dir, overwrite) {
        Ok(project_path) => {
            let project_path_str = project_path.to_string_lossy().into_owned();
            record_recent(&project_path_str);
            let destination = project_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(&output_dir));
            Response::json(
                200,
                json!({
                    "api_version": "1",
                    "project_path": project_path_str,
                    "output_dir": destination.to_string_lossy(),
                }),
            )
        }
        Err(message) => json_error(422, &message),
    }
}

/// Record a recently-accessed project under its display name.
fn record_recent(project_path: &str) {
    let name = project_recent_name(project_path);
    crate::projects::record_recent_project(project_path, &name);
}

/// A display name for a project file, derived from its filename stem.
fn project_recent_name(project_path: &str) -> String {
    let name = Path::new(project_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for suffix in [".seattrellis.json", ".project.json", ".json"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    name
}

/// Map a projects-domain `Result<String, String>` onto a response, translating
/// domain error strings to HTTP status codes (404 for missing artifacts, 422
/// for validation problems, 500 for a poisoned store).
fn project_result_response(result: Result<String, String>) -> Response {
    match result {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("poisoned") => json_error(500, &message),
        Err(message) if message.contains("not found") || message.contains("does not exist") => {
            json_error(404, &message)
        }
        Err(message) => json_error(422, &message),
    }
}

/// Resolve a path from a request against the current working directory so the
/// project domain modules always receive an absolute reference. Absolute paths
/// pass through unchanged.
fn resolve_request_path(path: &str) -> String {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return path.to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(candidate).to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Migration routes
// ---------------------------------------------------------------------------

/// `POST /api/v1/projects/migration/preview`: preview a migration of a project
/// artifact (`{project_path, artifact_path?, in_place?}`).
fn migration_preview_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    match crate::migration::migration_preview_json(&project_path) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => migration_error_response(&message),
    }
}

/// `POST /api/v1/projects/migration/apply`: apply a migration to a project
/// artifact (`{project_path, artifact_path?, in_place?}`).
fn migration_apply_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let in_place = match optional_bool(&value, "in_place") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    match crate::migration::migration_apply_json(&project_path, in_place) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => migration_error_response(&message),
    }
}

/// `POST /api/v1/projects/migration/reference-checks`: report per-field
/// reference status for a project artifact. The workbench surfaces these inside
/// the migration preview; the standalone route keeps the underlying check
/// available to scripts and tests.
fn migration_reference_checks_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    match crate::migration::migration_reference_checks_json(&project_path) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => migration_error_response(&message),
    }
}

/// `POST /api/v1/projects/migration/batch/preview`: preview migrations for a
/// set of project artifacts (`{project_paths, in_place?}`).
fn migration_batch_preview_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let paths = match required_string_array(&value, "project_paths") {
        Ok(paths) => paths,
        Err(response) => return response,
    };
    let paths: Vec<String> = paths.iter().map(|path| resolve_request_path(path)).collect();
    match crate::migration::migration_batch_preview_json(&paths) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => migration_error_response(&message),
    }
}

/// `POST /api/v1/projects/migration/batch/apply`: apply migrations for a set of
/// project artifacts (`{project_paths, in_place?}`), rolling back on failure.
fn migration_batch_apply_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let paths = match required_string_array(&value, "project_paths") {
        Ok(paths) => paths,
        Err(response) => return response,
    };
    let in_place = match optional_bool(&value, "in_place") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let paths: Vec<String> = paths.iter().map(|path| resolve_request_path(path)).collect();
    match crate::migration::migration_batch_apply_json(&paths, in_place) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => migration_error_response(&message),
    }
}

/// `POST /api/v1/projects/migration/restore`: restore a migration backup over
/// its original artifact. The frontend sends `{project_path, source_path,
/// backup_path}`; the backup is restored onto `source_path`.
fn migration_restore_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let backup_path = match required_string(&value, "backup_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let source_path = match required_string(&value, "source_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let backup_path = resolve_request_path(&backup_path);
    let source_path = resolve_request_path(&source_path);
    match crate::migration::migration_restore_json(&backup_path, &source_path) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => migration_error_response(&message),
    }
}

/// Map a migration-domain error onto the matching HTTP status: 404 for a
/// missing artifact, 409 for a blocked batch, 422 for validation problems.
fn migration_error_response(message: &str) -> Response {
    if message.contains("poisoned") {
        json_error(500, message)
    } else if message.contains("does not exist") || message.contains("not found") {
        json_error(404, message)
    } else if message.contains("reference checks") {
        json_error(409, message)
    } else {
        json_error(422, message)
    }
}

/// Parse a JSON object request body, returning a 400 response on empty or
/// invalid JSON.
fn parse_body_json(body: &[u8]) -> Result<Value, Response> {
    if body.is_empty() {
        return Err(json_error(400, "empty request body"));
    }
    serde_json::from_slice(body).map_err(|_| json_error(400, "request body is not valid JSON"))
}

/// Read a required string field from a parsed JSON object body.
fn required_string(value: &Value, field: &str) -> Result<String, Response> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            json_error(
                400,
                &format!("request body is missing a '{field}' string field"),
            )
        })
}

/// Read an optional boolean field from a parsed JSON object body (default
/// false when absent).
fn optional_bool(value: &Value, field: &str) -> Result<bool, Response> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(json_error(
            400,
            &format!("request field '{field}' must be a boolean"),
        )),
    }
}

/// Read a required array-of-strings field from a parsed JSON object body.
fn required_string_array(value: &Value, field: &str) -> Result<Vec<String>, Response> {
    let array = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            json_error(
                400,
                &format!("request body is missing a '{field}' string array field"),
            )
        })?;
    array
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            json_error(
                400,
                &format!("request field '{field}' must be an array of strings"),
            )
        })
}

fn index_response(web_root: &Path) -> Response {
    match fs::read(web_root.join("index.html")) {
        Ok(bytes) => Response::text(200, "text/html; charset=utf-8", bytes),
        Err(_) => plain_response(500, "workbench index.html is missing"),
    }
}

fn static_response(web_root: &Path, path: &str) -> Response {
    let Some(target) = safe_join(web_root, path) else {
        return plain_response(404, "not found");
    };
    match fs::read(&target) {
        Ok(bytes) => {
            let content_type = content_type_for(&target);
            Response::text(200, content_type, bytes)
        }
        Err(_) => plain_response(404, "not found"),
    }
}

// ---------------------------------------------------------------------------
// Multipart parsing (minimal, dependency-free)
// ---------------------------------------------------------------------------

/// Extract the `boundary` value from a `multipart/form-data` Content-Type.
fn multipart_boundary(content_type: &str) -> Option<String> {
    if !content_type
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
    {
        return None;
    }
    let boundary = content_type
        .split(';')
        .skip(1)
        .map(str::trim)
        .find(|part| part.to_ascii_lowercase().starts_with("boundary="))?
        .split_once('=')?
        .1
        .trim()
        .trim_matches('"')
        .trim();
    if boundary.is_empty() {
        None
    } else {
        Some(boundary.to_string())
    }
}

/// Parse a `multipart/form-data` body into its fields (name -> raw bytes).
///
/// Handles the browser encoding exactly: parts are separated by
/// `--boundary` lines, each part carries `Content-Disposition` headers until a
/// blank line, the body is terminated by a final `--boundary--`. Works with
/// arbitrary (including randomly-generated) boundaries.
fn parse_multipart(body: &[u8], boundary: &str) -> Result<HashMap<String, Vec<u8>>, String> {
    let delimiter = format!("--{boundary}");
    let delimiter_bytes = delimiter.as_bytes();
    let mut fields: HashMap<String, Vec<u8>> = HashMap::new();

    // The body should begin with the first delimiter; tolerate a few leading
    // bytes (e.g. stray CRLFs from a client).
    let start = find_sequence(body, 0, delimiter_bytes)
        .ok_or_else(|| "multipart body does not contain the boundary".to_string())?;
    let mut pos = start + delimiter_bytes.len();

    loop {
        let rest = &body[pos..];
        if rest.starts_with(b"--") {
            break; // final `--boundary--`
        }
        if !rest.starts_with(b"\r\n") {
            return Err("malformed multipart boundary line".to_string());
        }
        pos += 2;

        // Part headers run to the first blank line.
        let header_end = find_sequence(body, pos, b"\r\n\r\n")
            .ok_or_else(|| "multipart part is missing a header terminator".to_string())?;
        let header_block = std::str::from_utf8(&body[pos..header_end])
            .map_err(|_| "multipart part headers are not valid ASCII".to_string())?;
        pos = header_end + 4;

        // Part content ends just before the next `\r\n--boundary`.
        let terminator = format!("\r\n{delimiter}");
        let content_end = find_sequence(body, pos, terminator.as_bytes())
            .ok_or_else(|| "multipart part content is not terminated".to_string())?;
        let content = &body[pos..content_end];

        let name = part_header(header_block, "content-disposition")
            .and_then(|value| quoted_param(value, "name"))
            .ok_or_else(|| "multipart part is missing a content-disposition name".to_string())?;
        fields.insert(name, content.to_vec());

        // Skip the `\r\n--boundary` that ended this part.
        pos = content_end + 2 + delimiter_bytes.len();
    }

    Ok(fields)
}

/// Read a header value (lowercased key) from a part's header block.
fn part_header<'a>(header_block: &'a str, key: &str) -> Option<&'a str> {
    header_block.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case(key) {
            Some(value.trim())
        } else {
            None
        }
    })
}

/// Read `param="value"` (or `param=value`) from a header value such as
/// `form-data; name="file"; filename="roster.csv"`. Splitting on `;` prevents
/// `filename="..."` from being mistaken for a `name` parameter.
fn quoted_param(value: &str, param: &str) -> Option<String> {
    let prefix = format!("{param}=");
    value.split(';').map(str::trim).find_map(|segment| {
        let rest = segment.strip_prefix(&prefix)?;
        if let Some(stripped) = rest.strip_prefix('"') {
            let end = stripped.find('"')?;
            Some(stripped[..end].to_string())
        } else {
            Some(rest.split(';').next()?.trim().to_string())
        }
    })
}

/// Byte-substring search from an offset.
fn find_sequence(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|index| from + index)
}

// ---------------------------------------------------------------------------
// Path safety helpers
// ---------------------------------------------------------------------------

/// Resolve a request path inside `web_root`, rejecting any traversal.
///
/// - Percent-encoded characters are decoded first.
/// - NUL bytes and `..` path segments are rejected outright.
/// - As defense in depth, the canonicalised target must remain under the
///   canonicalised root.
fn safe_join(web_root: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path).ok()?;
    if decoded.contains('\0') {
        return None;
    }
    let trimmed = decoded.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(web_root.join("index.html"));
    }

    let mut segments: Vec<&str> = Vec::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            segment => segments.push(segment),
        }
    }

    let joined = segments.join("/");
    if joined.is_empty() {
        return Some(web_root.join("index.html"));
    }
    let candidate = web_root.join(joined);

    let root_canonical = web_root.canonicalize().ok()?;
    let candidate_canonical = candidate.canonicalize().ok()?;
    if !candidate_canonical.starts_with(&root_canonical) {
        return None;
    }
    Some(candidate)
}

/// Minimal RFC 3986 percent-decoding (uppercase/lowercase hex).
fn percent_decode(input: &str) -> Result<String, ()> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(());
                }
                let high = hex_value(bytes[index + 1]).ok_or(())?;
                let low = hex_value(bytes[index + 2]).ok_or(())?;
                out.push((high << 4) | low);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Map a file extension to a content type for static assets.
fn content_type_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match ext.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map") | Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        Some("webmanifest") => "application/manifest+json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Serialise a response onto the wire.
fn write_response(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    let status_text = STATUS_TEXT
        .iter()
        .find(|(status, _)| *status == response.status)
        .map(|(_, text)| *text)
        .unwrap_or("Unknown");

    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, status_text);
    head.push_str("Server: seattrellis-backend\r\n");
    head.push_str("Connection: close\r\n");
    if let Some(content_type) = response.content_type {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    if let Some(disposition) = &response.content_disposition {
        head.push_str(&format!("Content-Disposition: {disposition}\r\n"));
    }
    head.push_str("X-Content-Type-Options: nosniff\r\n");
    head.push_str("Cache-Control: no-store\r\n");
    head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    head.push_str("\r\n");

    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

/// Monotonic-ish unique id for editor drafts (time prefix + atomic counter).
static DRAFT_SEQ: AtomicU64 = AtomicU64::new(0);

fn new_draft_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = DRAFT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("draft-{nanos:x}{seq:x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_DIR_SEQ: AtomicU32 = AtomicU32::new(0);

    fn test_web_root() -> PathBuf {
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seattrellis_app_test_{}_{}",
            std::process::id(),
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("assets")).unwrap();
        fs::write(dir.join("index.html"), "<html>test workbench</html>").unwrap();
        fs::write(dir.join("assets/app.js"), "console.log('hi');").unwrap();
        dir
    }

    /// A fresh editor store + solve-request store for one route call. Roster
    /// drafts live in a process-global store (see `roster.rs`), so those tests
    /// use the returned draft ids directly.
    fn route_one(request: &Request, root: &Path) -> Response {
        let editor_store = editing::new_draft_store();
        let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());
        route(request, root, &editor_store, &solve_requests)
    }

    fn request(method: &str, path: &str, body: &[u8]) -> Request {
        request_with_content_type(method, path, body, None)
    }

    fn request_with_content_type(
        method: &str,
        path: &str,
        body: &[u8],
        content_type: Option<&str>,
    ) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            content_type: content_type.map(String::from),
            body: body.to_vec(),
        }
    }

    fn body_json(response: &Response) -> serde_json::Value {
        serde_json::from_slice(&response.body).unwrap()
    }

    /// Build a minimal multipart body with a single `file` field.
    fn multipart_body(file_bytes: &[u8], filename: &str, boundary: &str) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: text/csv\r\n\r\n");
        body.extend_from_slice(file_bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    #[test]
    fn health_route_returns_expected_shape() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/api/v1/health", b""), &root);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("application/json; charset=utf-8"));
        let value = body_json(&response);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["service"], "seattrellis");
        assert_eq!(value["api_version"], "1");
    }

    #[test]
    fn catalogs_route_returns_bilingual_catalog() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/api/v1/catalogs", b""), &root);
        assert_eq!(response.status, 200);
        let value = body_json(&response);
        let rooms = value["roomTemplates"].as_array().unwrap();
        assert_eq!(rooms.len(), 3);
        assert_eq!(rooms[0]["id"], "standard-30");
        assert_eq!(rooms[0]["name"]["zh-CN"].as_str().unwrap(), "30 座教室");
        assert_eq!(rooms[0]["rows"], 5);
        assert_eq!(rooms[0]["columns"], 6);
        let goals = value["teacherGoals"].as_array().unwrap();
        let goal_ids: Vec<&str> = goals
            .iter()
            .map(|goal| goal["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            goal_ids,
            vec!["daily-rotation", "quick-shuffle", "fair-shuffle", "peer-support"]
        );
        let formats = value["exportFormats"].as_array().unwrap();
        let format_ids: Vec<&str> = formats
            .iter()
            .map(|format| format["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            format_ids,
            vec!["svg", "html", "png", "pdf", "print-html"]
        );
    }

    #[test]
    fn index_route_serves_workbench() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/", b""), &root);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("text/html; charset=utf-8"));
        assert_eq!(response.body, b"<html>test workbench</html>");
    }

    #[test]
    fn static_asset_route_serves_file() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/assets/app.js", b""), &root);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("text/javascript; charset=utf-8"));
        assert_eq!(response.body, b"console.log('hi');");
    }

    #[test]
    fn dotdot_traversal_is_rejected() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/../etc/passwd", b""), &root);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn percent_encoded_traversal_is_rejected() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/%2e%2e/secret", b""), &root);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn unknown_static_file_is_404() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/does-not-exist.js", b""), &root);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn generate_feasible_returns_class_response_with_editor() {
        let root = test_web_root();
        let problem = json!({
            "api_version": 2,
            "student_count": 5,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0],[4.0,1.0],[5.0,1.0],[6.0,1.0],[7.0,1.0],[8.0,1.0],[9.0,1.0]]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let value = body_json(&response);
        assert_eq!(value["class_name"], "Classroom");
        assert_eq!(value["warnings"], json!([]));
        assert_eq!(value["goal"]["goal_id"], "daily-rotation");
        let recommended = value["recommended_candidate_id"].as_str().unwrap();
        let candidates = value["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["candidate_id"], recommended);
        assert_eq!(candidates[0]["recommended"], true);
        let editor = &value["editor"];
        assert_eq!(editor["kind"], "seattrellis_editor_state");
        assert_eq!(editor["protocol_version"], "1.0");
        assert_eq!(editor["draft_id"], recommended);
        assert_eq!(editor["students"].as_array().map(Vec::len), Some(5));
        assert_eq!(editor["seats"].as_array().map(Vec::len), Some(9));
        for student in editor["students"].as_array().unwrap() {
            assert!(student["seat_id"].is_string());
        }
    }

    #[test]
    fn generate_with_named_students_uses_keys() {
        let root = test_web_root();
        let problem = json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "students": [
                {"key": "S1", "display_name": "Alice", "score": 93.0},
                {"key": "S2", "display_name": "Bob", "score": 81.0}
            ]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 200);
        let value = body_json(&response);
        let editor = &value["editor"];
        let student_keys: Vec<&str> = editor["students"]
            .as_array()
            .unwrap()
            .iter()
            .map(|student| student["student_key"].as_str().unwrap())
            .collect();
        assert_eq!(student_keys, vec!["S1", "S2"]);
    }

    #[test]
    fn solve_alias_route_works() {
        let root = test_web_root();
        let problem = json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0]]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/solve", &body), &root);
        assert_eq!(response.status, 200);
        let value = body_json(&response);
        assert!(value["editor"]["draft_id"].is_string());
    }

    /// The React workbench's `GenerateClassRequest` shape: a draft carrying
    /// students, a room template id and a goal id. It must be expanded onto a
    /// room grid + goal rules, solved, and returned as a `GenerateClassResponse`
    /// with a created editor draft.
    #[test]
    fn generate_frontend_class_request_adapts_and_creates_draft() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Physics Period 3",
                "students": [
                    {"student_id": "S1", "name": "Alice", "score": 93, "height_cm": 160},
                    {"student_id": "S2", "name": "Bob", "score": 81, "height_cm": 172},
                    {"student_id": "S3", "name": "Carol", "score": 75, "height_cm": 150}
                ],
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "daily-rotation"}
            },
            "options": {"candidate_count": 1, "seed": 42}
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let value = body_json(&response);

        // GenerateClassResponse shape.
        assert_eq!(value["goal"]["goal_id"], "daily-rotation");
        assert_eq!(value["warnings"], json!([]));
        let draft_id = value["editor"]["draft_id"].as_str().unwrap();
        assert_eq!(value["recommended_candidate_id"], draft_id);
        let candidates = value["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["candidate_id"], draft_id);
        assert_eq!(candidates[0]["recommended"], true);
        let total_score = candidates[0]["total_score"].as_f64().expect("total_score is a number");
        assert!(total_score.is_finite());

        // The editor draft mirrors the 3 students and the 30-seat template.
        let editor = &value["editor"];
        assert_eq!(editor["draft_id"], draft_id);
        let students = editor["students"].as_array().unwrap();
        assert_eq!(students.len(), 3);
        let keys: Vec<&str> = students
            .iter()
            .map(|student| student["student_key"].as_str().unwrap())
            .collect();
        assert_eq!(keys, vec!["S1", "S2", "S3"]);
        for student in students {
            assert!(
                student["seat_id"].is_string(),
                "every student is seated: {student}"
            );
        }
        assert_eq!(editor["seats"].as_array().map(Vec::len), Some(30));
        // The template's row-1 leftmost seat id is R1C1 (grid coordinates).
        let seats = editor["seats"].as_array().unwrap();
        assert_eq!(seats[0]["seat_id"], "R1C1");
        assert_eq!(seats[0]["enabled"], true);
        // Seat 30 is the last enabled seat: row 5, grid column 7.
        assert_eq!(seats[29]["seat_id"], "R5C7");
    }

    /// The frontend path must echo the requested goal id (not hardcode it).
    #[test]
    fn generate_frontend_class_request_echoes_requested_goal() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Peer Class",
                "students": [
                    {"student_id": "S1", "name": "Alice", "score": 95},
                    {"student_id": "S2", "name": "Bob", "score": 60},
                    {"student_id": "S3", "name": "Carol", "score": 40}
                ],
                "room": {"template_id": "standard-48"},
                "goal": {"goal_id": "peer-support"}
            }
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let value = body_json(&response);
        assert_eq!(value["goal"]["goal_id"], "peer-support");
        assert_eq!(value["editor"]["students"].as_array().map(Vec::len), Some(3));
        assert_eq!(value["editor"]["seats"].as_array().map(Vec::len), Some(48));
    }

    /// An unknown room template id on the frontend path is a 422.
    #[test]
    fn generate_frontend_class_request_unknown_room_is_422() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "X",
                "students": [{"student_id": "S1", "name": "Alice"}],
                "room": {"template_id": "standard-99"},
                "goal": {"goal_id": "daily-rotation"}
            }
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(
            response.status,
            422,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let value = body_json(&response);
        assert_eq!(value["error"], "room_not_found");
        assert!(value["message"].as_str().unwrap().contains("standard-99"));
    }

    /// An unknown goal id on the frontend path is a 422.
    #[test]
    fn generate_frontend_class_request_unknown_goal_is_422() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "X",
                "students": [{"student_id": "S1", "name": "Alice"}],
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "warp-speed"}
            }
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(
            response.status,
            422,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let value = body_json(&response);
        assert_eq!(value["error"], "unknown_goal");
        assert!(value["message"].as_str().unwrap().contains("warp-speed"));
    }

    #[test]
    fn solve_constraint_infeasible_is_409() {
        let root = test_web_root();
        // Two students that must sit adjacent, but the graph has no edges at
        // all, so the greedy cannot satisfy the adjacency requirement.
        let problem = json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0,1.0],[2.0,1.0]],
            "must_be_adjacent": [[0, 1]]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 409);
        let value = body_json(&response);
        assert_eq!(value["error"], "plan_not_found");
    }

    #[test]
    fn solve_invalid_json_is_400() {
        let root = test_web_root();
        let response = route_one(
            &request("POST", "/api/v1/classes/generate", b"not json at all"),
            &root,
        );
        assert_eq!(response.status, 400);
        assert!(body_json(&response)["error"].is_string());
    }

    #[test]
    fn solve_too_many_students_is_400() {
        let root = test_web_root();
        let problem = json!({
            "api_version": 2,
            "student_count": 10,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0]]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 400);
        assert!(body_json(&response)["error"].as_str().unwrap().contains("cannot seat more students"));
    }

    #[test]
    fn roster_upload_preview_get_delete_flow() {
        let root = test_web_root();
        let csv = b"student_id,name,gender,height_cm,score,vision\nS1,Alice,F,160,90,0.8\nS2,Bob,M,165,81,0.6\nS3,Carol,F,150,75,1.0\n";
        let body = multipart_body(csv, "roster.csv", "----testboundary");
        let response = route_one(
            &request_with_content_type(
                "POST",
                "/api/v1/rosters/drafts",
                &body,
                Some("multipart/form-data; boundary=----testboundary"),
            ),
            &root,
        );
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let roster = body_json(&response);
        assert_eq!(roster["source_format"], "csv");
        assert_eq!(roster["row_count"], 3);
        assert_eq!(roster["column_count"], 6);
        let roster_id = roster["draft_id"].as_str().unwrap().to_string();

        let get = route_one(
            &request("GET", &format!("/api/v1/rosters/drafts/{roster_id}"), b""),
            &root,
        );
        assert_eq!(get.status, 200);
        assert_eq!(body_json(&get)["draft_id"], roster_id);

        let mapping = roster["suggested_mapping"].clone();
        let preview_body = json!({
            "mapping": mapping,
            "mode": "incremental",
            "current_students": [],
            "current_revision": 0,
            "updated_fields": ["name"]
        });
        let preview = route_one(
            &request(
                "POST",
                &format!("/api/v1/rosters/drafts/{roster_id}/preview"),
                &serde_json::to_vec(&preview_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            preview.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&preview.body)
        );
        let preview_val = body_json(&preview);
        assert_eq!(preview_val["draft_id"], roster_id);
        assert_eq!(preview_val["mode"], "incremental");
        assert_eq!(preview_val["can_apply"], true);
        assert!(preview_val["changes"].as_array().map(|changes| !changes.is_empty()).unwrap_or(false));

        let del = route_one(
            &request("DELETE", &format!("/api/v1/rosters/drafts/{roster_id}"), b""),
            &root,
        );
        assert_eq!(del.status, 204);
        let del_again = route_one(
            &request("DELETE", &format!("/api/v1/rosters/drafts/{roster_id}"), b""),
            &root,
        );
        assert_eq!(del_again.status, 404);
    }

    #[test]
    fn roster_upload_rejects_missing_file_field() {
        let root = test_web_root();
        // A multipart body with no `file` part at all.
        let body = b"--b\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--b--\r\n";
        let response = route_one(
            &request_with_content_type(
                "POST",
                "/api/v1/rosters/drafts",
                body,
                Some("multipart/form-data; boundary=b"),
            ),
            &root,
        );
        assert_eq!(response.status, 422);
        assert!(body_json(&response)["error"].as_str().unwrap().contains("file"));
    }

    #[test]
    fn roster_upload_rejects_invalid_csv() {
        let root = test_web_root();
        let body = multipart_body(b"", "roster.csv", "bnd");
        let response = route_one(
            &request_with_content_type(
                "POST",
                "/api/v1/rosters/drafts",
                &body,
                Some("multipart/form-data; boundary=bnd"),
            ),
            &root,
        );
        assert_eq!(response.status, 422);
    }

    #[test]
    fn roster_upload_requires_multipart_content_type() {
        let root = test_web_root();
        let response = route_one(
            &request("POST", "/api/v1/rosters/drafts", b"some csv"),
            &root,
        );
        assert_eq!(response.status, 400);
    }

    #[test]
    fn roster_get_missing_is_404() {
        let root = test_web_root();
        let response = route_one(
            &request("GET", "/api/v1/rosters/drafts/does-not-exist", b""),
            &root,
        );
        assert_eq!(response.status, 404);
        assert!(body_json(&response).get("error").is_some());
    }

    #[test]
    fn multipart_parser_handles_realistic_browser_boundary() {
        let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
        let csv = b"name\nAlice\nBob\n";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"roster.csv\"\r\n");
        body.extend_from_slice(b"Content-Type: text/csv\r\n\r\n");
        body.extend_from_slice(csv);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"note\"\r\n\r\nhello\r\n");
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let fields = parse_multipart(&body, boundary).unwrap();
        assert_eq!(fields.get("file").map(Vec::as_slice), Some(csv.as_slice()));
        assert_eq!(fields.get("note").map(Vec::as_slice), Some(b"hello".as_slice()));
    }

    #[test]
    fn multipart_parser_handles_filename_before_name() {
        let boundary = "b";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; filename=\"x.csv\"; name=\"file\"\r\n\r\nabc\r\n--{boundary}--\r\n"
        );
        let fields = parse_multipart(body.as_bytes(), boundary).unwrap();
        assert_eq!(fields.get("file").map(Vec::as_slice), Some(b"abc".as_slice()));
    }

    #[test]
    fn multipart_boundary_extraction_is_robust() {
        assert_eq!(
            multipart_boundary("multipart/form-data; boundary=abc"),
            Some("abc".to_string())
        );
        assert_eq!(
            multipart_boundary("multipart/form-data; boundary=\"abc\""),
            Some("abc".to_string())
        );
        assert_eq!(
            multipart_boundary("multipart/form-data; charset=utf-8; boundary=--xyz"),
            Some("--xyz".to_string())
        );
        assert_eq!(multipart_boundary("application/json"), None);
        assert_eq!(multipart_boundary("multipart/form-data"), None);
    }

    #[test]
    fn full_teacher_flow_upload_generate_edit_export() {
        let root = test_web_root();
        let editor_store = editing::new_draft_store();
        let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());

        // 1. Upload a roster.
        let csv = b"student_id,name,gender,height_cm,score,vision\nS1,Alice,F,160,90,0.8\nS2,Bob,M,165,81,0.6\nS3,Carol,F,150,75,1.0\nS4,Dave,M,175,88,0.9\nS5,Eve,F,158,90,0.7\n";
        let boundary = "----WebKitFormBoundaryFlowBoundary";
        let upload_body = multipart_body(csv, "roster.csv", boundary);
        let upload = route(
            &request_with_content_type(
                "POST",
                "/api/v1/rosters/drafts",
                &upload_body,
                Some(&format!("multipart/form-data; boundary={boundary}")),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(upload.status, 200);
        let roster = body_json(&upload);
        assert_eq!(roster["row_count"], 5);
        let roster_id = roster["draft_id"].as_str().unwrap().to_string();

        // 2. Preview an incremental update.
        let preview_body = json!({
            "mapping": roster["suggested_mapping"],
            "mode": "incremental",
            "current_students": [],
            "current_revision": 0,
            "updated_fields": ["name"]
        });
        let preview = route(
            &request(
                "POST",
                &format!("/api/v1/rosters/drafts/{roster_id}/preview"),
                &serde_json::to_vec(&preview_body).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(preview.status, 200, "body: {}", String::from_utf8_lossy(&preview.body));
        let preview_val = body_json(&preview);
        assert_eq!(preview_val["draft_id"], roster_id);
        assert_eq!(preview_val["can_apply"], true);

        // 3. Generate a plan for the five uploaded students.
        let problem = json!({
            "api_version": 2,
            "student_count": 5,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0],[4.0,1.0],[5.0,1.0],[6.0,1.0],[7.0,1.0],[8.0,1.0],[9.0,1.0]],
            "students": [
                {"key": "S1", "display_name": "Alice", "score": 93.0},
                {"key": "S2", "display_name": "Bob", "score": 81.0},
                {"key": "S3", "display_name": "Carol", "score": 75.0},
                {"key": "S4", "display_name": "Dave", "score": 88.0},
                {"key": "S5", "display_name": "Eve", "score": 90.0}
            ]
        });
        let gen = route(
            &request(
                "POST",
                "/api/v1/classes/generate",
                &serde_json::to_vec(&problem).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(gen.status, 200, "body: {}", String::from_utf8_lossy(&gen.body));
        let gen_val = body_json(&gen);
        let draft_id = gen_val["editor"]["draft_id"].as_str().unwrap().to_string();
        assert_eq!(gen_val["recommended_candidate_id"], draft_id);
        assert_eq!(gen_val["candidates"][0]["recommended"], true);
        assert_eq!(gen_val["class_name"], "Classroom");

        // 4. Fetch the editor state.
        let fetch = route(
            &request(
                "GET",
                &format!("/api/v1/editing/drafts/{draft_id}"),
                b"",
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(fetch.status, 200);
        let before = body_json(&fetch);
        assert_eq!(before["revision"], 0);
        assert_eq!(before["students"].as_array().map(Vec::len), Some(5));
        let s1_before = before["students"]
            .as_array()
            .unwrap()
            .iter()
            .find(|student| student["student_key"] == "S1")
            .unwrap()["seat_id"]
            .clone();
        let s2_before = before["students"]
            .as_array()
            .unwrap()
            .iter()
            .find(|student| student["student_key"] == "S2")
            .unwrap()["seat_id"]
            .clone();

        // 5. Swap two students.
        let command = json!({
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "cmd-flow-1",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operations": [
                {"kind": "swap_students", "payload": {"first_student": "S1", "second_student": "S2"}}
            ]
        });
        let swapped = route(
            &request(
                "POST",
                &format!("/api/v1/editing/drafts/{draft_id}/commands"),
                &serde_json::to_vec(&command).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(swapped.status, 200, "body: {}", String::from_utf8_lossy(&swapped.body));
        let swapped_val = body_json(&swapped);
        assert_eq!(swapped_val["revision"], 1);
        let s1_after = swapped_val["students"]
            .as_array()
            .unwrap()
            .iter()
            .find(|student| student["student_key"] == "S1")
            .unwrap()["seat_id"]
            .clone();
        let s2_after = swapped_val["students"]
            .as_array()
            .unwrap()
            .iter()
            .find(|student| student["student_key"] == "S2")
            .unwrap()["seat_id"]
            .clone();
        assert_eq!(s1_after, s2_before);
        assert_eq!(s2_after, s1_before);

        // 6. Export the edited plan as SVG.
        let export_body = json!({
            "draft_id": draft_id,
            "format": "svg",
            "template": "teacher",
            "privacy": {"hide_scores": false, "hide_notes": false, "hide_special_needs": false, "anonymize": false, "show_height": false, "show_vision": false},
            "orientation": "portrait",
            "page_scale": 1.0,
            "locale": "zh",
            "show_student_ids": true
        });
        let export = route(
            &request(
                "POST",
                "/api/v1/exports",
                &serde_json::to_vec(&export_body).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(export.status, 200, "body: {}", String::from_utf8_lossy(&export.body));
        assert_eq!(export.content_type, Some("image/svg+xml"));
        assert!(export
            .content_disposition
            .as_deref()
            .unwrap()
            .contains("filename=\"seat-plan.svg\""));
        assert!(export.body.starts_with(b"<svg"));

        // 7. Delete the roster draft (204, then 404).
        let del = route(
            &request(
                "DELETE",
                &format!("/api/v1/rosters/drafts/{roster_id}"),
                b"",
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(del.status, 204);
        let del_again = route(
            &request(
                "DELETE",
                &format!("/api/v1/rosters/drafts/{roster_id}"),
                b"",
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(del_again.status, 404);
    }

    #[test]
    fn export_print_html_normalizes_to_html() {
        let root = test_web_root();
        let editor_store = editing::new_draft_store();
        let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());
        let problem = json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "students": [{"key": "S1"}, {"key": "S2"}]
        });
        let gen = route(
            &request(
                "POST",
                "/api/v1/classes/generate",
                &serde_json::to_vec(&problem).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(gen.status, 200);
        let draft_id = body_json(&gen)["editor"]["draft_id"]
            .as_str()
            .unwrap()
            .to_string();

        let export_body = json!({
            "draft_id": draft_id,
            "format": "print-html",
            "template": "public",
            "privacy": {"hide_scores": false, "hide_notes": false, "hide_special_needs": false, "anonymize": false, "show_height": false, "show_vision": false},
            "orientation": "portrait",
            "page_scale": 1.0,
            "locale": "en",
            "show_student_ids": false
        });
        let export = route(
            &request(
                "POST",
                "/api/v1/exports",
                &serde_json::to_vec(&export_body).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(export.status, 200, "body: {}", String::from_utf8_lossy(&export.body));
        assert_eq!(export.content_type, Some("text/html; charset=utf-8"));
        assert!(export.body.starts_with(b"<!doctype html") || export.body.windows(5).any(|w| w == b"<html"));
    }

    #[test]
    fn export_unknown_draft_is_404() {
        let root = test_web_root();
        let editor_store = editing::new_draft_store();
        let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());
        let export_body = json!({
            "draft_id": "draft-missing",
            "format": "svg",
            "template": "teacher",
            "privacy": {},
            "orientation": "portrait",
            "page_scale": 1.0,
            "locale": "zh",
            "show_student_ids": true
        });
        let export = route(
            &request(
                "POST",
                "/api/v1/exports",
                &serde_json::to_vec(&export_body).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(export.status, 404);
    }

    #[test]
    fn editing_command_validation_errors_map_to_4xx() {
        let root = test_web_root();
        let editor_store = editing::new_draft_store();
        let solve_requests: SolveRequestStore = Mutex::new(HashMap::new());

        // Unknown draft -> 404.
        let command = json!({
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "cmd-x",
            "draft_id": "missing",
            "base_revision": 0,
            "action": "apply",
            "operations": [{"kind": "swap_students", "payload": {"first_student": "A", "second_student": "B"}}]
        });
        let response = route(
            &request(
                "POST",
                "/api/v1/editing/drafts/missing/commands",
                &serde_json::to_vec(&command).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(response.status, 404);

        // Stale base revision -> 409 after one applied command.
        let problem = json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0]],
            "students": [{"key": "A"}, {"key": "B"}]
        });
        let gen = route(
            &request(
                "POST",
                "/api/v1/classes/generate",
                &serde_json::to_vec(&problem).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(gen.status, 200);
        let draft_id = body_json(&gen)["editor"]["draft_id"]
            .as_str()
            .unwrap()
            .to_string();

        let first = json!({
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "cmd-1",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operations": [{"kind": "swap_students", "payload": {"first_student": "A", "second_student": "B"}}]
        });
        let ok = route(
            &request(
                "POST",
                &format!("/api/v1/editing/drafts/{draft_id}/commands"),
                &serde_json::to_vec(&first).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(ok.status, 200);
        assert_eq!(body_json(&ok)["revision"], 1);

        // Same base_revision again (fresh command id) is now stale -> 409.
        let stale_body = json!({
            "kind": "seattrellis_editor_command",
            "protocol_version": "1.0",
            "command_id": "cmd-2",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operations": [{"kind": "swap_students", "payload": {"first_student": "A", "second_student": "B"}}]
        });
        let stale = route(
            &request(
                "POST",
                &format!("/api/v1/editing/drafts/{draft_id}/commands"),
                &serde_json::to_vec(&stale_body).unwrap(),
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(stale.status, 409);
        assert!(body_json(&stale)["error"].as_str().unwrap().contains("stale"));

        // Malformed JSON body -> 400.
        let bad = route(
            &request(
                "POST",
                &format!("/api/v1/editing/drafts/{draft_id}/commands"),
                b"not json",
            ),
            &root,
            &editor_store,
            &solve_requests,
        );
        assert_eq!(bad.status, 400);
    }

    #[test]
    fn method_not_allowed_on_api() {
        let root = test_web_root();
        let response = route_one(&request("PUT", "/api/v1/health", b""), &root);
        assert_eq!(response.status, 405);
    }

    #[test]
    fn unknown_api_route_is_404_json() {
        let root = test_web_root();
        let response = route_one(&request("GET", "/api/v1/nope", b""), &root);
        assert_eq!(response.status, 404);
        assert!(body_json(&response).get("error").is_some());
    }

    #[test]
    fn percent_decode_roundtrips() {
        assert_eq!(percent_decode("abc").unwrap(), "abc");
        assert_eq!(percent_decode("%2e%2E/x").unwrap(), "../x");
        assert_eq!(percent_decode("%20").unwrap(), " ");
        assert!(percent_decode("%2").is_err());
        assert!(percent_decode("%zz").is_err());
        assert!(percent_decode("%00").unwrap().contains('\0'));
    }

    #[test]
    fn safe_join_blocks_escapes() {
        let root = test_web_root();
        assert!(safe_join(&root, "/assets/app.js").is_some());
        assert!(safe_join(&root, "/").is_some());
        assert!(safe_join(&root, "/..").is_none());
        assert!(safe_join(&root, "/assets/../../secret").is_none());
        assert!(safe_join(&root, "/%2e%2e/secret").is_none());
    }

    // --- Layout route integration tests ------------------------------------

    /// Layout routes: create a draft, fetch it, dispatch a command, compile it,
    /// and delete it. Covers the full draft lifecycle against the real JSON
    /// contract (`LayoutStateResponse` / `LayoutCommand` / `CompiledLayoutResponse`).
    #[test]
    fn layout_draft_lifecycle_create_get_command_compiled_delete() {
        let root = test_web_root();

        // 1. Create a 3x4 rectangular draft.
        let create_body = json!({ "name": "Layout Test", "rows": 3, "columns": 4 });
        let create = route_one(
            &request(
                "POST",
                "/api/v1/layouts/drafts",
                &serde_json::to_vec(&create_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            create.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&create.body)
        );
        let state = body_json(&create);
        assert_eq!(state["kind"], "seattrellis_layout_state");
        assert_eq!(state["api_version"], "1");
        assert_eq!(state["name"], "Layout Test");
        assert_eq!(state["rows"], 3);
        assert_eq!(state["columns"], 4);
        assert_eq!(state["revision"], 0);
        assert_eq!(state["usable_seat_count"], 12);
        let draft_id = state["draft_id"].as_str().unwrap().to_string();

        // 2. Fetch the state through the GET route.
        let get = route_one(
            &request("GET", &format!("/api/v1/layouts/drafts/{draft_id}"), b""),
            &root,
        );
        assert_eq!(get.status, 200);
        assert_eq!(body_json(&get)["draft_id"], draft_id);

        // 3. Dispatch a command that converts a seat into an aisle.
        let command = json!({
            "command_id": "cmd-layout-1",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "aisle"}}
        });
        let applied = route_one(
            &request(
                "POST",
                &format!("/api/v1/layouts/drafts/{draft_id}/commands"),
                &serde_json::to_vec(&command).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            applied.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&applied.body)
        );
        let after = body_json(&applied);
        assert_eq!(after["revision"], 1);
        assert_eq!(after["usable_seat_count"], 11);

        // 4. Compile the edited draft into the strict solver layout.
        let compiled = route_one(
            &request(
                "GET",
                &format!("/api/v1/layouts/drafts/{draft_id}/compiled"),
                b"",
            ),
            &root,
        );
        assert_eq!(
            compiled.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&compiled.body)
        );
        let compiled_val = body_json(&compiled);
        assert_eq!(compiled_val["api_version"], "1");
        assert_eq!(compiled_val["draft_id"], draft_id);
        // 11 seats + the 1 aisle cell = 12 layout nodes.
        assert_eq!(
            compiled_val["layout"]["seats"].as_array().map(Vec::len),
            Some(12)
        );

        // 5. Delete the draft (204, then 404).
        let del = route_one(
            &request("DELETE", &format!("/api/v1/layouts/drafts/{draft_id}"), b""),
            &root,
        );
        assert_eq!(del.status, 204);
        let del_again = route_one(
            &request("DELETE", &format!("/api/v1/layouts/drafts/{draft_id}"), b""),
            &root,
        );
        assert_eq!(del_again.status, 404);
    }

    /// Layout command error mapping: unknown draft -> 404, malformed body -> 400,
    /// stale base revision -> 409.
    #[test]
    fn layout_command_errors_map_to_4xx() {
        let root = test_web_root();

        // Unknown draft -> 404.
        let command = json!({
            "command_id": "cmd-missing",
            "draft_id": "missing",
            "base_revision": 0,
            "action": "apply",
            "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "seat"}}
        });
        let response = route_one(
            &request(
                "POST",
                "/api/v1/layouts/drafts/missing/commands",
                &serde_json::to_vec(&command).unwrap(),
            ),
            &root,
        );
        assert_eq!(response.status, 404);

        // Malformed JSON body -> 400.
        let bad = route_one(
            &request("POST", "/api/v1/layouts/drafts/x/commands", b"not json"),
            &root,
        );
        assert_eq!(bad.status, 400);

        // Stale base revision -> 409 after one applied command.
        let create_body = json!({ "name": "Stale", "rows": 2, "columns": 2 });
        let create = route_one(
            &request(
                "POST",
                "/api/v1/layouts/drafts",
                &serde_json::to_vec(&create_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(create.status, 200);
        let draft_id = body_json(&create)["draft_id"].as_str().unwrap().to_string();
        let first = json!({
            "command_id": "cmd-1",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "aisle"}}
        });
        let ok = route_one(
            &request(
                "POST",
                &format!("/api/v1/layouts/drafts/{draft_id}/commands"),
                &serde_json::to_vec(&first).unwrap(),
            ),
            &root,
        );
        assert_eq!(ok.status, 200);
        let stale = json!({
            "command_id": "cmd-2",
            "draft_id": draft_id,
            "base_revision": 0,
            "action": "apply",
            "operation": {"kind": "set_cell", "payload": {"row": 1, "column": 1, "kind": "aisle"}}
        });
        let conflict = route_one(
            &request(
                "POST",
                &format!("/api/v1/layouts/drafts/{draft_id}/commands"),
                &serde_json::to_vec(&stale).unwrap(),
            ),
            &root,
        );
        assert_eq!(conflict.status, 409);
    }

    /// Layout create validation: multiple sources or an unknown template are 422.
    #[test]
    fn layout_create_validation_error_is_422() {
        let root = test_web_root();
        let multiple_sources = json!({ "template_id": "standard-30", "rows": 5, "columns": 6 });
        let response = route_one(
            &request(
                "POST",
                "/api/v1/layouts/drafts",
                &serde_json::to_vec(&multiple_sources).unwrap(),
            ),
            &root,
        );
        assert_eq!(response.status, 422);

        let unknown_template = json!({ "template_id": "standard-999" });
        let response = route_one(
            &request(
                "POST",
                "/api/v1/layouts/drafts",
                &serde_json::to_vec(&unknown_template).unwrap(),
            ),
            &root,
        );
        assert_eq!(response.status, 422);
    }

    // --- Project route integration tests ------------------------------------

    /// Copy the repo's example project (plus every referenced file) into a fresh
    /// temporary directory, returning `(dir, copied project path)`.
    fn example_project_copy(tag: &str) -> (PathBuf, PathBuf) {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seattrellis_projects_test_{}_{}_{}",
            std::process::id(),
            tag,
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "project.seattrellis.json",
            "students.csv",
            "classroom.json",
            "rules_multi_candidate.json",
        ] {
            fs::copy(examples.join(name), dir.join(name)).unwrap();
        }
        for name in ["history", "outputs"] {
            copy_dir(&examples.join(name), &dir.join(name));
        }
        let project_path = dir.join("project.seattrellis.json");
        (dir, project_path)
    }

    fn copy_dir(source: &Path, dest: &Path) {
        if !source.is_dir() {
            return;
        }
        fs::create_dir_all(dest).unwrap();
        for entry in fs::read_dir(source).unwrap().flatten() {
            let target = dest.join(entry.file_name());
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// Build a multipart body from named fields (name -> raw bytes).
    fn multipart_form(fields: &[(&str, &[u8])], boundary: &str) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            );
            body.extend_from_slice(value);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        body
    }

    /// Project routes: list recent projects under a root, read history and
    /// privacy for a real project, pack it into a zip, and restore the zip into
    /// a fresh destination. Covers the full workspace flow against example data.
    #[test]
    fn project_routes_list_history_privacy_pack_restore() {
        let root = test_web_root();
        let (dir, project_path) = example_project_copy("flow");
        // Canonicalize so the asserted path matches the `list_projects` output
        // (which canonicalizes; on macOS `/var` resolves to `/private/var`).
        let project_path_str = fs::canonicalize(&project_path)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let root_str = dir.to_string_lossy().into_owned();

        // 1. List recent projects under the fixture directory.
        let list = route_one(
            &request(
                "GET",
                &format!("/api/v1/projects/recent?root={root_str}&limit=20"),
                b"",
            ),
            &root,
        );
        assert_eq!(
            list.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&list.body)
        );
        let list_val = body_json(&list);
        assert_eq!(list_val["api_version"], "1");
        let projects = list_val["projects"].as_array().unwrap();
        assert!(
            projects.iter().any(|project| project["path"] == project_path_str),
            "project list should include {project_path_str}: {projects:?}"
        );

        // 2. Project history.
        let history_body = json!({ "project_path": project_path_str, "include_outputs": true });
        let history = route_one(
            &request(
                "POST",
                "/api/v1/projects/history",
                &serde_json::to_vec(&history_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            history.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&history.body)
        );
        let history_val = body_json(&history);
        assert_eq!(history_val["api_version"], "1");
        assert_eq!(history_val["project_name"], "Demo Class");
        assert!(history_val["history"].is_array());
        assert!(history_val["outputs"].is_array());

        // 3. Project privacy scan.
        let privacy_body = json!({ "project_path": project_path_str, "include_outputs": true });
        let privacy = route_one(
            &request(
                "POST",
                "/api/v1/projects/privacy",
                &serde_json::to_vec(&privacy_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            privacy.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&privacy.body)
        );
        let privacy_val = body_json(&privacy);
        assert_eq!(privacy_val["api_version"], "1");
        assert!(
            privacy_val["files_scanned"].as_u64().unwrap_or(0) > 0,
            "privacy scan should read at least one file"
        );

        // 4. Pack the project into a zip.
        let bundle_body = json!({ "project_path": project_path_str, "include_outputs": true });
        let bundle = route_one(
            &request(
                "POST",
                "/api/v1/projects/bundle",
                &serde_json::to_vec(&bundle_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            bundle.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&bundle.body)
        );
        assert_eq!(bundle.content_type, Some("application/zip"));
        assert!(bundle
            .content_disposition
            .as_deref()
            .unwrap()
            .contains("filename=\"project.seattrellis.zip\""));
        assert!(
            bundle.body.starts_with(b"PK"),
            "zip bytes should start with the PK magic"
        );

        // 5. Restore the zip into a fresh destination directory.
        let output_dir = dir.join("restored");
        let output_dir_str = output_dir.to_string_lossy().into_owned();
        let boundary = "----SeatTrellisRestoreBoundary";
        let restore_body = multipart_form(
            &[
                ("bundle", bundle.body.as_slice()),
                ("output_dir", output_dir_str.as_bytes()),
                ("overwrite", b"false"),
            ],
            boundary,
        );
        let restore = route_one(
            &request_with_content_type(
                "POST",
                "/api/v1/projects/restore",
                &restore_body,
                Some(&format!("multipart/form-data; boundary={boundary}")),
            ),
            &root,
        );
        assert_eq!(
            restore.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&restore.body)
        );
        let restore_val = body_json(&restore);
        assert_eq!(restore_val["api_version"], "1");
        let restored_path = restore_val["project_path"].as_str().unwrap();
        assert!(
            Path::new(restored_path).is_file(),
            "restored project file should exist: {restored_path}"
        );
    }

    /// Project routes error mapping: a missing project file is 404, an
    /// existing-but-invalid project is 422, and a bad bundle upload is 422.
    #[test]
    fn project_routes_validation_errors() {
        let root = test_web_root();

        // Missing project file -> 404.
        let history_body = json!({ "project_path": "/nonexistent/project.seattrellis.json" });
        let history = route_one(
            &request(
                "POST",
                "/api/v1/projects/history",
                &serde_json::to_vec(&history_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(history.status, 404);

        let bundle_body = json!({ "project_path": "/nonexistent/project.seattrellis.json" });
        let bundle = route_one(
            &request(
                "POST",
                "/api/v1/projects/bundle",
                &serde_json::to_vec(&bundle_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(bundle.status, 404);

        // An existing file that is not a project artifact -> 422.
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seattrellis_projects_invalid_test_{seq}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let invalid = dir.join("broken.seattrellis.json");
        fs::write(&invalid, r#"{"not": "a project"}"#).unwrap();
        let invalid_str = invalid.to_string_lossy().into_owned();
        let history_body = json!({ "project_path": invalid_str });
        let history = route_one(
            &request(
                "POST",
                "/api/v1/projects/history",
                &serde_json::to_vec(&history_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(history.status, 422);

        // A multipart restore without a `bundle` field is 422.
        let boundary = "bnd";
        let body = multipart_form(&[("output_dir", b"/tmp")], boundary);
        let restore = route_one(
            &request_with_content_type(
                "POST",
                "/api/v1/projects/restore",
                &body,
                Some(&format!("multipart/form-data; boundary={boundary}")),
            ),
            &root,
        );
        assert_eq!(restore.status, 422);
    }

    // --- Migration route integration tests ----------------------------------

    /// Migration routes: preview, reference checks, single apply, batch preview
    /// and apply, and backup restore against real project fixtures in a temp dir.
    #[test]
    fn migration_routes_preview_checks_apply_batch_restore() {
        let root = test_web_root();
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seattrellis_migration_server_test_{}_{}",
            std::process::id(),
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // Real referenced files so reference checks pass and batch apply is ready.
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
        for name in ["students.csv", "classroom.json", "rules_multi_candidate.json"] {
            fs::copy(examples.join(name), dir.join(name)).unwrap();
        }
        fs::create_dir_all(dir.join("history")).unwrap();
        fs::create_dir_all(dir.join("outputs")).unwrap();

        // Two minimal (pre-migration) project files missing the canonical fields.
        let minimal = r#"{
            "kind": "seattrellis_project",
            "name": "Mig Demo",
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules_multi_candidate.json",
            "history_dir": "history",
            "outputs_dir": "outputs"
        }"#;
        fs::write(dir.join("mig1.seattrellis.json"), minimal).unwrap();
        fs::write(dir.join("mig2.seattrellis.json"), minimal).unwrap();
        let p1 = dir.join("mig1.seattrellis.json").to_string_lossy().into_owned();
        let p2 = dir.join("mig2.seattrellis.json").to_string_lossy().into_owned();

        // 1. Preview a single migration -> changes detected, refs ok.
        let preview_body = json!({ "project_path": p1, "in_place": false });
        let preview = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/preview",
                &serde_json::to_vec(&preview_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            preview.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&preview.body)
        );
        let preview_val = body_json(&preview);
        assert_eq!(preview_val["api_version"], "1");
        assert_eq!(preview_val["dry_run"], true);
        assert!(
            preview_val["change_count"].as_u64().unwrap() > 0,
            "the minimal fixture should be missing canonical fields"
        );
        assert!(preview_val["reference_checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["status"] == "ok"));

        // 2. Standalone reference checks route.
        let checks_body = json!({ "project_path": p1 });
        let checks = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/reference-checks",
                &serde_json::to_vec(&checks_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            checks.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&checks.body)
        );
        assert_eq!(body_json(&checks)["ready"], true);

        // 3. Apply a single migration out-of-place -> a migrated sibling file.
        let apply_body = json!({ "project_path": p1, "in_place": false });
        let apply = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/apply",
                &serde_json::to_vec(&apply_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            apply.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&apply.body)
        );
        let apply_val = body_json(&apply);
        assert_eq!(apply_val["dry_run"], false);
        assert!(apply_val["change_count"].as_u64().unwrap() > 0);
        let output_path = apply_val["output_path"].as_str().unwrap();
        assert!(Path::new(output_path).is_file(), "migrated file should exist");

        // 4. Batch preview + batch apply over both fixtures.
        let batch_body = json!({ "project_paths": [p1, p2], "in_place": false });
        let batch_preview = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/batch/preview",
                &serde_json::to_vec(&batch_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            batch_preview.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&batch_preview.body)
        );
        let batch_preview_val = body_json(&batch_preview);
        assert_eq!(
            batch_preview_val["projects"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(batch_preview_val["ready"], true);

        let batch_apply = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/batch/apply",
                &serde_json::to_vec(&batch_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            batch_apply.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&batch_apply.body)
        );
        let batch_apply_val = body_json(&batch_apply);
        assert_eq!(
            batch_apply_val["projects"].as_array().map(Vec::len),
            Some(2)
        );

        // 5. Apply in place creates a backup, then restore it.
        let in_place_body = json!({ "project_path": p1, "in_place": true });
        let in_place = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/apply",
                &serde_json::to_vec(&in_place_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            in_place.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&in_place.body)
        );
        let in_place_val = body_json(&in_place);
        let backup_path = in_place_val["backup_path"].as_str().unwrap().to_string();
        assert!(
            Path::new(&backup_path).is_file(),
            "in-place apply should create a backup: {backup_path}"
        );

        let restore_body = json!({
            "project_path": p1,
            "source_path": p1,
            "backup_path": backup_path,
        });
        let restore = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/restore",
                &serde_json::to_vec(&restore_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            restore.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&restore.body)
        );
        let restore_val = body_json(&restore);
        assert_eq!(restore_val["restored_valid"], true);
        assert_eq!(restore_val["source_path"], p1);
    }

    /// Migration error mapping: a missing artifact is 404 and a batch with a
    /// single path is 422.
    #[test]
    fn migration_routes_validation_errors() {
        let root = test_web_root();

        // Missing project artifact -> 404.
        let preview_body = json!({ "project_path": "/nonexistent/mig.json" });
        let preview = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/preview",
                &serde_json::to_vec(&preview_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(preview.status, 404);

        // Batch preview requires at least 2 paths -> 422.
        let batch_body = json!({ "project_paths": ["/tmp/only-one.json"] });
        let batch = route_one(
            &request(
                "POST",
                "/api/v1/projects/migration/batch/preview",
                &serde_json::to_vec(&batch_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(batch.status, 422);
    }

    /// `projects/recent` rejects a non-numeric limit as 422.
    #[test]
    fn projects_recent_invalid_limit_is_422() {
        let root = test_web_root();
        let response = route_one(
            &request("GET", "/api/v1/projects/recent?root=.&limit=abc", b""),
            &root,
        );
        assert_eq!(response.status, 422);
        let response = route_one(
            &request("GET", "/api/v1/projects/recent?root=.&limit=0", b""),
            &root,
        );
        assert_eq!(response.status, 422);
    }
}
