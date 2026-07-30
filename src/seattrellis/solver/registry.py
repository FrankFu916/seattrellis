"""Registry of concrete solver backend implementations."""

from __future__ import annotations

from types import MappingProxyType
from typing import Mapping

from seattrellis.solver.backend import (
    CONCRETE_SOLVER_BACKENDS,
    ConcreteSolverBackend,
    normalize_solver_backend,
)
from seattrellis.solver.fallback_backend import solve_with_fallback
from seattrellis.solver.native_backend import solve_with_native
from seattrellis.solver.ortools_backend import solve_with_ortools
from seattrellis.solver.protocol import (
    BackendCapabilities,
    BackendStrategy,
    FunctionSolverBackend,
    SolverBackendProtocol,
)

_HARD_RULES = frozenset(
    {
        "fixed_seats",
        "must_be_adjacent",
        "cannot_be_adjacent",
        "min_distance",
    }
)
_SOFT_RULES = frozenset(
    {
        "vision_front",
        "height_back",
        "randomize",
        "score_balance",
        "fair_rotation",
        "avoid_recent_neighbors",
    }
)
_FALLBACK_SOFT_RULES = _SOFT_RULES | frozenset(
    {"score_position", "score_distribution", "mentor_pairing"}
)


def _capabilities(
    strategy: BackendStrategy,
    *,
    supported_soft_rules: frozenset[str] = _SOFT_RULES,
    requires_optional_dependency: bool = False,
    experimental: bool = False,
) -> BackendCapabilities:
    return BackendCapabilities(
        strategy=strategy,
        supported_hard_rules=_HARD_RULES,
        supported_soft_rules=supported_soft_rules,
        supports_history=True,
        supports_candidate_exclusions=True,
        supports_seed=True,
        supports_time_limit=True,
        requires_optional_dependency=requires_optional_dependency,
        experimental=experimental,
    )


_BACKENDS: Mapping[ConcreteSolverBackend, SolverBackendProtocol] = MappingProxyType(
    {
        "fallback": FunctionSolverBackend(
            name="fallback",
            capabilities=_capabilities(
                "heuristic",
                supported_soft_rules=_FALLBACK_SOFT_RULES,
            ),
            solve_function=solve_with_fallback,
        ),
        "ortools": FunctionSolverBackend(
            name="ortools",
            capabilities=_capabilities(
                "constraint-programming",
                requires_optional_dependency=True,
            ),
            solve_function=solve_with_ortools,
        ),
        "native": FunctionSolverBackend(
            name="native",
            capabilities=_capabilities(
                "hybrid-validation",
                supported_soft_rules=_FALLBACK_SOFT_RULES,
                requires_optional_dependency=True,
                experimental=True,
            ),
            solve_function=solve_with_native,
        ),
    }
)


def registered_solver_backends() -> tuple[SolverBackendProtocol, ...]:
    """Return concrete backends in the stable user-facing order."""

    return tuple(_BACKENDS[name] for name in CONCRETE_SOLVER_BACKENDS)


def get_solver_backend(name: str) -> SolverBackendProtocol:
    """Return a concrete backend after user-facing name validation."""

    normalized = normalize_solver_backend(name)
    if normalized == "auto":
        raise ValueError("The auto backend selector must be resolved before registry lookup.")
    return _BACKENDS[normalized]
