"""Verify a seating assignment against the shared hard-rule representation."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Mapping, Sequence

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import MinDistanceRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment
from seattrellis.models.student import Student
from seattrellis.solver.adjacency import graph_distance, seat_distance
from seattrellis.solver.rule_compiler import (
    ResolvedHardRules,
    ResolvedStudentReference,
    resolve_hard_rules,
)

if TYPE_CHECKING:
    from seattrellis.solver.problem import CompiledProblem


@dataclass(frozen=True)
class AssignmentValidationResult:
    """Solver-neutral result returned by hard-constraint verification."""

    satisfied: bool
    checked_rule_count: int
    violations: tuple[str, ...]
    details: Mapping[str, Any]

    @property
    def violation_count(self) -> int:
        return len(self.violations)


def validate_assignment(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
) -> AssignmentValidationResult:
    """Resolve hard rules tolerantly and verify an arbitrary assignment."""

    resolved = resolve_hard_rules(students, layout, rules)
    return validate_resolved_assignment(
        assignments,
        students,
        layout,
        resolved,
    )


def validate_compiled_assignment(
    problem: "CompiledProblem",
    assignments: Sequence[SeatAssignment],
) -> AssignmentValidationResult:
    """Verify a solver result without rebuilding rule references or topology."""

    return validate_resolved_assignment(
        assignments,
        problem.students,
        problem.layout,
        problem.rules_resolved,
    )


def validate_resolved_assignment(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    resolved: ResolvedHardRules,
) -> AssignmentValidationResult:
    """Verify an assignment using a previously resolved hard-rule structure."""

    violations: list[str] = []
    assignment_by_student = {
        assignment.student_key: assignment.seat_id for assignment in assignments
    }
    assigned_students = [assignment.student_key for assignment in assignments]
    assigned_seats = [assignment.seat_id for assignment in assignments]
    expected_students = {student.key for student in students}
    enabled_seats = set(resolved.topology.seat_index_by_id)

    # Keep the historical count contract: three assignment integrity groups,
    # followed by one check for each configured hard rule.
    checked = 3
    if len(assigned_students) != len(set(assigned_students)):
        violations.append("A student is assigned more than once.")
    if len(assigned_seats) != len(set(assigned_seats)):
        violations.append("A seat is assigned more than once.")
    if set(assigned_students) != expected_students:
        violations.append("Assignments do not contain every current student exactly once.")
    unknown_seats = sorted(set(assigned_seats) - enabled_seats)
    if unknown_seats:
        violations.append(
            f"Assignments use unknown or disabled seats: {', '.join(unknown_seats)}."
        )

    for entry in resolved.fixed_seats:
        checked += 1
        student_key = _student_key(students, entry.student)
        if (
            student_key is None
            or assignment_by_student.get(student_key) != entry.rule.seat_id
        ):
            violations.append(
                f"fixed_seats is not satisfied for {entry.rule.student!r}."
            )

    for label, entries, expected_adjacent in (
        ("must_be_adjacent", resolved.must_be_adjacent, True),
        ("cannot_be_adjacent", resolved.cannot_be_adjacent, False),
    ):
        for entry in entries:
            checked += 1
            first_key = _student_key(students, entry.first)
            second_key = _student_key(students, entry.second)
            first_seat = assignment_by_student.get(first_key or "")
            second_seat = assignment_by_student.get(second_key or "")
            adjacent = _seats_are_adjacent(resolved, first_seat, second_seat)
            if adjacent != expected_adjacent:
                violations.append(
                    f"{label} is not satisfied for {entry.rule.students!r}."
                )

    all_seats = {seat.seat_id: seat for seat in layout.seats}
    for entry in resolved.min_distance:
        checked += 1
        rule = entry.rule
        if not isinstance(rule, MinDistanceRule):  # defensive internal guard
            continue
        first_key = _student_key(students, entry.first)
        second_key = _student_key(students, entry.second)
        first_seat_id = assignment_by_student.get(first_key or "")
        second_seat_id = assignment_by_student.get(second_key or "")
        first_seat = all_seats.get(first_seat_id or "")
        second_seat = all_seats.get(second_seat_id or "")
        if first_seat is None or second_seat is None:
            violations.append(
                f"min_distance cannot be evaluated for {rule.students!r}."
            )
            continue

        first_index = resolved.topology.seat_index_by_id.get(first_seat.seat_id)
        second_index = resolved.topology.seat_index_by_id.get(second_seat.seat_id)
        if first_index is not None and second_index is not None:
            distance = resolved.topology.distance(
                first_index,
                second_index,
                rule.metric,
            )
        elif rule.metric == "graph":
            # Preserve diagnostics for invalid drafts that reference a disabled
            # but otherwise known seat.
            distance = graph_distance(layout, first_seat.seat_id, second_seat.seat_id)
        else:
            distance = seat_distance(first_seat, second_seat)
        if distance < rule.distance:
            violations.append(
                f"min_distance is not satisfied for {rule.students!r}."
            )

    details = {
        "student_count": len(students),
        "assignment_count": len(assignments),
        "fixed_seat_count": len(resolved.fixed_seats),
        "must_be_adjacent_count": len(resolved.must_be_adjacent),
        "cannot_be_adjacent_count": len(resolved.cannot_be_adjacent),
        "min_distance_count": len(resolved.min_distance),
    }
    return AssignmentValidationResult(
        satisfied=not violations,
        checked_rule_count=checked,
        violations=tuple(violations),
        details=details,
    )


def _student_key(
    students: Sequence[Student],
    reference: ResolvedStudentReference,
) -> str | None:
    if reference.index is None:
        return None
    return students[reference.index].key


def _seats_are_adjacent(
    resolved: ResolvedHardRules,
    first_seat_id: str | None,
    second_seat_id: str | None,
) -> bool:
    if first_seat_id is None or second_seat_id is None:
        return False
    first_index = resolved.topology.seat_index_by_id.get(first_seat_id)
    second_index = resolved.topology.seat_index_by_id.get(second_seat_id)
    if first_index is None or second_index is None:
        return False
    return resolved.topology.seats_are_adjacent(first_index, second_index)
