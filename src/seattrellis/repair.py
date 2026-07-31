"""Compile a manual seating draft into a constrained re-solve request.

This module intentionally contains no file or UI handling. It translates
temporary locks and a local repair scope into solver constraints while keeping
the original project rules unchanged in the resulting snapshot.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable, Sequence, TypeVar

from pydantic import BaseModel

from seattrellis.editing import (
    LOCK_STATE_METADATA_KEY,
    EditingSession,
    lock_state_from_snapshot,
)
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import FixedSeatRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.solver.problem import CompiledProblem, compile_problem


ModelT = TypeVar("ModelT", bound=BaseModel)


@dataclass(frozen=True)
class RepairContext:
    """Solver inputs and trace data derived from one seating draft."""

    solver_layout: ClassroomLayout
    solver_rules: RuleSet
    requested_affected_students: list[str]
    effective_affected_students: list[str]
    closure_added_students: list[str]
    locked_students: list[str]
    locked_seats: list[str]
    mutable_students: list[str]
    fixed_assignments: dict[str, str]
    temporary_fixed_assignments: dict[str, str]
    reserved_empty_seats: list[str]


def compile_repair_context(
    snapshot: SeatingSnapshot,
    *,
    affected_students: Sequence[str] = (),
    locked_students: Sequence[str] = (),
    locked_seats: Sequence[str] = (),
    reuse_saved_locks: bool = True,
) -> RepairContext:
    """Prepare a one-time constrained solve without mutating the draft.

    When a local scope is supplied, it expands by one hop to include students
    connected by a hard pair rule or occupying an adjacent seat. Every
    currently assigned student outside that effective scope is fixed to the
    draft seat. Without a local scope, only explicit or saved locks are fixed
    and the remaining students may be globally re-arranged. Students without a
    current seat are always movable.
    """

    saved_students, saved_seats = (
        _saved_locks(snapshot) if reuse_saved_locks else ([], [])
    )
    combined_students = _normalized_values(
        [*_normalized_values(saved_students), *_normalized_values(locked_students)]
    )
    combined_seats = _normalized_values(
        [*_normalized_values(saved_seats), *_normalized_values(locked_seats)]
    )

    session = EditingSession.from_snapshot(
        snapshot,
        locked_students=combined_students,
        locked_seats=combined_seats,
    )
    known_students = {student.key for student in snapshot.students}
    requested_affected = _normalized_values(affected_students)
    unknown = sorted(set(requested_affected) - known_students)
    if unknown:
        raise ValueError(
            "Affected students are unknown: " + ", ".join(unknown) + "."
        )

    assignments_by_student = session.assignment_by_student()
    assignments_by_seat = session.assignment_by_seat()
    baseline_problem = compile_problem(
        snapshot.students,
        snapshot.layout,
        snapshot.rules,
    )
    baseline_fixed = _baseline_fixed_assignments(snapshot, baseline_problem)
    unseated_locks = sorted(
        student_key
        for student_key in combined_students
        if student_key not in assignments_by_student
    )
    if unseated_locks:
        raise ValueError(
            "Locked students must have a current seat before re-solving: "
            + ", ".join(unseated_locks)
            + "."
        )

    conflicting_students = sorted(set(requested_affected) & set(combined_students))
    if conflicting_students:
        raise ValueError(
            "Affected students cannot also be locked: "
            + ", ".join(conflicting_students)
            + "."
        )

    locked_occupants = {
        assignment.student_key
        for seat_id, assignment in assignments_by_seat.items()
        if seat_id in combined_seats
    }
    conflicting_seat_occupants = sorted(
        set(requested_affected) & locked_occupants
    )
    if conflicting_seat_occupants:
        raise ValueError(
            "Affected students occupy locked seats: "
            + ", ".join(conflicting_seat_occupants)
            + "."
        )

    effective_affected = _expand_local_scope(
        snapshot,
        requested_affected,
        assignments_by_student,
        assignments_by_seat,
        baseline_problem,
    )
    closure_added = sorted(set(effective_affected) - set(requested_affected))
    if requested_affected:
        fixed_students = set(assignments_by_student) - set(effective_affected)
    else:
        fixed_students = set()
    fixed_students.update(combined_students)
    fixed_students.update(locked_occupants)

    requested_fixed_assignments = {
        student_key: assignments_by_student[student_key].seat_id
        for student_key in sorted(fixed_students)
    }
    reserved_empty_seats = sorted(
        seat_id for seat_id in combined_seats if seat_id not in assignments_by_seat
    )
    reserved_fixed_conflicts = sorted(
        (student_key, seat_id)
        for student_key, seat_id in baseline_fixed.items()
        if seat_id in reserved_empty_seats
    )
    if reserved_fixed_conflicts:
        details = ", ".join(
            f"{student_key}->{seat_id}"
            for student_key, seat_id in reserved_fixed_conflicts
        )
        raise ValueError(
            "Cannot reserve an empty locked seat required by existing hard rules: "
            + details
            + "."
        )
    solver_layout = _layout_with_reserved_seats(snapshot.layout, reserved_empty_seats)
    temporary_fixed_assignments = _validated_temporary_fixed_assignments(
        requested_fixed_assignments,
        baseline_fixed,
    )
    fixed_assignments = {**baseline_fixed, **temporary_fixed_assignments}
    solver_rules = _rules_with_temporary_fixed_assignments(
        snapshot,
        temporary_fixed_assignments,
    )
    mutable_students = sorted(known_students - set(fixed_assignments))

    return RepairContext(
        solver_layout=solver_layout,
        solver_rules=solver_rules,
        requested_affected_students=requested_affected,
        effective_affected_students=effective_affected,
        closure_added_students=closure_added,
        locked_students=combined_students,
        locked_seats=combined_seats,
        mutable_students=mutable_students,
        fixed_assignments=fixed_assignments,
        temporary_fixed_assignments=temporary_fixed_assignments,
        reserved_empty_seats=reserved_empty_seats,
    )


def _validated_temporary_fixed_assignments(
    requested_fixed_assignments: dict[str, str],
    baseline_fixed: dict[str, str],
) -> dict[str, str]:
    """Validate anchors against source fixed-seat rules."""

    baseline_by_seat = {
        seat_id: student_key for student_key, seat_id in baseline_fixed.items()
    }
    temporary_fixed: dict[str, str] = {}
    for student_key, seat_id in requested_fixed_assignments.items():
        required_seat = baseline_fixed.get(student_key)
        if required_seat is not None and required_seat != seat_id:
            raise ValueError(
                f"Cannot preserve {student_key} at {seat_id}: existing hard rules "
                f"fix the student to {required_seat}."
            )
        required_student = baseline_by_seat.get(seat_id)
        if required_student is not None and required_student != student_key:
            raise ValueError(
                f"Cannot preserve {student_key} at {seat_id}: existing hard rules "
                f"fix {required_student} to that seat."
            )
        if required_seat is None:
            temporary_fixed[student_key] = seat_id

    return temporary_fixed


def _rules_with_temporary_fixed_assignments(
    snapshot: SeatingSnapshot,
    temporary_fixed_assignments: dict[str, str],
) -> RuleSet:
    """Clone source rules and add already validated one-time anchors."""

    rules = _copy_model(snapshot.rules)
    for student_key, seat_id in temporary_fixed_assignments.items():
        rules.hard.fixed_seats.append(FixedSeatRule(student=student_key, seat_id=seat_id))
    return rules


def _baseline_fixed_assignments(
    snapshot: SeatingSnapshot,
    baseline_problem: CompiledProblem,
) -> dict[str, str]:
    """Resolve the source rules to stable student keys and seat identifiers."""

    return {
        snapshot.students[student_index].key: baseline_problem.seats[seat_index].seat_id
        for student_index, seat_index in baseline_problem.rules_compiled.fixed_seats.items()
    }


def _expand_local_scope(
    snapshot: SeatingSnapshot,
    requested_affected: Sequence[str],
    assignments_by_student: dict[str, SeatAssignment],
    assignments_by_seat: dict[str, SeatAssignment],
    baseline_problem: CompiledProblem,
) -> list[str]:
    """Expand a requested repair scope by one hard-rule or seat-adjacency hop."""

    if not requested_affected:
        return []

    requested = set(requested_affected)
    expanded = set(requested)
    requested_indexes = {
        baseline_problem.student_index_by_key[student_key]
        for student_key in requested
    }
    compiled = baseline_problem.rules_compiled
    pair_indexes = [
        *compiled.must_be_adjacent,
        *compiled.cannot_be_adjacent,
        *[
            (first_index, second_index)
            for first_index, second_index, _rule in compiled.min_distance
        ],
    ]
    for first_index, second_index in pair_indexes:
        if first_index in requested_indexes:
            expanded.add(snapshot.students[second_index].key)
        if second_index in requested_indexes:
            expanded.add(snapshot.students[first_index].key)

    adjacent_seats: dict[str, set[str]] = {}
    for first_seat, second_seat in baseline_problem.edges:
        adjacent_seats.setdefault(first_seat, set()).add(second_seat)
        adjacent_seats.setdefault(second_seat, set()).add(first_seat)
    for student_key in requested:
        assignment = assignments_by_student.get(student_key)
        if assignment is None:
            continue
        for adjacent_seat in adjacent_seats.get(assignment.seat_id, set()):
            occupant = assignments_by_seat.get(adjacent_seat)
            if occupant is not None:
                expanded.add(occupant.student_key)

    return sorted(expanded)


def format_repair_solve_failure(
    context: RepairContext,
    error: Exception,
) -> str:
    """Add lock and local-scope actions to a solver failure diagnostic."""

    lines = [
        "Repair could not find a feasible seating plan with the current locks "
        "and local scope.",
        "",
        "Active repair restrictions:",
        f"- locked students: {_format_values(context.locked_students)}",
        f"- locked seats: {_format_values(context.locked_seats)}",
        (
            "- requested affected students: "
            f"{_format_values(context.requested_affected_students)}"
        ),
        (
            "- effective one-hop scope: "
            f"{_format_values(context.effective_affected_students)}"
        ),
        f"- fixed assignments: {len(context.fixed_assignments)}",
        f"- reserved empty seats: {_format_values(context.reserved_empty_seats)}",
        "",
        "Solver diagnostic:",
        str(error).strip() or error.__class__.__name__,
        "",
        "Try one of these changes:",
    ]
    if context.locked_students:
        lines.append(
            "- unlock one or more students and retry: "
            + _format_values(context.locked_students)
            + ";"
        )
    if context.locked_seats:
        lines.append(
            "- unlock one or more seats and retry: "
            + _format_values(context.locked_seats)
            + ";"
        )
    if context.requested_affected_students:
        lines.append(
            "- add more --affected-student values, or omit the option for a "
            "global repair;"
        )
    if context.locked_students or context.locked_seats:
        lines.append(
            "- if these locks came from a saved draft, review them or retry "
            "with --ignore-saved-locks;"
        )
    lines.append(
        "- if the solver timed out, increase --time-limit before relaxing "
        "hard constraints."
    )
    return "\n".join(lines)


def _format_values(values: Sequence[str], *, limit: int = 8) -> str:
    if not values:
        return "none"
    shown = ", ".join(values[:limit])
    if len(values) > limit:
        return f"{shown}, ... ({len(values)} total)"
    return shown


def _layout_with_reserved_seats(
    layout: ClassroomLayout,
    reserved_seat_ids: Iterable[str],
) -> ClassroomLayout:
    """Temporarily disable empty locked seats for one solver invocation."""

    reserved = set(reserved_seat_ids)
    if not reserved:
        return layout
    seats = [
        _copy_seat(seat, enabled=False) if seat.seat_id in reserved else _copy_seat(seat)
        for seat in layout.seats
    ]
    adjacency = _copy_model(layout.adjacency)
    adjacency.custom_edges = [
        edge
        for edge in adjacency.custom_edges
        if edge[0] not in reserved and edge[1] not in reserved
    ]
    return ClassroomLayout(
        layout_id=layout.layout_id,
        name=layout.name,
        seats=seats,
        adjacency=adjacency,
        metadata=dict(layout.metadata),
    )


def _copy_seat(seat: SeatNode, *, enabled: bool | None = None) -> SeatNode:
    if enabled is None:
        return _copy_model(seat)
    if hasattr(seat, "model_copy"):
        return seat.model_copy(  # type: ignore[attr-defined,return-value]
            update={"enabled": enabled}
        )
    return seat.copy(update={"enabled": enabled})


def _copy_model(model: ModelT) -> ModelT:
    return model.model_copy(deep=True)


def _saved_locks(snapshot: SeatingSnapshot) -> tuple[list[str], list[str]]:
    """Read the formal state first, then tolerate older draft metadata."""

    formal = lock_state_from_snapshot(snapshot)
    if LOCK_STATE_METADATA_KEY in snapshot.metadata:
        return list(formal.locked_students), list(formal.locked_seats)
    for key in ("lock_state", "manual_edit", "repair"):
        stored = snapshot.metadata.get(key)
        if not isinstance(stored, dict):
            continue
        return (
            _normalized_values(stored.get("locked_students", ())),
            _normalized_values(stored.get("locked_seats", ())),
        )
    return (
        [],
        [],
    )


def _normalized_values(values: object) -> list[str]:
    if isinstance(values, str):
        candidates = [values]
    elif isinstance(values, Iterable):
        candidates = values
    else:
        return []
    normalized: list[str] = []
    for value in candidates:
        text = str(value).strip()
        if text and text not in normalized:
            normalized.append(text)
    return sorted(normalized)
