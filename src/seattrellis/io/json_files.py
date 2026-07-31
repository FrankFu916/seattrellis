from __future__ import annotations

import json
from pathlib import Path
from typing import Any, TypeVar

from pydantic import BaseModel, ValidationError

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.candidate import CandidateSet, PlanComparisonReport
from seattrellis.models.rules import RuleSet
from seattrellis.models.rotation import RotationPlan
from seattrellis.models.snapshot import SeatingSnapshot

ModelT = TypeVar("ModelT", bound=BaseModel)


class InputFileError(ValueError):
    """Raised when an input file cannot be read or validated."""


def read_json(path: str | Path) -> dict[str, Any]:
    source = Path(path)
    if not source.exists():
        raise InputFileError(f"Input file not found: {source}")
    try:
        with source.open("r", encoding="utf-8") as file:
            data = json.load(file)
    except json.JSONDecodeError as exc:
        raise InputFileError(
            f"Invalid JSON in {source}: line {exc.lineno}, column {exc.colno}: {exc.msg}"
        ) from exc
    except UnicodeDecodeError as exc:
        raise InputFileError(f"Invalid UTF-8 text in {source}: {exc}") from exc
    except OSError as exc:
        raise InputFileError(f"Could not read input file {source}: {exc}") from exc
    if not isinstance(data, dict):
        raise InputFileError(f"Invalid JSON in {source}: top-level value must be an object.")
    return data


def load_layout(path: str | Path) -> ClassroomLayout:
    return _parse_model(ClassroomLayout, read_json(path), path, "classroom layout")


def load_rules(path: str | Path) -> RuleSet:
    return parse_rules_data(read_json(path), path)


def parse_rules_data(data: dict[str, Any], source: str | Path = "<generated rules>") -> RuleSet:
    return _parse_model(RuleSet, data, source, "rules file")


def load_snapshot(path: str | Path) -> SeatingSnapshot:
    return _parse_model(SeatingSnapshot, read_json(path), path, "snapshot")


def load_candidate_set(path: str | Path) -> CandidateSet:
    return _parse_model(CandidateSet, read_json(path), path, "candidate set")


def load_plan_comparison_report(path: str | Path) -> PlanComparisonReport:
    return _parse_model(PlanComparisonReport, read_json(path), path, "plan comparison report")


def load_rotation_plan(path: str | Path) -> RotationPlan:
    """Load a versioned multi-period rotation plan."""

    return _parse_model(RotationPlan, read_json(path), path, "rotation plan")


def load_seating_artifact(path: str | Path) -> SeatingSnapshot | CandidateSet:
    data = read_json(path)
    if data.get("kind") == "candidate_set":
        return _parse_model(CandidateSet, data, path, "candidate set")
    return _parse_model(SeatingSnapshot, data, path, "snapshot")


def write_json_model(model: BaseModel, path: str | Path) -> Path:
    output = Path(path)
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("w", encoding="utf-8") as file:
            json.dump(_model_to_data(model), file, ensure_ascii=False, indent=2)
            file.write("\n")
    except OSError as exc:
        raise InputFileError(
            f"Could not write JSON file {output}: {exc}"
        ) from exc
    return output


def _parse_model(model_type: type[ModelT], data: dict[str, Any], path: str | Path, label: str) -> ModelT:
    try:
        return model_type.model_validate(data)
    except ValidationError as exc:
        errors = "; ".join(_format_validation_error(error) for error in exc.errors())
        raise InputFileError(f"Invalid {label}: {Path(path)}\n{errors}") from exc


def _format_validation_error(error: dict[str, Any]) -> str:
    location_items = [item for item in error.get("loc", ()) if item != "__root__"]
    location = ".".join(str(item) for item in location_items)
    message = error.get("msg", "invalid value")
    return f"{location}: {message}" if location else str(message)


def _model_to_data(model: BaseModel) -> dict[str, Any]:
    return model.model_dump(mode="json")
