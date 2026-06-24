from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, Literal

try:
    from pydantic.v1 import BaseModel, Field, root_validator, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, root_validator, validator

from seattrellis.models.snapshot import SeatingSnapshot


ScoreStatus = Literal["available", "not_available"]
ScoreRating = Literal["high", "medium", "low", "not_available"]


class ScoreDimension(BaseModel):
    status: ScoreStatus
    score: float | None = None
    raw_value: float | None = None
    weight: float = 0.0
    rating: ScoreRating = "not_available"
    details: dict[str, Any] = Field(default_factory=dict)

    @validator("score")
    def score_in_range(cls, value: float | None) -> float | None:
        if value is not None and not 0 <= value <= 100:
            raise ValueError("score must be between 0 and 100.")
        return value


class HardConstraintSummary(BaseModel):
    satisfied: bool
    checked_rule_count: int = 0
    violation_count: int = 0
    violations: list[str] = Field(default_factory=list)
    details: dict[str, Any] = Field(default_factory=dict)


class ScoreBreakdown(BaseModel):
    fair_rotation_score: ScoreDimension
    avoid_recent_neighbors_score: ScoreDimension
    score_balance_score: ScoreDimension
    height_preference_score: ScoreDimension
    vision_preference_score: ScoreDimension
    diversity_score: ScoreDimension
    stability_score: ScoreDimension
    hard_constraint_summary: HardConstraintSummary


class PlanScore(BaseModel):
    total: float
    breakdown: ScoreBreakdown

    @validator("total")
    def total_in_range(cls, value: float) -> float:
        if not 0 <= value <= 100:
            raise ValueError("total score must be between 0 and 100.")
        return value


class CandidatePlan(BaseModel):
    candidate_id: str
    snapshot: SeatingSnapshot
    score: PlanScore
    hard_constraints_satisfied: bool
    warnings: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)

    @property
    def total_score(self) -> float:
        return self.score.total

    @property
    def score_breakdown(self) -> ScoreBreakdown:
        return self.score.breakdown


class CandidateSet(BaseModel):
    schema_version: str = "0.2.2"
    kind: Literal["candidate_set"] = "candidate_set"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    metadata: dict[str, Any] = Field(default_factory=dict)
    candidates: list[CandidatePlan]
    recommended_candidate_id: str
    warnings: list[str] = Field(default_factory=list)

    @root_validator(skip_on_failure=True)
    def validate_candidate_ids(cls, values: dict[str, Any]) -> dict[str, Any]:
        candidates = values.get("candidates") or []
        candidate_ids = [candidate.candidate_id for candidate in candidates]
        if not candidates:
            raise ValueError("candidate set must contain at least one candidate.")
        if len(candidate_ids) != len(set(candidate_ids)):
            raise ValueError("candidate_id values must be unique.")
        if values.get("recommended_candidate_id") not in set(candidate_ids):
            raise ValueError("recommended_candidate_id must reference a candidate.")
        return values

    def get_candidate(self, candidate_id: str) -> CandidatePlan:
        selected_id = self.recommended_candidate_id if candidate_id == "recommended" else candidate_id
        for candidate in self.candidates:
            if candidate.candidate_id == selected_id:
                return candidate
        available = ", ".join(candidate.candidate_id for candidate in self.candidates)
        raise ValueError(f"Unknown candidate ID {candidate_id!r}. Available candidates: {available}.")


class PlanComparisonEntry(BaseModel):
    candidate_id: str
    total_score: float
    hard_constraints_satisfied: bool
    dimension_scores: dict[str, float | None] = Field(default_factory=dict)
    advantages: list[str] = Field(default_factory=list)
    costs: list[str] = Field(default_factory=list)
    history_comparison: dict[str, str] = Field(default_factory=dict)


class PlanComparisonReport(BaseModel):
    schema_version: str = "0.2.2"
    kind: Literal["plan_comparison_report"] = "plan_comparison_report"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    candidate_count: int
    recommended_candidate_id: str
    candidates: list[PlanComparisonEntry]
    warnings: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)


class MultiSolveOptions(BaseModel):
    candidate_count: int = 1
    seed: int = 42
    max_attempts: int | None = None

    @validator("candidate_count")
    def candidate_count_in_range(cls, value: int) -> int:
        if not 1 <= value <= 20:
            raise ValueError("candidates must be between 1 and 20.")
        return value

    @validator("max_attempts")
    def max_attempts_positive(cls, value: int | None) -> int | None:
        if value is not None and value <= 0:
            raise ValueError("max_attempts must be positive.")
        return value

    @property
    def attempt_limit(self) -> int:
        return self.max_attempts or max(self.candidate_count * 8, self.candidate_count + 4)
