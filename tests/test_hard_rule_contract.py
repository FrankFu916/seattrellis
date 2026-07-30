from __future__ import annotations

import pytest

from seattrellis.io.validation import validate_loaded_inputs
from seattrellis.models import (
    ClassroomLayout,
    FixedSeatRule,
    HardRules,
    PairRule,
    RuleSet,
    SeatNode,
    Student,
)
from seattrellis.models.snapshot import SeatAssignment
from seattrellis.scoring import evaluate_hard_constraints
from seattrellis.solver import CompiledRules, SeatTrellisSolveError
from seattrellis.solver.assignment_validator import validate_compiled_assignment
from seattrellis.solver.problem import compile_problem


def _layout() -> ClassroomLayout:
    return ClassroomLayout(
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
            SeatNode(seat_id="A3", row=1, col=3),
        ]
    )


def _students() -> list[Student]:
    return [
        Student(student_id="S1", name="Alice"),
        Student(student_id="S2", name="Bob"),
    ]


def test_compiled_problem_retains_shared_resolved_rule_references() -> None:
    rules = RuleSet(
        hard=HardRules(
            fixed_seats=[FixedSeatRule(student="Alice", seat_id="A1")],
            must_be_adjacent=[PairRule(students=("S1", "Bob"))],
        )
    )

    problem = compile_problem(_students(), _layout(), rules)

    assert isinstance(problem.rules_compiled, CompiledRules)
    assert problem.rules_resolved.fixed_seats[0].student.index == 0
    assert problem.rules_resolved.fixed_seats[0].seat.index == 0
    assert problem.rules_resolved.must_be_adjacent[0].first.index == 0
    assert problem.rules_resolved.must_be_adjacent[0].second.index == 1
    assert problem.rules_compiled.fixed_seats == {0: 0}
    assert problem.rules_compiled.must_be_adjacent == [(0, 1)]


def test_compiled_assignment_validator_matches_public_scoring_summary() -> None:
    students = _students()
    layout = _layout()
    rules = RuleSet(
        hard=HardRules(
            fixed_seats=[FixedSeatRule(student="Alice", seat_id="A1")],
            cannot_be_adjacent=[PairRule(students=("S1", "S2"))],
        )
    )
    assignments = [
        SeatAssignment(student_key="S1", student_name="Alice", seat_id="A1"),
        SeatAssignment(student_key="S2", student_name="Bob", seat_id="A2"),
    ]
    problem = compile_problem(students, layout, rules)

    internal = validate_compiled_assignment(problem, assignments)
    public = evaluate_hard_constraints(assignments, students, layout, rules)

    assert internal.satisfied is False
    assert public.satisfied == internal.satisfied
    assert public.checked_rule_count == internal.checked_rule_count
    assert public.violation_count == internal.violation_count
    assert public.violations == list(internal.violations)
    assert public.details == dict(internal.details)


def test_validation_keeps_location_aware_ambiguous_reference_message() -> None:
    students = [
        Student(student_id="S1", name="Shared"),
        Student(student_id="S2", name="Shared"),
    ]
    rules = RuleSet(
        hard=HardRules(
            must_be_adjacent=[PairRule(students=("Shared", "S1"))],
        )
    )

    report = validate_loaded_inputs(students, _layout(), rules)

    assert report.errors == [
        'hard.must_be_adjacent[1] references ambiguous student: "Shared". '
        "Use a unique student_id."
    ]


def test_strict_compiler_keeps_duplicate_fixed_rule_behavior() -> None:
    duplicate = FixedSeatRule(student="S1", seat_id="A1")
    rules = RuleSet(hard=HardRules(fixed_seats=[duplicate, duplicate.copy()]))

    assert validate_loaded_inputs(_students(), _layout(), rules).ok
    with pytest.raises(
        SeatTrellisSolveError,
        match="Student 'S1' is fixed to more than one seat",
    ):
        compile_problem(_students(), _layout(), rules)


def test_unknown_student_cannot_adjacent_remains_non_blocking_in_draft_check() -> None:
    students = _students()
    rules = RuleSet(
        hard=HardRules(
            cannot_be_adjacent=[PairRule(students=("UNKNOWN", "S2"))],
        )
    )
    assignments = [
        SeatAssignment(student_key="S1", student_name="Alice", seat_id="A1"),
        SeatAssignment(student_key="S2", student_name="Bob", seat_id="A2"),
    ]

    summary = evaluate_hard_constraints(assignments, students, _layout(), rules)

    assert summary.satisfied is True
    assert summary.checked_rule_count == 4
    assert summary.violations == []


def test_ambiguous_name_keeps_legacy_last_match_in_draft_check() -> None:
    students = [
        Student(student_id="S1", name="Shared"),
        Student(student_id="S2", name="Shared"),
    ]
    rules = RuleSet(
        hard=HardRules(
            must_be_adjacent=[PairRule(students=("Shared", "S1"))],
        )
    )
    assignments = [
        SeatAssignment(student_key="S1", student_name="Shared", seat_id="A1"),
        SeatAssignment(student_key="S2", student_name="Shared", seat_id="A2"),
    ]

    summary = evaluate_hard_constraints(assignments, students, _layout(), rules)

    assert summary.satisfied is True
    assert summary.violations == []
