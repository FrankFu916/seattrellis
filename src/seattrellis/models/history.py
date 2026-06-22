from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Any

try:
    from pydantic.v1 import BaseModel, Field, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, validator


class SeatPositionCategory(str, Enum):
    FRONT = "front"
    BACK = "back"
    MIDDLE = "middle"
    SIDE = "side"
    CORNER = "corner"
    NEAR_WINDOW = "near_window"
    NEAR_DOOR = "near_door"
    NEAR_PLATFORM = "near_platform"
    NEAR_AC = "near_ac"
    UNKNOWN = "unknown"


ROTATION_CATEGORIES: tuple[SeatPositionCategory, ...] = (
    SeatPositionCategory.FRONT,
    SeatPositionCategory.BACK,
    SeatPositionCategory.MIDDLE,
    SeatPositionCategory.SIDE,
    SeatPositionCategory.CORNER,
    SeatPositionCategory.NEAR_WINDOW,
    SeatPositionCategory.NEAR_DOOR,
    SeatPositionCategory.NEAR_PLATFORM,
    SeatPositionCategory.NEAR_AC,
)


class SeatHistoryRecord(BaseModel):
    snapshot_index: int
    snapshot_id: str | None = None
    created_at: datetime | None = None
    seat_id: str
    categories: list[SeatPositionCategory] = Field(default_factory=list)
    unknown_seat: bool = False
    disabled_seat: bool = False

    @validator("categories", pre=True)
    def normalize_categories(cls, value: Any) -> list[str]:
        if value is None:
            return []
        return [
            item.value if isinstance(item, SeatPositionCategory) else str(item)
            for item in value
        ]


class StudentSeatHistory(BaseModel):
    student_key: str
    student_name: str | None = None
    total_assignments: int = 0
    seat_counts: dict[str, int] = Field(default_factory=dict)
    category_counts: dict[str, int] = Field(default_factory=dict)
    records: list[SeatHistoryRecord] = Field(default_factory=list)

    def recent_category_counts(self, lookback: int | None = None) -> dict[str, int]:
        if lookback is not None and lookback <= 0:
            return {}
        records = self.records[-lookback:] if lookback else self.records
        counts: dict[str, int] = {}
        for record in records:
            for category in record.categories:
                key = category.value
                counts[key] = counts.get(key, 0) + 1
        return counts


class SeatHistory(BaseModel):
    history_count: int = 0
    students: dict[str, StudentSeatHistory] = Field(default_factory=dict)
    category_totals: dict[str, int] = Field(default_factory=dict)
    warnings: list[str] = Field(default_factory=list)


class FairnessReport(BaseModel):
    history_count: int
    student_count: int
    category_totals: dict[str, int] = Field(default_factory=dict)
    students: list[StudentSeatHistory] = Field(default_factory=list)
    summary: dict[str, Any] = Field(default_factory=dict)
    warnings: list[str] = Field(default_factory=list)
