"""Compiled solver problem shared by backend implementations."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, Sequence

from seattrellis.io.validation import validate_loaded_inputs
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import MinDistanceRule, PairRule, RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.adjacency import (
    SeatEdge,
    build_adjacency_edges,
    graph_distance,
    normalize_edge,
    seat_distance,
)
from seattrellis.solver.errors import SeatTrellisSolveError


@dataclass(frozen=True)
class CompiledRules:
    """Hard rules resolved to student and seat indexes."""

    fixed_seats: dict[int, int]
    must_be_adjacent: list[tuple[int, int]]
    cannot_be_adjacent: list[tuple[int, int]]
    min_distance: list[tuple[int, int, MinDistanceRule]]


@dataclass(frozen=True)
class CompiledProblem:
    """Precomputed seating problem consumed by solver backends."""

    students: list[Student]
    layout: ClassroomLayout
    rules: RuleSet
    seats: list[SeatNode]
    edges: set[SeatEdge]
    rules_compiled: CompiledRules
    excluded_assignments: list[dict[int, int]]
    student_index_by_key: dict[str, int]
    seat_index_by_id: dict[str, int]


def compile_problem(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    *,
    excluded_assignments: Sequence[Mapping[str, str]] | None = None,
    validate: bool = True,
) -> CompiledProblem:
    """Validate and precompute a solver-ready problem."""

    seats = sorted(layout.enabled_seats, key=lambda seat: (seat.row, seat.col, seat.seat_id))
    if not students:
        raise SeatTrellisSolveError("At least one student is required.")
    if len(students) > len(seats):
        raise SeatTrellisSolveError(
            f"Not enough enabled seats: {len(students)} students but only {len(seats)} enabled seats."
        )

    _validate_unique_students(students)
    if validate:
        validation_report = validate_loaded_inputs(students, layout, rules)
        if validation_report.errors:
            raise SeatTrellisSolveError(validation_report.format_failure(title="Input validation failed."))

    edges = build_adjacency_edges(layout)
    student_index_by_key = {student.key: index for index, student in enumerate(students)}
    seat_index_by_id = {seat.seat_id: index for index, seat in enumerate(seats)}
    rules_compiled = _compile_rules(students, seats, layout, rules, edges)
    excluded = _compile_excluded_assignments(students, seats, excluded_assignments or [])
    return CompiledProblem(
        students=students,
        layout=layout,
        rules=rules,
        seats=seats,
        edges=edges,
        rules_compiled=rules_compiled,
        excluded_assignments=excluded,
        student_index_by_key=student_index_by_key,
        seat_index_by_id=seat_index_by_id,
    )


def assignment_is_excluded(
    assignment: Mapping[int, int],
    excluded_assignments: Sequence[Mapping[int, int]],
) -> bool:
    """Return true when an assignment exactly matches a prior candidate."""

    return any(
        len(assignment) == len(excluded)
        and all(assignment.get(student_index) == seat_index for student_index, seat_index in excluded.items())
        for excluded in excluded_assignments
    )


def seat_indexes_adjacent(seats: list[SeatNode], first_index: int, second_index: int, edges: set[SeatEdge]) -> bool:
    """Check adjacency for two seat indexes in a compiled problem."""

    if first_index == second_index:
        return False
    return normalize_edge(seats[first_index].seat_id, seats[second_index].seat_id) in edges


def distance_for_rule(
    layout: ClassroomLayout,
    first_seat: SeatNode,
    second_seat: SeatNode,
    rule: MinDistanceRule,
) -> float:
    """Distance metric used by min-distance hard rules."""

    if rule.metric == "graph":
        return graph_distance(layout, first_seat.seat_id, second_seat.seat_id)
    return seat_distance(first_seat, second_seat)


def _compile_rules(
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    edges: set[SeatEdge],
) -> CompiledRules:
    student_refs = _student_reference_map(students)
    seat_index_by_id = {seat.seat_id: index for index, seat in enumerate(seats)}
    fixed: dict[int, int] = {}
    fixed_seats_seen: dict[int, int] = {}

    for rule in rules.hard.fixed_seats:
        student_index = _resolve_student(rule.student, student_refs)
        if rule.seat_id not in seat_index_by_id:
            raise SeatTrellisSolveError(f"Fixed seat {rule.seat_id!r} is unknown or disabled.")
        seat_index = seat_index_by_id[rule.seat_id]
        if student_index in fixed:
            raise SeatTrellisSolveError(f"Student {rule.student!r} is fixed to more than one seat.")
        if seat_index in fixed_seats_seen:
            raise SeatTrellisSolveError(f"Seat {rule.seat_id!r} is fixed to more than one student.")
        fixed[student_index] = seat_index
        fixed_seats_seen[seat_index] = student_index

    compiled = CompiledRules(
        fixed_seats=fixed,
        must_be_adjacent=[_compile_pair(rule, student_refs) for rule in rules.hard.must_be_adjacent],
        cannot_be_adjacent=[_compile_pair(rule, student_refs) for rule in rules.hard.cannot_be_adjacent],
        min_distance=[
            (*_compile_pair(rule, student_refs), rule)
            for rule in rules.hard.min_distance
        ],
    )
    _validate_compiled_rule_conflicts(compiled, seats, layout, edges)
    return compiled


def _compile_excluded_assignments(
    students: Sequence[Student],
    seats: Sequence[SeatNode],
    excluded_assignments: Sequence[Mapping[str, str]],
) -> list[dict[int, int]]:
    student_index_by_key = {student.key: index for index, student in enumerate(students)}
    seat_index_by_id = {seat.seat_id: index for index, seat in enumerate(seats)}
    compiled: list[dict[int, int]] = []
    for excluded in excluded_assignments:
        if set(excluded) != set(student_index_by_key):
            raise SeatTrellisSolveError(
                "Each excluded assignment must contain every current student exactly once."
            )
        try:
            item = {
                student_index_by_key[student_key]: seat_index_by_id[seat_id]
                for student_key, seat_id in excluded.items()
            }
        except KeyError as exc:
            raise SeatTrellisSolveError(
                f"Excluded assignment references an unknown student or enabled seat: {exc.args[0]!r}."
            ) from exc
        compiled.append(item)
    return compiled


def _validate_compiled_rule_conflicts(
    compiled: CompiledRules,
    seats: list[SeatNode],
    layout: ClassroomLayout,
    edges: set[SeatEdge],
) -> None:
    must_pairs = {_pair_key(first, second) for first, second in compiled.must_be_adjacent}
    cannot_pairs = {_pair_key(first, second) for first, second in compiled.cannot_be_adjacent}
    conflicts = must_pairs & cannot_pairs
    if conflicts:
        raise SeatTrellisSolveError(
            "Conflicting hard rules: the same student pair appears in both must_be_adjacent and cannot_be_adjacent."
        )

    fixed_by_student = compiled.fixed_seats
    for first_index, second_index in compiled.must_be_adjacent:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            first_seat = seats[fixed_by_student[first_index]]
            second_seat = seats[fixed_by_student[second_index]]
            if normalize_edge(first_seat.seat_id, second_seat.seat_id) not in edges:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats do not satisfy a must_be_adjacent rule."
                )
    for first_index, second_index in compiled.cannot_be_adjacent:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            first_seat = seats[fixed_by_student[first_index]]
            second_seat = seats[fixed_by_student[second_index]]
            if normalize_edge(first_seat.seat_id, second_seat.seat_id) in edges:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats violate a cannot_be_adjacent rule."
                )
    for first_index, second_index, rule in compiled.min_distance:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            first_seat = seats[fixed_by_student[first_index]]
            second_seat = seats[fixed_by_student[second_index]]
            if distance_for_rule(layout, first_seat, second_seat, rule) < rule.distance:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats violate a min_distance rule."
                )


def _pair_key(first_index: int, second_index: int) -> tuple[int, int]:
    return (first_index, second_index) if first_index < second_index else (second_index, first_index)


def _compile_pair(rule: PairRule, student_refs: dict[str, int]) -> tuple[int, int]:
    first_index = _resolve_student(rule.students[0], student_refs)
    second_index = _resolve_student(rule.students[1], student_refs)
    if first_index == second_index:
        raise SeatTrellisSolveError("A pair rule must reference two different students.")
    return first_index, second_index


def _student_reference_map(students: list[Student]) -> dict[str, int]:
    refs: dict[str, int] = {}
    for index, student in enumerate(students):
        for value in (student.student_id, student.name):
            if not value:
                continue
            if value in refs and refs[value] != index:
                raise SeatTrellisSolveError(f"Ambiguous student reference: {value!r}.")
            refs[value] = index
    return refs


def _resolve_student(ref: str, refs: dict[str, int]) -> int:
    if ref not in refs:
        raise SeatTrellisSolveError(f"Unknown student reference: {ref!r}.")
    return refs[ref]


def _validate_unique_students(students: list[Student]) -> None:
    keys = [student.key for student in students]
    duplicates = sorted({key for key in keys if keys.count(key) > 1})
    if duplicates:
        raise SeatTrellisSolveError(f"Duplicate student identifiers: {', '.join(duplicates)}")
