"""Stateful Streamlit panels for manual editing and constrained repair."""

from __future__ import annotations

from pathlib import Path
from typing import Callable, Sequence

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
from seattrellis.web.keys import (
    PROJECT_EDIT_ACTION_SELECT,
    PROJECT_EDIT_APPLY_BUTTON,
    PROJECT_LOCK_SEAT_BUTTON,
    PROJECT_LOCK_SEAT_SELECT,
    PROJECT_LOCK_STUDENT_BUTTON,
    PROJECT_LOCK_STUDENT_SELECT,
    PROJECT_REDO_BUTTON,
    PROJECT_REPAIR_BUTTON,
    PROJECT_SWAP_BUTTON,
    PROJECT_UNDO_BUTTON,
    QUICK_REDO_BUTTON,
    QUICK_EDIT_ACTION_SELECT,
    QUICK_EDIT_APPLY_BUTTON,
    QUICK_LOCK_SEAT_BUTTON,
    QUICK_LOCK_SEAT_SELECT,
    QUICK_LOCK_STUDENT_BUTTON,
    QUICK_LOCK_STUDENT_SELECT,
    QUICK_REPAIR_BUTTON,
    QUICK_SWAP_BUTTON,
    QUICK_UNDO_BUTTON,
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


def render_repair_panel(
    result: WebSolveResult,
    candidate_id: str,
    *,
    output_dir: Path,
    translate: Translate,
    render_error: RenderError,
    project_path: Path | None = None,
    quick_history_paths: HistoryPaths | None = None,
) -> None:
    """Render lock-aware repair controls without duplicating domain rules."""
    snapshot = selected_snapshot(result, candidate_id)
    prefix = "project" if project_path is not None else "quick"
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
            key=f"{prefix}_repair_affected_students",
        )
        locked_students = st.multiselect(
            translate("locked_students"),
            student_keys,
            format_func=label_student,
            key=f"{prefix}_repair_locked_students",
        )
        locked_seats = st.multiselect(
            translate("locked_seats"),
            seat_ids,
            key=f"{prefix}_repair_locked_seats",
        )
        reuse_saved_locks = st.checkbox(
            translate("reuse_saved_locks"),
            value=True,
            key=f"{prefix}_repair_reuse_saved_locks",
        )
        settings = st.columns(2)
        backend = settings[0].selectbox(
            translate("repair_backend"),
            ["auto", "fallback", "ortools", "native"],
            key=f"{prefix}_repair_backend",
        )
        time_limit_seconds = settings[1].number_input(
            translate("repair_time_limit"),
            min_value=0.1,
            max_value=30.0,
            value=3.0,
            step=0.5,
            key=f"{prefix}_repair_time_limit",
        )
        button_key = (
            PROJECT_REPAIR_BUTTON if project_path is not None else QUICK_REPAIR_BUTTON
        )
        if not st.button(
            translate("run_repair"), type="primary", key=button_key
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
            if project_path is not None:
                repaired = project_repair_for_web(
                    result,
                    project_path=project_path,
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
            st.session_state["artifact_json"] = (
                None
                if project_path is not None
                else repaired.artifact_path.read_bytes()
            )
            st.session_state["report_json"] = None
            st.session_state["current_candidate_id"] = "recommended"
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
) -> None:
    """Render replayable swap, undo, and redo controls."""
    prefix = "project" if project else "quick"
    state_key = f"_{prefix}_editing_draft"
    draft = st.session_state.get(state_key)
    if not isinstance(draft, WebEditingDraft) or (
        draft.current_result.artifact_path != result.artifact_path
        or (result.is_candidate_set and draft.candidate_id != candidate_id)
    ):
        draft = begin_web_editing(result, candidate_id)
        st.session_state[state_key] = draft

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

        lock_clicked, lock_operation = _render_lock_controls(
            prefix=prefix,
            project=project,
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
            key=f"{prefix}_edit_first_student",
        )
        second_student = columns[1].selectbox(
            translate("second_student"),
            movable_students,
            index=1 if len(movable_students) > 1 else 0,
            format_func=label_student,
            key=f"{prefix}_edit_second_student",
        )
        buttons = st.columns(3)
        swap_clicked = buttons[0].button(
            translate("swap_students"),
            type="primary",
            disabled=len(movable_students) < 2 or first_student == second_student,
            key=PROJECT_SWAP_BUTTON if project else QUICK_SWAP_BUTTON,
        )
        undo_clicked = buttons[1].button(
            translate("undo"),
            disabled=not draft.can_undo,
            key=PROJECT_UNDO_BUTTON if project else QUICK_UNDO_BUTTON,
        )
        redo_clicked = buttons[2].button(
            translate("redo"),
            disabled=not draft.can_redo,
            key=PROJECT_REDO_BUTTON if project else QUICK_REDO_BUTTON,
        )
        operation_clicked, operation = _render_other_edit_action(
            prefix=prefix,
            project=project,
            seated_students=movable_students,
            unseated_students=unseated_students,
            empty_seats=empty_seats,
            label_student=label_student,
            translate=translate,
        )
        try:
            if lock_clicked and lock_operation is not None:
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
            else:
                return
            st.session_state[state_key] = draft
            st.session_state["result"] = draft.current_result
            st.session_state["artifact_json"] = (
                None if project else draft.current_result.artifact_path.read_bytes()
            )
            st.session_state["report_json"] = None
            st.success(translate("edit_complete"))
            st.rerun()
        except (EditingError, InputFileError, ValidationError, ValueError) as exc:
            render_error(exc)


def _render_lock_controls(
    *,
    prefix: str,
    project: bool,
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
        key=(
            PROJECT_LOCK_STUDENT_SELECT
            if project
            else QUICK_LOCK_STUDENT_SELECT
        ),
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
        key=PROJECT_LOCK_SEAT_SELECT if project else QUICK_LOCK_SEAT_SELECT,
    )
    buttons = st.columns(2)
    student_is_locked = (
        student_key is not None and student_key in locked_students
    )
    seat_is_locked = seat_id is not None and seat_id in locked_seats
    student_clicked = buttons[0].button(
        translate("unlock_student" if student_is_locked else "lock_student"),
        key=(
            PROJECT_LOCK_STUDENT_BUTTON
            if project
            else QUICK_LOCK_STUDENT_BUTTON
        ),
    )
    seat_clicked = buttons[1].button(
        translate("unlock_seat" if seat_is_locked else "lock_seat"),
        key=PROJECT_LOCK_SEAT_BUTTON if project else QUICK_LOCK_SEAT_BUTTON,
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


def _render_other_edit_action(
    *,
    prefix: str,
    project: bool,
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
    action_key = (
        PROJECT_EDIT_ACTION_SELECT if project else QUICK_EDIT_ACTION_SELECT
    )
    selected_label = st.selectbox(
        translate("other_edit_action"),
        list(action_labels.values()),
        key=action_key,
    )
    action = next(
        key for key, label in action_labels.items() if label == selected_label
    )

    student_options = unseated_students if action == "seat" else seated_students
    student_key = st.selectbox(
        translate("student_to_edit"),
        student_options,
        format_func=label_student,
        key=f"{prefix}_edit_action_student_{action}",
    )
    target_seat: str | None = None
    if action in {"move", "seat"}:
        target_seat = st.selectbox(
            translate("target_empty_seat"),
            empty_seats,
            key=f"{prefix}_edit_action_seat_{action}",
        )
        if not empty_seats:
            st.info(translate("no_empty_seats"))
    if action == "seat" and not unseated_students:
        st.info(translate("no_unseated_students"))

    disabled = not student_options or (
        action in {"move", "seat"} and not empty_seats
    )
    apply_key = PROJECT_EDIT_APPLY_BUTTON if project else QUICK_EDIT_APPLY_BUTTON
    clicked = st.button(
        translate("apply_edit"),
        disabled=disabled,
        key=apply_key,
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
