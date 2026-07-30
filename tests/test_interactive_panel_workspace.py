import sys
from pathlib import Path

import pytest

from seattrellis.web.interactive_panels import _resolve_panel_namespace
from seattrellis.web.keys import (
    PROJECT_BATCH_MOVE_BUTTON,
    PROJECT_BATCH_SEATS_SELECT,
    PROJECT_BATCH_STUDENTS_SELECT,
    PROJECT_CANVAS_MODE_SELECT,
    PROJECT_EDIT_ACTION_SELECT,
    PROJECT_EDIT_APPLY_BUTTON,
    PROJECT_EXPORT_PREFIX,
    PROJECT_LOCK_SEAT_BUTTON,
    PROJECT_LOCK_SEAT_SELECT,
    PROJECT_LOCK_STUDENT_BUTTON,
    PROJECT_LOCK_STUDENT_SELECT,
    PROJECT_REDO_BUTTON,
    PROJECT_REPAIR_BUTTON,
    PROJECT_SWAP_BUTTON,
    PROJECT_UNDO_BUTTON,
    QUICK_BATCH_MOVE_BUTTON,
    QUICK_BATCH_SEATS_SELECT,
    QUICK_BATCH_STUDENTS_SELECT,
    QUICK_CANVAS_MODE_SELECT,
    QUICK_EDIT_ACTION_SELECT,
    QUICK_EDIT_APPLY_BUTTON,
    QUICK_EXPORT_PREFIX,
    QUICK_LOCK_SEAT_BUTTON,
    QUICK_LOCK_SEAT_SELECT,
    QUICK_LOCK_STUDENT_BUTTON,
    QUICK_LOCK_STUDENT_SELECT,
    QUICK_REDO_BUTTON,
    QUICK_REPAIR_BUTTON,
    QUICK_SWAP_BUTTON,
    QUICK_UNDO_BUTTON,
)


CONTROL_KEYS = (
    ("repair_button", QUICK_REPAIR_BUTTON, PROJECT_REPAIR_BUTTON),
    ("swap_button", QUICK_SWAP_BUTTON, PROJECT_SWAP_BUTTON),
    ("undo_button", QUICK_UNDO_BUTTON, PROJECT_UNDO_BUTTON),
    ("redo_button", QUICK_REDO_BUTTON, PROJECT_REDO_BUTTON),
    ("edit_action_select", QUICK_EDIT_ACTION_SELECT, PROJECT_EDIT_ACTION_SELECT),
    ("edit_apply_button", QUICK_EDIT_APPLY_BUTTON, PROJECT_EDIT_APPLY_BUTTON),
    (
        "lock_student_select",
        QUICK_LOCK_STUDENT_SELECT,
        PROJECT_LOCK_STUDENT_SELECT,
    ),
    (
        "lock_student_button",
        QUICK_LOCK_STUDENT_BUTTON,
        PROJECT_LOCK_STUDENT_BUTTON,
    ),
    ("lock_seat_select", QUICK_LOCK_SEAT_SELECT, PROJECT_LOCK_SEAT_SELECT),
    ("lock_seat_button", QUICK_LOCK_SEAT_BUTTON, PROJECT_LOCK_SEAT_BUTTON),
    (
        "batch_students_select",
        QUICK_BATCH_STUDENTS_SELECT,
        PROJECT_BATCH_STUDENTS_SELECT,
    ),
    (
        "batch_seats_select",
        QUICK_BATCH_SEATS_SELECT,
        PROJECT_BATCH_SEATS_SELECT,
    ),
    ("batch_move_button", QUICK_BATCH_MOVE_BUTTON, PROJECT_BATCH_MOVE_BUTTON),
    ("canvas_mode_select", QUICK_CANVAS_MODE_SELECT, PROJECT_CANVAS_MODE_SELECT),
    ("export_prefix", QUICK_EXPORT_PREFIX, PROJECT_EXPORT_PREFIX),
)


@pytest.fixture(scope="module", autouse=True)
def _release_panel_module_after_key_tests():
    """Let Streamlit AppTest import the panel within its own script context."""

    yield
    sys.modules.pop("seattrellis.web.interactive_panels", None)


def test_legacy_panel_workspace_resolution_is_unchanged() -> None:
    assert _resolve_panel_namespace().workspace == "quick"
    assert _resolve_panel_namespace(project=True).workspace == "project"
    assert (
        _resolve_panel_namespace(project_path=Path("class.seattrellis.json")).workspace
        == "project"
    )


@pytest.mark.parametrize("workspace", ["teacher", "quick", "project"])
def test_explicit_panel_workspace_is_supported(workspace: str) -> None:
    assert _resolve_panel_namespace(workspace).workspace == workspace


@pytest.mark.parametrize("workspace", ["", "Teacher", "results", "quick "])
def test_unknown_panel_workspace_is_rejected(workspace: str) -> None:
    with pytest.raises(ValueError, match="Unknown panel workspace"):
        _resolve_panel_namespace(workspace)


def test_legacy_project_selectors_cannot_cross_workspace_boundaries() -> None:
    with pytest.raises(ValueError, match="project=True"):
        _resolve_panel_namespace("teacher", project=True)
    with pytest.raises(ValueError, match="project_path"):
        _resolve_panel_namespace(
            "quick",
            project_path=Path("class.seattrellis.json"),
        )


@pytest.mark.parametrize("name,quick_key,project_key", CONTROL_KEYS)
def test_quick_and_project_control_keys_keep_their_existing_values(
    name: str,
    quick_key: str,
    project_key: str,
) -> None:
    assert _resolve_panel_namespace("quick").control_key(name) == quick_key
    assert _resolve_panel_namespace("project").control_key(name) == project_key
    assert _resolve_panel_namespace("teacher").control_key(name) == (
        f"teacher_{quick_key.removeprefix('quick_')}"
    )


def test_workspace_session_and_widget_keys_are_isolated() -> None:
    keys_by_workspace: dict[str, set[str]] = {}
    for workspace in ("teacher", "quick", "project"):
        namespace = _resolve_panel_namespace(workspace)
        keys_by_workspace[workspace] = {
            namespace.state_key("editing_draft"),
            namespace.state_key("canvas_source_seat"),
            namespace.state_key("canvas_mode_value"),
            namespace.widget_key("canvas_seat_A1"),
            namespace.widget_key("repair_affected_students"),
            namespace.widget_key("repair_locked_students"),
            namespace.widget_key("repair_locked_seats"),
            namespace.widget_key("repair_reuse_saved_locks"),
            namespace.widget_key("repair_backend"),
            namespace.widget_key("repair_time_limit"),
            namespace.widget_key("edit_first_student"),
            namespace.widget_key("edit_second_student"),
            namespace.widget_key("edit_action_student_move"),
            namespace.widget_key("edit_action_seat_move"),
            namespace.prepared_export_state_key,
            *(namespace.control_key(name) for name, _quick, _project in CONTROL_KEYS),
        }

    assert keys_by_workspace["teacher"].isdisjoint(keys_by_workspace["quick"])
    assert keys_by_workspace["teacher"].isdisjoint(keys_by_workspace["project"])
    assert keys_by_workspace["quick"].isdisjoint(keys_by_workspace["project"])
    assert (
        _resolve_panel_namespace("teacher").prepared_export_state_key
        == "teacher_export_prepared_download"
    )
