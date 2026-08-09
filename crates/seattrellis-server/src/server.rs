//! Loopback-only HTTP backend for the SeatTrellis desktop app.
//!
//! Serves the compiled React workbench (`web_static/`) and exposes the native
//! endpoints the workbench's teacher flow needs end-to-end: roster upload &
//! preview, class generation (which also creates an editable draft), the
//! command-driven seating editor, export, and the static catalogs.
//!
//! The HTTP transport is axum/hyper/tokio (M1-04): [`crate::http`] adapts
//! every request into the legacy [`Request`] shape and dispatches through
//! [`route`], so the business layer and its tests are unchanged. Bounded
//! concurrency, 64 MiB body limit (413) and graceful shutdown come from the
//! maintained stack instead of a hand-rolled parser.
//!
//! Security posture (from-zero standards):
//! - Binds loopback only (`127.0.0.1`); never exposes a LAN address.
//! - No CORS headers are ever emitted; clients must already be same-origin.
//! - Static files are confined to the configured web root; `..` traversal and
//!   percent-encoded escapes are rejected, and canonical paths are re-checked.
//! - Errors are coarse (`404 not found`) and never leak internal paths.
//! - No unwrap/expect on the request path; all failures become HTTP errors.
//! - Session/token/Host checks are the M1-05 milestone (not yet landed).

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use seattrellis_domain::editing::{self, EditorDraftStore};

/// Compiled React workbench location resolved at build time. Used as a
/// fallback so the binary serves assets regardless of the launch directory.
const BUILTIN_WEB_STATIC: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../src/seattrellis/web_static");
/// Display path used when a release binary serves the compiled-in workbench.
/// It deliberately does not point at a real filesystem directory.
const EMBEDDED_WEB_STATIC: &str = "<embedded>/src/seattrellis/web_static";

/// The solve-request store now lives in the application layer (M1-02);
/// re-exported here so the transport keeps a single import path.
pub(crate) use seattrellis_application::SolveRequestStore;

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
    /// Set by the shell (Tauri exit) to stop the accept loop gracefully.
    shutdown: Arc<AtomicBool>,
    /// 256-bit random session token (M1-05). Required as `Bearer` on every
    /// `/api/*` request; injected into the WebView memory by the shell.
    session_token: String,
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
            shutdown: Arc::new(AtomicBool::new(false)),
            session_token: generate_session_token(),
        })
    }

    /// The actual bound address (useful when port 0 auto-assigns).
    pub fn addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The 256-bit session token; shells inject it into the WebView memory.
    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    /// Ask the accept loop to stop (used by the Tauri shell on exit).
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// The shutdown flag, for shells that need to set it from a handler
    /// without holding the `Server` (e.g. a Tauri exit callback).
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Serve the loopback API until a shutdown signal arrives (Ctrl-C/SIGTERM
    /// or [`Server::request_shutdown`]). Blocking facade over the tokio
    /// runtime; axum/hyper handle connections (M1-04).
    pub fn serve(&self) -> io::Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.serve_async())
    }

    async fn serve_async(&self) -> io::Result<()> {
        // The std listener was bound in blocking mode; tokio requires the
        // non-blocking flag before registering the fd with the reactor.
        let listener_std = self.listener.try_clone()?;
        listener_std.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(listener_std)?;
        let state = crate::http::AppState {
            web_root: Arc::clone(&self.web_root),
            editor_store: Arc::clone(&self.editor_store),
            solve_requests: Arc::clone(&self.solve_requests),
            shutdown: Arc::clone(&self.shutdown),
            session_token: Arc::new(self.session_token.clone()),
            bound_host: self.local_addr.ip().to_string(),
            bound_port: self.local_addr.port(),
        };
        let router = crate::http::build_router(state);
        axum::serve(listener, router)
            .with_graceful_shutdown(crate::http::shutdown_signal(Arc::clone(&self.shutdown)))
            .await
    }
}

/// Generate the 256-bit loopback session token (32 CSPRNG bytes, hex).
fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS entropy source must be available");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Locate a complete workbench build (`index.html` present) from, in order:
/// 1. the `SEATTRELLIS_WEB_STATIC` env var,
/// 2. the launch working directory (`src/seattrellis/web_static` or a
///    `../src/...` when launched from the app crate),
/// 3. the workbench embedded at build time,
/// 4. the compile-time path baked into the binary (a development fallback).
pub fn resolve_web_root() -> Result<PathBuf, ServerError> {
    let disk_candidates = [
        std::env::var_os("SEATTRELLIS_WEB_STATIC").map(PathBuf::from),
        Some(PathBuf::from("src/seattrellis/web_static")),
        Some(PathBuf::from("../src/seattrellis/web_static")),
    ];

    for candidate in disk_candidates.into_iter().flatten() {
        if let Ok(resolved) = candidate.canonicalize() {
            if resolved.join("index.html").is_file() {
                return Ok(resolved);
            }
        }
    }

    if crate::embedded_web::has_index() {
        return Ok(PathBuf::from(EMBEDDED_WEB_STATIC));
    }

    // This is only reachable for an unusual development build that omitted
    // the generated asset manifest. Keep the source-tree fallback so the
    // error remains actionable for contributors.
    if let Ok(resolved) = Path::new(BUILTIN_WEB_STATIC).canonicalize() {
        if resolved.join("index.html").is_file() {
            return Ok(resolved);
        }
    }

    Err(ServerError::MissingWebRoot(format!(
        "no workbench build found under SEATTRELLIS_WEB_STATIC, the launch \
         directory, or the built-in path {BUILTIN_WEB_STATIC:?}; build the \
         React frontend first (index.html must exist in web_static/)"
    )))
}

// ---------------------------------------------------------------------------
// Legacy request/response shapes
// ---------------------------------------------------------------------------

/// A parsed HTTP/1.1 request (head + body). The axum adapter (crate::http)
/// fills this from the hyper request; the dispatcher and handlers consume it.
pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    /// The request's `Content-Type` header, if any (needed for multipart).
    pub(crate) content_type: Option<String>,
    pub(crate) body: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Routing and handlers
// ---------------------------------------------------------------------------

/// A minimal response: status code, optional content type, optional
/// `Content-Disposition`, and the raw body.
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) content_type: Option<&'static str>,
    pub(crate) content_disposition: Option<String>,
    pub(crate) body: Vec<u8>,
}

impl Response {
    pub(crate) fn json(status: u16, value: serde_json::Value) -> Response {
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

/// Split a request path (query string already stripped) into segments.
fn path_segments(path: &str) -> Vec<&str> {
    path.trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Map an application-layer error onto an HTTP response (M1-02). The
/// `invalid_solve_request` code carries the frozen SolveStatus (M1-03).
fn app_error_response(error: seattrellis_application::AppError) -> Response {
    let mut body = json!({
        "error": error.code,
        "message": error.message,
    });
    if error.code == "invalid_solve_request" {
        body["status"] = json!(seattrellis_core::classify_solve_error(
            body["message"].as_str().unwrap_or_default(),
        ));
    }
    Response::json(error.status, body)
}

/// `POST /api/v1/classes/generate` (and `/api/v1/solve`): thin transport
/// adapter - the orchestration lives in [`seattrellis_application::class_generation`].
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
    match seattrellis_application::class_generation::generate_class(
        &raw_request,
        editor_store,
        solve_requests,
    ) {
        Ok(outcome) => {
            if !outcome.feasible {
                return Response::json(
                    409,
                    json!({
                        "error": "plan_not_found",
                        "status": outcome.status,
                        "message": "No seating plan was found with the current room and rules.",
                    }),
                );
            }
            Response::json(
                200,
                json!({
                    "class_name": outcome.class_name,
                    "goal": {
                        "goal_id": outcome.goal_id,
                        "title": "日常轮换",
                        "description": "兼顾视力和身高需求，减少近期重复邻座，并适度轮换位置。",
                        "preset_name": null,
                    },
                    "warnings": [],
                    "recommended_candidate_id": outcome.draft_id,
                    "candidates": [{
                        "candidate_id": outcome.draft_id,
                        "recommended": true,
                        "total_score": outcome.total_score,
                    }],
                    "editor": outcome.editor,
                }),
            )
        }
        Err(error) => app_error_response(error),
    }
}

/// `POST /api/v1/classes/rotation`: thin transport adapter - the
/// orchestration lives in [`seattrellis_application::rotation`] (M2 parity,
/// ledger A.1: the rotation-generation main flow the workbench depends on).
fn rotation_generate_response(
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
    match seattrellis_application::rotation::generate_rotation_plan(
        &raw_request,
        editor_store,
        solve_requests,
    ) {
        Ok(outcome) => Response::json(
            200,
            json!({
                "class_name": outcome.class_name,
                "warnings": outcome.warnings,
                "rotation_plan": outcome.plan,
                "editor": outcome.editor,
            }),
        ),
        Err(error) => app_error_response(error),
    }
}

/// `POST /api/v1/exports`: thin transport adapter - the orchestration lives
/// in [`seattrellis_application::export`].
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
    match seattrellis_application::export::export_draft(&value, editor_store, solve_requests) {
        Ok(outcome) => Response {
            status: 200,
            content_type: Some(outcome.content_type),
            content_disposition: Some(outcome.content_disposition),
            body: outcome.body,
        },
        Err(error) => app_error_response(error),
    }
}

/// Dispatch a parsed request to the matching handler.
pub(crate) fn route(
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
        ("POST", ["api", "v1", "classes", "rotation"]) => {
            rotation_generate_response(&request.body, editor_store, solve_requests)
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
        ("POST", ["api", "v1", "projects", "rotation", "save"]) => {
            rotation_save_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "rotation", "load"]) => {
            rotation_load_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "rotation", "group-register"]) => {
            rotation_register_download_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "rotation", "group-register", "preview"]) => {
            rotation_register_preview_response(&request.body)
        }
        ("POST", ["api", "v1", "projects", "rotation", "group-register", "save"]) => {
            rotation_register_save_response(&request.body, request.content_type.as_deref())
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
///    a `CoreSolveRequest` via [`seattrellis_domain::room_templates::room_template_grid`]
///    and [`seattrellis_domain::goal_rules::goal_rules`] before solving.
///
/// Returns the frontend `GenerateClassResponse` shape (`class_name`, `goal`,
/// `warnings`, `recommended_candidate_id`, `candidates`, `editor`). When the
/// solver reports the plan infeasible, the response is `409 plan_not_found`;
/// an unknown room template or goal on the frontend path is `422`.
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
    match seattrellis_io::roster::upload_draft_json(file_bytes) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => json_error(422, &message),
    }
}

fn roster_get_response(draft_id: &str) -> Response {
    match seattrellis_io::roster::get_draft_json(draft_id) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(_) => json_error(404, "roster draft was not found"),
    }
}

fn roster_preview_response(draft_id: &str, body: &[u8]) -> Response {
    let body_str = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return json_error(400, "request body is not valid UTF-8"),
    };
    match seattrellis_io::roster::preview_update_json(draft_id, body_str) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("not found") => {
            json_error(404, "roster draft was not found")
        }
        Err(message) => json_error(400, &message),
    }
}

fn roster_delete_response(draft_id: &str) -> Response {
    if seattrellis_io::roster::delete_draft(draft_id) {
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
    match seattrellis_domain::layouts::create_layout_draft_json(body_str) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("poisoned") => json_error(500, &message),
        Err(message) => json_error(422, &message),
    }
}

/// `GET /api/v1/layouts/drafts/{id}`: fetch the current layout state.
fn layout_get_response(draft_id: &str) -> Response {
    match seattrellis_domain::layouts::get_layout_state_json(draft_id) {
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
    match seattrellis_domain::layouts::dispatch_layout_command_json(draft_id, body_str) {
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
    match seattrellis_domain::layouts::compile_layout_draft_json(draft_id) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) if message.contains("poisoned") => json_error(500, &message),
        Err(message) if message.contains("unknown layout draft") => json_error(404, &message),
        Err(message) => json_error(422, &message),
    }
}

/// `DELETE /api/v1/layouts/drafts/{id}`: remove a layout draft (204), or 404
/// when it never existed.
fn layout_delete_response(draft_id: &str) -> Response {
    if seattrellis_domain::layouts::delete_layout_draft(draft_id) {
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
    match seattrellis_io::projects::list_projects_json(&root, limit) {
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
    project_result_response(seattrellis_io::projects::project_history_json(&project_path))
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
    project_result_response(seattrellis_io::projects::project_privacy_json(&project_path))
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
    match seattrellis_io::projects::pack_project_json(&project_path) {
        Ok(bytes) => {
            record_recent(&project_path);
            let filename = seattrellis_io::projects::default_bundle_name(&project_path)
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

    match seattrellis_io::projects::restore_project_bundle(bundle, &output_dir, overwrite) {
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
    seattrellis_io::projects::record_recent_project(project_path, &name);
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
    match seattrellis_io::migration::migration_preview_json(&project_path) {
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
    match seattrellis_io::migration::migration_apply_json(&project_path, in_place) {
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
    match seattrellis_io::migration::migration_reference_checks_json(&project_path) {
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
    match seattrellis_io::migration::migration_batch_preview_json(&paths) {
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
    match seattrellis_io::migration::migration_batch_apply_json(&paths, in_place) {
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
    match seattrellis_io::migration::migration_restore_json(&backup_path, &source_path) {
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

// ---------------------------------------------------------------------------
// Rotation routes
// ---------------------------------------------------------------------------

/// `POST /api/v1/projects/rotation/save`: persist a rotation plan into the
/// project outputs (`{project_path, rotation_plan, draft_ids?, output_name?}`).
/// `draft_ids` / `output_name` are accepted for workbench compatibility; the
/// native module derives its artifact name from the outputs directory.
fn rotation_save_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(rotation_plan) = value.get("rotation_plan") else {
        return json_error(400, "request body is missing a 'rotation_plan' field");
    };
    let project_path = resolve_request_path(&project_path);
    let plan_json = rotation_plan.to_string();
    match seattrellis_io::rotation::rotation_save_json(&project_path, &plan_json) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => rotation_error_response(&message),
    }
}

/// `POST /api/v1/projects/rotation/load`: read the saved rotation plan back
/// (`{project_path, artifact_path?}`). `artifact_path` is accepted for
/// workbench compatibility; the module locates `rotation-plan.json` in the
/// project's outputs directory.
fn rotation_load_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    match seattrellis_io::rotation::rotation_load_json(&project_path) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => rotation_error_response(&message),
    }
}

/// `POST /api/v1/projects/rotation/group-register`: render a printable HTML or
/// tabular CSV register for one rotation period. The workbench sends
/// `{project_path, artifact_path?, format, locale?}` and reads the bytes plus
/// the `Content-Disposition` filename. `period_index` selects the period
/// (default 1) because the native module renders one period at a time.
fn rotation_register_download_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let period_index = match optional_i64(&value, "period_index") {
        Ok(value) => value.unwrap_or(1),
        Err(response) => return response,
    };
    let format_name = match optional_string(&value, "format") {
        Ok(value) => value.unwrap_or_else(|| "html".to_string()),
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    if format_name.eq_ignore_ascii_case("csv") {
        match seattrellis_io::rotation::group_register_csv_json(&project_path, period_index) {
            Ok(bytes) => Response {
                status: 200,
                content_type: Some("text/csv; charset=utf-8"),
                content_disposition: Some("attachment; filename=\"group-register.csv\"".to_string()),
                body: bytes,
            },
            Err(message) => rotation_error_response(&message),
        }
    } else if format_name.eq_ignore_ascii_case("html") {
        match seattrellis_io::rotation::group_register_html_json(&project_path, period_index) {
            Ok(bytes) => Response {
                status: 200,
                content_type: Some("text/html; charset=utf-8"),
                content_disposition: Some("attachment; filename=\"group-register.html\"".to_string()),
                body: bytes,
            },
            Err(message) => rotation_error_response(&message),
        }
    } else {
        json_error(400, "request field 'format' must be \"html\" or \"csv\"")
    }
}

/// `POST /api/v1/projects/rotation/group-register/preview`: summarize one
/// rotation period's membership grouped by seat row and column
/// (`{project_path, artifact_path?, period_index?}`).
fn rotation_register_preview_response(body: &[u8]) -> Response {
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let period_index = match optional_i64(&value, "period_index") {
        Ok(value) => value.unwrap_or(1),
        Err(response) => return response,
    };
    let project_path = resolve_request_path(&project_path);
    match seattrellis_io::rotation::group_register_preview_json(&project_path, period_index) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => rotation_error_response(&message),
    }
}

/// `POST /api/v1/projects/rotation/group-register/save`: persist a group
/// register payload to the project outputs (`{project_path, groups}` or a
/// multipart form with `project_path` + `groups` fields). The groups payload
/// may be a JSON array or an object with a `groups` array.
fn rotation_register_save_response(body: &[u8], content_type: Option<&str>) -> Response {
    let is_multipart = content_type
        .map(|value| {
            value
                .to_ascii_lowercase()
                .starts_with("multipart/form-data")
        })
        .unwrap_or(false);
    if is_multipart {
        let Some(boundary) = content_type.and_then(multipart_boundary) else {
            return json_error(400, "multipart/form-data boundary is missing");
        };
        let fields = match parse_multipart(body, &boundary) {
            Ok(fields) => fields,
            Err(message) => return json_error(422, &message),
        };
        let project_path = match fields.get("project_path") {
            Some(bytes) => match std::str::from_utf8(bytes) {
                Ok(path) => path.to_string(),
                Err(_) => return json_error(400, "multipart 'project_path' is not valid UTF-8"),
            },
            None => return json_error(422, "upload is missing a 'project_path' field"),
        };
        let groups_json = match fields.get("groups") {
            Some(bytes) => match std::str::from_utf8(bytes) {
                Ok(json) => json.to_string(),
                Err(_) => return json_error(400, "multipart 'groups' is not valid UTF-8"),
            },
            None => return json_error(422, "upload is missing a 'groups' field"),
        };
        let project_path = resolve_request_path(&project_path);
        return match seattrellis_io::rotation::group_register_save_json(&project_path, &groups_json) {
            Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
            Err(message) => rotation_error_response(&message),
        };
    }
    let value = match parse_body_json(body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let project_path = match required_string(&value, "project_path") {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(groups) = value.get("groups").filter(|groups| !groups.is_null()) else {
        return json_error(400, "request body is missing a 'groups' field");
    };
    let project_path = resolve_request_path(&project_path);
    let groups_json = groups.to_string();
    match seattrellis_io::rotation::group_register_save_json(&project_path, &groups_json) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => rotation_error_response(&message),
    }
}

/// Map a rotation-domain error onto the matching HTTP status: 404 for a
/// missing project file or rotation artifact (or an out-of-range period), 422
/// for validation problems.
fn rotation_error_response(message: &str) -> Response {
    if message.contains("poisoned") {
        json_error(500, message)
    } else if message.contains("not found")
        || message.contains("does not exist")
        || message.contains("out of range")
        || message.contains("No saved rotation plan")
    {
        json_error(404, message)
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

/// Read an optional string field from a parsed JSON object body (`None` when
/// absent).
fn optional_string(value: &Value, field: &str) -> Result<Option<String>, Response> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(json_error(
            400,
            &format!("request field '{field}' must be a string"),
        )),
    }
}

/// Read an optional integer field from a parsed JSON object body (`None` when
/// absent).
fn optional_i64(value: &Value, field: &str) -> Result<Option<i64>, Response> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| json_error(400, &format!("request field '{field}' must be an integer"))),
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
        Err(_) => match crate::embedded_web::get("index.html") {
            Some(bytes) => Response::text(200, "text/html; charset=utf-8", bytes.to_vec()),
            None => plain_response(500, "workbench index.html is missing"),
        },
    }
}

fn static_response(web_root: &Path, path: &str) -> Response {
    if let Some(target) = safe_join(web_root, path) {
        if let Ok(bytes) = fs::read(&target) {
            let content_type = content_type_for(&target);
            return Response::text(200, content_type, bytes);
        }
    }

    let Some(asset_path) = normalized_asset_path(path) else {
        return plain_response(404, "not found");
    };
    match crate::embedded_web::get(&asset_path) {
        Some(bytes) => Response::text(
            200,
            content_type_for(Path::new(&asset_path)),
            bytes.to_vec(),
        ),
        None => plain_response(404, "not found"),
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
    let joined = normalized_asset_path(path)?;
    let candidate = web_root.join(joined);

    let root_canonical = web_root.canonicalize().ok()?;
    let candidate_canonical = candidate.canonicalize().ok()?;
    if !candidate_canonical.starts_with(&root_canonical) {
        return None;
    }
    Some(candidate)
}

/// Normalize a URL path for both filesystem and embedded-asset lookups.
/// Keeping this check shared prevents the embedded fallback from becoming a
/// weaker path than the development filesystem server.
fn normalized_asset_path(path: &str) -> Option<String> {
    let decoded = percent_decode(path).ok()?;
    if decoded.contains('\0') {
        return None;
    }

    let mut segments = Vec::new();
    for segment in decoded.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            segment => segments.push(segment),
        }
    }
    let joined = segments.join("/");
    Some(if joined.is_empty() {
        "index.html".to_string()
    } else {
        joined
    })
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

    /// Seat coordinates `(row, col)` for a seated student in an editor draft.
    fn editor_seat_coords(editor: &Value, student_key: &str) -> Option<(i64, i64)> {
        let entry = editor["students"]
            .as_array()?
            .iter()
            .find(|student| student["student_key"].as_str() == Some(student_key))?;
        let seat_id = entry["seat_id"].as_str()?;
        let seat = editor["seats"]
            .as_array()?
            .iter()
            .find(|seat| seat["seat_id"].as_str() == Some(seat_id))?;
        Some((seat["row"].as_i64()?, seat["col"].as_i64()?))
    }

    /// Four enabled seats in a single row (deterministic adjacency for tests).
    fn line_of_four_layout() -> Value {
        json!({
            "layout_id": "line-4",
            "name": "Line of four",
            "seats": [
                {"seat_id": "P1", "row": 1, "col": 1, "enabled": true},
                {"seat_id": "P2", "row": 1, "col": 2, "enabled": true},
                {"seat_id": "P3", "row": 1, "col": 3, "enabled": true},
                {"seat_id": "P4", "row": 1, "col": 4, "enabled": true}
            ],
            "adjacency": {
                "include_horizontal": true,
                "include_vertical": false,
                "include_diagonal": false
            }
        })
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
    fn embedded_workbench_is_used_when_filesystem_root_is_missing() {
        let root = std::env::temp_dir().join(format!(
            "seattrellis_missing_web_root_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        let index = route_one(&request("GET", "/", b""), &root);
        assert_eq!(index.status, 200);
        assert_eq!(index.content_type, Some("text/html; charset=utf-8"));
        assert_eq!(index.body.as_slice(), crate::embedded_web::get("index.html").unwrap());

        let asset_path = crate::embedded_web::EMBEDDED_WEB_ASSETS
            .iter()
            .find_map(|(path, _)| path.strip_prefix("assets/").map(|_| *path))
            .expect("embedded workbench should contain an asset");
        let asset = route_one(&request("GET", &format!("/{asset_path}"), b""), &root);
        assert_eq!(asset.status, 200);
        assert_eq!(asset.body.as_slice(), crate::embedded_web::get(asset_path).unwrap());
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
    fn rotation_generate_creates_multi_period_plan() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Physics Rotation",
                "students": [
                    {"student_id": "S1", "name": "Alice", "score": 93},
                    {"student_id": "S2", "name": "Bob", "score": 81},
                    {"student_id": "S3", "name": "Carol", "score": 75}
                ],
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "daily-rotation"}
            },
            "period_count": 3,
            "period_labels": ["Week 1", "Week 2", "Week 3"],
            "options": {"seed": 42}
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/rotation", &body), &root);
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let value = body_json(&response);

        // RotationPlan shape: three labelled periods, each with assignments.
        let plan = &value["rotation_plan"];
        assert_eq!(plan["kind"], "rotation_plan");
        assert_eq!(plan["name"], "Physics Rotation");
        let periods = plan["periods"].as_array().unwrap();
        assert_eq!(periods.len(), 3);
        assert_eq!(periods[0]["label"], "Week 1");
        assert_eq!(periods[2]["label"], "Week 3");
        for period in periods {
            let assignments = period["snapshot"]["assignments"].as_array().unwrap();
            assert_eq!(assignments.len(), 3, "every period seats all students");
            assert!(period["snapshot"]["solver_status"].is_string());
        }
        assert_eq!(plan["base_history_count"], 0);
        assert_eq!(plan["metadata"]["period_count"], 3);
        assert_eq!(plan["metadata"]["backend"], "native");
        // Fairness + pair summaries are present and count the periods.
        assert_eq!(plan["fairness_summary"]["history_count"], 3);
        assert_eq!(plan["pair_repeat_summary"]["history_count"], 3);

        // The response carries a first-period editor draft the workbench can
        // open immediately.
        let editor = &value["editor"];
        assert!(editor["draft_id"].as_str().is_some());
        assert_eq!(editor["students"].as_array().unwrap().len(), 3);
        assert!(
            value["class_name"].as_str().is_some_and(|name| !name.is_empty()),
            "class_name: {}",
            value["class_name"]
        );
        assert_eq!(value["warnings"], json!([]));
    }

    #[test]
    fn rotation_generate_rejects_unknown_room() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "students": [{"student_id": "S1", "name": "Alice"}],
                "room": {"template_id": "standard-99"},
                "goal": {"goal_id": "daily-rotation"}
            },
            "period_count": 2
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/rotation", &body), &root);
        assert_eq!(response.status, 422, "body: {}", String::from_utf8_lossy(&response.body));
    }

    #[test]
    fn rotation_generate_uses_base_history_snapshots() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Rotation With History",
                "students": [
                    {"student_id": "S1", "name": "Alice"},
                    {"student_id": "S2", "name": "Bob"}
                ],
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "daily-rotation"},
                "history_snapshots": [{
                    "assignments": [
                        {"student_key": "S1", "seat_id": "R1C1"},
                        {"student_key": "S2", "seat_id": "R1C2"}
                    ]
                }]
            },
            "period_count": 2,
            "options": {"seed": 7}
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/rotation", &body), &root);
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
        let plan = &body_json(&response)["rotation_plan"];
        assert_eq!(plan["base_history_count"], 1, "one base snapshot");
        assert_eq!(plan["fairness_summary"]["history_count"], 3, "base + 2 generated periods");
    }

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

    /// `rules_overlay.groups` must reach the solver: a `together` group is
    /// seated adjacently and a `separate` group is kept apart.
    #[test]
    fn frontend_rules_overlay_groups_reach_the_solver() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Groups",
                "students": [
                    {"student_id": "S1", "name": "Ann"},
                    {"student_id": "S2", "name": "Ben"},
                    {"student_id": "S3", "name": "Cid"},
                    {"student_id": "S4", "name": "Dee"}
                ],
                "room": {"layout": line_of_four_layout()},
                "goal": {
                    "goal_id": "quick-shuffle",
                    "rules_overlay": {
                        "groups": [
                            {"name": "buddy", "students": ["S1", "S2"], "together": true},
                            {"name": "rival", "students": ["S3", "S4"], "separate": true}
                        ]
                    }
                }
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
        let editor = body_json(&response)["editor"].clone();
        let coords = |key: &str| editor_seat_coords(&editor, key).expect("seated student");
        let (row_a, col_a) = coords("S1");
        let (row_b, col_b) = coords("S2");
        let (row_c, col_c) = coords("S3");
        let (row_d, col_d) = coords("S4");
        assert_eq!(row_a, row_b, "S1 and S2 share a row");
        assert_eq!((col_a - col_b).abs(), 1, "S1 and S2 must sit together");
        assert!(
            row_c != row_d || (col_c - col_d).abs() != 1,
            "S3 and S4 must sit apart"
        );
    }

    /// `hard_rules` (fixed seat + adjacency pairs) must be resolved from
    /// student keys and seat ids into enforced index pairs.
    #[test]
    fn frontend_hard_rules_are_resolved_and_enforced() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Pinned",
                "students": [
                    {"student_id": "S1", "name": "Ann"},
                    {"student_id": "S2", "name": "Ben"},
                    {"student_id": "S3", "name": "Cid"},
                    {"student_id": "S4", "name": "Dee"}
                ],
                "room": {"layout": line_of_four_layout()},
                "goal": {
                    "goal_id": "quick-shuffle",
                    "hard_rules": {
                        "fixed_seats": [{"student": "S1", "seat_id": "P4"}],
                        "must_be_adjacent": [{"students": ["S2", "S3"]}]
                    }
                }
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
        let editor = body_json(&response)["editor"].clone();
        let coords = |key: &str| editor_seat_coords(&editor, key).expect("seated student");
        assert_eq!(coords("S1"), (1, 4), "S1 is pinned to P4");
        let (row_b, col_b) = coords("S2");
        let (row_c, col_c) = coords("S3");
        assert_eq!(row_b, row_c, "S2 and S3 share a row");
        assert_eq!((col_b - col_c).abs(), 1, "S2 and S3 must sit adjacent");
    }

    /// A custom `draft.room.layout` (the React room builder) must be accepted
    /// and drive the grid instead of a template id.
    #[test]
    fn frontend_custom_layout_generates() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Custom room",
                "students": [
                    {"student_id": "S1", "name": "Ann"},
                    {"student_id": "S2", "name": "Ben"}
                ],
                "room": {"layout": line_of_four_layout()},
                "goal": {"goal_id": "quick-shuffle"}
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
        assert_eq!(value["editor"]["seats"].as_array().map(Vec::len), Some(4));
    }

    /// The custom goal requires `custom_rules`; a full document is accepted and
    /// a missing one is a 422.
    #[test]
    fn frontend_custom_goal_requires_custom_rules() {
        let root = test_web_root();
        let missing = json!({
            "draft": {
                "name": "Custom",
                "students": [
                    {"student_id": "S1", "name": "Ann"},
                    {"student_id": "S2", "name": "Ben"}
                ],
                "room": {"template_id": "standard-30"},
                "goal": {"goal_id": "custom"}
            }
        });
        let body = serde_json::to_vec(&missing).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 422, "custom goal without rules must be a 422");
        assert_eq!(body_json(&response)["error"], "invalid_class_draft");

        let with_rules = json!({
            "draft": {
                "name": "Custom",
                "students": [
                    {"student_id": "S1", "name": "Ann", "score": 90},
                    {"student_id": "S2", "name": "Ben", "score": 70}
                ],
                "room": {"template_id": "standard-30"},
                "goal": {
                    "goal_id": "custom",
                    "custom_rules": {
                        "seed": 1,
                        "soft": {
                            "vision_front": {"enabled": true, "weight": 20},
                            "randomize": {"enabled": true, "weight": 1}
                        }
                    }
                }
            }
        });
        let body = serde_json::to_vec(&with_rules).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(
            response.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&response.body)
        );
    }

    /// A hard rule that names an unknown student must be a 422, not silently
    /// dropped.
    #[test]
    fn frontend_hard_rule_unknown_student_is_422() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "Bad",
                "students": [
                    {"student_id": "S1", "name": "Ann"},
                    {"student_id": "S2", "name": "Ben"}
                ],
                "room": {"layout": line_of_four_layout()},
                "goal": {
                    "goal_id": "quick-shuffle",
                    "hard_rules": {
                        "fixed_seats": [{"student": "GHOST", "seat_id": "P1"}]
                    }
                }
            }
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 422, "unknown hard-rule student must be a 422");
        let value = body_json(&response);
        assert_eq!(value["error"], "invalid_class_draft");
        assert!(value["message"].as_str().unwrap().contains("GHOST"));
    }

    /// `history_snapshots` must be forwarded as core `history` + `pair_history`
    /// so fair_rotation and recent-neighbor costs see past placements.
    #[test]
    fn history_snapshots_forward_fair_rotation_and_pair_data() {
        let grid = seattrellis_domain::room_templates::grid_from_layout(&line_of_four_layout())
            .expect("line layout is valid");
        let students: Vec<Value> = json!([{ "key": "S1" }, { "key": "S2" }])
            .as_array()
            .unwrap()
            .clone();
        let snapshots: Vec<Value> = json!([{
            "schema_version": "1",
            "assignments": [
                {"student_key": "S1", "seat_id": "P1"},
                {"student_key": "S2", "seat_id": "P2"}
            ]
        }])
        .as_array()
        .unwrap()
        .clone();

        let (history, pair_history) =
            seattrellis_application::class_generation::build_history_json(&students, &grid, &snapshots).expect("snapshots build");
        assert_eq!(history["history_count"], 1);
        let s1 = &history["students"]["S1"];
        assert_eq!(s1["records"].as_array().map(Vec::len), Some(1));
        let s1_categories = s1["records"][0]["categories"].as_array().unwrap();
        // P1 is col 1 in the single-row layout: side + corner (no zones, so the
        // single row is inferred as "middle" rather than "front").
        for expected in ["side", "corner"] {
            assert!(
                s1_categories.iter().any(|category| category == expected),
                "S1 (P1) should include {expected}: {s1_categories:?}"
            );
        }
        for category in s1_categories {
            assert_eq!(
                s1["category_counts"][category.as_str().unwrap()],
                json!(1),
                "category_counts must agree with records"
            );
        }
        // S1 (P1) and S2 (P2) sit side by side, so their pair relation is
        // recorded and the recent-neighbor cost can penalize a repeat.
        assert_eq!(pair_history["history_count"], 1);
        let pair = &pair_history["pairs"]["S1|S2"];
        assert!(pair.is_object(), "adjacent S1/S2 must appear in pair history");
        let relations = pair["records"][0]["relations"].as_array().unwrap();
        assert!(
            relations.iter().any(|relation| relation == "desk_mate"),
            "side-by-side seats are desk mates: {relations:?}"
        );
    }

    /// A frontend request carrying history_snapshots must still generate (the
    /// history is forwarded, not rejected).
    #[test]
    fn frontend_history_snapshots_do_not_break_generation() {
        let root = test_web_root();
        let problem = json!({
            "draft": {
                "name": "History",
                "students": [
                    {"student_id": "S1", "name": "Ann", "score": 92},
                    {"student_id": "S2", "name": "Ben", "score": 84}
                ],
                "room": {"layout": line_of_four_layout()},
                "goal": {"goal_id": "daily-rotation"},
                "history_snapshots": [{
                    "schema_version": "1",
                    "assignments": [
                        {"student_key": "S1", "seat_id": "P1"},
                        {"student_key": "S2", "seat_id": "P2"}
                    ]
                }]
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
        assert_eq!(value["goal"]["goal_id"], "daily-rotation");
        assert_eq!(value["editor"]["students"].as_array().map(Vec::len), Some(2));
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
        // M1-03: the frozen SolveStatus rides along; greedy exhaustion is
        // `Unknown`, never `ProvenInfeasible`.
        assert_eq!(value["status"], "Unknown");
    }

    #[test]
    fn solve_invalid_request_carries_frozen_status() {
        let root = test_web_root();
        // Unsupported api_version is a validation failure: 400 + InvalidInput.
        let problem = json!({
            "api_version": 99,
            "student_count": 2,
            "seat_positions": [[1.0,1.0],[2.0,1.0]]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route_one(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 400);
        let value = body_json(&response);
        assert_eq!(value["status"], "InvalidInput");
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
        let value = body_json(&response);
        assert_eq!(value["error"], "invalid_solve_request");
        assert_eq!(value["status"], "InvalidInput");
        assert!(value["message"].as_str().unwrap().contains("cannot seat more students"));
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
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
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
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
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

    /// A fresh temp project directory for the rotation routes.
    fn rotation_project_dir() -> PathBuf {
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "seattrellis_rotation_server_test_{}_{}",
            std::process::id(),
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a minimal `seattrellis_project` file and return its path.
    fn rotation_project_file(dir: &Path) -> String {
        let project = json!({
            "kind": "seattrellis_project",
            "schema_version": 1,
            "students": "students.csv",
            "layout": "classroom.json",
            "rules": "rules.json",
            "outputs_dir": "outputs",
        });
        let project_file = dir.join("project.seattrellis.json");
        fs::write(&project_file, serde_json::to_vec(&project).unwrap()).unwrap();
        fs::write(dir.join("students.csv"), "student_id,name\n").unwrap();
        fs::write(dir.join("classroom.json"), r#"{"seats":[]}"#).unwrap();
        fs::write(dir.join("rules.json"), r#"{}"#).unwrap();
        project_file.to_string_lossy().into_owned()
    }

    /// A two-period rotation plan matching the module's test fixture.
    fn rotation_plan_value() -> Value {
        json!({
            "schema_version": "1.0",
            "kind": "rotation_plan",
            "name": "Weekly Rotation",
            "periods": [
                {
                    "period": 1,
                    "label": "Week 1",
                    "snapshot": {
                        "solver_status": "FEASIBLE",
                        "assignments": [
                            {"student_key": "STU001", "student_name": "Alice", "seat_id": "R1C1"},
                            {"student_key": "STU002", "student_name": "Bob", "seat_id": "R1C3"}
                        ],
                        "students": [
                            {"student_id": "STU001", "name": "Alice"},
                            {"student_id": "STU002", "name": "Bob"}
                        ],
                        "layout": {"seats": [
                            {"seat_id": "R1C1", "row": 1, "col": 1, "enabled": true},
                            {"seat_id": "R1C3", "row": 1, "col": 3, "enabled": true}
                        ]}
                    }
                },
                {
                    "period": 2,
                    "label": "Week 2",
                    "snapshot": {
                        "solver_status": "FEASIBLE",
                        "assignments": [
                            {"student_key": "STU001", "student_name": "Alice", "seat_id": "R2C2"}
                        ]
                    }
                }
            ]
        })
    }

    /// Save + load round trip, preview, and the HTML/CSV download magic bytes.
    #[test]
    fn rotation_routes_save_load_preview_download() {
        let root = test_web_root();
        let dir = rotation_project_dir();
        let project_path = rotation_project_file(&dir);

        // 1. Save accepts the workbench shape (extra fields ignored) and
        //    returns the module's `ProjectRotationSaveResponse` envelope.
        let save_body = json!({
            "project_path": project_path,
            "rotation_plan": rotation_plan_value(),
            "draft_ids": ["draft-1", "draft-2"],
        });
        let save = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/save",
                &serde_json::to_vec(&save_body).unwrap(),
            ),
            &root,
        );
        assert_eq!(
            save.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&save.body)
        );
        let save_val = body_json(&save);
        assert_eq!(save_val["api_version"], "1");
        assert_eq!(save_val["period_count"], 2);
        assert!(save_val["saved_at"].as_str().unwrap().ends_with("+00:00"));
        let output_path = save_val["output_path"].as_str().unwrap().to_string();
        assert!(output_path.ends_with("rotation-plan.json"));

        // 2. Load returns the plan stored on disk.
        let load = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/load",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "artifact_path": output_path,
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(
            load.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&load.body)
        );
        let load_val = body_json(&load);
        assert_eq!(load_val["artifact_path"].as_str().unwrap(), output_path);
        assert_eq!(load_val["project_path"].as_str().unwrap(), save_val["project_path"]);
        assert_eq!(load_val["rotation_plan"]["name"], "Weekly Rotation");
        assert_eq!(load_val["rotation_plan"]["periods"].as_array().unwrap().len(), 2);

        // 3. Preview defaults to period 1 and groups by row and column.
        let preview = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/preview",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "artifact_path": output_path,
                }))
                .unwrap(),
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
        assert_eq!(preview_val["period"], 1);
        assert_eq!(preview_val["period_label"], "Week 1");
        assert_eq!(preview_val["plan_name"], "Weekly Rotation");
        assert_eq!(preview_val["period_count"], 2);
        assert!(!preview_val["row_groups"].as_array().unwrap().is_empty());
        assert!(!preview_val["column_groups"].as_array().unwrap().is_empty());

        // An explicit period_index selects the other period.
        let preview2 = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/preview",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "period_index": 2,
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(preview2.status, 200);
        assert_eq!(body_json(&preview2)["period"], 2);

        // 4. HTML download: text/html, doctype magic, attachment filename.
        let html = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "format": "html",
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(
            html.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&html.body)
        );
        assert_eq!(html.content_type, Some("text/html; charset=utf-8"));
        assert!(html.body.starts_with(b"<!doctype html>"));
        assert!(
            html.content_disposition
                .as_deref()
                .unwrap()
                .contains("filename=\"group-register.html\"")
        );

        // 5. CSV download: text/csv with a UTF-8 BOM magic prefix.
        let csv = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "format": "csv",
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(
            csv.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&csv.body)
        );
        assert_eq!(csv.content_type, Some("text/csv; charset=utf-8"));
        assert_eq!(&csv.body[..3], &[0xEF, 0xBB, 0xBF]);
        assert!(
            csv.content_disposition
                .as_deref()
                .unwrap()
                .contains("filename=\"group-register.csv\"")
        );

        // 6. Persist a group register (JSON body) and read it back from disk.
        let groups = json!({ "groups": [{ "name": "A", "students": ["STU001"] }] });
        let saved_groups = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/save",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "groups": groups,
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(
            saved_groups.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&saved_groups.body)
        );
        let saved_groups_val = body_json(&saved_groups);
        assert_eq!(saved_groups_val["group_count"], 1);
        assert!(saved_groups_val["output_path"]
            .as_str()
            .unwrap()
            .ends_with("group-register.json"));
        let on_disk: Value =
            serde_json::from_slice(&fs::read(dir.join("outputs").join("group-register.json")).unwrap())
                .unwrap();
        assert_eq!(on_disk["groups"][0]["name"], "A");

        // 7. The same save endpoint accepts a multipart form.
        let boundary = "rotation-multipart-boundary";
        let mut multipart_body = Vec::new();
        multipart_body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"project_path\"\r\n\r\n{project_path}\r\n"
            )
            .as_bytes(),
        );
        multipart_body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"groups\"\r\n\r\n{groups}\r\n"
            )
            .as_bytes(),
        );
        multipart_body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let multipart_save = route_one(
            &request_with_content_type(
                "POST",
                "/api/v1/projects/rotation/group-register/save",
                &multipart_body,
                Some(&format!("multipart/form-data; boundary={boundary}")),
            ),
            &root,
        );
        assert_eq!(
            multipart_save.status,
            200,
            "body: {}",
            String::from_utf8_lossy(&multipart_save.body)
        );
        assert_eq!(body_json(&multipart_save)["group_count"], 1);
    }

    /// Missing artifacts and bad request shapes map to 400/404/422.
    #[test]
    fn rotation_routes_error_mapping() {
        let root = test_web_root();
        let dir = rotation_project_dir();
        let project_path = rotation_project_file(&dir);

        // No saved plan yet -> load is 404.
        let load = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/load",
                &serde_json::to_vec(&json!({ "project_path": project_path })).unwrap(),
            ),
            &root,
        );
        assert_eq!(load.status, 404);

        // Preview and register before any save are also 404.
        let preview = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/preview",
                &serde_json::to_vec(&json!({ "project_path": project_path })).unwrap(),
            ),
            &root,
        );
        assert_eq!(preview.status, 404);
        let register = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register",
                &serde_json::to_vec(&json!({ "project_path": project_path })).unwrap(),
            ),
            &root,
        );
        assert_eq!(register.status, 404);

        // Missing `rotation_plan` -> 400.
        let bad_save = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/save",
                &serde_json::to_vec(&json!({ "project_path": project_path })).unwrap(),
            ),
            &root,
        );
        assert_eq!(bad_save.status, 400);

        // Invalid JSON body -> 400.
        let bad_json = route_one(
            &request("POST", "/api/v1/projects/rotation/save", b"not json"),
            &root,
        );
        assert_eq!(bad_json.status, 400);

        // Invalid plan shape -> 422.
        let bad_plan = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/save",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "rotation_plan": { "periods": [] },
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(bad_plan.status, 422);

        // Save a valid plan so we can probe period/format errors.
        route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/save",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "rotation_plan": rotation_plan_value(),
                }))
                .unwrap(),
            ),
            &root,
        );

        // Out-of-range period -> 404.
        let bad_period = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/preview",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "period_index": 99,
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(bad_period.status, 404);

        // Unknown download format -> 400.
        let bad_format = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "format": "pdf",
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(bad_format.status, 400);

        // Missing groups on the save endpoint -> 400.
        let bad_groups = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/save",
                &serde_json::to_vec(&json!({ "project_path": project_path })).unwrap(),
            ),
            &root,
        );
        assert_eq!(bad_groups.status, 400);

        // Invalid groups payload -> 422.
        let invalid_groups = route_one(
            &request(
                "POST",
                "/api/v1/projects/rotation/group-register/save",
                &serde_json::to_vec(&json!({
                    "project_path": project_path,
                    "groups": { "not_groups": true },
                }))
                .unwrap(),
            ),
            &root,
        );
        assert_eq!(invalid_groups.status, 422);
    }
}
