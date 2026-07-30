"""Compiled solver problem shared by backend implementations."""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Mapping, Sequence

from seattrellis.io.validation import _validate_resolved_inputs
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import MinDistanceRule, RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.precompute import CompiledTopology, precompute_topology
from seattrellis.solver.rule_compiler import (
    CompiledRules,
    ResolvedHardRules,
    compile_hard_rules,
    resolve_hard_rules,
)


@dataclass(frozen=True)
class CompiledProblem:
    """Precomputed seating problem consumed by solver backends."""

    students: list[Student]
    layout: ClassroomLayout
    rules: RuleSet
    topology: CompiledTopology
    rules_resolved: ResolvedHardRules
    rules_compiled: CompiledRules
    excluded_assignments: list[dict[int, int]]

    @property
    def seats(self) -> list[SeatNode]:
        """Enabled seats in their stable solver index order."""

        return self.topology.seats

    @property
    def edges(self) -> set[tuple[str, str]]:
        """String-based adjacency edges retained for existing cost helpers."""

        return self.topology.edges

    @property
    def student_index_by_key(self) -> dict[str, int]:
        return self.topology.student_index_by_key

    @property
    def seat_index_by_id(self) -> dict[str, int]:
        return self.topology.seat_index_by_id

    def with_excluded_assignments(
        self,
        excluded_assignments: Sequence[Mapping[str, str]],
    ) -> CompiledProblem:
        """Return a lightweight solve view with new candidate exclusions.

        Student and seat indexes are resolved against the existing topology;
        validation, rule compilation, adjacency and distance matrices are not
        rebuilt.
        """

        return replace(
            self,
            excluded_assignments=_compile_excluded_assignments(
                self.topology,
                excluded_assignments,
            ),
        )


def compile_problem(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    *,
    excluded_assignments: Sequence[Mapping[str, str]] | None = None,
    validate: bool = True,
) -> CompiledProblem:
    """Validate and precompute a solver-ready problem."""

    if not students:
        raise SeatTrellisSolveError("At least one student is required.")
    enabled_seat_count = len(layout.enabled_seats)
    if len(students) > enabled_seat_count:
        raise SeatTrellisSolveError(
            f"Not enough enabled seats: {len(students)} students but only {enabled_seat_count} enabled seats."
        )

    _validate_unique_students(students)
    topology = precompute_topology(students, layout)
    rules_resolved = resolve_hard_rules(
        students,
        layout,
        rules,
        topology=topology,
    )
    if validate:
        validation_report = _validate_resolved_inputs(
            students,
            layout,
            rules,
            rules_resolved,
        )
        if validation_report.errors:
            raise SeatTrellisSolveError(validation_report.format_failure(title="Input validation failed."))

    rules_compiled = compile_hard_rules(rules_resolved)
    excluded = _compile_excluded_assignments(topology, excluded_assignments or [])
    return CompiledProblem(
        students=students,
        layout=layout,
        rules=rules,
        topology=topology,
        rules_resolved=rules_resolved,
        rules_compiled=rules_compiled,
        excluded_assignments=excluded,
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


def seat_indexes_adjacent(
    problem: CompiledProblem,
    first_index: int,
    second_index: int,
) -> bool:
    """Check adjacency for two seat indexes in a compiled problem."""

    return problem.topology.seats_are_adjacent(first_index, second_index)


def distance_for_rule(
    problem: CompiledProblem,
    first_seat_index: int,
    second_seat_index: int,
    rule: MinDistanceRule,
) -> float:
    """Distance metric used by min-distance hard rules."""

    return problem.topology.distance(
        first_seat_index,
        second_seat_index,
        rule.metric,
    )


def _compile_excluded_assignments(
    topology: CompiledTopology,
    excluded_assignments: Sequence[Mapping[str, str]],
) -> list[dict[int, int]]:
    student_index_by_key = topology.student_index_by_key
    seat_index_by_id = topology.seat_index_by_id
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


def _validate_unique_students(students: list[Student]) -> None:
    keys = [student.key for student in students]
    duplicates = sorted({key for key in keys if keys.count(key) > 1})
    if duplicates:
        raise SeatTrellisSolveError(f"Duplicate student identifiers: {', '.join(duplicates)}")
