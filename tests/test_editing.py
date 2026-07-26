from __future__ import annotations

import pytest

from seattrellis.editing import EditingError, EditingSession
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import HardRules, PairRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student


def test_swap_students_updates_assignments_and_supports_undo_redo() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    summary = session.swap_students("s1", "s2")

    assert summary.satisfied
    assert _seat_for(session, "s1") == "A2"
    assert _seat_for(session, "s2") == "A1"
    assert len(session.operation_log) == 1

    undo_summary = session.undo()

    assert undo_summary.satisfied
    assert _seat_for(session, "s1") == "A1"
    assert _seat_for(session, "s2") == "A2"

    redo_summary = session.redo()

    assert redo_summary.satisfied
    assert _seat_for(session, "s1") == "A2"
    assert _seat_for(session, "s2") == "A1"


def test_move_student_to_occupied_seat_unseats_previous_student() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    summary = session.move_student("s1", "A2")

    assert not summary.satisfied
    assert "Assignments do not contain every current student exactly once." in summary.violations
    assert _seat_for(session, "s1") == "A2"
    assert "s2" not in session.assignment_by_student()
    assert session.unseated_students() == ["s2"]

    session.undo()

    assert _seat_for(session, "s1") == "A1"
    assert _seat_for(session, "s2") == "A2"
    assert session.unseated_students() == []


def test_unseat_and_seat_student_track_unseated_students() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    unseat_summary = session.unseat_student("s3")

    assert not unseat_summary.satisfied
    assert session.unseated_students() == ["s3"]

    seat_summary = session.seat_student("s3", "B2")

    assert seat_summary.satisfied
    assert session.unseated_students() == []
    assert _seat_for(session, "s3") == "B2"


def test_student_and_seat_locks_prevent_changes() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    session.lock_student("s1")
    with pytest.raises(EditingError, match="Student is locked"):
        session.move_student("s1", "B2")
    with pytest.raises(EditingError, match="Student is locked"):
        session.swap_students("s1", "s2")

    session.undo()
    session.lock_seat("A2")
    with pytest.raises(EditingError, match="Seat is locked"):
        session.move_student("s1", "A2")
    with pytest.raises(EditingError, match="Seat is locked"):
        session.swap_students("s1", "s2")


def test_locking_empty_seat_blocks_later_assignment_until_undo() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    session.lock_seat("B2")
    with pytest.raises(EditingError, match="Seat is locked"):
        session.move_student("s1", "B2")

    session.undo()
    summary = session.move_student("s1", "B2")

    assert summary.satisfied
    assert _seat_for(session, "s1") == "B2"


def test_hard_constraint_summary_reports_manual_conflict() -> None:
    rules = RuleSet(
        hard=HardRules(cannot_be_adjacent=[PairRule(students=("s1", "s2"))])
    )
    session = EditingSession.from_snapshot(
        _snapshot(
            rules=rules,
            assignments=[
                SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
                SeatAssignment(student_key="s2", student_name="周雨", seat_id="B1"),
                SeatAssignment(student_key="s3", student_name="许然", seat_id="A2"),
            ],
        )
    )

    initial_summary = session.hard_constraint_summary()

    assert initial_summary.satisfied

    summary = session.swap_students("s2", "s3")

    assert not summary.satisfied
    assert "cannot_be_adjacent is not satisfied for ('s1', 's2')." in summary.violations


def test_session_rejects_duplicate_assignments() -> None:
    snapshot = _snapshot(
        assignments=[
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A1"),
            SeatAssignment(student_key="s3", student_name="许然", seat_id="B1"),
        ]
    )

    with pytest.raises(EditingError, match="Duplicate seat assignments"):
        EditingSession.from_snapshot(snapshot)


def test_session_rejects_unknown_students_and_disabled_seats() -> None:
    unknown_student = _snapshot(
        assignments=[
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="missing", student_name="未知", seat_id="A2"),
            SeatAssignment(student_key="s3", student_name="许然", seat_id="B1"),
        ]
    )
    disabled_seat = _snapshot(
        assignments=[
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
            SeatAssignment(student_key="s3", student_name="许然", seat_id="X1"),
        ]
    )

    with pytest.raises(EditingError, match="unknown students"):
        EditingSession.from_snapshot(unknown_student)
    with pytest.raises(EditingError, match="unknown or disabled seats"):
        EditingSession.from_snapshot(disabled_seat)


def test_undo_redo_raise_when_stack_is_empty() -> None:
    session = EditingSession.from_snapshot(_snapshot())

    with pytest.raises(EditingError, match="undo"):
        session.undo()
    with pytest.raises(EditingError, match="redo"):
        session.redo()


def _snapshot(
    *,
    assignments: list[SeatAssignment] | None = None,
    rules: RuleSet | None = None,
) -> SeatingSnapshot:
    students = [
        Student(student_id="s1", name="林安"),
        Student(student_id="s2", name="周雨"),
        Student(student_id="s3", name="许然"),
    ]
    layout = ClassroomLayout(
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
            SeatNode(seat_id="B1", row=2, col=1),
            SeatNode(seat_id="B2", row=2, col=2),
            SeatNode(seat_id="X1", row=3, col=1, enabled=False),
        ]
    )
    return SeatingSnapshot(
        seed=7,
        students=students,
        layout=layout,
        rules=rules or RuleSet(seed=7),
        assignments=assignments
        or [
            SeatAssignment(student_key="s1", student_name="林安", seat_id="A1"),
            SeatAssignment(student_key="s2", student_name="周雨", seat_id="A2"),
            SeatAssignment(student_key="s3", student_name="许然", seat_id="B1"),
        ],
        solver_status="manual-test",
    )


def _seat_for(session: EditingSession, student_key: str) -> str:
    return session.assignment_by_student()[student_key].seat_id
