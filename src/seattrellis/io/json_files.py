from __future__ import annotations

import json
from pathlib import Path
from typing import Any, TypeVar

try:
    from pydantic.v1 import BaseModel, ValidationError
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, ValidationError

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.rules import RuleSet
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


def load_seating_artifact(path: str | Path) -> SeatingSnapshot | CandidateSet:
    data = read_json(path)
    if data.get("kind") == "candidate_set":
        return _parse_model(CandidateSet, data, path, "candidate set")
    return _parse_model(SeatingSnapshot, data, path, "snapshot")


def write_json_model(model: BaseModel, path: str | Path) -> Path:
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8") as file:
        json.dump(_model_to_data(model), file, ensure_ascii=False, indent=2)
        file.write("\n")
    return output


def _parse_model(model_type: type[ModelT], data: dict[str, Any], path: str | Path, label: str) -> ModelT:
    try:
        if hasattr(model_type, "model_validate"):
            return model_type.model_validate(data)  # type: ignore[attr-defined,return-value]
        return model_type.parse_obj(data)
    except ValidationError as exc:
        errors = "; ".join(_format_validation_error(error) for error in exc.errors())
        raise InputFileError(f"Invalid {label}: {Path(path)}\n{errors}") from exc


def _format_validation_error(error: dict[str, Any]) -> str:
    location_items = [item for item in error.get("loc", ()) if item != "__root__"]
    location = ".".join(str(item) for item in location_items)
    message = error.get("msg", "invalid value")
    return f"{location}: {message}" if location else str(message)


def _model_to_data(model: BaseModel) -> dict[str, Any]:
    if hasattr(model, "model_dump"):
        return model.model_dump(mode="json")  # type: ignore[attr-defined,no-any-return]
    return json.loads(model.json())
