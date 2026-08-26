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
//! - Concurrency is bounded by a tower limit layer (the old server spawned an
//!   unbounded thread per connection).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response as AxumResponse;
use axum::Router;
use serde_json::json;
use tower::limit::ConcurrencyLimitLayer;

use crate::server::{route, Request, Response, SolveRequestStore};
use seattrellis_domain::editing::EditorDraftStore;

/// Maximum accepted request body size (matches the old `MAX_BODY_BYTES`).
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Maximum concurrent in-flight requests (single-user local app; the old
/// server had no bound at all).
pub const MAX_CONCURRENT_REQUESTS: usize = 64;
/// How often the graceful-shutdown future polls the exit flag.
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
        .fallback(adapt)
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(ConcurrencyLimitLayer::new(MAX_CONCURRENT_REQUESTS))
}

/// Adapt one axum request into the legacy dispatch and back, after the
/// M1-05 security checks: exact loopback `Host` (DNS rebinding), same-origin
/// `Origin` when present (CSRF), and the `Bearer` session token on `/api/*`.
///
/// Typed extractors keep the body read under `DefaultBodyLimit` (oversized
/// bodies are rejected with 413 by the extractor, matching the plan's
/// "oversized body rejected" exit gate).
async fn adapt(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
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

    // 5) Legacy dispatch (static assets and /api/* both flow through here),
    //    handed the same normalized path the checks above saw. Solves can run
    //    for seconds, so the synchronous dispatch moves onto tokio's blocking
    //    pool instead of stalling an async worker; request/response are owned
    //    data, which makes the closure `Send + 'static`.
    let legacy = Request {
        method: method.to_string(),
        path: match query {
            Some(query) => format!("{normalized_path}?{query}"),
            None => normalized_path,
        },
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        body: body.to_vec(),
    };
    let response = tokio::task::spawn_blocking(move || {
        route(
            &legacy,
            &state.web_root,
            &state.editor_store,
            &state.solve_requests,
            &state.trusted_root,
        )
    })
    .await;
    match response {
        Ok(response) => into_axum(response),
        // The dispatch itself cannot fail; only task panics or shutdown land
        // here. Stay coarse and never leak internals.
        Err(_) => into_axum(error_response(500, "dispatch failed")),
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
    let (host_port, _path) = rest.split_once('/').unwrap_or((rest, ""));
    let (name, port) = split_host_port(host_port);
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
        .header(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"))
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
        // M1-05: CSP locks the workbench to same-origin resources (inline
        // styles are React's style attributes), X-Frame-Options blocks
        // embedding, Referrer-Policy stops token-URL leakage.
        .header(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';                  img-src 'self' data:; connect-src 'self'; font-src 'self';                  frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            ),
        )
        .header(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"))
        .header(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    if let Some(content_type) = response.content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
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
    let mut poll = tokio::time::interval(SHUTDOWN_POLL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return,
            _ = poll.tick() => {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as AxumRequest;
    use std::collections::HashMap;
    use std::sync::Mutex;

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
}
