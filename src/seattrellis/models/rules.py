from __future__ import annotations

from math import isfinite
from typing import Any, Literal

try:
    from pydantic.v1 import BaseModel, Field, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, validator

from seattrellis.models.history import (
    NEIGHBOR_RELATION_TYPES,
    ROTATION_CATEGORIES,
    NeighborRelationType,
    SeatPositionCategory,
)
from seattrellis.schema import RULESET_SCHEMA_VERSION, require_schema_version


class FixedSeatRule(BaseModel):
    student: str
    seat_id: str

    @validator("student", "seat_id", pre=True)
    def clean_required_text(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("value cannot be empty.")
        return text

    class Config:
        extra = "forbid"


class PairRule(BaseModel):
    students: tuple[str, str]

    @validator("students", pre=True)
    def normalize_students(cls, value: object) -> tuple[str, str]:
        if not isinstance(value, (list, tuple)) or len(value) != 2:
            raise ValueError("students must contain exactly two student references.")
        first = str(value[0]).strip()
        second = str(value[1]).strip()
        if not first or not second:
            raise ValueError("students cannot contain empty references.")
        return (first, second)

    class Config:
        extra = "forbid"


class MinDistanceRule(PairRule):
    distance: float
    metric: Literal["euclidean", "graph"] = "euclidean"

    @validator("distance")
    def positive_distance(cls, value: float) -> float:
        if not isfinite(value) or value <= 0:
            raise ValueError("distance must be a positive finite number.")
        return value


class WeightedRule(BaseModel):
    enabled: bool = False
    weight: int = 1

    @validator("weight")
    def positive_weight(cls, value: int) -> int:
        if value < 0:
            raise ValueError("weight must be non-negative.")
        return value

    class Config:
        extra = "forbid"


class FairRotationRule(WeightedRule):
    avoid_repeating_categories: list[SeatPositionCategory] = Field(
        default_factory=lambda: [
            SeatPositionCategory.FRONT,
            SeatPositionCategory.BACK,
            SeatPositionCategory.SIDE,
            SeatPositionCategory.CORNER,
            SeatPositionCategory.NEAR_WINDOW,
            SeatPositionCategory.NEAR_DOOR,
            SeatPositionCategory.NEAR_AC,
        ]
    )
    lookback: int = 4

    @validator("avoid_repeating_categories")
    def known_rotation_categories(cls, value: list[SeatPositionCategory]) -> list[SeatPositionCategory]:
        allowed = set(ROTATION_CATEGORIES)
        unknown = [category.value for category in value if category not in allowed]
        if unknown:
            raise ValueError(f"Unsupported seat position categories: {', '.join(unknown)}")
        return value

    @validator("lookback")
    def non_negative_lookback(cls, value: int) -> int:
        if value < 0:
            raise ValueError("lookback must be non-negative.")
        return value


class AvoidRecentNeighborsRule(WeightedRule):
    relation_types: list[NeighborRelationType] = Field(
        default_factory=lambda: [
            NeighborRelationType.DESK_MATE,
            NeighborRelationType.ADJACENT_ANY,
        ]
    )
    lookback: int = 4
    max_recent_count: int = 1
    within_distance: int = 2

    @validator("relation_types")
    def known_neighbor_relation_types(cls, value: list[NeighborRelationType]) -> list[NeighborRelationType]:
        allowed = set(NEIGHBOR_RELATION_TYPES)
        unknown = [relation.value for relation in value if relation not in allowed]
        if unknown:
            raise ValueError(f"Unsupported neighbor relation types: {', '.join(unknown)}")
        return value

    @validator("lookback")
    def non_negative_lookback(cls, value: int) -> int:
        if value < 0:
            raise ValueError("lookback must be non-negative.")
        return value

    @validator("max_recent_count")
    def non_negative_max_recent_count(cls, value: int) -> int:
        if value < 0:
            raise ValueError("max_recent_count must be non-negative.")
        return value

    @validator("within_distance")
    def positive_within_distance(cls, value: int) -> int:
        if value <= 0:
            raise ValueError("within_distance must be positive.")
        return value


class GroupRule(BaseModel):
    """Defines a named group of students for separation or togetherness rules."""

    name: str
    students: list[str] = Field(default_factory=list)
    separate: bool = False
    together: bool = False

    @validator("name", pre=True)
    def clean_name(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("group name cannot be empty.")
        return text

    @validator("students", pre=True)
    def clean_students(cls, value: object) -> list[str]:
        if value is None:
            return []
        if isinstance(value, list):
            return [str(v).strip() for v in value if str(v).strip()]
        return [str(value).strip()]

    class Config:
        extra = "forbid"


class CoolingRule(WeightedRule):
    """Cooling period between repeated desk-mate / neighbor assignments."""

    cooling_period: int = 3
    relation_types: list[str] = Field(default_factory=lambda: ["desk_mate", "adjacent_any"])

    @validator("cooling_period")
    def positive_cooling(cls, value: int) -> int:
        if value < 1:
            raise ValueError("cooling_period must be positive.")
        return value

    class Config:
        extra = "forbid"


class ScorePositionRule(WeightedRule):
    """Place score ranks toward the front or back without forcing an order.

    Scores are converted to rank percentiles by the objective compiler, so
    schools can use any grading scale and ties remain neutral.
    """

    direction: Literal["high_front", "high_back"] = "high_front"


class ScoreDistributionRule(WeightedRule):
    """Balance score-rank means across physical rows or named seat groups."""

    scope: Literal["row", "group"] = "row"


class MentorPairingRule(WeightedRule):
    """Pair high- and low-ranked students through a soft proximity goal."""

    mentor_percentile: float = 0.75
    learner_percentile: float = 0.25
    relation: Literal["desk_mate", "adjacent_any"] = "desk_mate"
    avoid_recent_repeats: bool = True
    history_lookback: int = 4

    @validator("mentor_percentile", "learner_percentile")
    def percentile_in_range(cls, value: float) -> float:
        if not isfinite(value) or not 0 <= value <= 1:
            raise ValueError("percentiles must be finite values between 0 and 1.")
        return value

    @validator("history_lookback")
    def non_negative_history_lookback(cls, value: int) -> int:
        if value < 0:
            raise ValueError("history_lookback must be non-negative.")
        return value

    @validator("learner_percentile")
    def learner_below_mentor(cls, value: float, values: dict[str, Any]) -> float:
        mentor = values.get("mentor_percentile", 0.75)
        if value >= mentor:
            raise ValueError("learner_percentile must be lower than mentor_percentile.")
        return value


class HardRules(BaseModel):
    fixed_seats: list[FixedSeatRule] = Field(default_factory=list)
    must_be_adjacent: list[PairRule] = Field(default_factory=list)
    cannot_be_adjacent: list[PairRule] = Field(default_factory=list)
    min_distance: list[MinDistanceRule] = Field(default_factory=list)

    class Config:
        extra = "forbid"


class SoftRules(BaseModel):
    vision_front: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=True, weight=20))
    height_back: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=True, weight=1))
    randomize: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=True, weight=1))
    score_balance: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=False, weight=1))
    score_position: ScorePositionRule = Field(default_factory=ScorePositionRule)
    score_distribution: ScoreDistributionRule = Field(default_factory=ScoreDistributionRule)
    mentor_pairing: MentorPairingRule = Field(default_factory=MentorPairingRule)
    fair_rotation: FairRotationRule = Field(default_factory=lambda: FairRotationRule(enabled=False, weight=10))
    avoid_recent_neighbors: AvoidRecentNeighborsRule = Field(
        default_factory=lambda: AvoidRecentNeighborsRule(enabled=False, weight=10)
    )
    cooling: CoolingRule = Field(default_factory=lambda: CoolingRule(enabled=False, weight=5))

    class Config:
        extra = "forbid"


class RuleSet(BaseModel):
    schema_version: int = RULESET_SCHEMA_VERSION
    seed: int = 42
    hard: HardRules = Field(default_factory=HardRules)
    soft: SoftRules = Field(default_factory=SoftRules)
    groups: list[GroupRule] = Field(default_factory=list)

    @validator("schema_version", pre=True)
    def supported_schema_version(cls, value: object) -> int:
        return require_schema_version(
            value,
            expected=RULESET_SCHEMA_VERSION,
            artifact="ruleset",
        )

    class Config:
        extra = "forbid"
