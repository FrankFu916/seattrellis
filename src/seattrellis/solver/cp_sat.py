"""Compatibility entrypoint for the seating solver."""

from __future__ import annotations

from math import isfinite
from typing import Mapping, Sequence

from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.backend import normalize_solver_backend, resolve_solver_backend
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.fallback_backend import solve_with_fallback
from seattrellis.solver.native_backend import solve_with_native
from seattrellis.solver.ortools_backend import solve_with_ortools
from seattrellis.solver.problem import compile_problem
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

    requested_backend = normalize_solver_backend(backend)
    effective_backend = resolve_solver_backend(requested_backend)
    if effective_backend == "ortools":
        return solve_with_ortools(
            problem,
            history,
            pair_history,
            seed,
            time_limit_seconds,
            requested_backend,
        )
    if effective_backend == "native":
        return solve_with_native(
            problem,
            history,
            pair_history,
            seed,
            time_limit_seconds,
            requested_backend,
        )
    return solve_with_fallback(
        problem,
        history,
        pair_history,
        seed,
        time_limit_seconds,
        requested_backend,
    )
