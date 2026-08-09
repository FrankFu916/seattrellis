//! Application layer (M1-02): use-case orchestration, separated from the
//! HTTP transport. Business modules here never touch `Request`/`Response`
//! or the socket layer; they return typed outcomes and [`AppError`] values
//! the transport maps onto HTTP.

pub mod class_generation;
pub mod export;
pub mod rotation;

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// Original solve request bodies, keyed by editor draft id. The export route
/// needs the request that produced a draft so it can reconstruct the full
/// renderable plan (request + current assignment) after edits.
pub type SolveRequestStore = Mutex<HashMap<String, Value>>;

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
