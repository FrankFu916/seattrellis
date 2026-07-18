"""Versioned editor protocol adapter for the Streamlit-independent Web draft."""

from __future__ import annotations

from dataclasses import replace
from pathlib import Path

from seattrellis.editing import (
    EditingOperation,
    EditingSession,
    lock_state_from_snapshot,
)
from seattrellis.editing_protocol import (
    EDITOR_PROTOCOL_VERSION,
    EditorCommandEnvelope,
    EditorHardConstraintState,
    EditorProtocolConflictError,
    EditorSeatState,
    EditorStateEnvelope,
    EditorStudentState,
    operation_to_domain,
)
from seattrellis.web.workflow import (
    WebEditingDraft,
    apply_web_edits,
    redo_web_edit,
    selected_candidate,
    selected_snapshot,
    undo_web_edit,
)


def dispatch_editor_command_for_web(
    draft: WebEditingDraft,
    command: EditorCommandEnvelope,
    *,
    output_dir: str | Path,
) -> WebEditingDraft:
    """Validate and apply a versioned editor command to a Web draft."""
    if command.draft_id != draft.draft_id:
        raise EditorProtocolConflictError(
            "The editor command targets a different draft."
        )
    if command.command_id in draft.applied_command_ids:
        raise EditorProtocolConflictError(
            f"Editor command {command.command_id!r} has already been applied."
        )
    if command.base_revision != draft.revision:
        raise EditorProtocolConflictError(
            "The editor command is stale: "
            f"base revision {command.base_revision}, current revision {draft.revision}."
        )

    if command.action == "apply":
        updated = apply_web_edits(
            draft,
            tuple(operation_to_domain(item) for item in command.operations),
            output_dir=output_dir,
        )
    elif command.action == "undo":
        updated = undo_web_edit(draft, output_dir=output_dir)
    else:
        updated = redo_web_edit(draft, output_dir=output_dir)

    return replace(
        updated,
        applied_command_ids=(*draft.applied_command_ids, command.command_id),
    )


def build_editor_state_for_web(draft: WebEditingDraft) -> EditorStateEnvelope:
    """Build a data-minimized state document for interactive editor clients."""
    snapshot = selected_snapshot(draft.current_result, draft.candidate_id)
    locks = lock_state_from_snapshot(snapshot)
    locked_students = set(locks.locked_students)
    locked_seats = set(locks.locked_seats)
    assignment_by_student = {
        assignment.student_key: assignment for assignment in snapshot.assignments
    }
    assignment_by_seat = {
        assignment.seat_id: assignment for assignment in snapshot.assignments
    }
    hard = EditingSession.from_snapshot(
        snapshot,
        locked_students=locked_students,
        locked_seats=locked_seats,
    ).hard_constraint_summary()
    candidate = selected_candidate(draft.source_result, draft.candidate_id)

    return EditorStateEnvelope(
        kind="seattrellis_editor_state",
        protocol_version=EDITOR_PROTOCOL_VERSION,
        draft_id=draft.draft_id,
        revision=draft.revision,
        candidate_id=candidate.candidate_id if candidate is not None else None,
        undo_depth=_history_depth(draft.operation_batches, draft.operations),
        redo_depth=_history_depth(
            draft.redo_operation_batches,
            draft.redo_operations,
        ),
        students=[
            EditorStudentState(
                student_key=student.key,
                display_name=student.display_name,
                seat_id=(
                    assignment_by_student[student.key].seat_id
                    if student.key in assignment_by_student
                    else None
                ),
                locked=student.key in locked_students,
            )
            for student in snapshot.students
        ],
        seats=[
            EditorSeatState(
                seat_id=seat.seat_id,
                row=seat.row,
                col=seat.col,
                enabled=seat.enabled,
                student_key=(
                    assignment_by_seat[seat.seat_id].student_key
                    if seat.seat_id in assignment_by_seat
                    else None
                ),
                locked=seat.seat_id in locked_seats,
            )
            for seat in snapshot.layout.seats
        ],
        hard_constraints=EditorHardConstraintState(
            satisfied=hard.satisfied,
            checked_rule_count=hard.checked_rule_count,
            violation_count=hard.violation_count,
            violations=list(hard.violations),
        ),
    )


def _history_depth(
    batches: tuple[tuple[EditingOperation, ...], ...],
    operations: tuple[EditingOperation, ...],
) -> int:
    return len(batches) if batches else len(operations)
