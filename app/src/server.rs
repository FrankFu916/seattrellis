//! Loopback-only HTTP backend for the SeatTrellis desktop app.
//!
//! Serves the compiled React workbench (`web_static/`) and exposes the native
//! solve endpoint. The server is deliberately dependency-free beyond
//! `seattrellis_core` and `serde_json`: a hand-rolled, minimal HTTP/1.1
//! server keeps the release binary small and the surface auditable.
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
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::json;

/// Status-code -> reason text table for the responses we emit.
const STATUS_TEXT: &[(u16, &str)] = &[
    (100, "Continue"),
    (200, "OK"),
    (400, "Bad Request"),
    (404, "Not Found"),
    (405, "Method Not Allowed"),
    (411, "Length Required"),
    (413, "Payload Too Large"),
    (500, "Internal Server Error"),
    (501, "Not Implemented"),
];

/// Maximum size of the request head (request line + headers), in bytes.
const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Maximum accepted solve request body size, in bytes (64 MiB).
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Idle read/write timeout per connection.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);

/// Compiled React workbench location resolved at build time. Used as a
/// fallback so the binary serves assets regardless of the launch directory.
const BUILTIN_WEB_STATIC: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../src/seattrellis/web_static");

/// Native solve endpoint (matches the Python `API_PREFIX` convention).
const SOLVE_ENDPOINT: &str = "/api/v1/classes/generate";
/// Short alias accepted for convenience.
const SOLVE_ENDPOINT_ALIAS: &str = "/api/v1/solve";

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

/// The running backend: a bound loopback listener plus the web root to serve.
pub struct Server {
    listener: TcpListener,
    local_addr: SocketAddr,
    web_root: Arc<PathBuf>,
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
                    let _ = thread::Builder::new()
                        .name("seattrellis-conn".to_string())
                        .spawn(move || handle_connection(stream, web_root));
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
fn handle_connection(stream: TcpStream, web_root: Arc<PathBuf>) {
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

    let response = route(&request, &web_root);
    let _ = write_response(&mut write_stream, &response);
}

/// A parsed HTTP/1.1 request (head + body). The head's `Content-Length` /
/// `Expect` / `Transfer-Encoding` headers are consumed during parsing.
struct Request {
    method: String,
    path: String,
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
        body,
    }))
}

fn trim_line(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

// ---------------------------------------------------------------------------
// Routing and handlers
// ---------------------------------------------------------------------------

/// A minimal response: status code, optional content type, raw body.
struct Response {
    status: u16,
    content_type: Option<&'static str>,
    body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: serde_json::Value) -> Response {
        let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
        Response {
            status,
            content_type: Some("application/json; charset=utf-8"),
            body,
        }
    }

    fn text(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Response {
        Response {
            status,
            content_type: Some(content_type),
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

/// Dispatch a parsed request to the matching handler.
fn route(request: &Request, web_root: &Path) -> Response {
    // Ignore any query string when routing.
    let path = match request.path.split_once('?') {
        Some((path, _)) => path,
        None => &request.path,
    };

    match (request.method.as_str(), path) {
        ("GET", "/api/v1/health") => health_response(),
        ("POST", SOLVE_ENDPOINT) | ("POST", SOLVE_ENDPOINT_ALIAS) => solve_response(&request.body),
        ("GET", "/") | ("GET", "/index.html") => index_response(web_root),
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

/// `POST /api/v1/classes/generate` (and `/api/v1/solve`): run the native
/// cost-ranked greedy solver over the request body.
fn solve_response(body: &[u8]) -> Response {
    if body.is_empty() {
        return json_error(400, "empty request body");
    }
    let body_str = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return json_error(400, "request body is not valid UTF-8"),
    };

    match seattrellis_core::solve_problem_json(body_str) {
        Ok(json) => Response::text(200, "application/json; charset=utf-8", json),
        Err(message) => {
            // Malformed request bodies surface serde internals; map those to a
            // coarse message so we never leak schema/parser details. Domain
            // messages (capacity, unsupported api_version, ...) are fine to
            // return verbatim.
            let detail = if message.starts_with("invalid native solve request:") {
                "request body is not a valid solve problem".to_string()
            } else {
                message
            };
            json_error(400, &detail)
        }
    }
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
    head.push_str("X-Content-Type-Options: nosniff\r\n");
    head.push_str("Cache-Control: no-store\r\n");
    head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    head.push_str("\r\n");

    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
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

    fn request(method: &str, path: &str, body: &[u8]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            body: body.to_vec(),
        }
    }

    fn body_json(response: &Response) -> serde_json::Value {
        serde_json::from_slice(&response.body).unwrap()
    }

    #[test]
    fn health_route_returns_expected_shape() {
        let root = test_web_root();
        let response = route(&request("GET", "/api/v1/health", b""), &root);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("application/json; charset=utf-8"));
        let value = body_json(&response);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["service"], "seattrellis");
        assert_eq!(value["api_version"], "1");
    }

    #[test]
    fn index_route_serves_workbench() {
        let root = test_web_root();
        let response = route(&request("GET", "/", b""), &root);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("text/html; charset=utf-8"));
        assert_eq!(response.body, b"<html>test workbench</html>");
    }

    #[test]
    fn static_asset_route_serves_file() {
        let root = test_web_root();
        let response = route(&request("GET", "/assets/app.js", b""), &root);
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, Some("text/javascript; charset=utf-8"));
        assert_eq!(response.body, b"console.log('hi');");
    }

    #[test]
    fn dotdot_traversal_is_rejected() {
        let root = test_web_root();
        let response = route(&request("GET", "/../etc/passwd", b""), &root);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn percent_encoded_traversal_is_rejected() {
        let root = test_web_root();
        let response = route(&request("GET", "/%2e%2e/secret", b""), &root);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn unknown_static_file_is_404() {
        let root = test_web_root();
        let response = route(&request("GET", "/does-not-exist.js", b""), &root);
        assert_eq!(response.status, 404);
    }

    #[test]
    fn solve_feasible_returns_assignment() {
        let root = test_web_root();
        let problem = json!({
            "api_version": 2,
            "student_count": 5,
            "seat_positions": [[1.0,1.0],[2.0,1.0],[3.0,1.0],[4.0,1.0],[5.0,1.0],[6.0,1.0],[7.0,1.0],[8.0,1.0],[9.0,1.0]]
        });
        let body = serde_json::to_vec(&problem).unwrap();
        let response = route(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 200, "body: {}", String::from_utf8_lossy(&response.body));
        let value = body_json(&response);
        assert_eq!(value["feasible"], true);
        assert_eq!(value["assignment"].as_array().map(Vec::len), Some(5));
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
        let response = route(&request("POST", "/api/v1/solve", &body), &root);
        assert_eq!(response.status, 200);
        assert_eq!(body_json(&response)["feasible"], true);
    }

    #[test]
    fn solve_constraint_infeasible_returns_feasible_false() {
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
        let response = route(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 200, "body: {}", String::from_utf8_lossy(&response.body));
        let value = body_json(&response);
        assert_eq!(value["feasible"], false);
        assert_eq!(value["assignment"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn solve_invalid_json_is_400() {
        let root = test_web_root();
        let response = route(&request("POST", "/api/v1/classes/generate", b"not json at all"), &root);
        assert_eq!(response.status, 400);
        assert_eq!(body_json(&response)["error"].as_str().is_some(), true);
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
        let response = route(&request("POST", "/api/v1/classes/generate", &body), &root);
        assert_eq!(response.status, 400);
        assert!(body_json(&response)["error"].as_str().unwrap().contains("cannot seat more students"));
    }

    #[test]
    fn method_not_allowed_on_api() {
        let root = test_web_root();
        let response = route(&request("PUT", "/api/v1/health", b""), &root);
        assert_eq!(response.status, 405);
    }

    #[test]
    fn unknown_api_route_is_404_json() {
        let root = test_web_root();
        let response = route(&request("GET", "/api/v1/nope", b""), &root);
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
}
