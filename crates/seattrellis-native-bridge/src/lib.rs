//! Versioned, synchronous C ABI over the existing application layer.
//!
//! Platform clients own file access and dispatch on a background thread. Each
//! session has independent drafts and accepts one call at a time. Cancellation
//! and destruction never acquire an application/store lock or wait for a solve.
//! See `bindings/include/seattrellis.h` and `docs/native-bridge.md` for ownership,
//! wire contracts, quotas, and the limits of foreign-pointer validation.

#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;
use std::io::{self, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwapOption;
use base64::Engine;
use seattrellis_application::{AppError, SolveRequestStore};
use seattrellis_core::SolveControl;
use seattrellis_domain::editing::{self, EditorDraftStore};
use serde::Deserialize;
use serde_json::{json, Value};

pub const ABI_VERSION: u32 = 1;
pub const MAX_SESSIONS: usize = 16;
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_OUTSTANDING_BUFFERS: usize = 256;
pub const MAX_OUTSTANDING_BUFFER_BYTES: usize = 128 * 1024 * 1024;

/// Owned immutable bytes, without a trailing NUL. Copy before freeing.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SeattrellisBuffer {
    pub data: *mut u8,
    pub len: usize,
}

impl SeattrellisBuffer {
    fn empty() -> Self {
        Self {
            data: std::ptr::null_mut(),
            len: 0,
        }
    }
}

struct Session {
    id: u64,
    closed: AtomicBool,
    active: ArcSwapOption<ActiveRequest>,
    editors: EditorDraftStore,
    sources: SolveRequestStore,
}

impl Session {
    fn new(id: u64) -> Self {
        Self {
            id,
            closed: AtomicBool::new(false),
            active: ArcSwapOption::empty(),
            editors: editing::new_draft_store(),
            sources: SolveRequestStore::default(),
        }
    }

    fn cancel(&self) -> i32 {
        // Pin exactly one request allocation. A delayed cancellation can
        // never reload a new call's control after the old call completes.
        if let Some(request) = self.active.load_full() {
            request.control.cancel();
            1
        } else {
            0
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(request) = self.active.load_full() {
            request.control.cancel();
        }
    }
}

struct ActiveRequest {
    control: SolveControl,
}

fn sessions() -> &'static [ArcSwapOption<Session>; MAX_SESSIONS] {
    static SESSIONS: OnceLock<[ArcSwapOption<Session>; MAX_SESSIONS]> = OnceLock::new();
    SESSIONS.get_or_init(|| std::array::from_fn(|_| ArcSwapOption::empty()))
}

fn find_session(id: u64) -> Option<(usize, Arc<Session>)> {
    if id == 0 {
        return None;
    }
    sessions().iter().enumerate().find_map(|(index, slot)| {
        slot.load_full()
            .filter(|session| session.id == id && !session.closed.load(Ordering::Acquire))
            .map(|session| (index, session))
    })
}

fn destroy_session(id: u64) {
    if let Some((index, session)) = find_session(id) {
        session.closed.store(true, Ordering::Release);
        session.cancel();
        // Compare the allocation, not just the slot: a reused slot must never
        // be removed by a simultaneous destroy using an older handle.
        sessions()[index].compare_and_swap(&Some(session), None);
    }
}

struct ActiveCall<'a> {
    session: &'a Session,
    request: Arc<ActiveRequest>,
}

impl Drop for ActiveCall<'_> {
    fn drop(&mut self) {
        self.session
            .active
            .compare_and_swap(&Some(self.request.clone()), None);
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchRequest {
    protocol_version: u32,
    operation: String,
    payload: Value,
}

fn bridge_error(status: u16, code: &'static str, message: impl Into<String>) -> AppError {
    AppError {
        status,
        code,
        message: message.into(),
    }
}

fn required_string<'a>(payload: &'a Value, key: &str) -> Result<&'a str, AppError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request(format!("{key} must be a non-empty string")))
}

fn state_value(session: &Session, state: editing::EditorState) -> Result<Value, AppError> {
    let validation = seattrellis_application::export::editor_validation_report(
        &state.draft_id,
        &state,
        &session.sources,
    )?;
    let mut result = serde_json::to_value(state).map_err(|e| AppError::internal(e.to_string()))?;
    result["validation"] = validation;
    Ok(result)
}

fn operation(session: &Session, request: DispatchRequest) -> Result<Value, AppError> {
    use seattrellis_application::{class_document, class_generation, draft_audit, export};
    let payload = request.payload;
    match request.operation.as_str() {
        "generate" => {
            let outcome =
                class_generation::generate_class(&payload, &session.editors, &session.sources)?;
            if outcome.status == seattrellis_core::SolveStatus::Cancelled {
                return Err(bridge_error(408, "cancelled", "request was cancelled"));
            }
            let candidates: Vec<_> = outcome
                .candidates
                .iter()
                .map(|candidate| {
                    json!({
                        "candidate_id": candidate.draft_id,
                        "recommended": candidate.recommended,
                        "total_score": candidate.total_score,
                    })
                })
                .collect();
            Ok(json!({
                "status": outcome.status,
                "feasible": outcome.feasible,
                "class_name": outcome.class_name,
                "goal_id": outcome.goal_id,
                "total_score": outcome.total_score,
                "recommended_candidate_id": outcome.recommended_candidate_id,
                "candidates": candidates,
                "editor": outcome.editor,
            }))
        }
        "state" => {
            let id = required_string(&payload, "draft_id")?;
            let state = editing::fetch_state(&session.editors, id).map_err(AppError::not_found)?;
            state_value(session, state)
        }
        "command" => {
            let command: editing::EditorCommandEnvelope = serde_json::from_value(payload)
                .map_err(|_| AppError::bad_request("invalid editor command envelope"))?;
            let state =
                editing::apply_command_in_store(&session.editors, &command).map_err(|message| {
                    if message.contains("unknown editor draft") {
                        AppError::not_found(message)
                    } else if message.contains("stale")
                        || message.contains("protocol version")
                        || message.contains("already been applied")
                        || message.contains("different draft")
                        || message.contains("command kind")
                        || message.contains("command_id")
                    {
                        bridge_error(409, "command_conflict", message)
                    } else {
                        AppError::bad_request(message)
                    }
                })?;
            state_value(session, state)
        }
        "serialize" => {
            class_document::serialize_document(&payload, &session.editors, &session.sources)
        }
        "open" => class_document::open_document(&payload, &session.editors, &session.sources),
        "repair" => class_document::repair_draft(
            required_string(&payload, "draft_id")?,
            &payload,
            &session.editors,
            &session.sources,
        ),
        "audit" => draft_audit::audit_draft(
            &session.editors,
            &session.sources,
            required_string(&payload, "draft_id")?,
        ),
        "delete" => {
            let id = required_string(&payload, "draft_id")?;
            let mut editors = session
                .editors
                .lock()
                .map_err(|_| AppError::internal("editor store is poisoned"))?;
            let mut sources = session
                .sources
                .lock()
                .map_err(|_| AppError::internal("solve store is poisoned"))?;
            if editors.remove(id).is_none() {
                return Err(AppError::not_found("editor draft was not found"));
            }
            sources.remove(id);
            Ok(json!({"deleted": true}))
        }
        "export" => {
            let id = required_string(&payload, "draft_id")?;
            let format = required_string(&payload, "format")?;
            let parsed_format = seattrellis_export::export::ExportFormat::parse(format)
                .map_err(AppError::bad_request)?;
            let mut options = match payload.get("options") {
                None => serde_json::Map::new(),
                Some(Value::Object(options)) => options.clone(),
                Some(_) => return Err(AppError::bad_request("export options must be an object")),
            };
            const OPTION_KEYS: &[&str] = &[
                "title",
                "template",
                "privacy",
                "orientation",
                "page_scale",
                "paper_size",
                "margin_mm",
                "locale",
                "show_student_ids",
                "expected_revision",
            ];
            if options
                .keys()
                .any(|key| !OPTION_KEYS.contains(&key.as_str()))
            {
                return Err(AppError::bad_request("unknown export option"));
            }
            options.insert("draft_id".into(), json!(id));
            options.insert("format".into(), json!(format));
            let outcome = export::export_draft_isolated(
                &Value::Object(options),
                &session.editors,
                &session.sources,
            )?;
            if outcome.body.len() > (MAX_OUTPUT_BYTES - 4096) / 4 * 3 {
                return Err(bridge_error(
                    413,
                    "output_too_large",
                    "export exceeds native response limit",
                ));
            }
            Ok(json!({
                "filename": format!("seat-plan.{}", parsed_format.extension()),
                "mime_type": outcome.content_type,
                "base64": base64::engine::general_purpose::STANDARD.encode(outcome.body),
                "warnings": outcome.warnings,
            }))
        }
        _ => Err(bridge_error(
            400,
            "unknown_operation",
            "unsupported native operation",
        )),
    }
}

fn clean_created(session: &Session, tracker: &Mutex<Vec<String>>) {
    if let Ok(ids) = tracker.lock() {
        for id in ids.iter() {
            editing::delete_draft(&session.editors, id);
            seattrellis_application::delete_solve_request(&session.sources, id);
        }
    }
}

/// Generate/open can evict complete contexts. Stage them so cancellation or
/// an oversized response cannot remove existing user drafts without a result.
fn stage_contexts(session: &Session) -> Result<Session, AppError> {
    let editors = session
        .editors
        .lock()
        .map_err(|_| AppError::internal("editor store is poisoned"))?;
    let sources = session
        .sources
        .lock()
        .map_err(|_| AppError::internal("solve store is poisoned"))?;
    let mut staged = Session::new(session.id);
    *staged
        .editors
        .get_mut()
        .map_err(|_| AppError::internal("editor store is poisoned"))? = editors.clone();
    *staged
        .sources
        .get_mut()
        .map_err(|_| AppError::internal("solve store is poisoned"))? = sources.clone();
    Ok(staged)
}

fn commit_contexts(session: &Session, mut staged: Session) -> Result<(), AppError> {
    let mut editors = session
        .editors
        .lock()
        .map_err(|_| AppError::internal("editor store is poisoned"))?;
    let mut sources = session
        .sources
        .lock()
        .map_err(|_| AppError::internal("solve store is poisoned"))?;
    seattrellis_application::ensure_request_active()?;
    *editors = std::mem::take(
        staged
            .editors
            .get_mut()
            .map_err(|_| AppError::internal("editor store is poisoned"))?,
    );
    *sources = std::mem::take(
        staged
            .sources
            .get_mut()
            .map_err(|_| AppError::internal("solve store is poisoned"))?,
    );
    Ok(())
}

fn error_value(error: AppError) -> Value {
    json!({
        "protocol_version": ABI_VERSION,
        "ok": false,
        "error": {"code": error.code, "message": error.message, "status": error.status},
    })
}

struct BoundedBytes(Vec<u8>);

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("native output limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode(value: &Value) -> Result<Vec<u8>, AppError> {
    let mut bytes = BoundedBytes(Vec::new());
    serde_json::to_writer(&mut bytes, value).map_err(|_| {
        bridge_error(
            413,
            "output_too_large",
            "native response exceeds size limit",
        )
    })?;
    Ok(bytes.0)
}

fn error_bytes(error: AppError) -> Vec<u8> {
    // All transport errors fit the bound; fallback avoids unwinding if a
    // future application error accidentally contains a very large message.
    encode(&error_value(error)).unwrap_or_else(|_| {
        br#"{"protocol_version":1,"ok":false,"error":{"code":"internal_error","message":"could not encode native error","status":500}}"#.to_vec()
    })
}

fn dispatch_bytes(session: &Session, input: &[u8]) -> Vec<u8> {
    let request = Arc::new(ActiveRequest {
        control: SolveControl::new(),
    });
    if session
        .active
        .compare_and_swap(std::ptr::null::<ActiveRequest>(), Some(request.clone()))
        .is_some()
    {
        return error_bytes(bridge_error(
            409,
            "session_busy",
            "another native call is active",
        ));
    }
    let _active = ActiveCall {
        session,
        request: request.clone(),
    };
    let control = request.control.clone();
    if session.closed.load(Ordering::Acquire) {
        control.cancel();
    }
    let tracker = Arc::new(Mutex::new(Vec::new()));
    let result = catch_unwind(AssertUnwindSafe(|| {
        seattrellis_application::with_request_resources(control.clone(), tracker.clone(), || {
            seattrellis_application::ensure_request_active()?;
            let request: DispatchRequest = serde_json::from_slice(input).map_err(|_| {
                bridge_error(
                    400,
                    "invalid_json",
                    "request must be a valid UTF-8 JSON envelope",
                )
            })?;
            if request.protocol_version != ABI_VERSION {
                return Err(bridge_error(
                    409,
                    "protocol_mismatch",
                    "native protocol_version must be 1",
                ));
            }
            #[cfg(test)]
            let inject_oversized_response = request
                .payload
                .pointer("/metadata/__test_response_oversize")
                == Some(&json!(true));
            #[cfg(test)]
            let inject_precommit_cancel =
                request.payload.pointer("/metadata/__test_precommit_cancel") == Some(&json!(true));
            #[cfg(test)]
            if request.operation == "__test_panic_after_draft" {
                operation(
                    session,
                    DispatchRequest {
                        protocol_version: ABI_VERSION,
                        operation: "generate".into(),
                        payload: json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0]]}),
                    },
                )?;
                panic!("forced panic after context publication");
            }
            let staged = if matches!(request.operation.as_str(), "generate" | "open") {
                Some(stage_contexts(session)?)
            } else {
                None
            };
            let result = operation(staged.as_ref().unwrap_or(session), request)?;
            #[cfg(test)]
            let result = if inject_oversized_response {
                let mut result = result;
                result["__test_padding"] = json!("x".repeat(MAX_OUTPUT_BYTES));
                result
            } else {
                result
            };
            // A successful operation has committed. Cancellation arriving
            // after commit must not disguise the resulting state as failure.
            // The shared solver/repair layer checks before publishing changes.
            let bytes =
                encode(&json!({"protocol_version": ABI_VERSION, "ok": true, "result": result}))?;
            #[cfg(test)]
            if inject_precommit_cancel {
                control.cancel();
            }
            if let Some(staged) = staged {
                commit_contexts(session, staged)?;
            }
            Ok(bytes)
        })
    }));
    match result {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(error)) => {
            clean_created(session, &tracker);
            error_bytes(error)
        }
        Err(_) => {
            destroy_session(session.id);
            clean_created(session, &tracker);
            error_bytes(bridge_error(
                500,
                "internal_panic",
                "native session failed; create a new session",
            ))
        }
    }
}

#[derive(Default)]
struct BufferRegistry {
    bytes: usize,
    reserved_bytes: usize,
    reserved_count: usize,
    owned: HashMap<usize, Box<[u8]>>,
}

fn buffers() -> &'static Mutex<BufferRegistry> {
    static BUFFERS: OnceLock<Mutex<BufferRegistry>> = OnceLock::new();
    BUFFERS.get_or_init(Mutex::default)
}

struct ResponseReservation;

impl ResponseReservation {
    fn acquire() -> Option<Self> {
        let mut registry = buffers().lock().unwrap_or_else(|e| e.into_inner());
        if registry.owned.len() + registry.reserved_count >= MAX_OUTSTANDING_BUFFERS
            || MAX_OUTPUT_BYTES
                > MAX_OUTSTANDING_BUFFER_BYTES
                    .saturating_sub(registry.bytes)
                    .saturating_sub(registry.reserved_bytes)
        {
            return None;
        }
        registry.reserved_bytes += MAX_OUTPUT_BYTES;
        registry.reserved_count += 1;
        Some(Self)
    }

    fn finish(self, bytes: Vec<u8>) -> SeattrellisBuffer {
        let mut registry = buffers().lock().unwrap_or_else(|e| e.into_inner());
        let mut bytes = bytes.into_boxed_slice();
        let buffer = SeattrellisBuffer {
            data: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
        registry.bytes += bytes.len();
        registry.owned.insert(buffer.data as usize, bytes);
        drop(registry);
        buffer
    }
}

impl Drop for ResponseReservation {
    fn drop(&mut self) {
        let mut registry = buffers().lock().unwrap_or_else(|e| e.into_inner());
        registry.reserved_bytes -= MAX_OUTPUT_BYTES;
        registry.reserved_count -= 1;
    }
}

/// Return the ABI/protocol major version.
#[no_mangle]
pub extern "C" fn seattrellis_abi_version() -> u32 {
    ABI_VERSION
}

/// Create an isolated session; zero means the session quota is exhausted.
#[no_mangle]
pub extern "C" fn seattrellis_session_create() -> u64 {
    catch_unwind(|| {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let Ok(id) =
            NEXT_ID.fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
        else {
            return 0;
        };
        let session = Arc::new(Session::new(id));
        for slot in sessions() {
            if slot
                .compare_and_swap(std::ptr::null::<Session>(), Some(session.clone()))
                .is_none()
            {
                return id;
            }
        }
        0
    })
    .unwrap_or(0)
}

/// Remove a session and request cancellation; an active call retains its own
/// reference until it finishes. Zero/unknown/already destroyed handles are safe.
#[no_mangle]
pub extern "C" fn seattrellis_session_destroy(session: u64) {
    let _ = catch_unwind(|| destroy_session(session));
}

/// Dispatch immutable UTF-8 JSON, synchronously on the caller's thread.
///
/// # Safety
/// For a non-null `input`, the caller must provide `len` initialized readable
/// bytes in one live allocation, immutable throughout this call. Null and
/// oversized inputs are rejected before dereference. Arbitrary non-null foreign
/// addresses cannot be validated: a dangling/mis-sized pointer is caller UB.
#[no_mangle]
pub unsafe extern "C" fn seattrellis_session_dispatch(
    session: u64,
    input: *const u8,
    len: usize,
) -> SeattrellisBuffer {
    catch_unwind(AssertUnwindSafe(|| {
        // Reserve a slot and the full output budget before application mutation.
        // An exhausted caller gets {NULL,0} without application side effects.
        let Some(reservation) = ResponseReservation::acquire() else {
            return SeattrellisBuffer::empty();
        };
        let bytes = catch_unwind(AssertUnwindSafe(|| {
            let bytes = if len > MAX_INPUT_BYTES {
                error_bytes(bridge_error(
                    413,
                    "input_too_large",
                    "native request exceeds size limit",
                ))
            } else if input.is_null() || len == 0 {
                error_bytes(bridge_error(
                    400,
                    "invalid_buffer",
                    "native request must contain JSON bytes",
                ))
            } else if let Some((_, session)) = find_session(session) {
                // SAFETY: the required readable, live, immutable allocation is the
                // caller's documented obligation; null/length caps were checked.
                dispatch_bytes(&session, unsafe { std::slice::from_raw_parts(input, len) })
            } else {
                error_bytes(bridge_error(
                    404,
                    "invalid_session",
                    "native session was not found",
                ))
            };
            bytes
        }))
        .unwrap_or_else(|_| error_bytes(AppError::internal("native dispatch failed")));
        reservation.finish(bytes)
    }))
    .unwrap_or_else(|_| {
        seattrellis_session_destroy(session);
        SeattrellisBuffer::empty()
    })
}

/// Request cancellation without waiting for dispatch or application locks.
/// Return 1 for an active call, 0 for an idle session, -1 for an invalid handle.
#[no_mangle]
pub extern "C" fn seattrellis_session_cancel(session: u64) -> i32 {
    catch_unwind(|| find_session(session).map_or(-1, |(_, session)| session.cancel())).unwrap_or(-1)
}

/// Release an unchanged returned buffer pair once, after copying its bytes.
/// Null, unknown and length-mismatched pairs are ignored without dereference.
/// Concurrent reads of a freed buffer and allocator-address reuse are caller
/// errors; this registry is a quota/ownership check, not a foreign-memory sandbox.
#[no_mangle]
pub extern "C" fn seattrellis_buffer_free(buffer: SeattrellisBuffer) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mut registry = buffers().lock().unwrap_or_else(|e| e.into_inner());
        let key = buffer.data as usize;
        if registry
            .owned
            .get(&key)
            .is_some_and(|owned| owned.len() == buffer.len)
        {
            if let Some(bytes) = registry.owned.remove(&key) {
                registry.bytes -= bytes.len();
            }
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn heavy_request() -> Vec<u8> {
        let count = 80;
        let students: Vec<_> = (0..count)
            .map(|index| {
                json!({
                    "key":format!("S{index:03}"),"score":40.0+(index as f64*7.3)%59.0,
                })
            })
            .collect();
        let positions: Vec<_> = (0..count)
            .map(|index| [(index % 6) as f64, (index / 6) as f64])
            .collect();
        serde_json::to_vec(&json!({"protocol_version":1,"operation":"generate","payload":{
            "api_version":2,"student_count":count,"seat_positions":positions,"students":students,
            "seed":42,"time_budget_ms":30000,"options":{"candidate_count":20},
            "rules":{"soft":{
                "score_position":{"enabled":true,"weight":15,"direction":"high_back"},
                "score_distribution":{"enabled":true,"weight":8},
                "randomize":{"enabled":true,"weight":2}
            }}
        }})).unwrap()
    }

    fn wait_active(session: &Session) {
        let started = Instant::now();
        while session.active.load().is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "dispatch did not start"
            );
            std::thread::yield_now();
        }
    }

    fn parsed_buffer(buffer: SeattrellisBuffer) -> Value {
        assert!(!buffer.data.is_null());
        // SAFETY: this buffer is returned by the ABI and remains owned until
        // its single free below, after serde has copied the parsed document.
        let value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(buffer.data, buffer.len) })
                .unwrap();
        seattrellis_buffer_free(buffer);
        value
    }

    #[test]
    fn busy_cancel_and_destroy_do_not_wait_for_active_dispatch_or_poison_next_call() {
        let _serial = TEST_LOCK.lock().unwrap();
        let id = seattrellis_session_create();
        let (_, session) = find_session(id).unwrap();
        let input = heavy_request();
        let worker = std::thread::spawn(move || {
            // SAFETY: this worker owns the input Vec throughout the call.
            parsed_buffer(unsafe { seattrellis_session_dispatch(id, input.as_ptr(), input.len()) })
        });
        wait_active(&session);
        let overlap = dispatch_bytes(
            &session,
            br#"{"protocol_version":1,"operation":"state","payload":{"draft_id":"missing"}}"#,
        );
        let overlap: Value = serde_json::from_slice(&overlap).unwrap();
        assert_eq!(overlap["error"]["code"], "session_busy");
        let started = Instant::now();
        assert_eq!(seattrellis_session_cancel(id), 1);
        assert!(started.elapsed() < Duration::from_secs(1));
        let cancelled = worker.join().unwrap();
        assert_eq!(cancelled["error"]["code"], "cancelled", "{cancelled}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(session.editors.lock().unwrap().is_empty());
        assert!(session.sources.lock().unwrap().is_empty());
        let fresh = dispatch_bytes(&session, br#"{"protocol_version":1,"operation":"generate","payload":{"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0]]}}"#);
        let fresh: Value = serde_json::from_slice(&fresh).unwrap();
        assert_eq!(fresh["result"]["status"], "Solved", "{fresh}");

        let input = heavy_request();
        let worker = std::thread::spawn(move || {
            // SAFETY: the worker owns these immutable input bytes.
            parsed_buffer(unsafe { seattrellis_session_dispatch(id, input.as_ptr(), input.len()) })
        });
        wait_active(&session);
        let started = Instant::now();
        seattrellis_session_destroy(id);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(seattrellis_session_cancel(id), -1);
        let replacement = seattrellis_session_create();
        assert_ne!(replacement, id);
        seattrellis_session_destroy(id);
        assert_eq!(seattrellis_session_cancel(replacement), 0);
        let cancelled = worker.join().unwrap();
        assert_eq!(cancelled["error"]["code"], "cancelled", "{cancelled}");
        assert!(started.elapsed() < Duration::from_secs(5));
        seattrellis_session_destroy(replacement);
    }

    #[test]
    fn pinned_old_request_cancellation_and_cleanup_cannot_affect_new_request() {
        let _serial = TEST_LOCK.lock().unwrap();
        let session = Session::new(1);
        let old = Arc::new(ActiveRequest {
            control: SolveControl::new(),
        });
        session.active.store(Some(old.clone()));
        // Model a cancel caller paused immediately after its single load.
        let pinned_cancel = session.active.load_full().unwrap();
        drop(ActiveCall {
            session: &session,
            request: old.clone(),
        });
        let fresh = Arc::new(ActiveRequest {
            control: SolveControl::new(),
        });
        session.active.store(Some(fresh.clone()));
        pinned_cancel.control.cancel();
        // Even a delayed old guard can only remove its own allocation.
        drop(ActiveCall {
            session: &session,
            request: old,
        });
        assert!(pinned_cancel.control.is_cancelled());
        assert!(!fresh.control.is_cancelled());
        assert!(Arc::ptr_eq(&session.active.load_full().unwrap(), &fresh));
    }

    #[test]
    fn outstanding_byte_budget_includes_inflight_response_reservations() {
        let _serial = TEST_LOCK.lock().unwrap();
        let mut owned = Vec::new();
        for _ in 0..3 {
            let reservation = ResponseReservation::acquire().unwrap();
            owned.push(reservation.finish(vec![b'x'; MAX_OUTPUT_BYTES]));
        }
        let final_reservation = ResponseReservation::acquire().unwrap();
        assert!(ResponseReservation::acquire().is_none());
        drop(final_reservation);
        for buffer in owned {
            seattrellis_buffer_free(buffer);
        }
        assert!(ResponseReservation::acquire().is_some());
    }

    #[test]
    fn forced_panic_is_contained_retires_handle_and_cleans_request_contexts() {
        let _serial = TEST_LOCK.lock().unwrap();
        let id = seattrellis_session_create();
        let (_, session) = find_session(id).unwrap();
        let input =
            br#"{"protocol_version":1,"operation":"__test_panic_after_draft","payload":{}}"#;
        // SAFETY: static initialized input remains immutable for this call.
        let result =
            parsed_buffer(unsafe { seattrellis_session_dispatch(id, input.as_ptr(), input.len()) });
        assert_eq!(result["error"]["code"], "internal_panic");
        assert_eq!(seattrellis_session_cancel(id), -1);
        assert!(session.editors.lock().unwrap().is_empty());
        assert!(session.sources.lock().unwrap().is_empty());
        let replacement = seattrellis_session_create();
        assert_ne!(replacement, 0);
        assert_ne!(replacement, id);
        seattrellis_session_destroy(replacement);
    }

    #[test]
    fn oversized_encoded_response_is_rejected_at_the_writer_bound() {
        let _serial = TEST_LOCK.lock().unwrap();
        let value = json!({"data":"x".repeat(MAX_OUTPUT_BYTES)});
        let error = encode(&value).unwrap_err();
        assert_eq!(error.code, "output_too_large");
        assert_eq!(error.status, 413);
    }

    #[test]
    fn cancelled_or_oversized_generated_response_does_not_evict_existing_contexts() {
        let _serial = TEST_LOCK.lock().unwrap();
        let id = seattrellis_session_create();
        let (_, session) = find_session(id).unwrap();
        let source = json!({"api_version":2,"student_count":2,"seat_positions":[[0,0],[1,0]]});
        let mut original_ids = Vec::new();
        for index in 0..seattrellis_application::MAX_SOLVE_REQUESTS {
            let draft_id = format!("existing-{index:06}");
            let draft = seattrellis_application::class_document::restored_draft(
                &source,
                &draft_id,
                None,
                &[("STU001", "seat-1"), ("STU002", "seat-2")],
                &[],
                &[],
            )
            .unwrap();
            seattrellis_application::store_draft_context(
                &session.editors,
                &session.sources,
                draft,
                source.clone(),
            )
            .unwrap();
            original_ids.push(draft_id);
        }
        for (flag, expected_code) in [
            ("__test_precommit_cancel", "cancelled"),
            ("__test_response_oversize", "output_too_large"),
        ] {
            let mut payload = source.clone();
            payload["metadata"] = json!({flag:true});
            let request = serde_json::to_vec(
                &json!({"protocol_version":1,"operation":"generate","payload":payload}),
            )
            .unwrap();
            // SAFETY: this Vec stays initialized/live/immutable across dispatch.
            let response = parsed_buffer(unsafe {
                seattrellis_session_dispatch(id, request.as_ptr(), request.len())
            });
            assert_eq!(response["error"]["code"], expected_code, "{response}");
            let editors = session.editors.lock().unwrap();
            let sources = session.sources.lock().unwrap();
            assert_eq!(editors.len(), original_ids.len());
            assert_eq!(sources.len(), original_ids.len());
            assert!(original_ids
                .iter()
                .all(|id| editors.contains_key(id) && sources.contains_key(id)));
        }
        seattrellis_session_destroy(id);
    }
}
