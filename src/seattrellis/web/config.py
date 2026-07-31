"""Serializable settings for the quick-solve Web workflow."""

from __future__ import annotations

import json
from math import isfinite
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, ValidationError, field_validator

from seattrellis.io.json_files import InputFileError


class WebSessionConfig(BaseModel):
    kind: Literal["seattrellis_web_config"] = "seattrellis_web_config"
    schema_version: Literal[1] = 1
    preset_name: str | None = None
    rules_overlay: dict[str, Any] | None = None
    candidate_count: int = 3
    seed: int | None = None
    time_limit_seconds: float = 3.0

    @field_validator("preset_name", mode="before")
    @classmethod
    def clean_preset_name(cls, value: object) -> str | None:
        if value is None:
            return None
        text = str(value).strip()
        return text or None

    @field_validator("candidate_count")
    @classmethod
    def candidate_count_in_range(cls, value: int) -> int:
        if not 1 <= value <= 20:
            raise ValueError("candidate_count must be between 1 and 20")
        return value

    @field_validator("time_limit_seconds")
    @classmethod
    def valid_time_limit(cls, value: float) -> float:
        if not isfinite(value) or value < 0.1:
            raise ValueError("time_limit_seconds must be a finite number >= 0.1")
        return value

    model_config = ConfigDict(extra="forbid")

    @property
    def contains_student_references(self) -> bool:
        overlay = self.rules_overlay or {}
        hard = overlay.get("hard")
        if isinstance(hard, dict):
            for key in (
                "fixed_seats",
                "must_be_adjacent",
                "cannot_be_adjacent",
                "min_distance",
            ):
                value = hard.get(key)
                if isinstance(value, list) and value:
                    return True
        groups = overlay.get("groups")
        return isinstance(groups, list) and bool(groups)


def load_web_config(data: bytes | str) -> WebSessionConfig:
    try:
        decoded = data.decode("utf-8") if isinstance(data, bytes) else data
    except UnicodeDecodeError as exc:
        raise InputFileError(f"Web config must be UTF-8 JSON: {exc}") from exc
    try:
        payload = json.loads(decoded)
    except json.JSONDecodeError as exc:
        raise InputFileError(
            f"Invalid Web config JSON: line {exc.lineno}, column {exc.colno}: "
            f"{exc.msg}"
        ) from exc
    if not isinstance(payload, dict):
        raise InputFileError("Invalid Web config: top-level value must be an object.")
    try:
        return WebSessionConfig.model_validate(payload)
    except ValidationError as exc:
        details = "; ".join(
            f"{'.'.join(str(part) for part in error['loc'])}: {error['msg']}"
            for error in exc.errors()
        )
        raise InputFileError(f"Invalid Web config: {details}") from exc


def dump_web_config(config: WebSessionConfig) -> bytes:
    payload = config.model_dump(mode="json")
    return (json.dumps(payload, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
