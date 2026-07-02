from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

try:
    from pydantic.v1 import BaseModel, Field, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, validator

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.schema import SNAPSHOT_SCHEMA_VERSION, require_schema_version


class SeatAssignment(BaseModel):
    student_key: str
    student_name: str
    seat_id: str


class SeatingSnapshot(BaseModel):
    schema_version: str = SNAPSHOT_SCHEMA_VERSION
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    seed: int = 42
    metadata: dict[str, Any] = Field(default_factory=dict)
    students: list[Student]
    layout: ClassroomLayout
    rules: RuleSet
    assignments: list[SeatAssignment]
    solver_status: str
    objective_value: float | None = None
    metrics: dict[str, Any] = Field(default_factory=dict)

    @validator("schema_version", pre=True)
    def supported_schema_version(cls, value: object) -> str:
        return require_schema_version(
            value,
            expected=SNAPSHOT_SCHEMA_VERSION,
            artifact="snapshot",
        )
