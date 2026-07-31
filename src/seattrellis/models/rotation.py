"""Versioned multi-period seating plans.

The plan stores the generated snapshots instead of a second assignment format.
That keeps every period compatible with the existing history, editing, export,
and privacy boundaries while adding cross-period summaries for teachers.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.schema import ROTATION_PLAN_SCHEMA_VERSION, require_schema_version


class RotationPeriod(BaseModel):
    """One labelled period in a generated rotation plan."""

    period: int = Field(ge=1)
    label: str
    snapshot: SeatingSnapshot

    @field_validator("label", mode="before")
    def clean_label(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("period label cannot be empty.")
        return text

    model_config = ConfigDict(extra="forbid")


class RotationPlan(BaseModel):
    """A durable, reproducible set of future seating periods."""

    schema_version: str = ROTATION_PLAN_SCHEMA_VERSION
    kind: Literal["rotation_plan"] = "rotation_plan"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    name: str = "SeatTrellis Rotation Plan"
    periods: list[RotationPeriod] = Field(min_length=1)
    base_history_count: int = Field(default=0, ge=0)
    fairness_summary: dict[str, Any] = Field(default_factory=dict)
    pair_repeat_summary: dict[str, Any] = Field(default_factory=dict)
    warnings: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)

    @field_validator("schema_version", mode="before")
    def supported_schema_version(cls, value: object) -> str:
        return require_schema_version(
            value,
            expected=ROTATION_PLAN_SCHEMA_VERSION,
            artifact="rotation plan",
        )

    @field_validator("name", mode="before")
    def clean_name(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("rotation plan name cannot be empty.")
        return text

    @model_validator(mode="after")
    def validate_periods(cls, model: "RotationPlan") -> "RotationPlan":
        numbers = [period.period for period in model.periods]
        expected = list(range(1, len(numbers) + 1))
        if numbers != expected:
            raise ValueError("rotation periods must be numbered consecutively from 1.")
        return model

    @property
    def period_count(self) -> int:
        return len(self.periods)

    @property
    def snapshots(self) -> list[SeatingSnapshot]:
        """Return snapshots in display and history order."""

        return [period.snapshot for period in self.periods]

    model_config = ConfigDict(extra="forbid")
