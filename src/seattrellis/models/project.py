from __future__ import annotations

from pathlib import Path, PureWindowsPath
from typing import Literal

try:
    from pydantic.v1 import BaseModel, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, validator


class SeatTrellisProject(BaseModel):
    """Portable configuration for a local SeatTrellis project workspace."""

    kind: Literal["seattrellis_project"] = "seattrellis_project"
    schema_version: Literal[1] = 1
    name: str = "SeatTrellis Project"
    students: str
    layout: str
    rules: str
    history_dir: str | None = None
    outputs_dir: str = "outputs"
    default_candidates: int = 5
    default_candidate: str = "recommended"
    default_export_format: Literal["html", "excel", "png"] = "html"

    @validator("name", "default_candidate", pre=True)
    def clean_required_text(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("value cannot be empty.")
        return text

    @validator("students", "layout", "rules", "outputs_dir", pre=True)
    def validate_required_relative_path(cls, value: object, field: object) -> str:
        return _validate_relative_path(value, field_name=getattr(field, "name", "path"))

    @validator("history_dir", pre=True)
    def validate_optional_relative_path(cls, value: object) -> str | None:
        if value is None:
            return None
        return _validate_relative_path(value, field_name="history_dir")

    @validator("default_candidates")
    def validate_candidate_count(cls, value: int) -> int:
        if not 1 <= value <= 20:
            raise ValueError("default_candidates must be between 1 and 20.")
        return value

    class Config:
        extra = "forbid"


def _validate_relative_path(value: object, *, field_name: str) -> str:
    text = str(value).strip()
    if not text:
        raise ValueError(f"{field_name} cannot be empty.")
    if Path(text).is_absolute() or PureWindowsPath(text).is_absolute():
        raise ValueError(f"{field_name} must be a relative path.")
    return text
