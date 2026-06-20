from __future__ import annotations

from typing import Literal

try:
    from pydantic.v1 import BaseModel, Field, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, validator


class FixedSeatRule(BaseModel):
    student: str
    seat_id: str

    @validator("student", "seat_id", pre=True)
    def clean_required_text(cls, value: object) -> str:
        text = str(value).strip()
        if not text:
            raise ValueError("value cannot be empty.")
        return text


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


class MinDistanceRule(PairRule):
    distance: float
    metric: Literal["euclidean", "graph"] = "euclidean"

    @validator("distance")
    def positive_distance(cls, value: float) -> float:
        if value <= 0:
            raise ValueError("distance must be positive.")
        return value


class WeightedRule(BaseModel):
    enabled: bool = False
    weight: int = 1

    @validator("weight")
    def positive_weight(cls, value: int) -> int:
        if value < 0:
            raise ValueError("weight must be non-negative.")
        return value


class HardRules(BaseModel):
    fixed_seats: list[FixedSeatRule] = Field(default_factory=list)
    must_be_adjacent: list[PairRule] = Field(default_factory=list)
    cannot_be_adjacent: list[PairRule] = Field(default_factory=list)
    min_distance: list[MinDistanceRule] = Field(default_factory=list)


class SoftRules(BaseModel):
    vision_front: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=True, weight=20))
    height_back: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=True, weight=1))
    randomize: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=True, weight=1))
    score_balance: WeightedRule = Field(default_factory=lambda: WeightedRule(enabled=False, weight=1))


class RuleSet(BaseModel):
    seed: int = 42
    hard: HardRules = Field(default_factory=HardRules)
    soft: SoftRules = Field(default_factory=SoftRules)
