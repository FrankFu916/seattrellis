"""Stable Streamlit widget keys used by tests and workflow code."""

UI_LANGUAGE_SELECT = "ui_language_choice"

QUICK_LOAD_DEMO_BUTTON = "quick_load_demo"
QUICK_STEP_RADIO = "quick_step"
QUICK_INSPECT_HISTORY_BUTTON = "quick_inspect_history"
QUICK_CANDIDATE_SELECT = "quick_candidate_selector"
QUICK_SOLVE_STATUS = "quick_solve_status"
QUICK_RESULTS_STATUS = "quick_results_status"
QUICK_REPAIR_BUTTON = "quick_repair"
QUICK_SWAP_BUTTON = "quick_swap_students"
QUICK_UNDO_BUTTON = "quick_edit_undo"
QUICK_REDO_BUTTON = "quick_edit_redo"
QUICK_EDIT_ACTION_SELECT = "quick_edit_action"
QUICK_EDIT_APPLY_BUTTON = "quick_edit_apply"
QUICK_LOCK_STUDENT_SELECT = "quick_lock_student"
QUICK_LOCK_STUDENT_BUTTON = "quick_toggle_student_lock"
QUICK_LOCK_SEAT_SELECT = "quick_lock_seat"
QUICK_LOCK_SEAT_BUTTON = "quick_toggle_seat_lock"
QUICK_BATCH_STUDENTS_SELECT = "quick_batch_students"
QUICK_BATCH_SEATS_SELECT = "quick_batch_seats"
QUICK_BATCH_MOVE_BUTTON = "quick_batch_move"
QUICK_CANVAS_MODE_SELECT = "quick_canvas_mode"
QUICK_GENERATE_BUTTON = "quick_generate"
QUICK_CANDIDATE_COUNT_INPUT = "quick_candidate_count"
QUICK_EXPORT_FORMAT_SELECT = "quick_export_format"
QUICK_EXPORT_ALL_CANDIDATES_CHECKBOX = "quick_export_all_candidates"
QUICK_EXPORT_DOWNLOAD_ARTIFACT = "quick_export_download_artifact"
QUICK_EXPORT_DOWNLOAD_REPORT = "quick_export_download_report"
QUICK_EXPORT_PREFIX = "quick_export"

PROJECT_EXPORT_DOWNLOAD_ARTIFACT = "project_export_download_artifact"
PROJECT_EXPORT_DOWNLOAD_REPORT = "project_export_download_report"
PROJECT_EXPORT_PREFIX = "project_export"
PROJECT_REPAIR_BUTTON = "project_repair"
PROJECT_SWAP_BUTTON = "project_swap_students"
PROJECT_UNDO_BUTTON = "project_edit_undo"
PROJECT_REDO_BUTTON = "project_edit_redo"
PROJECT_EDIT_ACTION_SELECT = "project_edit_action"
PROJECT_EDIT_APPLY_BUTTON = "project_edit_apply"
PROJECT_LOCK_STUDENT_SELECT = "project_lock_student"
PROJECT_LOCK_STUDENT_BUTTON = "project_toggle_student_lock"
PROJECT_LOCK_SEAT_SELECT = "project_lock_seat"
PROJECT_LOCK_SEAT_BUTTON = "project_toggle_seat_lock"
PROJECT_BATCH_STUDENTS_SELECT = "project_batch_students"
PROJECT_BATCH_SEATS_SELECT = "project_batch_seats"
PROJECT_BATCH_MOVE_BUTTON = "project_batch_move"
PROJECT_CANVAS_MODE_SELECT = "project_canvas_mode"


def export_prepare_key(export_prefix: str, output_format: str) -> str:
    """Return the widget key for preparing an export."""

    return f"{export_prefix}_prepare_{output_format}"


def export_prepared_download_key(export_prefix: str) -> str:
    """Return the widget key for downloading a prepared export."""

    return f"{export_prefix}_download_prepared"


def widget_region_key(widget_key: str) -> str:
    """Return a stable container key for browser automation and styling."""

    return f"{widget_key}_region"
