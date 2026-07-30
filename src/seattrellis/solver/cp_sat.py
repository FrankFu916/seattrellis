"""Compatibility entrypoint for the seating solver."""

from __future__ import annotations

from math import isfinite
from typing import Mapping, Sequence

from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.backend import normalize_solver_backend, resolve_solver_backend
from seattrellis.solver.problem import CompiledProblem, compile_problem
from seattrellis.solver.registry import get_solver_backend
from seattrellis.solver.result import SeatingSolution


def solve_seating(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet | None = None,
    *,
    history: SeatHistory | None = None,
    pair_history: PairHistory | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    excluded_assignments: Sequence[Mapping[str, str]] | None = None,
    backend: str = "auto",
) -> SeatingSolution:
    """Solve a seating plan with the selected backend."""

    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")
    rules = rules or RuleSet()
    seed = rules.seed if seed is None else seed
    problem = compile_problem(
        students,
        layout,
        rules,
        excluded_assignments=excluded_assignments or [],
    )

    return solve_compiled(
        problem,
        history=history,
        pair_history=pair_history,
        seed=seed,
        time_limit_seconds=time_limit_seconds,
        backend=backend,
    )


def solve_compiled(
    problem: CompiledProblem,
    *,
    history: SeatHistory | None = None,
    pair_history: PairHistory | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    excluded_assignments: Sequence[Mapping[str, str]] | None = None,
    backend: str = "auto",
) -> SeatingSolution:
    """Solve an existing compiled problem without rebuilding its topology.

    When exclusions are supplied, they replace the problem's current exclusion
    list in a shallow solve view while retaining the compiled topology and
    rules.
    """

    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")
    seed = problem.rules.seed if seed is None else seed
    if excluded_assignments is not None:
        problem = problem.with_excluded_assignments(excluded_assignments)

    requested_backend = normalize_solver_backend(backend)
    effective_backend = resolve_solver_backend(requested_backend)
    solver_backend = get_solver_backend(effective_backend)
    return solver_backend.solve(
        problem,
        history,
        pair_history,
        seed,
        time_limit_seconds,
        requested_backend,
    )
