from __future__ import annotations

import pytest

from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import read_students
from seattrellis.scoring import evaluate_hard_constraints
from seattrellis.solver import native_backend
from seattrellis.solver import solve_seating
from seattrellis.solver.cp_sat import solve_compiled
from seattrellis.solver.native import (
    EXPECTED_NATIVE_API_VERSION,
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
    assert solution.metrics["native_core"]["role"] == "post-solve-assignment-validator"
    assert solution.metrics["native_core"]["validated_unique_assignment"] is True


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
    assert solution.metrics["native_core"]["validated_unique_assignment"] is True


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
