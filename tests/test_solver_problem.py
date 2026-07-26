from __future__ import annotations

from math import isinf

import pytest

from seattrellis.models import (
    ClassroomLayout,
    FixedSeatRule,
    HardRules,
    MinDistanceRule,
    RuleSet,
    SeatNode,
    Student,
)
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.solver import precompute as precompute_module
from seattrellis.solver.problem import (
    compile_problem,
    distance_for_rule,
    seat_indexes_adjacent,
)


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


def test_replacing_candidate_exclusions_reuses_compiled_topology() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    problem = compile_problem(students, _layout(), RuleSet())

    excluded_problem = problem.with_excluded_assignments(
        [{"S1": "A1", "S2": "A2"}]
    )

    assert excluded_problem is not problem
    assert excluded_problem.topology is problem.topology
    assert excluded_problem.rules_compiled is problem.rules_compiled
    assert problem.excluded_assignments == []
    assert excluded_problem.excluded_assignments == [{0: 0, 1: 1}]


def test_compiled_problem_rejects_exclusions_for_disabled_or_unknown_seats() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]

    with pytest.raises(SeatTrellisSolveError, match="unknown student or enabled seat"):
        compile_problem(
            students,
            _layout(),
            RuleSet(),
            excluded_assignments=[{"S1": "A1", "S2": "Z9"}],
        )


def test_compiled_topology_contains_index_adjacency_and_distance_matrices() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]

    problem = compile_problem(students, _layout(), RuleSet())
    topology = problem.topology

    assert topology.adjacent_seat_index_pairs == frozenset({(0, 1)})
    assert topology.adjacency_by_seat_index == (
        frozenset({1}),
        frozenset({0}),
        frozenset(),
    )
    assert seat_indexes_adjacent(problem, 0, 1) is True
    assert seat_indexes_adjacent(problem, 0, 2) is False
    assert topology.euclidean_distance_matrix[0][1] == pytest.approx(1.0)
    assert topology.euclidean_distance_matrix[0][2] == pytest.approx(1.0)
    assert topology.graph_distance_matrix[0][1] == pytest.approx(1.0)
    assert isinf(topology.graph_distance_matrix[0][2])
    assert topology.graph_distance_matrix[2][2] == 0.0


def test_rule_distance_uses_precomputed_topology_without_rebuilding_graph(
    monkeypatch,
) -> None:
    adjacency_builds = 0
    original_build = precompute_module.build_adjacency_edges

    def counting_build(layout: ClassroomLayout):
        nonlocal adjacency_builds
        adjacency_builds += 1
        return original_build(layout)

    monkeypatch.setattr(precompute_module, "build_adjacency_edges", counting_build)
    students = [Student(student_id="S1"), Student(student_id="S2")]
    problem = compile_problem(students, _layout(), RuleSet())
    graph_rule = MinDistanceRule(
        students=("S1", "S2"),
        distance=1.0,
        metric="graph",
    )
    euclidean_rule = MinDistanceRule(
        students=("S1", "S2"),
        distance=1.0,
        metric="euclidean",
    )

    for _ in range(5):
        assert distance_for_rule(problem, 0, 1, graph_rule) == 1.0
        assert distance_for_rule(problem, 0, 2, euclidean_rule) == 1.0

    assert adjacency_builds == 1
