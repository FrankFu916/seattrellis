"""Solver APIs."""

from seattrellis.solver.result import SeatingSolution

__all__ = ["SeatTrellisSolveError", "SeatingSolution", "solve_seating"]


def __getattr__(name: str):
    if name in {"SeatTrellisSolveError", "solve_seating"}:
        from seattrellis.solver.cp_sat import SeatTrellisSolveError, solve_seating

        return {"SeatTrellisSolveError": SeatTrellisSolveError, "solve_seating": solve_seating}[name]
    raise AttributeError(f"module 'seattrellis.solver' has no attribute {name!r}")
