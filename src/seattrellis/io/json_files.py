from __future__ import annotations

import json
from pathlib import Path
from typing import Any, TypeVar

try:
    from pydantic.v1 import BaseModel
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatingSnapshot

ModelT = TypeVar("ModelT", bound=BaseModel)


def read_json(path: str | Path) -> dict[str, Any]:
    with Path(path).open("r", encoding="utf-8") as file:
        return json.load(file)


def load_layout(path: str | Path) -> ClassroomLayout:
    return _parse_model(ClassroomLayout, read_json(path))


def load_rules(path: str | Path) -> RuleSet:
    return _parse_model(RuleSet, read_json(path))


def load_snapshot(path: str | Path) -> SeatingSnapshot:
    return _parse_model(SeatingSnapshot, read_json(path))


def write_json_model(model: BaseModel, path: str | Path) -> Path:
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8") as file:
        json.dump(_model_to_data(model), file, ensure_ascii=False, indent=2)
        file.write("\n")
    return output


def _parse_model(model_type: type[ModelT], data: dict[str, Any]) -> ModelT:
    if hasattr(model_type, "model_validate"):
        return model_type.model_validate(data)  # type: ignore[attr-defined,return-value]
    return model_type.parse_obj(data)


def _model_to_data(model: BaseModel) -> dict[str, Any]:
    if hasattr(model, "model_dump"):
        return model.model_dump(mode="json")  # type: ignore[attr-defined,no-any-return]
    return json.loads(model.json())
