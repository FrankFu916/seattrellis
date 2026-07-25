"""Experimental Rust native backend adapter."""

from __future__ import annotations

from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.fallback_backend import solve_with_fallback
from seattrellis.solver.native import require_native_core
from seattrellis.solver.problem import CompiledProblem
from seattrellis.solver.result import SeatingSolution


def solve_with_native(
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    """Run the experimental native backend.

    The v1.4 native backend keeps Python fallback search in place and uses the
    Rust extension for a narrow structural assignment check. This proves the
    call boundary without making Rust a production solver yet.
    """

    native_core = require_native_core()
    solution = solve_with_fallback(
        problem,
        history,
        pair_history,
        seed,
        time_limit_seconds,
        requested_backend,
    )
    assignment_pairs = [
        (
            problem.student_index_by_key[assignment.student_key],
            problem.seat_index_by_id[assignment.seat_id],
        )
        for assignment in solution.assignments
    ]
    if not native_core.assignment_is_unique(
        len(problem.students),
        len(problem.seats),
        assignment_pairs,
    ):
        raise SeatTrellisSolveError(
            "Native assignment-structure validation failed: "
            "the assignment is not complete and unique."
        )
    solution.metrics.update(
        {
            "solver": "fallback-heuristic+native-validator",
            "solver_backend_effective": "fallback",
            "solver_validation_backend": "native",
            "native_core": {
                "module": "seattrellis_native",
                "version": getattr(native_core, "__version__", None),
                "api_version": getattr(native_core, "NATIVE_API_VERSION", None),
                "role": "post-solve-assignment-validator",
                "validated_unique_assignment": True,
            },
        }
    )
    return solution
