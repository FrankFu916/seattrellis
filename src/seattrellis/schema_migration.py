"""Schema migration helpers for durable SeatTrellis JSON artifacts."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from pydantic.v1 import BaseModel, ValidationError
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, ValidationError

from seattrellis.io.json_files import InputFileError, read_json, write_json_model
from seattrellis.models.candidate import CandidateSet, PlanComparisonReport
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.snapshot import SeatingSnapshot


@dataclass(frozen=True)
class SchemaMigrationResult:
    """Result of a schema migration or current-version normalization."""

    artifact: str
    schema_version: str | int
    output_path: Path


def migrate_json_file(
    source: str | Path,
    *,
    output: str | Path | None = None,
    in_place: bool = False,
) -> SchemaMigrationResult:
    """Validate and write a durable JSON artifact using the current schema.

    The current migration table only contains no-op migrations for supported
    schema versions. Unsupported versions still fail with explicit messages,
    preserving the existing compatibility contract while giving future schema
    changes a stable command and function boundary.
    """

    source_path = Path(source)
    output_path = _resolve_output_path(source_path, output=output, in_place=in_place)
    artifact, model = parse_migratable_artifact(read_json(source_path), source_path)
    write_json_model(model, output_path)
    schema_version = getattr(model, "schema_version")
    return SchemaMigrationResult(
        artifact=artifact,
        schema_version=schema_version,
        output_path=output_path,
    )


def parse_migratable_artifact(
    data: dict[str, Any],
    source: str | Path = "<schema migration>",
) -> tuple[str, BaseModel]:
    """Parse a JSON object as one of the durable schema-versioned artifacts."""

    artifact, model_type = _detect_artifact(data, source)
    try:
        if hasattr(model_type, "model_validate"):
            model = model_type.model_validate(data)  # type: ignore[attr-defined]
        else:
            model = model_type.parse_obj(data)
    except ValidationError as exc:
        details = "; ".join(_format_error(error) for error in exc.errors())
        raise InputFileError(f"Cannot migrate invalid {artifact}: {source}\n{details}") from exc
    return artifact, model


def _detect_artifact(
    data: dict[str, Any],
    source: str | Path,
) -> tuple[str, type[BaseModel]]:
    kind = data.get("kind")
    if kind == "candidate_set":
        return "candidate set", CandidateSet
    if kind == "plan_comparison_report":
        return "plan comparison report", PlanComparisonReport
    if kind == "seattrellis_project":
        return "project", SeatTrellisProject
    if {"students", "layout", "rules", "assignments"} <= set(data):
        return "snapshot", SeatingSnapshot
    if {"students", "layout", "rules"} <= set(data):
        return "project", SeatTrellisProject
    raise InputFileError(
        "Cannot identify a migratable SeatTrellis artifact: "
        f"{source}. Expected snapshot, candidate set, plan comparison report, or project JSON."
    )


def _resolve_output_path(
    source: Path,
    *,
    output: str | Path | None,
    in_place: bool,
) -> Path:
    if output is not None and in_place:
        raise ValueError("Use either --output or --in-place, not both.")
    if in_place:
        return source
    if output is None:
        raise ValueError("Schema migration requires --output unless --in-place is set.")
    return Path(output)


def _format_error(error: dict[str, Any]) -> str:
    location_items = [item for item in error.get("loc", ()) if item != "__root__"]
    location = ".".join(str(item) for item in location_items)
    message = error.get("msg", "invalid value")
    return f"{location}: {message}" if location else str(message)
