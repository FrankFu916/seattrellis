from __future__ import annotations

import json

import pytest

from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import read_students
from seattrellis.scoring import evaluate_hard_constraints
from seattrellis.solver import native_backend
from seattrellis.solver import solve_seating
from seattrellis.solver.cp_sat import solve_compiled
from seattrellis.solver.native import (
    EXPECTED_NATIVE_API_VERSION,
    evaluate_native_problem,
    native_core_status,
    require_native_core,
)
from seattrellis.solver.problem import compile_problem


def _fixture_problem():
    return (
        read_students("tests/fixtures/students.csv"),
        load_layout("tests/fixtures/classroom.json"),
        load_rules("tests/fixtures/rules.json"),
    )


@pytest.mark.parametrize("backend", ["fallback", "ortools"])
def test_backend_contract_for_hard_constraints(backend: str) -> None:
    if backend == "ortools":
        pytest.importorskip("ortools.sat.python.cp_model")
    students, layout, rules = _fixture_problem()

    solution = solve_seating(students, layout, rules, seed=rules.seed, backend=backend)

    _assert_solution_contract(solution, students, layout, rules, backend)


@pytest.mark.parametrize("backend", ["fallback", "ortools"])
def test_compiled_backend_contract_for_hard_constraints(backend: str) -> None:
    if backend == "ortools":
        pytest.importorskip("ortools.sat.python.cp_model")
    students, layout, rules = _fixture_problem()

    solution = solve_compiled(
        compile_problem(students, layout, rules),
        seed=rules.seed,
        backend=backend,
    )

    _assert_solution_contract(solution, students, layout, rules, backend)


def test_native_backend_contract_with_fake_native_core(monkeypatch) -> None:
    students, layout, rules = _fixture_problem()

    class FakeNativeCore:
        __version__ = "test"
        NATIVE_API_VERSION = EXPECTED_NATIVE_API_VERSION

        @staticmethod
        def assignment_is_unique(student_count: int, seat_count: int, assignments: list[tuple[int, int]]) -> bool:
            return (
                len(assignments) == student_count
                and len({student for student, _seat in assignments}) == student_count
                and len({seat for _student, seat in assignments}) == student_count
                and all(0 <= student < student_count and 0 <= seat < seat_count for student, seat in assignments)
            )

        @staticmethod
        def evaluate_problem(request_json: str) -> str:
            request = json.loads(request_json)
            checked = (
                3
                + len(request["fixed_seats"])
                + len(request["must_be_adjacent"])
                + len(request["cannot_be_adjacent"])
                + len(request["min_distance"])
            )
            seat_count = len(request["seat_positions"])
            return json.dumps(
                {
                    "api_version": EXPECTED_NATIVE_API_VERSION,
                    "assignment_unique": True,
                    "hard_constraints_satisfied": True,
                    "checked_rule_count": checked,
                    "violation_count": 0,
                    "violation_codes": [],
                    "graph_distance_matrix": [
                        [0 if first == second else None for second in range(seat_count)]
                        for first in range(seat_count)
                    ],
                    "peer_mixing_gap_sum": 0.0,
                    "peer_mixing_pair_count": 0,
                    "peer_mixing_mean_gap": None,
                }
            )

    monkeypatch.setattr(native_backend, "require_native_core", lambda: FakeNativeCore())

    solution = solve_seating(
        students,
        layout,
        rules,
        seed=rules.seed,
        backend="native",
    )

    _assert_solution_contract(solution, students, layout, rules, "fallback")
    assert solution.metrics["solver"] == "fallback-heuristic+native-validator"
    assert solution.metrics["solver_backend_requested"] == "native"
    assert solution.metrics["solver_validation_backend"] == "native"
    assert solution.metrics["native_core"]["api_version"] == EXPECTED_NATIVE_API_VERSION
    assert solution.metrics["native_core"]["role"] == "post-solve-constraint-validator"
    assert solution.metrics["native_core"]["validated_unique_assignment"] is True
    assert solution.metrics["native_core"]["validated_hard_constraints"] is True


@pytest.mark.skipif(
    not native_core_status().available,
    reason="The optional Rust extension is not installed.",
)
def test_native_backend_contract_with_installed_extension() -> None:
    students, layout, rules = _fixture_problem()
    native_core = require_native_core()

    assert native_core.NATIVE_API_VERSION == EXPECTED_NATIVE_API_VERSION
    assert native_core.seat_distance(
        first_x=1.0,
        first_y=1.0,
        second_x=4.0,
        second_y=5.0,
    ) == 5.0

    solution = solve_seating(
        students,
        layout,
        rules,
        seed=rules.seed,
        backend="native",
    )

    _assert_solution_contract(solution, students, layout, rules, "fallback")
    assert solution.metrics["solver"] == "fallback-heuristic+native-validator"
    assert solution.metrics["solver_backend_requested"] == "native"
    assert solution.metrics["solver_validation_backend"] == "native"
    assert solution.metrics["native_core"]["api_version"] == EXPECTED_NATIVE_API_VERSION
    assert solution.metrics["native_core"]["role"] == "post-solve-constraint-validator"
    assert solution.metrics["native_core"]["validated_unique_assignment"] is True
    assert solution.metrics["native_core"]["validated_hard_constraints"] is True


@pytest.mark.skipif(
    not native_core_status().available,
    reason="The optional Rust extension is not installed.",
)
def test_installed_native_core_matches_python_hard_rule_results() -> None:
    students, layout, rules = _fixture_problem()
    problem = compile_problem(students, layout, rules)
    solution = solve_compiled(problem, seed=rules.seed, backend="fallback")
    assignment_pairs = [
        (
            problem.student_index_by_key[assignment.student_key],
            problem.seat_index_by_id[assignment.seat_id],
        )
        for assignment in solution.assignments
    ]
    request = native_backend._native_evaluation_request(problem, assignment_pairs)
    response = evaluate_native_problem(require_native_core(), request)
    python_result = evaluate_hard_constraints(
        solution.assignments,
        students,
        layout,
        rules,
    )

    assert response["assignment_unique"] is True
    assert response["hard_constraints_satisfied"] == python_result.satisfied
    assert response["checked_rule_count"] == python_result.checked_rule_count
    assert response["violation_count"] == python_result.violation_count
    assert len(response["graph_distance_matrix"]) == len(problem.seats)
    student_by_seat = {
        problem.seat_index_by_id[assignment.seat_id]: problem.student_index_by_key[
            assignment.student_key
        ]
        for assignment in solution.assignments
    }
    expected_gap = 0.0
    expected_pairs = 0
    for first_seat, second_seat in problem.topology.adjacent_seat_index_pairs:
        first_student = student_by_seat.get(first_seat)
        second_student = student_by_seat.get(second_seat)
        if first_student is None or second_student is None:
            continue
        first_score = students[first_student].score
        second_score = students[second_student].score
        if first_score is None or second_score is None:
            continue
        expected_gap += abs(float(first_score) - float(second_score))
        expected_pairs += 1
    assert response["peer_mixing_gap_sum"] == pytest.approx(expected_gap)
    assert response["peer_mixing_pair_count"] == expected_pairs
    serialized_request = json.dumps(request)
    assert all(student.key not in serialized_request for student in students)
    assert all(student.display_name not in serialized_request for student in students)


def _assert_solution_contract(solution, students, layout, rules, expected_backend: str) -> None:
    assignments = solution.assignments
    enabled_seats = {seat.seat_id for seat in layout.enabled_seats}
    student_keys = {student.key for student in students}

    assert solution.metrics["solver_backend_effective"] == expected_backend
    assert {assignment.student_key for assignment in assignments} == student_keys
    assert len({assignment.student_key for assignment in assignments}) == len(students)
    assert len({assignment.seat_id for assignment in assignments}) == len(students)
    assert {assignment.seat_id for assignment in assignments}.issubset(enabled_seats)

    hard = evaluate_hard_constraints(assignments, students, layout, rules)
    assert hard.satisfied, hard.violations
