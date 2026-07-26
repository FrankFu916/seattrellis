from __future__ import annotations

import pytest

from seattrellis.models import ClassroomLayout, FixedSeatRule, HardRules, RuleSet, SeatNode, Student
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.solver.problem import compile_problem


def _layout() -> ClassroomLayout:
    return ClassroomLayout(
        seats=[
            SeatNode(seat_id="B1", row=2, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="Z9", row=9, col=9, enabled=False),
        ]
    )


def test_compiled_problem_sorts_enabled_seats_and_resolves_fixed_rules() -> None:
    students = [Student(student_id="S1", name="A"), Student(student_id="S2", name="B")]
    rules = RuleSet(hard=HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="A2")]))

    problem = compile_problem(students, _layout(), rules)

    assert [seat.seat_id for seat in problem.seats] == ["A1", "A2", "B1"]
    assert problem.student_index_by_key == {"S1": 0, "S2": 1}
    assert problem.seat_index_by_id == {"A1": 0, "A2": 1, "B1": 2}
    assert problem.rules_compiled.fixed_seats == {0: 1}


def test_compiled_problem_resolves_candidate_exclusions() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]

    problem = compile_problem(
        students,
        _layout(),
        RuleSet(),
        excluded_assignments=[{"S1": "A1", "S2": "A2"}],
    )

    assert problem.excluded_assignments == [{0: 0, 1: 1}]


def test_compiled_problem_rejects_exclusions_for_disabled_or_unknown_seats() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]

    with pytest.raises(SeatTrellisSolveError, match="unknown student or enabled seat"):
        compile_problem(
            students,
            _layout(),
            RuleSet(),
            excluded_assignments=[{"S1": "A1", "S2": "Z9"}],
        )
