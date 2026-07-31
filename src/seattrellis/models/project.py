from __future__ import annotations

from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Literal

from pydantic import BaseModel, ConfigDict, ValidationInfo, field_validator

from seattrellis.schema import PROJECT_SCHEMA_VERSION, require_schema_version


class SeatTrellisProject(BaseModel):
    """Portable configuration for a local SeatTrellis project workspace."""

    kind: Literal["seattrellis_project"] = "seattrellis_project"
    schema_version: int = PROJECT_SCHEMA_VERSION
    name: str = "SeatTrellis Project"
    students: str
    layout: str
    rules: str
    history_dir: str | None = None
    outputs_dir: str = "outputs"
    default_candidates: int = 5
    default_candidate: str = "recommended"
    default_export_format: Literal["html", "excel", "png"] = "html"

    @field_validator("schema_version", mode="before")
    def supported_schema_version(cls, value: object) -> int:
        return require_schema_version(
            value,
            expected=PROJECT_SCHEMA_VERSION,
            artifact="project",
        )

    @field_validator("name", "default_candidate", mode="before")
    def clean_required_text(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("value cannot be empty.")
        return text

    @field_validator("students", "layout", "rules", "outputs_dir", mode="before")
    def validate_required_relative_path(cls, value: object, info: ValidationInfo) -> str:
        return _validate_relative_path(value, field_name=info.field_name)

    @field_validator("history_dir", mode="before")
    def validate_optional_relative_path(cls, value: object) -> str | None:
        if value is None:
            return None
        return _validate_relative_path(value, field_name="history_dir")

    @field_validator("default_candidates")
    def validate_candidate_count(cls, value: int) -> int:
        if not 1 <= value <= 20:
            raise ValueError("default_candidates must be between 1 and 20.")
        return value

    model_config = ConfigDict(extra="forbid")


def _validate_relative_path(value: object, *, field_name: str) -> str:
    text = str(value).strip()
    if not text:
        raise ValueError(f"{field_name} cannot be empty.")
    if (
        Path(text).is_absolute()
        or PurePosixPath(text).is_absolute()
        or PureWindowsPath(text).is_absolute()
    ):
        raise ValueError(f"{field_name} must be a relative path.")
    return text
