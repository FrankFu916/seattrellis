"""Common interface and capability metadata for solver implementations."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Literal, Protocol, runtime_checkable

from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.solver.backend import ConcreteSolverBackend, SolverBackend
from seattrellis.solver.problem import CompiledProblem
from seattrellis.solver.result import SeatingSolution

BackendStrategy = Literal["heuristic", "constraint-programming", "hybrid-validation"]


@dataclass(frozen=True)
class BackendCapabilities:
    """Describe behavior callers may rely on for one solver backend."""

    strategy: BackendStrategy
    supported_hard_rules: frozenset[str]
    supported_soft_rules: frozenset[str]
    supports_history: bool
    supports_candidate_exclusions: bool
    supports_seed: bool
    supports_time_limit: bool
    requires_optional_dependency: bool = False
    experimental: bool = False


@runtime_checkable
class SolverBackendProtocol(Protocol):
    """Interface implemented by every concrete solver backend."""

    name: ConcreteSolverBackend
    capabilities: BackendCapabilities

    def solve(
        self,
        problem: CompiledProblem,
        history: SeatHistory | None,
        pair_history: PairHistory | None,
        seed: int,
        time_limit_seconds: float,
        requested_backend: SolverBackend,
    ) -> SeatingSolution:
        """Solve an already compiled problem."""


BackendSolveFunction = Callable[
    [
        CompiledProblem,
        SeatHistory | None,
        PairHistory | None,
        int,
        float,
        str,
    ],
    SeatingSolution,
]


@dataclass(frozen=True)
class FunctionSolverBackend:
    """Adapt the existing function-based implementations to the protocol."""

    name: ConcreteSolverBackend
    capabilities: BackendCapabilities
    solve_function: BackendSolveFunction

    def solve(
        self,
        problem: CompiledProblem,
        history: SeatHistory | None,
        pair_history: PairHistory | None,
        seed: int,
        time_limit_seconds: float,
        requested_backend: SolverBackend,
    ) -> SeatingSolution:
        return self.solve_function(
            problem,
            history,
            pair_history,
            seed,
            time_limit_seconds,
            requested_backend,
        )
