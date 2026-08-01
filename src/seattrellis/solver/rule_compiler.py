"""Resolve hard-rule references and compile them into solver indexes.

The tolerant ``ResolvedHardRules`` form is shared by input validation and
assignment verification. Solver backends consume the strict ``CompiledRules``
form, which is produced only after every reference is known and usable.
"""

from __future__ import annotations

from dataclasses import dataclass
from itertools import combinations
from typing import Literal, Sequence

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import FixedSeatRule, GroupRule, MinDistanceRule, PairRule, RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.precompute import CompiledTopology, precompute_topology

ReferenceProblem = Literal["unknown", "ambiguous"]


@dataclass(frozen=True)
class StudentReferenceIndex:
    """Student IDs and names mapped to stable input indexes."""

    by_reference: dict[str, int]
    ambiguous_references: tuple[str, ...]

    def resolve(self, value: str) -> ResolvedStudentReference:
        """Resolve a user-facing student reference without raising."""

        if value in self.ambiguous_references:
            # Keep the historical assignment-check behavior (last matching
            # student wins) while validation and strict compilation still
            # reject the ambiguity via ``problem``.
            return ResolvedStudentReference(
                value=value,
                index=self.by_reference.get(value),
                problem="ambiguous",
            )
        index = self.by_reference.get(value)
        if index is None:
            return ResolvedStudentReference(value=value, index=None, problem="unknown")
        return ResolvedStudentReference(value=value, index=index)


@dataclass(frozen=True)
class ResolvedStudentReference:
    """A student reference and its resolution outcome."""

    value: str
    index: int | None
    problem: ReferenceProblem | None = None


@dataclass(frozen=True)
class ResolvedSeatReference:
    """A fixed-seat target resolved against both all and enabled seats."""

    value: str
    index: int | None
    exists: bool
    enabled: bool


@dataclass(frozen=True)
class ResolvedFixedSeatRule:
    """Fixed-seat rule with stable source location and resolved references."""

    rule: FixedSeatRule
    location: str
    student: ResolvedStudentReference
    seat: ResolvedSeatReference


@dataclass(frozen=True)
class ResolvedPairRule:
    """Pair rule with stable source location and resolved student references."""

    rule: PairRule
    location: str
    first: ResolvedStudentReference
    second: ResolvedStudentReference

    @property
    def references_same_student(self) -> bool:
        return (
            self.first.index is not None
            and self.second.index is not None
            and self.first.index == self.second.index
        )


@dataclass(frozen=True)
class ResolvedHardRules:
    """Tolerant hard-rule representation used before strict compilation."""

    topology: CompiledTopology
    student_references: StudentReferenceIndex
    fixed_seats: tuple[ResolvedFixedSeatRule, ...]
    must_be_adjacent: tuple[ResolvedPairRule, ...]
    cannot_be_adjacent: tuple[ResolvedPairRule, ...]
    min_distance: tuple[ResolvedPairRule, ...]


@dataclass(frozen=True)
class CompiledRules:
    """Hard rules resolved to student and enabled-seat indexes."""

    fixed_seats: dict[int, int]
    must_be_adjacent: list[tuple[int, int]]
    cannot_be_adjacent: list[tuple[int, int]]
    min_distance: list[tuple[int, int, MinDistanceRule]]


def build_student_reference_index(
    students: Sequence[Student],
) -> StudentReferenceIndex:
    """Build the shared ID/name lookup while retaining ambiguity details."""

    references: dict[str, int] = {}
    ambiguous: list[str] = []
    for index, student in enumerate(students):
        for value in (student.student_id, student.name):
            if not value:
                continue
            previous = references.get(value)
            if previous is not None and previous != index:
                if value not in ambiguous:
                    ambiguous.append(value)
            references[value] = index
    return StudentReferenceIndex(
        by_reference=references,
        ambiguous_references=tuple(ambiguous),
    )


def resolve_hard_rules(
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    *,
    topology: CompiledTopology | None = None,
) -> ResolvedHardRules:
    """Resolve every hard rule without rejecting invalid references.

    Keeping resolution tolerant lets validation report several input problems
    at once and lets manual-edit diagnostics describe an invalid draft. Strict
    solver compilation is a separate step.
    """

    topology = topology or precompute_topology(students, layout)
    student_references = build_student_reference_index(students)
    all_seat_ids = {seat.seat_id for seat in layout.seats}

    fixed_seats = tuple(
        ResolvedFixedSeatRule(
            rule=rule,
            location=f"hard.fixed_seats[{index}]",
            student=student_references.resolve(rule.student),
            seat=ResolvedSeatReference(
                value=rule.seat_id,
                index=topology.seat_index_by_id.get(rule.seat_id),
                exists=rule.seat_id in all_seat_ids,
                enabled=rule.seat_id in topology.seat_index_by_id,
            ),
        )
        for index, rule in enumerate(rules.hard.fixed_seats, start=1)
    )
    group_must_be_adjacent, group_cannot_be_adjacent = _expand_group_rules(rules.groups)
    return ResolvedHardRules(
        topology=topology,
        student_references=student_references,
        fixed_seats=fixed_seats,
        must_be_adjacent=_resolve_pair_rules(
            [*rules.hard.must_be_adjacent, *group_must_be_adjacent],
            "hard.must_be_adjacent",
            student_references,
        ),
        cannot_be_adjacent=_resolve_pair_rules(
            [*rules.hard.cannot_be_adjacent, *group_cannot_be_adjacent],
            "hard.cannot_be_adjacent",
            student_references,
        ),
        min_distance=_resolve_pair_rules(
            rules.hard.min_distance,
            "hard.min_distance",
            student_references,
        ),
    )


def _expand_group_rules(groups: Sequence[GroupRule]) -> tuple[list[PairRule], list[PairRule]]:
    """Translate named group separation/togetherness into hard pair rules.

    A group with ``together`` requires every member pair to be adjacent; a
    group with ``separate`` requires every member pair not to be adjacent. This
    deliberately reuses the normal pair-rule compiler so validation, fallback,
    OR-Tools, native validation, and manual editing all share one definition.
    """

    must_be_adjacent: list[PairRule] = []
    cannot_be_adjacent: list[PairRule] = []
    for group in groups:
        members = tuple(dict.fromkeys(group.students))
        pairs = [PairRule(students=pair) for pair in combinations(members, 2)]
        if group.together:
            must_be_adjacent.extend(pairs)
        if group.separate:
            cannot_be_adjacent.extend(pairs)
    return must_be_adjacent, cannot_be_adjacent


def compile_hard_rules(resolved: ResolvedHardRules) -> CompiledRules:
    """Compile a resolved rule set for solver backends.

    Error messages intentionally match the previous in-place compiler because
    callers may surface them directly when validation is bypassed.
    """

    if resolved.student_references.ambiguous_references:
        value = resolved.student_references.ambiguous_references[0]
        raise SeatTrellisSolveError(f"Ambiguous student reference: {value!r}.")

    fixed: dict[int, int] = {}
    fixed_seats_seen: dict[int, int] = {}
    for entry in resolved.fixed_seats:
        student_index = _require_student(entry.student)
        if entry.seat.index is None:
            raise SeatTrellisSolveError(
                f"Fixed seat {entry.rule.seat_id!r} is unknown or disabled."
            )
        seat_index = entry.seat.index
        if student_index in fixed:
            raise SeatTrellisSolveError(
                f"Student {entry.rule.student!r} is fixed to more than one seat."
            )
        if seat_index in fixed_seats_seen:
            raise SeatTrellisSolveError(
                f"Seat {entry.rule.seat_id!r} is fixed to more than one student."
            )
        fixed[student_index] = seat_index
        fixed_seats_seen[seat_index] = student_index

    compiled = CompiledRules(
        fixed_seats=fixed,
        must_be_adjacent=[_compile_pair(entry) for entry in resolved.must_be_adjacent],
        cannot_be_adjacent=[
            _compile_pair(entry) for entry in resolved.cannot_be_adjacent
        ],
        min_distance=[
            (*_compile_pair(entry), entry.rule)
            for entry in resolved.min_distance
            if isinstance(entry.rule, MinDistanceRule)
        ],
    )
    _validate_compiled_rule_conflicts(compiled, resolved.topology)
    return compiled


def _resolve_pair_rules(
    rules: Sequence[PairRule],
    label: str,
    references: StudentReferenceIndex,
) -> tuple[ResolvedPairRule, ...]:
    return tuple(
        ResolvedPairRule(
            rule=rule,
            location=f"{label}[{index}]",
            first=references.resolve(rule.students[0]),
            second=references.resolve(rule.students[1]),
        )
        for index, rule in enumerate(rules, start=1)
    )


def _require_student(reference: ResolvedStudentReference) -> int:
    if reference.index is None:
        raise SeatTrellisSolveError(f"Unknown student reference: {reference.value!r}.")
    return reference.index


def _compile_pair(entry: ResolvedPairRule) -> tuple[int, int]:
    first_index = _require_student(entry.first)
    second_index = _require_student(entry.second)
    if first_index == second_index:
        raise SeatTrellisSolveError("A pair rule must reference two different students.")
    return first_index, second_index


def _validate_compiled_rule_conflicts(
    compiled: CompiledRules,
    topology: CompiledTopology,
) -> None:
    must_pairs = {_pair_key(first, second) for first, second in compiled.must_be_adjacent}
    cannot_pairs = {
        _pair_key(first, second) for first, second in compiled.cannot_be_adjacent
    }
    if must_pairs & cannot_pairs:
        raise SeatTrellisSolveError(
            "Conflicting hard rules: the same student pair appears in both "
            "must_be_adjacent and cannot_be_adjacent."
        )

    fixed_by_student = compiled.fixed_seats
    for first_index, second_index in compiled.must_be_adjacent:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            if not topology.seats_are_adjacent(
                fixed_by_student[first_index],
                fixed_by_student[second_index],
            ):
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats do not satisfy a "
                    "must_be_adjacent rule."
                )
    for first_index, second_index in compiled.cannot_be_adjacent:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            if topology.seats_are_adjacent(
                fixed_by_student[first_index],
                fixed_by_student[second_index],
            ):
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats violate a "
                    "cannot_be_adjacent rule."
                )
    for first_index, second_index, rule in compiled.min_distance:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            distance = topology.distance(
                fixed_by_student[first_index],
                fixed_by_student[second_index],
                rule.metric,
            )
            if distance < rule.distance:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats violate a min_distance rule."
                )


def _pair_key(first_index: int, second_index: int) -> tuple[int, int]:
    if first_index < second_index:
        return first_index, second_index
    return second_index, first_index
