from __future__ import annotations

from typing import Any

try:
    from pydantic.v1 import BaseModel, Field
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student


class SeatingSolution(BaseModel):
    assignments: list[SeatAssignment]
    solver_status: str
    objective_value: float | None = None
    metrics: dict[str, Any] = Field(default_factory=dict)

    @property
    def assignment_map(self) -> dict[str, str]:
        return {assignment.student_key: assignment.seat_id for assignment in self.assignments}

    def to_snapshot(
        self,
        *,
        students: list[Student],
        layout: ClassroomLayout,
        rules: RuleSet,
        seed: int,
    ) -> SeatingSnapshot:
        return SeatingSnapshot(
            seed=seed,
            students=students,
            layout=layout,
            rules=rules,
            assignments=self.assignments,
            solver_status=self.solver_status,
            objective_value=self.objective_value,
            metrics=self.metrics,
        )
