from __future__ import annotations

import pytest

from seattrellis.models import ClassroomLayout, RuleSet, SeatNode, Student
from seattrellis.solver import cp_sat
from seattrellis.solver.backend import CONCRETE_SOLVER_BACKENDS
from seattrellis.solver.protocol import SolverBackendProtocol
from seattrellis.solver.problem import compile_problem
from seattrellis.solver.registry import get_solver_backend, registered_solver_backends


def test_registry_contains_every_concrete_backend_in_public_order() -> None:
    backends = registered_solver_backends()

    assert tuple(backend.name for backend in backends) == CONCRETE_SOLVER_BACKENDS
    assert all(isinstance(backend, SolverBackendProtocol) for backend in backends)


def test_registry_exposes_backend_capabilities() -> None:
    fallback = get_solver_backend("fallback")
    ortools = get_solver_backend("ortools")
    native = get_solver_backend("native")

    assert fallback.capabilities.strategy == "heuristic"
    assert fallback.capabilities.requires_optional_dependency is False
    assert ortools.capabilities.strategy == "constraint-programming"
    assert ortools.capabilities.requires_optional_dependency is True
    assert native.capabilities.strategy == "hybrid-validation"
    assert native.capabilities.experimental is True
    assert fallback.capabilities.supported_hard_rules == {
        "fixed_seats",
        "must_be_adjacent",
        "cannot_be_adjacent",
        "min_distance",
    }
    assert "cooling" not in fallback.capabilities.supported_soft_rules


def test_auto_selector_must_be_resolved_before_registry_lookup() -> None:
    with pytest.raises(ValueError, match="must be resolved"):
        get_solver_backend("auto")


def test_solve_entrypoint_dispatches_through_registry(monkeypatch) -> None:
    sentinel = object()
    recorded: dict[str, object] = {}

    class RecordingBackend:
        def solve(self, problem, history, pair_history, seed, time_limit_seconds, requested_backend):
            recorded.update(
                {
                    "problem": problem,
                    "history": history,
                    "pair_history": pair_history,
                    "seed": seed,
                    "time_limit_seconds": time_limit_seconds,
                    "requested_backend": requested_backend,
                }
            )
            return sentinel

    def lookup(name: str):
        recorded["effective_backend"] = name
        return RecordingBackend()

    monkeypatch.setattr(cp_sat, "get_solver_backend", lookup)
    students = [Student(student_id="S1")]
    layout = ClassroomLayout(seats=[SeatNode(seat_id="A1", row=1, col=1)])

    result = cp_sat.solve_seating(
        students,
        layout,
        RuleSet(seed=17),
        backend="fallback",
        time_limit_seconds=2.5,
    )

    assert result is sentinel
    assert recorded["effective_backend"] == "fallback"
    assert recorded["seed"] == 17
    assert recorded["time_limit_seconds"] == 2.5
    assert recorded["requested_backend"] == "fallback"


def test_compiled_entrypoint_preserves_fallback_behavior() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    layout = ClassroomLayout(
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
        ]
    )
    rules = RuleSet(seed=17)

    compatible = cp_sat.solve_seating(
        students,
        layout,
        rules,
        backend="fallback",
        time_limit_seconds=2.5,
    )
    compiled = cp_sat.solve_compiled(
        compile_problem(students, layout, rules),
        backend="fallback",
        time_limit_seconds=2.5,
    )

    assert compiled.assignment_map == compatible.assignment_map
    assert compiled.solver_status == compatible.solver_status
    assert compiled.objective_value == compatible.objective_value
    assert compiled.metrics["solver_backend_requested"] == "fallback"
    assert compiled.metrics["solver_backend_effective"] == "fallback"
