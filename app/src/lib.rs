//! SeatTrellis desktop backend core (M1-02).
//!
//! Thin facade crate: re-exports the split crates so the Tauri shell and the
//! binary entry point keep a single import path. All real code lives in
//! `seattrellis-server` / `seattrellis-application` / `seattrellis-domain` /
//! `seattrellis-io` / `seattrellis-export`.

pub use seattrellis_domain::editing::{
    apply_command, apply_command_in_store, build_editor_state, create_draft, fetch_state,
    get_draft, new_draft_store, store_draft, EditorCommandEnvelope, EditorDraft, EditorDraftStore,
    EditorSeatSpec, EditorState,
};
pub use seattrellis_domain::layouts::{
    compile_layout_draft_json, create_layout_draft_json, delete_layout_draft,
    dispatch_layout_command_json, get_layout_state_json, new_layout_draft_store, LayoutCell,
    LayoutCellKind, LayoutCommandRequest, LayoutDraft, LayoutDraftStore, LayoutOperationRequest,
    LayoutStateResponse, MAX_LAYOUT_CELLS, MAX_LAYOUT_COLUMNS, MAX_LAYOUT_ROWS,
};
pub use seattrellis_io::migration::{
    migration_apply_json, migration_batch_apply_json, migration_batch_preview_json,
    migration_preview_json, migration_reference_checks_json, migration_restore_json,
};
pub use seattrellis_io::roster::{
    delete_draft, get_draft_json, parse_roster_csv, preview_roster_update, preview_update_json,
    upload_draft_json, RosterDraft, RosterDraftStore, RosterUpdatePreview, Student,
};
pub use seattrellis_io::rotation::{
    group_register_csv_json, group_register_html_json, group_register_preview_json,
    group_register_save_json, rotation_load_json, rotation_save_json, GROUP_REGISTER_FILE,
    ROTATION_PLAN_FILE,
};
pub use seattrellis_server::server::{resolve_web_root, Server, ServerConfig, ServerError};
