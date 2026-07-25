"""Optional Rust native core adapter.

The native core is experimental in v1.4. It is loaded only when explicitly
requested, so normal Python installs keep working without a Rust toolchain.
"""

from __future__ import annotations

from dataclasses import dataclass
from importlib import import_module
from types import ModuleType

from seattrellis.optional import MissingOptionalDependencyError

EXPECTED_NATIVE_API_VERSION = 1
REQUIRED_NATIVE_CALLABLES = ("assignment_is_unique", "seat_distance")


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
            "Rust native backend",
            "native",
            detail=(
                "The native backend is an experimental local extension. Build it with:\n"
                "  python -m pip install maturin\n"
                "  python -m maturin develop --manifest-path native/seattrellis_native/Cargo.toml "
                "--features extension-module"
            ),
        ) from exc
    _validate_native_core(core)
    return core


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
