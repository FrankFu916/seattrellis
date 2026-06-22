from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

try:
    from pydantic.v1 import BaseModel, Field
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student


class SeatAssignment(BaseModel):
    student_key: str
    student_name: str
    seat_id: str


class SeatingSnapshot(BaseModel):
    schema_version: str = "1.0"
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
