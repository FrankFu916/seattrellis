"""Optional Rust native core adapter.

The native core is experimental in v1.4. It is loaded only when explicitly
requested, so normal Python installs keep working without a Rust toolchain.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from importlib import import_module
from math import isfinite
from types import ModuleType
from typing import Any

from seattrellis.optional import MissingOptionalDependencyError

EXPECTED_NATIVE_API_VERSION = 2
REQUIRED_NATIVE_CALLABLES = (
    "assignment_is_unique",
    "seat_distance",
    "evaluate_problem",
)


@dataclass(frozen=True)
class NativeCoreStatus:
    available: bool
    version: str | None = None
    api_version: int | None = None
    error: str | None = None


def native_core_status() -> NativeCoreStatus:
    """Return whether the optional Rust extension can be imported."""

    try:
        core = _load_native_core()
        api_version = _validate_native_core(core)
    except Exception as exc:
        return NativeCoreStatus(available=False, error=str(exc))
    version = getattr(core, "__version__", None)
    return NativeCoreStatus(
        available=True,
        version=str(version) if version else None,
        api_version=api_version,
    )


def require_native_core() -> ModuleType:
    """Load the Rust extension or raise a user-facing optional dependency error."""

    try:
        core = _load_native_core()
    except Exception as exc:
        raise MissingOptionalDependencyError(
            "Rust native validation mode",
            None,
            detail=(
                "The experimental seattrellis_native extension is not bundled "
                "with SeatTrellis and is not installed by an optional extra. "
                "Use --backend fallback, or install the solver extra and use "
                "--backend ortools. To evaluate the native validator, build it "
                "from a matching source checkout. See "
                "https://frankfu916.github.io/seattrellis/native-core/."
            ),
        ) from exc
    _validate_native_core(core)
    return core


def evaluate_native_problem(
    core: ModuleType,
    request: dict[str, Any],
) -> dict[str, Any]:
    """Evaluate one coarse, versioned DTO through the optional Rust core."""

    payload = json.dumps(request, ensure_ascii=True, separators=(",", ":"))
    response_text = core.evaluate_problem(payload)
    try:
        response = json.loads(response_text)
    except (TypeError, json.JSONDecodeError) as exc:
        raise ValueError("Native evaluation returned invalid JSON.") from exc
    if not isinstance(response, dict):
        raise ValueError("Native evaluation response must be a JSON object.")
    api_version = response.get("api_version")
    if api_version != EXPECTED_NATIVE_API_VERSION:
        raise ValueError(
            "Incompatible native evaluation response: "
            f"expected api_version {EXPECTED_NATIVE_API_VERSION}, found "
            f"{api_version!r}."
        )
    required = {
        "assignment_unique",
        "hard_constraints_satisfied",
        "checked_rule_count",
        "violation_count",
        "violation_codes",
        "graph_distance_matrix",
        "peer_mixing_gap_sum",
        "peer_mixing_pair_count",
        "peer_mixing_mean_gap",
    }
    missing = sorted(required - set(response))
    if missing:
        raise ValueError(
            "Native evaluation response is missing field(s): " + ", ".join(missing)
        )
    for field_name in ("assignment_unique", "hard_constraints_satisfied"):
        if not isinstance(response[field_name], bool):
            raise ValueError(
                f"Native evaluation field {field_name!r} must be a boolean."
            )
    for field_name in (
        "checked_rule_count",
        "violation_count",
        "peer_mixing_pair_count",
    ):
        value = response[field_name]
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            raise ValueError(
                f"Native evaluation field {field_name!r} must be a non-negative integer."
            )
    if not isinstance(response["violation_codes"], list) or not all(
        isinstance(code, str) for code in response["violation_codes"]
    ):
        raise ValueError("Native evaluation violation_codes must be a string list.")
    if not isinstance(response["graph_distance_matrix"], list):
        raise ValueError("Native evaluation graph_distance_matrix must be a list.")
    for field_name in ("peer_mixing_gap_sum", "peer_mixing_mean_gap"):
        value = response[field_name]
        if value is not None and (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not isfinite(float(value))
        ):
            raise ValueError(
                f"Native evaluation field {field_name!r} must be finite or null."
            )
    return response


def _load_native_core() -> ModuleType:
    return import_module("seattrellis_native")


def _validate_native_core(core: ModuleType) -> int:
    """Validate the versioned API shared by the Python and Rust packages."""

    api_version = getattr(core, "NATIVE_API_VERSION", None)
    if (
        not isinstance(api_version, int)
        or isinstance(api_version, bool)
        or api_version != EXPECTED_NATIVE_API_VERSION
    ):
        found = "missing" if api_version is None else repr(api_version)
        raise ValueError(
            "Incompatible SeatTrellis native core API: "
            f"expected {EXPECTED_NATIVE_API_VERSION}, found {found}. "
            "Rebuild or reinstall the native extension for this SeatTrellis version."
        )
    missing_callables = [
        name for name in REQUIRED_NATIVE_CALLABLES if not callable(getattr(core, name, None))
    ]
    if missing_callables:
        missing = ", ".join(missing_callables)
        raise ValueError(
            "Incompatible SeatTrellis native core API: "
            f"missing required callable(s): {missing}. "
            "Rebuild or reinstall the native extension for this SeatTrellis version."
        )
    return api_version
