//! axum/hyper/tokio adapter for the loopback backend (M1-04).
//!
//! Replaces the hand-rolled HTTP/1.1 parser and per-connection threads with a
//! maintained, upstream-fuzzed stack while keeping the entire business layer
//! untouched: every request is adapted into the existing [`Request`] shape and
//! dispatched through [`crate::server::route`], so the 50+ routing tests keep
//! exercising the exact same code path.
//!
//! Behavioral notes vs. the old server:
//! - Oversized bodies: axum returns 413 (the old parser used 411).
//! - Transfer-Encoding chunked and keep-alive are now handled by hyper
//!   (the old parser rejected chunked and forced `Connection: close`).
//! - Authenticated work uses bounded admission and aggregate body budgets.
//!   Cancellation bypasses occupied work slots so active jobs can always stop.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, HttpBody};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response as AxumResponse;
use axum::Router;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::server::{route, Request, Response, SolveRequestStore};
use seattrellis_domain::editing::EditorDraftStore;

/// Maximum accepted request body size (matches the old `MAX_BODY_BYTES`).
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Maximum concurrent in-flight requests (single-user local app; the old
/// server had no bound at all).
pub const MAX_CONCURRENT_REQUESTS: usize = 64;
/// Bound sockets waiting for headers as well as admitted HTTP work.
pub const MAX_CONCURRENT_CONNECTIONS: usize = 128;
pub const REQUEST_HEADER_TIMEOUT: Duration = Duration::from_secs(15);
/// Bound retained in-flight request buffers as well as the per-request limit.
pub const MAX_INFLIGHT_BODY_BYTES: usize = 128 * 1024 * 1024;
/// Reading a complete request body has a finite deadline.
pub const REQUEST_BODY_TIMEOUT: Duration = Duration::from_secs(15);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// One policy shared by HTTP and the desktop shell. IPC origins are fixed, no wildcard ports.
pub const WORKBENCH_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; frame-src 'none'; object-src 'none'; connect-src 'self' http://ipc.localhost ipc:; font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

const SHUTDOWN_POLL: Duration = Duration::from_millis(100);

/// Shared state handed to every request: the web root plus the in-process
/// stores the old `Server` struct owned, plus the M1-05 security material.
#[derive(Clone)]
pub struct AppState {
    pub web_root: Arc<PathBuf>,
    pub editor_store: Arc<EditorDraftStore>,
    pub solve_requests: Arc<SolveRequestStore>,
    /// Root that typed file-read paths resolve against (PD-D14 red line).
    pub trusted_root: Arc<PathBuf>,
    /// Set by the shell (Tauri) to stop the accept loop gracefully.
    pub shutdown: Arc<AtomicBool>,
    /// 256-bit session token; every `/api/*` request (except the bootstrap
    /// endpoint) must present it as `Authorization: Bearer <token>`.
    pub session_token: Arc<String>,
    /// The IP the listener is bound to, e.g. `127.0.0.1`.
    pub bound_host: String,
    /// The bound TCP port; the `Host`/`Origin` headers must match it.
    pub bound_port: u16,
    /// Admission applies after authentication; cancellation bypasses busy work slots.
    pub request_slots: Arc<tokio::sync::Semaphore>,
    pub body_bytes: Arc<tokio::sync::Semaphore>,
}

/// Serve the Axum router through the maintained Hyper HTTP/1 parser, with an
/// explicit header-read timer and bounded connection lifetimes.
pub async fn serve_router(
    listener: tokio::net::TcpListener,
    router: Router,
    shutdown: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let signal = shutdown_signal(Arc::clone(&shutdown));
    tokio::pin!(signal);
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_CONNECTIONS));
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=&mut signal=>break,
            accepted=listener.accept()=>{
                let (socket,_)=match accepted {Ok(accepted)=>accepted,Err(error)=>{shutdown.store(true,Ordering::Release);return Err(error);}};
                let Ok(slot)=Arc::clone(&slots).try_acquire_owned() else {drop(socket);continue;};
                let router=router.clone();let shutdown=Arc::clone(&shutdown);
                connections.spawn(async move {
                    let _slot=slot;
                    let _=serve_connection(socket,router,shutdown,REQUEST_HEADER_TIMEOUT).await;
                });
            },
            _=connections.join_next(),if !connections.is_empty()=>{},
        }
    }
    // The shutdown signal sets the shared flag. Request futures cancel their
    // computations, and each connection gets a finite final drain period.
    let drained = tokio::time::timeout(SHUTDOWN_GRACE, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    Ok(())
}

async fn serve_connection(
    socket: tokio::net::TcpStream,
    router: Router,
    shutdown: Arc<AtomicBool>,
    header_timeout: Duration,
) -> Result<(), hyper::Error> {
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .timer(hyper_util::rt::TokioTimer::new())
        .header_read_timeout(header_timeout);
    let connection = builder.serve_connection(
        hyper_util::rt::TokioIo::new(socket),
        hyper_util::service::TowerToHyperService::new(router),
    );
    tokio::pin!(connection);
    tokio::select! {
        result=&mut connection=>result,
        _=wait_shutdown(shutdown)=>{
            connection.as_mut().graceful_shutdown();
            match tokio::time::timeout(SHUTDOWN_GRACE,&mut connection).await {Ok(result)=>result,Err(_)=>Ok(())}
        },
    }
}

/// Host names accepted for the loopback `Host` header (DNS-rebinding guard).
/// `localhost` always resolves to loopback; attacker-controlled names never
/// match.
const ALLOWED_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

/// Build the axum router that adapts incoming requests into the legacy
/// [`Request`] shape and dispatches through [`route`].
pub fn build_router(state: AppState) -> Router {
    // The fallback catches every path: path/query parsing, static files and
    // the 404/405 fallbacks all live in `route`, which is fully tested.
    Router::new()
        .fallback(adapt_raw)
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

/// Adapt one axum request into the legacy dispatch and back, after the
/// M1-05 security checks: exact loopback `Host` (DNS rebinding), same-origin
/// `Origin` when present (CSRF), and the `Bearer` session token on `/api/*`.
///
/// Body reading begins only after the header guards and admission checks.
/// Size, aggregate memory, read-time and shutdown limits apply to streaming bodies.
async fn adapt_raw(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::extract::Request,
) -> AxumResponse {
    let target = reconstruct_path(&uri);
    // The dispatcher (`server::route`) trims every leading slash and drops
    // empty segments before matching, so a literal `//api/v1/...` used to
    // reach the API handlers while the raw-path `starts_with("/api/")` Bearer
    // check below saw a non-`/api/` prefix (P0 auth bypass). Normalize the
    // path once, up front, and use that same normalized shape for the auth
    // checks and the dispatch, so both always agree. Percent-encoding is left
    // untouched here: static lookups decode inside `safe_join`, and API route
    // matching never decodes.
    let (raw_path, query) = split_query(&target);
    let normalized_path = normalize_request_path(raw_path);

    // 1) DNS-rebinding guard: the Host header must be the loopback address we
    //    are bound to (name + port). A rebinding attack uses an
    //    attacker-controlled host name, which never matches.
    if !host_allowed(&state, headers.get(header::HOST)) {
        return into_axum(error_response(400, "invalid host"));
    }

    // 2) CSRF guard: browsers attach Origin to cross-origin requests. When
    //    present it must be our exact loopback origin; absent Origin (curl,
    //    CLI, older clients) is allowed.
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !origin_allowed(&state, origin) {
            return into_axum(error_response(403, "cross-origin request rejected"));
        }
    }

    // 3) The session bootstrap endpoint issues the token to any same-origin
    //    page (Host-checked above); it must not require the token itself.
    //    Only the canonical literal path bootstraps: malformed spellings such
    //    as `//api/v1/session` fall through to the Bearer gate below and are
    //    rejected with 401 instead of handing out a token.
    if method == Method::GET && target == "/api/v1/session" {
        return into_axum(session_response(&state));
    }

    // 4) Every other /api/* request must carry the Bearer session token. The
    //    check runs on the normalized path so duplicate-slash spellings can
    //    never slip past it.
    if normalized_path.starts_with("/api/")
        && !bearer_valid(&state, headers.get(header::AUTHORIZATION))
    {
        return into_axum(error_response(401, "session required"));
    }

    if state.shutdown.load(Ordering::Acquire) {
        return into_axum(error_response(503, "backend is shutting down"));
    }
    if let Some(job_id) = normalized_path
        .strip_prefix("/api/v1/jobs/")
        .and_then(|path| path.strip_suffix("/cancel"))
    {
        if method != Method::POST || !valid_job_id(job_id) {
            return into_axum(error_response(400, "invalid cancellation request"));
        }
        let key = format!("{}:{job_id}", state.session_token);
        let cancelled = jobs()
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(&key).cloned())
            .map(|control| {
                control.cancel();
                true
            })
            .unwrap_or(false);
        return into_axum(Response::json(200, json!({"cancelled":cancelled})));
    }
    let work_slot = match Arc::clone(&state.request_slots).try_acquire_owned() {
        Ok(slot) => slot,
        Err(_) => return into_axum(error_response(503, "backend request capacity reached")),
    };
    let declared_bytes = body
        .body()
        .size_hint()
        .upper()
        .and_then(|bytes| usize::try_from(bytes).ok())
        .unwrap_or(MAX_BODY_BYTES);
    if declared_bytes > MAX_BODY_BYTES {
        return into_axum(error_response(413, "request body exceeds limit"));
    }
    let body_slot =
        match Arc::clone(&state.body_bytes).try_acquire_many_owned(declared_bytes as u32) {
            Ok(slot) => slot,
            Err(_) => return into_axum(error_response(503, "backend body capacity reached")),
        };
    let job_key = headers
        .get("X-Request-Id")
        .and_then(|value| value.to_str().ok())
        .filter(|id| valid_job_id(id))
        .map(|id| format!("{}:{id}", state.session_token));
    let control = seattrellis_core::SolveControl::new();
    if let Some(key) = &job_key {
        let mut registry = match jobs().lock() {
            Ok(registry) => registry,
            Err(_) => return into_axum(error_response(500, "job registry unavailable")),
        };
        if registry.contains_key(key) {
            return into_axum(error_response(409, "request id already active"));
        }
        registry.insert(key.clone(), control.clone());
    }
    let created = Arc::new(Mutex::new(Vec::new()));
    let mut cleanup = RequestCleanup {
        control: control.clone(),
        created: Arc::clone(&created),
        editors: Arc::clone(&state.editor_store),
        sources: Arc::clone(&state.solve_requests),
        job_key,
        completed: false,
    };
    // Authentication and origin validation deliberately precede any body consumption.
    let body_read = tokio::select! {
        body=tokio::time::timeout(REQUEST_BODY_TIMEOUT, axum::body::to_bytes(body.into_body(), MAX_BODY_BYTES))=>body,
        _=wait_shutdown(Arc::clone(&state.shutdown))=>return into_axum(error_response(503,"backend is shutting down")),
        _=wait_cancelled(control.clone())=>return into_axum(error_response(408,"request was cancelled")),
    };
    let body = match body_read {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => {
            return into_axum(error_response(
                413,
                "request body exceeds limit or is incomplete",
            ))
        }
        Err(_) => return into_axum(error_response(408, "request body timed out")),
    };
    let legacy = Request {
        method: method.to_string(),
        path: match query {
            Some(query) => format!("{normalized_path}?{query}"),
            None => normalized_path,
        },
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        body: body.to_vec(),
    };
    let shutdown = Arc::clone(&state.shutdown);
    let (timeout_tx, timeout_rx) = tokio::sync::oneshot::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let _work_slot = work_slot;
        let _body_slot = body_slot;
        let timeout = timeout_for_request(&legacy);
        match timeout {
            Ok(timeout) => {
                let _ = timeout_tx.send(timeout);
            }
            Err(response) => return response,
        }
        seattrellis_application::with_request_resources(control, created, || {
            route(
                &legacy,
                &state.web_root,
                &state.editor_store,
                &state.solve_requests,
                &state.trusted_root,
            )
        })
    });
    let timeout = tokio::select! {
        timeout=tokio::time::timeout(REQUEST_TIMEOUT,timeout_rx)=>timeout.ok().and_then(Result::ok).unwrap_or(REQUEST_TIMEOUT),
        _=wait_shutdown(Arc::clone(&shutdown))=>return into_axum(error_response(503,"backend is shutting down")),
        _=wait_cancelled(cleanup.control.clone())=>return into_axum(error_response(408,"request was cancelled")),
    };
    let response = tokio::select! {
        response=tokio::time::timeout(timeout,worker)=>response,
        _=wait_shutdown(shutdown)=>return into_axum(error_response(503,"backend is shutting down")),
    };
    match response {
        Ok(Ok(response)) if !cleanup.control.is_cancelled() => {
            cleanup.completed = true;
            into_axum(response)
        }
        Ok(Ok(_)) => into_axum(error_response(408, "request was cancelled")),
        Ok(Err(_)) => into_axum(error_response(500, "dispatch failed")),
        Err(_) => into_axum(error_response(504, "request timed out")),
    }
}

/// Transport budgets follow the requested solve budget, including every sequential rotation period.
/// Non-solve operations retain the default deadline; JSON inspection runs on the blocking worker.
fn timeout_for_request(request: &Request) -> Result<Duration, Response> {
    if !matches!(
        request.path.as_str(),
        "/api/v2/solve" | "/api/v1/solve" | "/api/v1/classes/generate" | "/api/v1/classes/rotation"
    ) {
        return Ok(REQUEST_TIMEOUT);
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
        return Ok(REQUEST_TIMEOUT);
    };
    let limit = value
        .pointer("/options/time_limit_seconds")
        .or_else(|| value.get("time_limit_seconds"))
        .and_then(serde_json::Value::as_f64);
    let Some(limit) = limit.filter(|limit| limit.is_finite() && *limit >= 0.0) else {
        return Ok(REQUEST_TIMEOUT);
    };
    let periods = if request.path == "/api/v1/classes/rotation" {
        value
            .get("period_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(4)
            .clamp(1, 20)
    } else {
        1
    };
    let seconds = limit * periods as f64 + 15.0;
    if seconds > 86_400.0 {
        return Err(error_response(
            422,
            "requested solve budget exceeds the one-day transport limit",
        ));
    }
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| error_response(422, "requested solve budget is out of range"))
}

fn valid_job_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn jobs() -> &'static Mutex<HashMap<String, seattrellis_core::SolveControl>> {
    static JOBS: OnceLock<Mutex<HashMap<String, seattrellis_core::SolveControl>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}
struct RequestCleanup {
    control: seattrellis_core::SolveControl,
    created: Arc<Mutex<Vec<String>>>,
    editors: Arc<EditorDraftStore>,
    sources: Arc<SolveRequestStore>,
    job_key: Option<String>,
    completed: bool,
}
impl Drop for RequestCleanup {
    fn drop(&mut self) {
        if !self.completed {
            self.control.cancel();
            if let (Ok(created), Ok(mut editors), Ok(mut sources)) = (
                self.created.lock(),
                self.editors.lock(),
                self.sources.lock(),
            ) {
                for id in created.iter() {
                    editors.remove(id);
                    sources.remove(id);
                }
            }
        }
        if let Some(key) = &self.job_key {
            if let Ok(mut jobs) = jobs().lock() {
                jobs.remove(key);
            }
        }
    }
}
async fn wait_cancelled(control: seattrellis_core::SolveControl) {
    let mut poll = tokio::time::interval(Duration::from_millis(50));
    loop {
        poll.tick().await;
        if control.is_cancelled() {
            return;
        }
    }
}
async fn wait_shutdown(flag: Arc<AtomicBool>) {
    let mut poll = tokio::time::interval(SHUTDOWN_POLL);
    loop {
        poll.tick().await;
        if flag.load(Ordering::Acquire) {
            return;
        }
    }
}

/// Split a request target into its path and optional query string. The query
/// is kept verbatim for the dispatcher, which splits it itself.
fn split_query(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

/// Collapse leading, trailing, and duplicated slashes so the authorization
/// checks see exactly the path shape [`crate::server::route`] will match on
/// (its `path_segments` trims leading slashes and drops empty segments).
/// Everything else is preserved verbatim — no percent-decoding, and `.`
/// segments stay in place so static-file behavior is unchanged.
fn normalize_request_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len() + 1);
    normalized.push('/');
    let mut first = true;
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        if !first {
            normalized.push('/');
        }
        normalized.push_str(segment);
        first = false;
    }
    normalized
}

/// `Host` header check: the host name must be loopback and the port must
/// match the bound port.
fn host_allowed(state: &AppState, host: Option<&HeaderValue>) -> bool {
    let Some(host) = host.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let (name, port) = split_host_port(host);
    if !ALLOWED_HOSTS.contains(&name) {
        return false;
    }
    port == Some(state.bound_port)
}

/// Split `host[:port]` (handling `[::1]:port` brackets).
fn split_host_port(host: &str) -> (&str, Option<u16>) {
    if let Some(rest) = host.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let name = &rest[..end];
            let port = rest[end + 1..]
                .strip_prefix(':')
                .and_then(|value| value.parse::<u16>().ok());
            return (name, port);
        }
    }
    match host.rsplit_once(':') {
        Some((name, port)) => (name, port.parse::<u16>().ok()),
        None => (host, None),
    }
}

/// `Origin` check: must be `http://{allowed host}:{bound port}`.
fn origin_allowed(state: &AppState, origin: &HeaderValue) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some(rest) = origin.strip_prefix("http://") else {
        return false; // https and others are never our origin
    };
    if rest.contains(['/', '?', '#', '@']) {
        return false;
    }
    let (name, port) = split_host_port(rest);
    ALLOWED_HOSTS.contains(&name) && port == Some(state.bound_port)
}

/// `Authorization: Bearer <token>` check with a constant-time comparison.
fn bearer_valid(state: &AppState, authorization: Option<&HeaderValue>) -> bool {
    let Some(authorization) = authorization.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Some(token) = authorization.strip_prefix("Bearer ") else {
        return false;
    };
    constant_time_eq(token.as_bytes(), state.session_token.as_bytes())
}

/// Constant-time byte comparison (no early exit on the first mismatch).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// The bootstrap response: the session token, plain JSON, no auth needed.
fn session_response(state: &AppState) -> Response {
    Response::json(
        200,
        json!({
            "api_version": 1,
            "session_token": state.session_token.as_str(),
        }),
    )
}

/// Coarse JSON error with a stable shape (never leaks internals).
fn error_response(status: u16, message: &str) -> Response {
    Response::json(status, json!({ "error": message }))
}

/// The legacy dispatcher splits `path?query` itself, so hand it the original
/// request target verbatim.
fn reconstruct_path(uri: &Uri) -> String {
    match uri.path_and_query() {
        Some(target) => target.as_str().to_string(),
        None => uri.path().to_string(),
    }
}

/// Convert the legacy [`Response`] into an axum response with the same
/// security headers the old writer emitted (`nosniff`, `no-store`). Hyper now
/// manages keep-alive, so no `Connection: close` is sent.
fn into_axum(response: Response) -> AxumResponse {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = AxumResponse::builder()
        .status(status)
        .header(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        )
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
        // M1-05: CSP locks the workbench to same-origin resources (inline
        // styles are React's style attributes), X-Frame-Options blocks
        // embedding, Referrer-Policy stops token-URL leakage.
        .header(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(WORKBENCH_CSP),
        )
        .header(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"))
        .header(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
    if let Some(content_type) = response.content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if !response.export_warnings.is_empty() {
        // Percent-encoded JSON keeps Unicode/newlines out of HTTP header
        // syntax. Warnings never contain roster contents.
        let json = serde_json::to_vec(&response.export_warnings).unwrap_or_default();
        let encoded: String = json.iter().map(|byte| format!("%{byte:02X}")).collect();
        if let Ok(value) = HeaderValue::from_str(&encoded) {
            builder = builder.header("X-Export-Warnings", value);
        }
    }
    if let Some(disposition) = response.content_disposition {
        if let Ok(value) = HeaderValue::from_str(&disposition) {
            builder = builder.header(header::CONTENT_DISPOSITION, value);
        }
    }
    builder
        .body(Body::from(response.body))
        .expect("response construction cannot fail")
}

/// Resolve when the process should stop accepting requests: an OS signal
/// (Ctrl-C / SIGTERM) or the shell setting the shutdown flag (Tauri exit).
pub async fn shutdown_signal(flag: Arc<AtomicBool>) {
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _=tokio::signal::ctrl_c()=>{},
        _=terminate=>{},
        _=wait_shutdown(Arc::clone(&flag))=>{},
    }
    flag.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as AxumRequest;
    use std::collections::HashMap;
    use std::sync::Mutex;

    async fn adapt(
        state: State<AppState>,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: axum::body::Bytes,
    ) -> AxumResponse {
        let raw = axum::http::Request::new(Body::from(body));
        adapt_raw(state, method, uri, headers, raw).await
    }

    fn test_state() -> AppState {
        AppState {
            web_root: Arc::new(PathBuf::from("/nonexistent")),
            editor_store: Arc::new(seattrellis_domain::editing::new_draft_store()),
            solve_requests: Arc::new(Mutex::new(HashMap::new())),
            trusted_root: Arc::new(PathBuf::from("/nonexistent")),
            shutdown: Arc::new(AtomicBool::new(false)),
            session_token: Arc::new("0123456789abcdef0123456789abcdef".to_string()),
            bound_host: "127.0.0.1".to_string(),
            bound_port: 8765,
            request_slots: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            body_bytes: Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_BODY_BYTES)),
        }
    }

    /// The adapter must dispatch a legacy-shaped request through `route`
    /// unchanged: a plain solve round-trip through the axum layer.
    #[tokio::test]
    async fn adapter_dispatches_through_legacy_route() {
        let problem = serde_json::json!({
            "api_version": 2,
            "student_count": 2,
            "seat_positions": [[1.0, 1.0], [1.0, 2.0]],
            "seed": 0
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let axum_request = AxumRequest::builder()
            .method("POST")
            .uri("/api/v1/solve")
            .header(header::HOST, "127.0.0.1:8765")
            .header(
                header::AUTHORIZATION,
                "Bearer 0123456789abcdef0123456789abcdef",
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();
        let (parts, body) = axum_request.into_parts();
        let response = adapt(
            State(test_state()),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, MAX_BODY_BYTES).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value["recommended_candidate_id"].is_string());
        assert!(value["candidates"].is_array());
    }

    /// Oversized bodies must be rejected with 413 by the limit layer.
    #[tokio::test]
    async fn oversized_body_is_413() {
        use tower::ServiceExt;
        let router = build_router(test_state());
        let oversized = vec![0u8; MAX_BODY_BYTES + 1];
        let response = router
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/v1/solve")
                    .header(header::HOST, "127.0.0.1:8765")
                    .header(
                        header::AUTHORIZATION,
                        "Bearer 0123456789abcdef0123456789abcdef",
                    )
                    .header(header::CONTENT_LENGTH, oversized.len().to_string())
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn response_conversion_preserves_headers_and_status() {
        let legacy = Response::json(409, serde_json::json!({"error": "plan_not_found"}));
        let axum = into_axum(legacy);
        assert_eq!(axum.status(), StatusCode::CONFLICT);
        assert_eq!(
            axum.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        assert_eq!(
            axum.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }

    /// Setting the shutdown flag must resolve the shutdown future.
    #[tokio::test]
    async fn shutdown_flag_stops_the_accept_loop() {
        let flag = Arc::new(AtomicBool::new(false));
        let handle = tokio::spawn(shutdown_signal(flag.clone()));
        flag.store(true, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("shutdown signal must resolve")
            .unwrap();
    }

    // ------------------------------------------------------------------
    // M1-05 threat-model tests: DNS rebinding / CSRF / session / headers
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn session_bootstrap_issues_token_without_bearer() {
        let state = test_state();
        let response = adapt(
            State(state.clone()),
            Method::GET,
            Uri::from_static("/api/v1/session"),
            HeaderMap::from_iter([(header::HOST, "127.0.0.1:8765".parse().unwrap())]),
            axum::body::Bytes::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["session_token"], "0123456789abcdef0123456789abcdef");
    }

    #[tokio::test]
    async fn wrong_host_is_rejected() {
        // DNS-rebinding style: attacker-controlled host name on the request.
        let state = test_state();
        let request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/session")
            .header(header::HOST, "evil.example.com:8765")
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn wrong_port_on_host_is_rejected() {
        let state = test_state();
        let request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/session")
            .header(header::HOST, "127.0.0.1:80")
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn cross_origin_post_is_rejected() {
        let state = test_state();
        let request = AxumRequest::builder()
            .method("POST")
            .uri("/api/v1/solve")
            .header(header::HOST, "127.0.0.1:8765")
            .header(header::ORIGIN, "https://evil.example.com")
            .header(
                header::AUTHORIZATION,
                "Bearer 0123456789abcdef0123456789abcdef",
            )
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn missing_bearer_is_401() {
        let state = test_state();
        let request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:8765")
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_bearer_is_401() {
        let state = test_state();
        let request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:8765")
            .header(
                header::AUTHORIZATION,
                "Bearer deadbeefdeadbeefdeadbeefdeadbeef",
            )
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_bearer_reaches_health() {
        let state = test_state();
        let request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:8765")
            .header(
                header::AUTHORIZATION,
                "Bearer 0123456789abcdef0123456789abcdef",
            )
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn static_assets_do_not_require_bearer() {
        // The workbench page itself is public; the API is what carries data.
        let state = test_state();
        let response = adapt(
            State(state),
            Method::GET,
            Uri::from_static("/"),
            HeaderMap::from_iter([(header::HOST, "127.0.0.1:8765".parse().unwrap())]),
            axum::body::Bytes::new(),
        )
        .await;
        // The legacy index handler serves the embedded workbench (200); the
        // point is that no bearer was demanded for public assets.
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn responses_carry_csp_frame_and_referrer_headers() {
        let state = test_state();
        let request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:8765")
            .header(
                header::AUTHORIZATION,
                "Bearer 0123456789abcdef0123456789abcdef",
            )
            .body(Body::empty())
            .unwrap();
        let (parts, body) = request.into_parts();
        let response = adapt(
            State(state),
            parts.method,
            parts.uri,
            parts.headers,
            axum::body::to_bytes(body, 1024).await.unwrap(),
        )
        .await;
        assert!(response
            .headers()
            .get(header::CONTENT_SECURITY_POLICY)
            .is_some());
        assert_eq!(
            response.headers().get(header::X_FRAME_OPTIONS).unwrap(),
            "DENY"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
    }

    #[test]
    fn split_host_port_handles_brackets_and_bare_hosts() {
        assert_eq!(split_host_port("127.0.0.1:8765"), ("127.0.0.1", Some(8765)));
        assert_eq!(split_host_port("[::1]:8765"), ("::1", Some(8765)));
        assert_eq!(split_host_port("localhost"), ("localhost", None));
        assert_eq!(split_host_port("evil.com:80"), ("evil.com", Some(80)));
    }

    #[test]
    fn normalize_request_path_collapses_only_slashes() {
        assert_eq!(
            normalize_request_path("/api/v1/health"),
            "/api/v1/health",
            "canonical paths are unchanged"
        );
        assert_eq!(normalize_request_path("//api/v1/health"), "/api/v1/health");
        assert_eq!(
            normalize_request_path("///api/v1/session"),
            "/api/v1/session"
        );
        assert_eq!(
            normalize_request_path("/a//b///c"),
            "/a/b/c",
            "duplicate inner slashes collapse"
        );
        // Non-slash segments stay verbatim: no percent-decoding, no `.`-segment
        // removal (static lookups keep their own normalization).
        assert_eq!(
            normalize_request_path("/./api/v1/health"),
            "/./api/v1/health"
        );
        assert_eq!(normalize_request_path("/%2e%2e/api"), "/%2e%2e/api");
        assert_eq!(normalize_request_path("/"), "/");
        assert_eq!(normalize_request_path(""), "/");
    }

    /// Drive one request through [`adapt`] with loopback Host headers and an
    /// optional Authorization header.
    async fn adapt_with_token(
        state: &AppState,
        method: Method,
        uri: Uri,
        authorization: Option<&str>,
    ) -> AxumResponse {
        let mut headers =
            HeaderMap::from_iter([(header::HOST, HeaderValue::from_static("127.0.0.1:8765"))]);
        if let Some(authorization) = authorization {
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(authorization).unwrap(),
            );
        }
        adapt(
            State(state.clone()),
            method,
            uri,
            headers,
            axum::body::Bytes::new(),
        )
        .await
    }

    // ------------------------------------------------------------------
    // Path-normalization regressions (P0): non-canonical `/api` spellings
    // used to slip past the Bearer gate while `path_segments` still routed
    // them into the API handlers.
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn double_slashed_api_health_requires_bearer() {
        let response = adapt_with_token(
            &test_state(),
            Method::GET,
            Uri::from_static("//api/v1/health"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn double_slashed_file_read_requires_bearer() {
        let response = adapt_with_token(
            &test_state(),
            Method::POST,
            Uri::from_static("//api/v1/files/read"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dot_segment_api_path_stays_unmatched_and_public() {
        // Locked behavior: `/./api/...` never matched an API route (the dot
        // segment survives normalization and falls through to the static
        // lookup, which drops it and finds no file). It must stay a 404 —
        // with or without a token it must never return handler output.
        let response = adapt_with_token(
            &test_state(),
            Method::GET,
            Uri::from_static("/./api/v1/health"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = adapt_with_token(
            &test_state(),
            Method::GET,
            Uri::from_static("/./api/v1/health"),
            Some("Bearer 0123456789abcdef0123456789abcdef"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn triple_slashed_session_endpoint_never_issues_a_token() {
        // Only the canonical literal path bootstraps a session; the malformed
        // spelling falls through to the Bearer gate and is rejected.
        let response = adapt_with_token(
            &test_state(),
            Method::GET,
            Uri::from_static("///api/v1/session"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn percent_encoded_dot_dot_variants_never_reach_handlers() {
        for uri in [
            "/%2e%2e/api/v1/health",
            "//%2e%2e/api/v1/health",
            "/api/%2e%2e/v1/health",
            "/%2E%2E/api/v1/files/root",
        ] {
            let response =
                adapt_with_token(&test_state(), Method::GET, uri.parse().unwrap(), None).await;
            let status = response.status();
            assert!(
                status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND,
                "{uri} must be 401/404 without a token, got {status}"
            );
            assert_ne!(status, StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn double_slashed_api_path_with_valid_bearer_reaches_handler() {
        // Normalization feeds the dispatcher too, so a well-authenticated
        // request with sloppy slashes routes exactly like the canonical form.
        let response = adapt_with_token(
            &test_state(),
            Method::GET,
            Uri::from_static("//api/v1/health"),
            Some("Bearer 0123456789abcdef0123456789abcdef"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // ------------------------------------------------------------------
    // Blocking-pool offload: a slow solve must not stall the async worker.
    // ------------------------------------------------------------------

    /// A problem whose search cannot finish before its wall-clock budget:
    /// a full 60-student class with rich cost data and dense min-distance
    /// rules exhausts any short deadline and reports `Timeout` (verified:
    /// the run lasts ~exactly `budget_seconds`, release and debug builds).
    fn slow_solve_body(budget_seconds: f64) -> Vec<u8> {
        let student_count = 60usize;
        let students: Vec<serde_json::Value> = (0..student_count)
            .map(|index| {
                serde_json::json!({
                    "key": format!("S{index}"),
                    "display_name": format!("Student {index}"),
                    "score": 60.0 + (index * 7 % 40) as f64,
                    "height_cm": 150 + (index * 3 % 30),
                })
            })
            .collect();
        let min_distance: Vec<serde_json::Value> = (0..student_count)
            .flat_map(|first| {
                ((first + 1)..student_count)
                    .filter(move |second| (first + second) % 5 == 0)
                    .map(move |second| {
                        serde_json::json!({
                            "students": [first, second],
                            "distance": 2.0,
                            "metric": "euclidean",
                        })
                    })
            })
            .collect();
        let seat_positions: Vec<[f64; 2]> = (0..student_count)
            .map(|index| [(index % 10) as f64, (index / 10) as f64])
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "api_version": 2,
            "student_count": student_count,
            "seat_positions": seat_positions,
            "students": students,
            "min_distance": min_distance,
            "seed": 42,
            "time_limit_seconds": budget_seconds,
        }))
        .unwrap()
    }

    /// On a current-thread runtime, a multi-second solve dispatched inline
    /// would starve every other request. With the dispatch parked on
    /// `spawn_blocking`, a concurrent health probe answers while the solve is
    /// still running. The solve's wall-clock budget makes the slow side
    /// deterministic; the probe must finish well inside it.
    #[tokio::test(flavor = "current_thread")]
    async fn slow_solve_does_not_block_concurrent_health() {
        use tower::ServiceExt;

        let router = build_router(test_state());
        let started = std::time::Instant::now();

        let solve_router = router.clone();
        let solve_task = tokio::spawn(async move {
            let request = AxumRequest::builder()
                .method("POST")
                .uri("/api/v2/solve")
                .header(header::HOST, "127.0.0.1:8765")
                .header(
                    header::AUTHORIZATION,
                    "Bearer 0123456789abcdef0123456789abcdef",
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(slow_solve_body(1.0)))
                .unwrap();
            let response = solve_router.oneshot(request).await.unwrap();
            (response.status(), started.elapsed())
        });

        // Let the solve task reach its blocking dispatch before probing.
        tokio::time::sleep(Duration::from_millis(100)).await;

        let health_request = AxumRequest::builder()
            .method("GET")
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:8765")
            .header(
                header::AUTHORIZATION,
                "Bearer 0123456789abcdef0123456789abcdef",
            )
            .body(Body::empty())
            .unwrap();
        let health_started = std::time::Instant::now();
        let health_response = router.oneshot(health_request).await.unwrap();
        let health_elapsed = health_started.elapsed();
        assert_eq!(health_response.status(), StatusCode::OK);

        // The probe answered while the solve was still burning its budget.
        assert!(
            health_elapsed < Duration::from_millis(800),
            "health took {health_elapsed:?}; the solve blocked the worker"
        );

        let (solve_status, solve_elapsed) = solve_task.await.unwrap();
        assert_eq!(solve_status, StatusCode::OK);
        assert!(
            solve_elapsed >= Duration::from_millis(900),
            "the solve finished in {solve_elapsed:?}; the fixture no longer \
             exercises a slow dispatch"
        );
    }
    #[test]
    fn transport_deadlines_honor_configured_single_and_rotation_budgets() {
        let make = |path: &str, body: serde_json::Value| Request {
            method: "POST".into(),
            path: path.into(),
            content_type: None,
            body: serde_json::to_vec(&body).ok().unwrap(),
        };
        assert_eq!(
            timeout_for_request(&make(
                "/api/v1/classes/generate",
                json!({"options":{"time_limit_seconds":300,"candidate_count":20}})
            ))
            .ok()
            .unwrap(),
            Duration::from_secs(315)
        );
        assert_eq!(
            timeout_for_request(&make(
                "/api/v1/classes/rotation",
                json!({"options":{"time_limit_seconds":300},"period_count":4})
            ))
            .ok()
            .unwrap(),
            Duration::from_secs(1215)
        );
        assert_eq!(
            timeout_for_request(&make("/api/v1/classes/generate", json!({})))
                .ok()
                .unwrap(),
            REQUEST_TIMEOUT
        );
    }
    #[tokio::test]
    async fn explicit_cancellation_stops_work_and_leaves_no_draft_context() {
        use tower::ServiceExt;
        let state = test_state();
        let sources = Arc::clone(&state.solve_requests);
        let editors = Arc::clone(&state.editor_store);
        let slots = Arc::clone(&state.request_slots);
        let router = build_router(state);
        let solve_router = router.clone();
        let solve = tokio::spawn(async move {
            solve_router
                .oneshot(
                    AxumRequest::builder()
                        .method("POST")
                        .uri("/api/v1/classes/generate")
                        .header(header::HOST, "127.0.0.1:8765")
                        .header(
                            header::AUTHORIZATION,
                            "Bearer 0123456789abcdef0123456789abcdef",
                        )
                        .header("X-Request-Id", "cancel-regression")
                        .body(Body::from(slow_solve_body(20.0)))
                        .unwrap(),
                )
                .await
                .unwrap()
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !jobs()
            .lock()
            .unwrap()
            .contains_key("0123456789abcdef0123456789abcdef:cancel-regression")
        {
            assert!(std::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Cancellation must remain available even while every work slot is occupied.
        let remaining = slots.available_permits() as u32;
        let occupied = Arc::clone(&slots)
            .try_acquire_many_owned(remaining)
            .unwrap();
        let cancelled = router
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/v1/jobs/cancel-regression/cancel")
                    .header(header::HOST, "127.0.0.1:8765")
                    .header(
                        header::AUTHORIZATION,
                        "Bearer 0123456789abcdef0123456789abcdef",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cancelled.status(), StatusCode::OK);
        let response = tokio::time::timeout(Duration::from_secs(2), solve)
            .await
            .expect("cancelled solve must terminate promptly")
            .unwrap();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert!(editors.lock().unwrap().is_empty());
        assert!(sources.lock().unwrap().is_empty());
        drop(occupied);
        assert!(!jobs()
            .lock()
            .unwrap()
            .contains_key("0123456789abcdef0123456789abcdef:cancel-regression"));
    }
    #[tokio::test]
    async fn maintained_http_parser_times_out_an_incomplete_header() {
        use std::io::{Read, Write};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut client = std::net::TcpStream::connect(address).unwrap();
        client
            .write_all(b"POST /api/v2/solve HTTP/1.1\r\nHost: ")
            .unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            serve_connection(
                socket,
                build_router(test_state()),
                Arc::new(AtomicBool::new(false)),
                Duration::from_millis(100),
            ),
        )
        .await
        .expect("header deadline must close incomplete requests");
        assert!(result.unwrap_err().is_timeout());
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut bytes = [0u8; 1024];
        let read = client.read(&mut bytes).unwrap();
        assert!(read == 0 || String::from_utf8_lossy(&bytes[..read]).contains("408"));
    }
}
