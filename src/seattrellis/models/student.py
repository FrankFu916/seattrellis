from __future__ import annotations

from math import isfinite
from typing import Any

from pydantic import (
    BaseModel,
    Field,
    ValidationInfo,
    field_validator,
    model_validator,
)


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

    @field_validator("student_id", "name", "gender", "notes", mode="before")
    def clean_optional_text(cls, value: Any) -> str | None:
        return _clean_text(value)

    @field_validator("tags", "needs", mode="before")
    def clean_lists(cls, value: Any) -> list[str]:
        return _normalize_list(value)

    @field_validator("height_cm", "score")
    def numeric_values_must_be_finite(
        cls,
        value: float | None,
        info: ValidationInfo,
    ) -> float | None:
        if value is None:
            return None
        if not isfinite(float(value)):
            raise ValueError(f"{info.field_name} must be a finite number.")
        if info.field_name == "height_cm" and value <= 0:
            raise ValueError("height_cm must be positive.")
        return value

    @model_validator(mode="after")
    def require_identifier(cls, model: Any) -> Any:
        if not model.student_id and not model.name:
            raise ValueError("Student requires at least one of student_id or name.")
        return model

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


_VISION_NEED_KEYWORDS = {
    "vision",
    "vision_front",
    "front",
    "poor",
    "low",
    "nearsighted",
    "short_sighted",
    "myopia",
    "视力",
    "近视",
    "靠前",
}


def student_needs_front(student: Student) -> bool:
    """Check whether a student should be seated near the front based on vision data or explicit markers."""
    values = [item.lower() for item in student.tags + student.needs]
    if student.vision is not None:
        values.append(str(student.vision).lower())
        try:
            return float(student.vision) < 1.0
        except (TypeError, ValueError):
            pass
    return bool(set(values) & _VISION_NEED_KEYWORDS)
