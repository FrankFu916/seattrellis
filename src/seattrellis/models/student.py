from __future__ import annotations

from math import isfinite
from typing import Any

try:
    from pydantic.v1 import BaseModel, Field, root_validator, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, root_validator, validator


def _clean_text(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def _normalize_list(value: Any) -> list[str]:
    if value is None:
        return []
    if isinstance(value, list):
        return [str(item).strip() for item in value if str(item).strip()]
    if isinstance(value, tuple) or isinstance(value, set):
        return [str(item).strip() for item in value if str(item).strip()]
    text = str(value).strip()
    if not text:
        return []
    for separator in [";", "；", ",", "，", "、", "|"]:
        text = text.replace(separator, ",")
    return [part.strip() for part in text.split(",") if part.strip()]


class Student(BaseModel):
    """A student record.

    Only one stable identifier is required: either ``student_id`` or ``name``.
    Project-specific columns can be stored in ``attributes``.
    """

    student_id: str | None = None
    name: str | None = None
    gender: str | None = None
    height_cm: float | None = None
    score: float | None = None
    vision: str | float | None = None
    notes: str | None = None
    tags: list[str] = Field(default_factory=list)
    needs: list[str] = Field(default_factory=list)
    attributes: dict[str, Any] = Field(default_factory=dict)

    @validator("student_id", "name", "gender", "notes", pre=True)
    def clean_optional_text(cls, value: Any) -> str | None:
        return _clean_text(value)

    @validator("tags", "needs", pre=True)
    def clean_lists(cls, value: Any) -> list[str]:
        return _normalize_list(value)

    @validator("height_cm", "score")
    def numeric_values_must_be_finite(cls, value: float | None, field: Any) -> float | None:
        if value is None:
            return None
        if not isfinite(float(value)):
            raise ValueError(f"{field.name} must be a finite number.")
        if field.name == "height_cm" and value <= 0:
            raise ValueError("height_cm must be positive.")
        return value

    @root_validator(skip_on_failure=True)
    def require_identifier(cls, values: dict[str, Any]) -> dict[str, Any]:
        if not values.get("student_id") and not values.get("name"):
            raise ValueError("Student requires at least one of student_id or name.")
        return values

    @property
    def key(self) -> str:
        return self.student_id or self.name or ""

    @property
    def display_name(self) -> str:
        return self.name or self.student_id or ""

    def has_need(self, *need_names: str) -> bool:
        needles = {item.lower() for item in need_names}
        values = [str(self.vision).lower()] if self.vision is not None else []
        values.extend(tag.lower() for tag in self.tags)
        values.extend(need.lower() for need in self.needs)
        return any(value in needles for value in values)
