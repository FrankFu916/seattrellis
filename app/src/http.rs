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
use tower::limit::ConcurrencyLimitLayer;

use crate::editing::EditorDraftStore;
use crate::server::{route, Request, Response, SolveRequestStore};

/// Maximum accepted request body size (matches the old `MAX_BODY_BYTES`).
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Maximum concurrent in-flight requests (single-user local app; the old
/// server had no bound at all).
pub const MAX_CONCURRENT_REQUESTS: usize = 64;
/// How often the graceful-shutdown future polls the exit flag.
const SHUTDOWN_POLL: Duration = Duration::from_millis(100);

/// Shared state handed to every request: the web root plus the in-process
/// stores the old `Server` struct owned.
#[derive(Clone)]
pub struct AppState {
    pub web_root: Arc<PathBuf>,
    pub editor_store: Arc<EditorDraftStore>,
    pub solve_requests: Arc<SolveRequestStore>,
    /// Set by the shell (Tauri) to stop the accept loop gracefully.
    pub shutdown: Arc<AtomicBool>,
}

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

/// Adapt one axum request into the legacy dispatch and back.
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
    let legacy = Request {
        method: method.to_string(),
        path: reconstruct_path(&uri),
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        body: body.to_vec(),
    };
    let response = route(
        &legacy,
        &state.web_root,
        &state.editor_store,
        &state.solve_requests,
    );
    into_axum(response)
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
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
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
            editor_store: Arc::new(crate::editing::new_draft_store()),
            solve_requests: Arc::new(Mutex::new(HashMap::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
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
}
