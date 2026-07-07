"""Solver APIs."""

from seattrellis.solver.result import SeatingSolution
from seattrellis.solver.backend import SolverBackend, normalize_solver_backend, resolve_solver_backend

__all__ = [
    "SeatTrellisSolveError",
    "SeatingSolution",
    "SolverBackend",
    "normalize_solver_backend",
    "resolve_solver_backend",
    "solve_seating",
]


def __getattr__(name: str):
    if name in {"SeatTrellisSolveError", "solve_seating"}:
        from seattrellis.solver.cp_sat import SeatTrellisSolveError, solve_seating

        return {"SeatTrellisSolveError": SeatTrellisSolveError, "solve_seating": solve_seating}[name]
    raise AttributeError(f"module 'seattrellis.solver' has no attribute {name!r}")
