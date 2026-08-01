//! SeatTrellis desktop backend core.
//!
//! A loopback-only HTTP server that serves the compiled React workbench
//! (`src/seattrellis/web_static/`) and exposes the native solve endpoint. This
//! is the guaranteed-to-build backend that a future Tauri shell can wrap; the
//! server itself has no Python/Node dependency.
//!
//! See [`server`] for the HTTP implementation and routes.

pub mod server;

pub use server::{resolve_web_root, Server, ServerConfig, ServerError};
