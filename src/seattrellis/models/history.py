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


class NeighborRelationType(str, Enum):
    DESK_MATE = "desk_mate"
    HORIZONTAL = "horizontal"
    VERTICAL = "vertical"
    DIAGONAL = "diagonal"
    ADJACENT_ANY = "adjacent_any"
    WITHIN_DISTANCE = "within_distance"


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

NEIGHBOR_RELATION_TYPES: tuple[NeighborRelationType, ...] = (
    NeighborRelationType.DESK_MATE,
    NeighborRelationType.HORIZONTAL,
    NeighborRelationType.VERTICAL,
    NeighborRelationType.DIAGONAL,
    NeighborRelationType.ADJACENT_ANY,
    NeighborRelationType.WITHIN_DISTANCE,
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


class PairHistoryRecord(BaseModel):
    snapshot_index: int
    snapshot_id: str | None = None
    created_at: datetime | None = None
    first_seat_id: str
    second_seat_id: str
    relations: list[NeighborRelationType] = Field(default_factory=list)
    row_delta: int = 0
    col_delta: int = 0
    chebyshev_distance: int = 0
    manhattan_distance: int = 0
    first_seat_disabled: bool = False
    second_seat_disabled: bool = False

    @validator("relations", pre=True)
    def normalize_relations(cls, value: Any) -> list[str]:
        if value is None:
            return []
        return [
            item.value if isinstance(item, NeighborRelationType) else str(item)
            for item in value
        ]


class StudentPairHistory(BaseModel):
    pair_key: str
    first_student_key: str
    second_student_key: str
    first_student_name: str | None = None
    second_student_name: str | None = None
    total_occurrences: int = 0
    relation_counts: dict[str, int] = Field(default_factory=dict)
    records: list[PairHistoryRecord] = Field(default_factory=list)

    def recent_relation_counts(self, lookback: int | None = None) -> dict[str, int]:
        if lookback is not None and lookback <= 0:
            return {}
        records = self.records[-lookback:] if lookback else self.records
        counts: dict[str, int] = {}
        for record in records:
            for relation in record.relations:
                key = relation.value
                counts[key] = counts.get(key, 0) + 1
        return counts

    def recent_occurrence_count(
        self,
        relation_types: set[NeighborRelationType],
        lookback: int | None = None,
    ) -> int:
        if not relation_types:
            return 0
        if lookback is not None and lookback <= 0:
            return 0
        records = self.records[-lookback:] if lookback else self.records
        return sum(1 for record in records if set(record.relations) & relation_types)


class PairHistory(BaseModel):
    history_count: int = 0
    student_count: int = 0
    pair_count: int = 0
    within_distance_metric: str = "chebyshev"
    within_distance: int = 2
    pairs: dict[str, StudentPairHistory] = Field(default_factory=dict)
    relation_totals: dict[str, int] = Field(default_factory=dict)
    warnings: list[str] = Field(default_factory=list)


class PairHistoryReport(BaseModel):
    history_count: int
    student_count: int
    pair_count: int
    within_distance_metric: str = "chebyshev"
    within_distance: int = 2
    relation_totals: dict[str, int] = Field(default_factory=dict)
    top_desk_mates: list[StudentPairHistory] = Field(default_factory=list)
    top_adjacent_pairs: list[StudentPairHistory] = Field(default_factory=list)
    pairs: list[StudentPairHistory] = Field(default_factory=list)
    summary: dict[str, Any] = Field(default_factory=dict)
    warnings: list[str] = Field(default_factory=list)


class SeatHistory(BaseModel):
    history_count: int = 0
    students: dict[str, StudentSeatHistory] = Field(default_factory=dict)
    category_totals: dict[str, int] = Field(default_factory=dict)
    pair_history: PairHistory | None = None
    warnings: list[str] = Field(default_factory=list)


class FairnessReport(BaseModel):
    history_count: int
    student_count: int
    category_totals: dict[str, int] = Field(default_factory=dict)
    students: list[StudentSeatHistory] = Field(default_factory=list)
    summary: dict[str, Any] = Field(default_factory=dict)
    warnings: list[str] = Field(default_factory=list)
