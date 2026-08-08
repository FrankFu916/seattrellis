//! SeatTrellis desktop backend core.
//!
//! A loopback-only HTTP server that serves the compiled React workbench
//! (`src/seattrellis/web_static/`) and exposes the native solve endpoint. This
//! is the guaranteed-to-build backend that a future Tauri shell can wrap; the
//! server itself has no Python/Node dependency.
//!
//! See [`server`] for the HTTP implementation and routes.

pub mod editing;
mod embedded_web;
pub mod http;
pub mod export;
pub mod goal_rules;
pub mod layouts;
pub mod migration;
pub mod projects;
pub mod render;
pub mod rotation;
pub mod room_templates;
pub mod roster;
pub mod server;
pub mod transaction;

pub use editing::{
    apply_command, apply_command_in_store, build_editor_state, create_draft, fetch_state,
    get_draft, new_draft_store, store_draft, EditorCommandEnvelope, EditorDraft,
    EditorDraftStore, EditorSeatSpec, EditorState,
};
pub use layouts::{
    compile_layout_draft_json, create_layout_draft_json, delete_layout_draft,
    dispatch_layout_command_json, get_layout_state_json, new_layout_draft_store, LayoutCell,
    LayoutCellKind, LayoutCommandRequest, LayoutDraft, LayoutDraftStore, LayoutOperationRequest,
    LayoutStateResponse, MAX_LAYOUT_CELLS, MAX_LAYOUT_COLUMNS, MAX_LAYOUT_ROWS,
};
pub use migration::{
    migration_apply_json, migration_batch_apply_json, migration_batch_preview_json,
    migration_preview_json, migration_reference_checks_json, migration_restore_json,
};
pub use roster::{
    delete_draft, get_draft_json, parse_roster_csv, preview_roster_update, preview_update_json,
    upload_draft_json, RosterDraft, RosterDraftStore, RosterUpdatePreview, Student,
};
pub use rotation::{
    group_register_csv_json, group_register_html_json, group_register_preview_json,
    group_register_save_json, rotation_load_json, rotation_save_json, GROUP_REGISTER_FILE,
    ROTATION_PLAN_FILE,
};
pub use server::{resolve_web_root, Server, ServerConfig, ServerError};
