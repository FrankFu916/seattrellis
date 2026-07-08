"""Optional Rust native core adapter.

The native core is experimental in v1.4. It is loaded only when explicitly
requested, so normal Python installs keep working without a Rust toolchain.
"""

from __future__ import annotations

from dataclasses import dataclass
from importlib import import_module
from types import ModuleType

from seattrellis.optional import MissingOptionalDependencyError


@dataclass(frozen=True)
class NativeCoreStatus:
    available: bool
    version: str | None = None
    error: str | None = None


def native_core_status() -> NativeCoreStatus:
    """Return whether the optional Rust extension can be imported."""

    try:
        core = _load_native_core()
    except Exception as exc:
        return NativeCoreStatus(available=False, error=str(exc))
    version = getattr(core, "__version__", None)
    return NativeCoreStatus(available=True, version=str(version) if version else None)


def require_native_core() -> ModuleType:
    """Load the Rust extension or raise a user-facing optional dependency error."""

    try:
        return _load_native_core()
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


def _load_native_core() -> ModuleType:
    return import_module("seattrellis_native")
