"""Stable schema-version constants and validation helpers."""

from __future__ import annotations

from typing import TypeVar


SNAPSHOT_SCHEMA_VERSION = "1.0"
CANDIDATE_SCHEMA_VERSION = "0.2.2"
PROJECT_SCHEMA_VERSION = 1

SchemaVersion = TypeVar("SchemaVersion", str, int)


def require_schema_version(
    value: object,
    *,
    expected: SchemaVersion,
    artifact: str,
) -> SchemaVersion:
    """Return a supported schema version or reject it with a clear message."""
    if type(value) is not type(expected) or value != expected:
        raise ValueError(
            f"Unsupported {artifact} schema_version {value!r}; "
            f"expected {expected!r}."
        )
    return expected
