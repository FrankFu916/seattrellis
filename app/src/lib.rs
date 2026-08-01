//! SeatTrellis desktop backend core.
//!
//! A loopback-only HTTP server that serves the compiled React workbench
//! (`src/seattrellis/web_static/`) and exposes the native solve endpoint. This
//! is the guaranteed-to-build backend that a future Tauri shell can wrap; the
//! server itself has no Python/Node dependency.
//!
//! See [`server`] for the HTTP implementation and routes.

pub mod editing;
pub mod export;
pub mod goal_rules;
pub mod render;
pub mod room_templates;
pub mod roster;
pub mod server;

pub use editing::{
    apply_command, apply_command_in_store, build_editor_state, create_draft, fetch_state,
    get_draft, new_draft_store, store_draft, EditorCommandEnvelope, EditorDraft,
    EditorDraftStore, EditorSeatSpec, EditorState,
};
pub use roster::{
    delete_draft, get_draft_json, parse_roster_csv, preview_roster_update, preview_update_json,
    upload_draft_json, RosterDraft, RosterDraftStore, RosterUpdatePreview, Student,
};
pub use server::{resolve_web_root, Server, ServerConfig, ServerError};
