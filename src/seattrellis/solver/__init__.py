"""Solver APIs."""

from seattrellis.solver.result import SeatingSolution
from seattrellis.solver.backend import SolverBackend, normalize_solver_backend, resolve_solver_backend
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.problem import CompiledProblem, CompiledRules, compile_problem
from seattrellis.solver.protocol import BackendCapabilities, SolverBackendProtocol

__all__ = [
    "BackendCapabilities",
    "CompiledProblem",
    "CompiledRules",
    "SeatTrellisSolveError",
    "SeatingSolution",
    "SolverBackend",
    "SolverBackendProtocol",
    "compile_problem",
    "normalize_solver_backend",
    "resolve_solver_backend",
    "solve_seating",
]


def __getattr__(name: str):
    if name == "solve_seating":
        from seattrellis.solver.cp_sat import solve_seating

        return solve_seating
    raise AttributeError(f"module 'seattrellis.solver' has no attribute {name!r}")
