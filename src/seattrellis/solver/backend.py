from __future__ import annotations

import os
from typing import Mapping, Literal

SolverBackend = Literal["auto", "fallback", "ortools", "native"]

SOLVER_BACKENDS: tuple[SolverBackend, ...] = ("auto", "fallback", "ortools", "native")
_TRUE_VALUES = {"1", "true", "TRUE", "yes", "YES"}


def normalize_solver_backend(value: str | None) -> SolverBackend:
    """Normalize a user-facing solver backend selector."""
    if value is None:
        return "auto"
    normalized = str(value).strip().lower()
    if normalized not in SOLVER_BACKENDS:
        supported = ", ".join(SOLVER_BACKENDS)
        raise ValueError(
            f"Unsupported solver backend {value!r}. Supported backends: {supported}."
        )
    return normalized  # type: ignore[return-value]


def resolve_solver_backend(
    requested: str | None = "auto",
    *,
    env: Mapping[str, str] | None = None,
) -> SolverBackend:
    """Resolve the effective backend while keeping legacy environment support."""
    requested_backend = normalize_solver_backend(requested)
    if requested_backend != "auto":
        return requested_backend

    env = os.environ if env is None else env
    configured = env.get("SEATTRELLIS_BACKEND")
    if configured:
        configured_backend = normalize_solver_backend(configured)
        if configured_backend != "auto":
            return configured_backend

    if env.get("SEATTRELLIS_USE_ORTOOLS") in _TRUE_VALUES:
        return "ortools"
    return "fallback"


def solver_backend_environment_summary(env: Mapping[str, str] | None = None) -> dict[str, str]:
    """Return environment values that affect backend selection."""
    env = os.environ if env is None else env
    return {
        "SEATTRELLIS_BACKEND": env.get("SEATTRELLIS_BACKEND", "(not set)"),
        "SEATTRELLIS_USE_ORTOOLS": env.get("SEATTRELLIS_USE_ORTOOLS", "(not set)"),
    }
