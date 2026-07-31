"""Stateful Streamlit panels for manual editing and constrained repair."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Literal, Sequence, cast

from pydantic import ValidationError

try:
    import streamlit as st
except Exception as exc:  # pragma: no cover - guarded by the web extra.
    from seattrellis.optional import MissingOptionalDependencyError

    raise MissingOptionalDependencyError("Streamlit web UI", "web") from exc

from seattrellis.editing import (
    EditingError,
    EditingOperation,
    lock_state_from_snapshot,
)
from seattrellis.io.json_files import InputFileError
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.web.components import layout_grid_axes
from seattrellis.web.keys import (
    PROJECT_BATCH_MOVE_BUTTON,
    PROJECT_BATCH_SEATS_SELECT,
    PROJECT_BATCH_STUDENTS_SELECT,
    PROJECT_CANVAS_MODE_SELECT,
    PROJECT_EDIT_ACTION_SELECT,
    PROJECT_EDIT_APPLY_BUTTON,
    PROJECT_LOCK_SEAT_BUTTON,
    PROJECT_LOCK_SEAT_SELECT,
    PROJECT_LOCK_STUDENT_BUTTON,
    PROJECT_LOCK_STUDENT_SELECT,
    PROJECT_REDO_BUTTON,
    PROJECT_REPAIR_BUTTON,
    PROJECT_EXPORT_PREFIX,
    PROJECT_SWAP_BUTTON,
    PROJECT_UNDO_BUTTON,
    QUICK_REDO_BUTTON,
    QUICK_EXPORT_PREFIX,
    QUICK_EDIT_ACTION_SELECT,
    QUICK_EDIT_APPLY_BUTTON,
    QUICK_BATCH_MOVE_BUTTON,
    QUICK_BATCH_SEATS_SELECT,
    QUICK_BATCH_STUDENTS_SELECT,
    QUICK_CANVAS_MODE_SELECT,
    QUICK_LOCK_SEAT_BUTTON,
    QUICK_LOCK_SEAT_SELECT,
    QUICK_LOCK_STUDENT_BUTTON,
    QUICK_LOCK_STUDENT_SELECT,
    QUICK_REPAIR_BUTTON,
    QUICK_SWAP_BUTTON,
    QUICK_UNDO_BUTTON,
    export_prepared_state_key,
)
from seattrellis.web.workflow import (
    WebEditingDraft,
    WebSolveResult,
    apply_web_edit,
    begin_web_editing,
    project_repair_for_web,
    redo_web_edit,
    repair_for_web,
    selected_snapshot,
    undo_web_edit,
)


Translate = Callable[..., str]
RenderError = Callable[[Exception], None]
HistoryPaths = Callable[[], Sequence[str | Path]]
ResultChanged = Callable[[WebSolveResult], None]
PanelWorkspace = Literal["teacher", "quick", "project"]


_CONTROL_KEYS = {
    "repair_button": (QUICK_REPAIR_BUTTON, PROJECT_REPAIR_BUTTON),
    "swap_button": (QUICK_SWAP_BUTTON, PROJECT_SWAP_BUTTON),
    "undo_button": (QUICK_UNDO_BUTTON, PROJECT_UNDO_BUTTON),
    "redo_button": (QUICK_REDO_BUTTON, PROJECT_REDO_BUTTON),
    "edit_action_select": (QUICK_EDIT_ACTION_SELECT, PROJECT_EDIT_ACTION_SELECT),
    "edit_apply_button": (QUICK_EDIT_APPLY_BUTTON, PROJECT_EDIT_APPLY_BUTTON),
    "lock_student_select": (
        QUICK_LOCK_STUDENT_SELECT,
        PROJECT_LOCK_STUDENT_SELECT,
    ),
    "lock_student_button": (
        QUICK_LOCK_STUDENT_BUTTON,
        PROJECT_LOCK_STUDENT_BUTTON,
    ),
    "lock_seat_select": (QUICK_LOCK_SEAT_SELECT, PROJECT_LOCK_SEAT_SELECT),
    "lock_seat_button": (QUICK_LOCK_SEAT_BUTTON, PROJECT_LOCK_SEAT_BUTTON),
    "batch_students_select": (
        QUICK_BATCH_STUDENTS_SELECT,
        PROJECT_BATCH_STUDENTS_SELECT,
    ),
    "batch_seats_select": (QUICK_BATCH_SEATS_SELECT, PROJECT_BATCH_SEATS_SELECT),
    "batch_move_button": (QUICK_BATCH_MOVE_BUTTON, PROJECT_BATCH_MOVE_BUTTON),
    "canvas_mode_select": (QUICK_CANVAS_MODE_SELECT, PROJECT_CANVAS_MODE_SELECT),
    "export_prefix": (QUICK_EXPORT_PREFIX, PROJECT_EXPORT_PREFIX),
}


@dataclass(frozen=True)
class _PanelNamespace:
    """Build session and widget keys for one independent results workspace."""

    workspace: PanelWorkspace

    def widget_key(self, suffix: str) -> str:
        """Return a Streamlit widget key scoped to this workspace."""

        return f"{self.workspace}_{suffix}"

    def state_key(self, suffix: str) -> str:
        """Return a private session-state key scoped to this workspace."""

        return f"_{self.workspace}_{suffix}"

    def control_key(self, name: str) -> str:
        """Return a stable control key while preserving legacy key values."""

        try:
            quick_key, project_key = _CONTROL_KEYS[name]
        except KeyError as exc:  # pragma: no cover - internal programming error.
            raise ValueError(f"Unknown panel control: {name}") from exc
        if self.workspace == "quick":
            return quick_key
        if self.workspace == "project":
            return project_key
        if not quick_key.startswith("quick_"):  # pragma: no cover - constant guard.
            raise ValueError(f"Quick control key has no quick prefix: {quick_key}")
        return f"teacher_{quick_key.removeprefix('quick_')}"

    @property
    def prepared_export_state_key(self) -> str:
        """Return the pending export session key for this workspace."""

        return export_prepared_state_key(self.control_key("export_prefix"))


def _resolve_panel_namespace(
    workspace: str | None = None,
    *,
    project: bool = False,
    project_path: Path | None = None,
) -> _PanelNamespace:
    """Resolve explicit and legacy workspace selectors without Streamlit state."""

    legacy_workspace = "project" if project or project_path is not None else "quick"
    if workspace is None:
        selected = legacy_workspace
    elif workspace not in {"teacher", "quick", "project"}:
        raise ValueError(
            "Unknown panel workspace. Expected 'teacher', 'quick', or 'project'."
        )
    else:
        selected = workspace

    if project and selected != "project":
        raise ValueError("project=True can only be used with the project workspace.")
    if project_path is not None and selected != "project":
        raise ValueError("project_path can only be used with the project workspace.")
    return _PanelNamespace(cast(PanelWorkspace, selected))


def render_repair_panel(
    result: WebSolveResult,
    candidate_id: str,
    *,
    output_dir: Path,
    translate: Translate,
    render_error: RenderError,
    project_path: Path | None = None,
    quick_history_paths: HistoryPaths | None = None,
    workspace: str | None = None,
) -> None:
    """Render lock-aware repair controls without duplicating domain rules."""
    namespace = _resolve_panel_namespace(workspace, project_path=project_path)
    if namespace.workspace == "project" and project_path is None:
        raise ValueError("The project workspace requires project_path.")
    snapshot = selected_snapshot(result, candidate_id)
    student_names = {student.key: student.name for student in snapshot.students}
    student_keys = sorted(student_names)
    seat_ids = sorted(seat.seat_id for seat in snapshot.layout.seats if seat.enabled)

    repair_metadata = snapshot.metadata.get("repair")
    if isinstance(repair_metadata, dict):
        changed = repair_metadata.get("changed_students", [])
        if isinstance(changed, list) and changed:
            labels = [student_names.get(str(key), str(key)) for key in changed]
            st.info(translate("repair_changes", students=", ".join(labels)))
        elif isinstance(changed, list):
            st.info(translate("repair_no_changes"))

    with st.expander(translate("repair_title"), expanded=False):
        st.caption(translate("repair_help"))

        def label_student(key: str) -> str:
            return f"{student_names[key]} ({key})"

        affected_students = st.multiselect(
            translate("affected_students"),
            student_keys,
            format_func=label_student,
            key=namespace.widget_key("repair_affected_students"),
        )
        locked_students = st.multiselect(
            translate("locked_students"),
            student_keys,
            format_func=label_student,
            key=namespace.widget_key("repair_locked_students"),
        )
        locked_seats = st.multiselect(
            translate("locked_seats"),
            seat_ids,
            key=namespace.widget_key("repair_locked_seats"),
        )
        reuse_saved_locks = st.checkbox(
            translate("reuse_saved_locks"),
            value=True,
            key=namespace.widget_key("repair_reuse_saved_locks"),
        )
        settings = st.columns(2)
        backend = settings[0].selectbox(
            translate("repair_backend"),
            ["auto", "fallback", "ortools", "native"],
            key=namespace.widget_key("repair_backend"),
        )
        time_limit_seconds = settings[1].number_input(
            translate("repair_time_limit"),
            min_value=0.1,
            max_value=30.0,
            value=3.0,
            step=0.5,
            key=namespace.widget_key("repair_time_limit"),
        )
        if not st.button(
            translate("run_repair"),
            type="primary",
            key=namespace.control_key("repair_button"),
        ):
            return

        try:
            common = {
                "candidate_id": candidate_id,
                "affected_students": affected_students,
                "locked_students": locked_students,
                "locked_seats": locked_seats,
                "reuse_saved_locks": reuse_saved_locks,
                "time_limit_seconds": float(time_limit_seconds),
                "backend": backend,
            }
            if namespace.workspace == "project":
                assert project_path is not None
                repaired = project_repair_for_web(
                    result,
                    project_path=project_path,
                    output_dir=output_dir,
                    **common,
                )
            else:
                if quick_history_paths is None:
                    raise ValueError("Quick repair requires a history-path provider.")
                repaired = repair_for_web(
                    result,
                    output_dir=output_dir,
                    history_paths=quick_history_paths(),
                    **common,
                )
            st.session_state["result"] = repaired
            st.session_state["artifact_json"] = repaired.artifact_path.read_bytes()
            st.session_state["report_json"] = None
            st.session_state["current_candidate_id"] = "recommended"
            st.session_state[
                namespace.state_key("editing_draft")
            ] = begin_web_editing(repaired)
            st.session_state[
                namespace.state_key("canvas_source_seat")
            ] = None
            st.session_state.pop(
                namespace.prepared_export_state_key,
                None,
            )
            st.success(translate("repair_complete"))
            st.rerun()
        except (
            InputFileError,
            MissingOptionalDependencyError,
            SeatTrellisSolveError,
            ValidationError,
            ValueError,
        ) as exc:
            render_error(exc)


def render_manual_edit_panel(
    result: WebSolveResult,
    candidate_id: str,
    *,
    output_dir: Path,
    translate: Translate,
    render_error: RenderError,
    project: bool = False,
    workspace: str | None = None,
    on_result_changed: ResultChanged | None = None,
) -> None:
    """Render replayable swap, undo, and redo controls.

    ``on_result_changed`` lets a task-oriented page keep its result in an
    isolated state model.  The legacy Quick and Project pages retain their
    established session keys when no callback is supplied.
    """
    namespace = _resolve_panel_namespace(workspace, project=project)
    state_key = namespace.state_key("editing_draft")
    draft = st.session_state.get(state_key)
    if not isinstance(draft, WebEditingDraft) or (
        draft.current_result.artifact_path != result.artifact_path
        or (result.is_candidate_set and draft.candidate_id != candidate_id)
    ):
        draft = begin_web_editing(result, candidate_id)
        st.session_state[state_key] = draft
        st.session_state[namespace.state_key("canvas_source_seat")] = None

    snapshot = selected_snapshot(result, candidate_id)
    student_names = {student.key: student.name for student in snapshot.students}
    seated_students = sorted(
        assignment.student_key for assignment in snapshot.assignments
    )
    assignments_by_student = {
        assignment.student_key: assignment for assignment in snapshot.assignments
    }
    assigned_seats = {assignment.seat_id for assignment in snapshot.assignments}
    all_empty_seats = sorted(
        seat.seat_id
        for seat in snapshot.layout.seats
        if seat.enabled and seat.seat_id not in assigned_seats
    )
    unseated_students = sorted(set(student_names) - set(seated_students))
    lock_state = lock_state_from_snapshot(snapshot)
    locked_students = set(lock_state.locked_students)
    locked_seats = set(lock_state.locked_seats)
    movable_students = [
        student_key
        for student_key in seated_students
        if student_key not in locked_students
        and assignments_by_student[student_key].seat_id not in locked_seats
    ]
    empty_seats = [
        seat_id for seat_id in all_empty_seats if seat_id not in locked_seats
    ]
    _render_manual_edit_status(snapshot, draft, translate)
    st.caption(translate("unseated_count", count=len(unseated_students)))
    st.caption(
        translate(
            "lock_summary",
            students=len(locked_students),
            seats=len(locked_seats),
        )
    )

    with st.expander(translate("manual_edit_title"), expanded=False):
        st.caption(translate("manual_edit_help"))

        def label_student(key: str) -> str:
            return f"{student_names.get(key, key)} ({key})"

        canvas_operation = _render_seat_canvas(
            namespace=namespace,
            snapshot=snapshot,
            locked_students=locked_students,
            locked_seats=locked_seats,
            translate=translate,
        )
        lock_clicked, lock_operation = _render_lock_controls(
            namespace=namespace,
            snapshot=snapshot,
            seated_students=seated_students,
            locked_students=locked_students,
            locked_seats=locked_seats,
            label_student=label_student,
            translate=translate,
        )
        columns = st.columns(2)
        first_student = columns[0].selectbox(
            translate("first_student"),
            movable_students,
            format_func=label_student,
            key=namespace.widget_key("edit_first_student"),
        )
        second_student = columns[1].selectbox(
            translate("second_student"),
            movable_students,
            index=1 if len(movable_students) > 1 else 0,
            format_func=label_student,
            key=namespace.widget_key("edit_second_student"),
        )
        buttons = st.columns(3)
        swap_clicked = buttons[0].button(
            translate("swap_students"),
            type="primary",
            disabled=len(movable_students) < 2 or first_student == second_student,
            key=namespace.control_key("swap_button"),
        )
        undo_clicked = buttons[1].button(
            translate("undo"),
            disabled=not draft.can_undo,
            key=namespace.control_key("undo_button"),
        )
        redo_clicked = buttons[2].button(
            translate("redo"),
            disabled=not draft.can_redo,
            key=namespace.control_key("redo_button"),
        )
        operation_clicked, operation = _render_other_edit_action(
            namespace=namespace,
            seated_students=movable_students,
            unseated_students=unseated_students,
            empty_seats=empty_seats,
            label_student=label_student,
            translate=translate,
        )
        batch_clicked, batch_operation = _render_batch_move(
            namespace=namespace,
            snapshot=snapshot,
            movable_students=movable_students,
            empty_seats=empty_seats,
            label_student=label_student,
            translate=translate,
        )
        try:
            if canvas_operation is not None:
                draft = apply_web_edit(
                    draft,
                    canvas_operation,
                    output_dir=output_dir,
                )
            elif lock_clicked and lock_operation is not None:
                draft = apply_web_edit(
                    draft,
                    lock_operation,
                    output_dir=output_dir,
                )
            elif swap_clicked:
                draft = apply_web_edit(
                    draft,
                    EditingOperation(
                        kind="swap_students",
                        payload={
                            "first_student": first_student,
                            "second_student": second_student,
                        },
                    ),
                    output_dir=output_dir,
                )
            elif undo_clicked:
                draft = undo_web_edit(draft, output_dir=output_dir)
            elif redo_clicked:
                draft = redo_web_edit(draft, output_dir=output_dir)
            elif operation_clicked and operation is not None:
                draft = apply_web_edit(draft, operation, output_dir=output_dir)
            elif batch_clicked and batch_operation is not None:
                draft = apply_web_edit(
                    draft,
                    batch_operation,
                    output_dir=output_dir,
                )
            else:
                return
            st.session_state[state_key] = draft
            if on_result_changed is None:
                st.session_state["result"] = draft.current_result
                st.session_state["artifact_json"] = (
                    draft.current_result.artifact_path.read_bytes()
                )
                st.session_state["report_json"] = None
            else:
                on_result_changed(draft.current_result)
            st.session_state.pop(
                namespace.prepared_export_state_key,
                None,
            )
            st.success(translate("edit_complete"))
            st.rerun()
        except (EditingError, InputFileError, ValidationError, ValueError) as exc:
            render_error(exc)


def _render_seat_canvas(
    *,
    namespace: _PanelNamespace,
    snapshot: SeatingSnapshot,
    locked_students: set[str],
    locked_seats: set[str],
    translate: Translate,
) -> EditingOperation | None:
    st.markdown(f"**{translate('seat_canvas_title')}**")
    st.caption(translate("seat_canvas_help"))
    mode_labels = {
        "move": translate("canvas_mode_move"),
        "lock": translate("canvas_mode_lock"),
    }
    selected_label = st.selectbox(
        translate("seat_canvas_mode"),
        list(mode_labels.values()),
        key=namespace.control_key("canvas_mode_select"),
    )
    mode = next(key for key, label in mode_labels.items() if label == selected_label)
    source_key = namespace.state_key("canvas_source_seat")
    prior_mode_key = namespace.state_key("canvas_mode_value")
    if st.session_state.get(prior_mode_key) != mode:
        st.session_state[prior_mode_key] = mode
        st.session_state[source_key] = None

    seats = list(snapshot.layout.seats)
    if not seats:
        st.info(translate("seat_map_unavailable"))
        return None
    seat_by_position = {(seat.row, seat.col): seat for seat in seats}
    assignments = {
        assignment.seat_id: assignment for assignment in snapshot.assignments
    }
    selected_source = st.session_state.get(source_key)
    source_assignment = assignments.get(selected_source)
    if (
        mode != "move"
        or source_assignment is None
        or selected_source in locked_seats
        or source_assignment.student_key in locked_students
    ):
        selected_source = None
        st.session_state[source_key] = None
    if selected_source is not None and source_assignment is not None:
        st.caption(
            translate(
                "canvas_source_selected",
                seat=selected_source,
                student=source_assignment.student_name,
            )
        )

    clicked_seat: str | None = None
    row_values, col_values = layout_grid_axes(seats)
    for row in row_values:
        columns = st.columns(len(col_values))
        for column_index, col in enumerate(col_values):
            seat = seat_by_position.get((row, col))
            if seat is None:
                columns[column_index].empty()
                continue
            assignment = assignments.get(seat.seat_id)
            locked = (
                seat.seat_id in locked_seats
                or (
                    assignment is not None
                    and assignment.student_key in locked_students
                )
            )
            if not seat.enabled:
                label = f"{seat.seat_id}\n{translate('disabled_seat')}"
            else:
                occupant = (
                    assignment.student_name
                    if assignment is not None
                    else translate("empty_seat")
                )
                marker = "● " if seat.seat_id == selected_source else ""
                lock_marker = " 🔒" if locked else ""
                label = f"{marker}{seat.seat_id}{lock_marker}\n{occupant}"
            disabled = not seat.enabled or (mode == "move" and locked)
            if columns[column_index].button(
                label,
                key=namespace.widget_key(f"canvas_seat_{seat.seat_id}"),
                disabled=disabled,
                use_container_width=True,
            ):
                clicked_seat = seat.seat_id

    if clicked_seat is None:
        return None
    if mode == "lock":
        return EditingOperation(
            kind=(
                "unlock_seat"
                if clicked_seat in locked_seats
                else "lock_seat"
            ),
            payload={"seat_id": clicked_seat},
        )

    target_assignment = assignments.get(clicked_seat)
    if selected_source is None:
        if target_assignment is None:
            st.info(translate("canvas_choose_occupied"))
            return None
        st.session_state[source_key] = clicked_seat
        st.rerun()
    st.session_state[source_key] = None
    if clicked_seat == selected_source:
        st.info(translate("canvas_selection_cleared"))
        return None
    if source_assignment is None:
        return None
    if target_assignment is None:
        return EditingOperation(
            kind="move_student",
            payload={
                "student_key": source_assignment.student_key,
                "seat_id": clicked_seat,
            },
        )
    return EditingOperation(
        kind="swap_students",
        payload={
            "first_student": source_assignment.student_key,
            "second_student": target_assignment.student_key,
        },
    )


def _render_lock_controls(
    *,
    namespace: _PanelNamespace,
    snapshot: SeatingSnapshot,
    seated_students: list[str],
    locked_students: set[str],
    locked_seats: set[str],
    label_student: Callable[[str], str],
    translate: Translate,
) -> tuple[bool, EditingOperation | None]:
    st.markdown(f"**{translate('lock_controls')}**")
    columns = st.columns(2)
    student_key = columns[0].selectbox(
        translate("student_lock_target"),
        seated_students,
        format_func=label_student,
        key=namespace.control_key("lock_student_select"),
    )
    seat_ids = sorted(seat.seat_id for seat in snapshot.layout.seats if seat.enabled)
    occupants = {
        assignment.seat_id: assignment.student_key
        for assignment in snapshot.assignments
    }

    def label_seat(seat_id: str) -> str:
        student_key = occupants.get(seat_id)
        if student_key is None:
            return seat_id
        return f"{seat_id} · {label_student(student_key)}"

    seat_id = columns[1].selectbox(
        translate("seat_lock_target"),
        seat_ids,
        format_func=label_seat,
        key=namespace.control_key("lock_seat_select"),
    )
    buttons = st.columns(2)
    student_is_locked = (
        student_key is not None and student_key in locked_students
    )
    seat_is_locked = seat_id is not None and seat_id in locked_seats
    student_clicked = buttons[0].button(
        translate("unlock_student" if student_is_locked else "lock_student"),
        key=namespace.control_key("lock_student_button"),
    )
    seat_clicked = buttons[1].button(
        translate("unlock_seat" if seat_is_locked else "lock_seat"),
        key=namespace.control_key("lock_seat_button"),
    )
    if student_clicked and student_key is not None:
        return True, EditingOperation(
            kind="unlock_student" if student_is_locked else "lock_student",
            payload={"student_key": student_key},
        )
    if seat_clicked and seat_id is not None:
        return True, EditingOperation(
            kind="unlock_seat" if seat_is_locked else "lock_seat",
            payload={"seat_id": seat_id},
        )
    return False, None


def _render_batch_move(
    *,
    namespace: _PanelNamespace,
    snapshot: SeatingSnapshot,
    movable_students: list[str],
    empty_seats: list[str],
    label_student: Callable[[str], str],
    translate: Translate,
) -> tuple[bool, EditingOperation | None]:
    st.markdown(f"**{translate('batch_move_title')}**")
    st.caption(translate("batch_move_help"))
    selected_students = st.multiselect(
        translate("batch_students"),
        movable_students,
        format_func=label_student,
        key=namespace.control_key("batch_students_select"),
    )
    assignments = {
        assignment.student_key: assignment.seat_id
        for assignment in snapshot.assignments
    }
    target_options = list(empty_seats)
    for student_key in selected_students:
        current_seat = assignments[student_key]
        if current_seat not in target_options:
            target_options.append(current_seat)
    selected_seats = st.multiselect(
        translate("batch_target_seats"),
        target_options,
        key=namespace.control_key("batch_seats_select"),
    )
    counts_match = (
        bool(selected_students)
        and len(selected_students) == len(selected_seats)
    )
    if selected_students or selected_seats:
        if counts_match:
            pairs = ", ".join(
                f"{label_student(student_key)} → {seat_id}"
                for student_key, seat_id in zip(
                    selected_students,
                    selected_seats,
                    strict=True,
                )
            )
            st.caption(translate("batch_pairing", pairs=pairs))
        else:
            st.info(translate("batch_count_mismatch"))
    clicked = st.button(
        translate("apply_batch_move"),
        disabled=not counts_match,
        key=namespace.control_key("batch_move_button"),
    )
    if not clicked or not counts_match:
        return clicked, None
    return True, EditingOperation(
        kind="batch_move",
        payload={
            "moves": [
                {"student_key": student_key, "seat_id": seat_id}
                for student_key, seat_id in zip(
                    selected_students,
                    selected_seats,
                    strict=True,
                )
            ]
        },
    )


def _render_other_edit_action(
    *,
    namespace: _PanelNamespace,
    seated_students: list[str],
    unseated_students: list[str],
    empty_seats: list[str],
    label_student: Callable[[str], str],
    translate: Translate,
) -> tuple[bool, EditingOperation | None]:
    action_labels = {
        "move": translate("action_move"),
        "unseat": translate("action_unseat"),
        "seat": translate("action_seat"),
    }
    selected_label = st.selectbox(
        translate("other_edit_action"),
        list(action_labels.values()),
        key=namespace.control_key("edit_action_select"),
    )
    action = next(
        key for key, label in action_labels.items() if label == selected_label
    )

    student_options = unseated_students if action == "seat" else seated_students
    student_key = st.selectbox(
        translate("student_to_edit"),
        student_options,
        format_func=label_student,
        key=namespace.widget_key(f"edit_action_student_{action}"),
    )
    target_seat: str | None = None
    if action in {"move", "seat"}:
        target_seat = st.selectbox(
            translate("target_empty_seat"),
            empty_seats,
            key=namespace.widget_key(f"edit_action_seat_{action}"),
        )
        if not empty_seats:
            st.info(translate("no_empty_seats"))
    if action == "seat" and not unseated_students:
        st.info(translate("no_unseated_students"))

    disabled = not student_options or (
        action in {"move", "seat"} and not empty_seats
    )
    clicked = st.button(
        translate("apply_edit"),
        disabled=disabled,
        key=namespace.control_key("edit_apply_button"),
    )
    if not clicked or student_key is None:
        return clicked, None
    if action == "unseat":
        return True, EditingOperation(
            kind="unseat_student",
            payload={"student_key": student_key},
        )
    if target_seat is None:
        return True, None
    return True, EditingOperation(
        kind="seat_student" if action == "seat" else "move_student",
        payload={"student_key": student_key, "seat_id": target_seat},
    )


def _render_manual_edit_status(
    snapshot: SeatingSnapshot,
    draft: WebEditingDraft,
    translate: Translate,
) -> None:
    manual_edit = snapshot.metadata.get("manual_edit")
    if not isinstance(manual_edit, dict):
        return
    count = int(manual_edit.get("operation_count", 0))
    st.caption(translate("edit_operations", count=count))
    violation_count = int(manual_edit.get("violation_count", 0))
    if bool(manual_edit.get("hard_constraints_satisfied")):
        st.success(translate("edit_hard_passed"))
        return

    summary = draft.current_result.summary or ""
    _heading, separator, violation_section = summary.partition("Violations:")
    violations = [
        line.removeprefix("- ")
        for line in violation_section.splitlines()
        if separator and line.startswith("- ")
    ]
    st.warning(
        translate(
            "edit_hard_failed",
            count=violation_count,
            items="; ".join(violations[-3:]) or "—",
        )
    )
