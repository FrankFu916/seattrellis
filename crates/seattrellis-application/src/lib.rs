//! Application layer (M1-02): use-case orchestration, separated from the
//! HTTP transport. Business modules here never touch `Request`/`Response`
//! or the socket layer; they return typed outcomes and [`AppError`] values
//! the transport maps onto HTTP.

pub mod class_document;
pub mod class_generation;
pub mod draft_audit;
pub mod export;
pub mod rotation;

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// Original solve request bodies, keyed by editor draft id. The export route
/// needs the request that produced a draft so it can reconstruct the full
/// renderable plan (request + current assignment) after edits.
pub type SolveRequestStore = Mutex<HashMap<String, Value>>;

/// Cap on stored solve requests (one per editor draft): mirrors
/// `editing::MAX_EDITOR_DRAFTS` so the two registries evict in lockstep.
/// Draft ids are server-generated monotonic, so the smallest key is the
/// oldest (FIFO, alpha.2/M7 item).
pub const MAX_SOLVE_REQUESTS: usize = 64;
/// Aggregate serialized source budget for one session or generated candidate batch.
pub const MAX_CONTEXT_SOURCE_BYTES: usize = 128 * 1024 * 1024;

/// Insert a solve request with the FIFO cap: at [`MAX_SOLVE_REQUESTS`] the
/// oldest entry (smallest draft id) is evicted, matching the editor store.
pub fn store_solve_request(
    store: &SolveRequestStore,
    draft_id: String,
    request: Value,
) -> Result<(), &'static str> {
    let mut guard = store
        .lock()
        .map_err(|_| "solve request store is poisoned")?;
    guard.insert(draft_id, request);
    if guard.len() > MAX_SOLVE_REQUESTS {
        if let Some(oldest) = guard.keys().min().cloned() {
            guard.remove(&oldest);
        }
    }
    Ok(())
}

/// Remove the sensitive solve request paired with an editor draft.
pub fn delete_solve_request(store: &SolveRequestStore, draft_id: &str) -> bool {
    let cleaned = draft_id.trim();
    if cleaned.is_empty() {
        return false;
    }
    store
        .lock()
        .map(|mut guard| guard.remove(cleaned).is_some())
        .unwrap_or(false)
}

/// A domain error from the application layer. `status` is the HTTP status
/// the transport should reply with; `code` is the stable machine-readable
/// error code; `message` is the human-facing detail.
#[derive(Debug, Clone)]
pub struct AppError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> AppError {
        AppError {
            status: 400,
            code: "bad_request",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> AppError {
        AppError {
            status: 404,
            code: "not_found",
            message: message.into(),
        }
    }

    pub fn unprocessable(code: &'static str, message: impl Into<String>) -> AppError {
        AppError {
            status: 422,
            code,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> AppError {
        AppError {
            status: 500,
            code: "internal_error",
            message: message.into(),
        }
    }

    /// A core solve rejection: input validation failures are InvalidInput
    /// (the transport adds the frozen `status` field, M1-03).
    pub fn solve_invalid_input(message: impl Into<String>) -> AppError {
        AppError {
            status: 400,
            code: "invalid_solve_request",
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn solve_request_store_evicts_oldest_at_the_cap() {
        // alpha.2/M7 item: mirrors the editor store cap so the two
        // registries evict in lockstep (smallest draft id = oldest).
        let store = SolveRequestStore::default();
        for index in 0..(MAX_SOLVE_REQUESTS + 4) {
            let id = format!("draft-{index:06}");
            store_solve_request(&store, id, json!({"index": index})).unwrap();
        }
        let guard = store.lock().unwrap();
        assert_eq!(guard.len(), MAX_SOLVE_REQUESTS, "store stays at the cap");
        assert!(!guard.contains_key("draft-000000"), "oldest evicted first");
        assert!(
            guard.contains_key(&format!("draft-{:06}", MAX_SOLVE_REQUESTS + 3)),
            "newest survives"
        );
    }

    #[test]
    fn solve_request_can_be_deleted_immediately() {
        let store = SolveRequestStore::default();
        store_solve_request(&store, "draft-1".to_string(), json!({"student": "S1"})).unwrap();
        assert!(delete_solve_request(&store, " draft-1 "));
        assert!(store.lock().unwrap().is_empty());
        assert!(!delete_solve_request(&store, "draft-1"));
        assert!(!delete_solve_request(&store, ""));
    }
}

// Request-owned cancellation propagates through legacy synchronous application entry points.
thread_local! {
    static REQUEST_CONTROL: std::cell::RefCell<Option<seattrellis_core::SolveControl>> = const { std::cell::RefCell::new(None) };
    static REQUEST_DRAFTS: std::cell::RefCell<Option<std::sync::Arc<Mutex<Vec<String>>>>> = const { std::cell::RefCell::new(None) };
}

pub fn with_request_control<T>(
    control: seattrellis_core::SolveControl,
    operation: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<seattrellis_core::SolveControl>);
    impl Drop for Restore {
        fn drop(&mut self) {
            REQUEST_CONTROL.with(|cell| *cell.borrow_mut() = self.0.take());
        }
    }
    let previous = REQUEST_CONTROL.with(|cell| cell.borrow_mut().replace(control));
    let _restore = Restore(previous);
    operation()
}

pub fn with_request_resources<T>(
    control: seattrellis_core::SolveControl,
    drafts: std::sync::Arc<Mutex<Vec<String>>>,
    operation: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<std::sync::Arc<Mutex<Vec<String>>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            REQUEST_DRAFTS.with(|cell| *cell.borrow_mut() = self.0.take());
        }
    }
    let previous = REQUEST_DRAFTS.with(|cell| cell.borrow_mut().replace(drafts));
    let _restore = Restore(previous);
    with_request_control(control, operation)
}

pub fn request_control() -> seattrellis_core::SolveControl {
    REQUEST_CONTROL.with(|cell| cell.borrow().clone().unwrap_or_default())
}

pub fn ensure_request_active() -> Result<(), AppError> {
    if request_control().is_cancelled() {
        return Err(AppError {
            status: 408,
            code: "cancelled",
            message: "request was cancelled".to_string(),
        });
    }
    Ok(())
}

/// Publish and evict the editor and its complete source together under a fixed lock order.
/// Count and aggregate source-byte limits keep long sessions bounded.
pub fn store_draft_context(
    editor_store: &seattrellis_domain::editing::EditorDraftStore,
    solve_requests: &SolveRequestStore,
    draft: seattrellis_domain::editing::EditorDraft,
    request: Value,
) -> Result<seattrellis_domain::editing::EditorState, AppError> {
    store_draft_contexts(editor_store, solve_requests, vec![(draft, request)])?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("draft context is missing"))
}

/// Publish a candidate/rotation set atomically; rejected or cancelled batches never evict user drafts.
pub fn store_draft_contexts(
    editor_store: &seattrellis_domain::editing::EditorDraftStore,
    solve_requests: &SolveRequestStore,
    contexts: Vec<(seattrellis_domain::editing::EditorDraft, Value)>,
) -> Result<Vec<seattrellis_domain::editing::EditorState>, AppError> {
    let tracker = REQUEST_DRAFTS.with(|cell| cell.borrow().clone());
    let mut created = match &tracker {
        Some(tracker) => Some(
            tracker
                .lock()
                .map_err(|_| AppError::internal("request tracker is poisoned"))?,
        ),
        None => None,
    };
    ensure_request_active()?;

    let bytes: usize = contexts
        .iter()
        .map(|(_, source)| source.to_string().len())
        .sum();
    if contexts.len() > MAX_SOLVE_REQUESTS || bytes > MAX_CONTEXT_SOURCE_BYTES {
        return Err(AppError::bad_request("draft set exceeds context capacity"));
    }
    let states: Vec<_> = contexts
        .iter()
        .map(|(draft, _)| seattrellis_domain::editing::build_editor_state(draft))
        .collect();
    let new_ids: std::collections::HashSet<_> =
        states.iter().map(|state| state.draft_id.clone()).collect();
    if new_ids.len() != states.len() {
        return Err(AppError::bad_request("duplicate draft id"));
    }
    let mut editors = editor_store
        .lock()
        .map_err(|_| AppError::internal("editor store is poisoned"))?;
    let mut sources = solve_requests
        .lock()
        .map_err(|_| AppError::internal("solve store is poisoned"))?;
    if new_ids.iter().any(|id| editors.contains_key(id)) {
        return Err(AppError::bad_request("draft id already exists"));
    }
    ensure_request_active()?;
    for (draft, source) in contexts {
        let id = draft.draft_id().to_string();
        editors.insert(id.clone(), draft);
        sources.insert(id.clone(), source);
        if let Some(created) = created.as_mut() {
            created.push(id);
        }
    }
    sources.retain(|id, _| editors.contains_key(id));
    let mut source_bytes: usize = sources
        .values()
        .map(|source| source.to_string().len())
        .sum();
    while editors.len() > MAX_SOLVE_REQUESTS || source_bytes > MAX_CONTEXT_SOURCE_BYTES {
        let Some(oldest) = editors
            .keys()
            .filter(|id| !new_ids.contains(*id))
            .min()
            .cloned()
        else {
            return Err(AppError::internal("context capacity invariant failed"));
        };
        editors.remove(&oldest);
        if let Some(source) = sources.remove(&oldest) {
            source_bytes = source_bytes.saturating_sub(source.to_string().len());
        }
    }
    Ok(states)
}

/// Candidate scoring is relative to its current peer assignments. Keep these
/// application-only references in namespaced metadata, outside solver fields.
pub fn attach_candidate_peers(
    contexts: &mut [(seattrellis_domain::editing::EditorDraft, Value)],
) -> Result<(), AppError> {
    let peers: Vec<String> = contexts
        .iter()
        .map(|(draft, _)| draft.draft_id().to_string())
        .collect();
    for (_, source) in contexts {
        let metadata = source
            .as_object_mut()
            .ok_or_else(|| AppError::bad_request("solve source must be an object"))?
            .entry("metadata")
            .or_insert_with(|| serde_json::json!({}));
        if metadata.is_null() {
            *metadata = serde_json::json!({});
        }
        let metadata = metadata
            .as_object_mut()
            .ok_or_else(|| AppError::bad_request("candidate source metadata must be an object"))?;
        metadata.insert(
            "_seattrellis_application".into(),
            serde_json::json!({"candidate_peer_ids":peers}),
        );
    }
    Ok(())
}
