//! Application layer (M1-02): use-case orchestration, separated from the
//! HTTP transport. Business modules here never touch `Request`/`Response`
//! or the socket layer; they return typed outcomes and [`AppError`] values
//! the transport maps onto HTTP.

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
}
