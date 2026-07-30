"""Versioned Pydantic contracts for the local Web API.

The models in this module deliberately have no dependency on FastAPI.  They
can be used by another local transport, by a desktop shell, or directly in
tests while preserving the same ``/api/v1`` contract.
"""

from __future__ import annotations

from math import isfinite
from typing import Any, Literal

try:
    from pydantic.v1 import BaseModel, Field, root_validator, validator
except ImportError:  # pragma: no cover - Pydantic v1 installed directly.
    from pydantic import BaseModel, Field, root_validator, validator

from seattrellis.models.candidate import CandidateSet, PlanComparisonReport
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.solver.backend import SolverBackend, normalize_solver_backend

API_VERSION = "1"
API_PREFIX = "/api/v1"


class ApiModel(BaseModel):
    """Strict base model for payloads owned by the API contract."""

    class Config:
        extra = "forbid"


class VersionedResponse(ApiModel):
    """Base response carrying the major contract version."""

    api_version: Literal["1"] = API_VERSION


class ApiErrorDetail(ApiModel):
    """One actionable error detail without a copy of the submitted value."""

    code: str
    message: str
    field: str | None = None


class ApiErrorPayload(ApiModel):
    code: str
    message: str
    details: list[ApiErrorDetail] = Field(default_factory=list)


class ErrorResponse(VersionedResponse):
    error: ApiErrorPayload


class HealthResponse(VersionedResponse):
    status: Literal["ok"] = "ok"
    service: Literal["seattrellis"] = "seattrellis"
    local_only: bool = True


class SolverBackendCapability(ApiModel):
    name: str
    strategy: str
    supports_history: bool
    supports_candidate_exclusions: bool
    supports_seed: bool
    supports_time_limit: bool
    requires_optional_dependency: bool
    experimental: bool


class CapabilitiesResponse(VersionedResponse):
    """Stable feature discovery for Web and desktop clients."""

    local_only: bool = True
    features: list[str]
    solver_backends: list[SolverBackendCapability]
    limits: dict[str, int | float]


class RoomTemplateItem(ApiModel):
    template_id: str
    name: str
    rows: int
    seats_per_row: int
    aisles_after: list[int]
    capacity: int
    grid_columns: int


class RoomTemplatesResponse(VersionedResponse):
    room_templates: list[RoomTemplateItem]


class TeacherGoalItem(ApiModel):
    goal_id: str
    title: str
    description: str
    default_candidate_count: int
    requires_custom_rules: bool


class TeacherGoalsResponse(VersionedResponse):
    teacher_goals: list[TeacherGoalItem]


class RoomSelection(ApiModel):
    """Select a built-in room or submit an already structured layout."""

    template_id: str | None = None
    layout: ClassroomLayout | None = None
    layout_id: str | None = None
    name: str | None = None

    @validator("template_id", "layout_id", "name", pre=True)
    def clean_optional_text(cls, value: object) -> object:
        if value is None:
            return None
        if not isinstance(value, str):
            raise ValueError("must be a string.")
        text = value.strip()
        return text or None

    @root_validator(skip_on_failure=True)
    def choose_exactly_one_room_source(cls, values: dict[str, Any]) -> dict[str, Any]:
        template_id = values.get("template_id")
        layout = values.get("layout")
        if (template_id is None) == (layout is None):
            raise ValueError("Choose either template_id or layout, but not both.")
        if layout is not None and (values.get("layout_id") or values.get("name")):
            raise ValueError(
                "layout_id and name overrides can only be used with template_id."
            )
        return values


class TeacherGoalRequest(ApiModel):
    goal_id: str = "daily-rotation"
    custom_rules: RuleSet | None = None

    @validator("goal_id", pre=True)
    def clean_goal_id(cls, value: object) -> str:
        if not isinstance(value, str):
            raise ValueError("goal_id must be a string.")
        text = value.strip().lower().replace("_", "-")
        if not text:
            raise ValueError("goal_id cannot be empty.")
        return text


class ClassDraftRequest(ApiModel):
    """Structured roster and room data for one stateless class operation."""

    name: str
    students: list[Student] = Field(min_items=1)
    room: RoomSelection
    goal: TeacherGoalRequest = Field(default_factory=TeacherGoalRequest)
    history_snapshots: list[SeatingSnapshot] = Field(default_factory=list)

    @validator("name", pre=True)
    def clean_class_name(cls, value: object) -> str:
        if not isinstance(value, str):
            raise ValueError("name must be a string.")
        text = value.strip()
        if not text:
            raise ValueError("name cannot be empty.")
        return text


class GenerateOptionsRequest(ApiModel):
    candidate_count: int | None = Field(default=None, ge=1, le=20)
    seed: int | None = None
    time_limit_seconds: float = Field(default=3.0, ge=0.1)
    backend: SolverBackend = "auto"

    @validator("backend", pre=True)
    def normalize_backend(cls, value: object) -> SolverBackend:
        if not isinstance(value, str):
            raise ValueError("backend must be a string.")
        return normalize_solver_backend(value)

    @validator("candidate_count", "seed", pre=True)
    def reject_boolean_integers(cls, value: object) -> object:
        if value is not None and (isinstance(value, bool) or not isinstance(value, int)):
            raise ValueError("must be an integer.")
        return value

    @validator("time_limit_seconds", pre=True)
    def require_finite_time_limit(cls, value: object) -> object:
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise ValueError("time_limit_seconds must be a finite number.")
        if not isfinite(float(value)):
            raise ValueError("time_limit_seconds must be a finite number.")
        return value


class GenerateClassRequest(ApiModel):
    draft: ClassDraftRequest
    options: GenerateOptionsRequest = Field(default_factory=GenerateOptionsRequest)


class ApiIssue(ApiModel):
    level: Literal["error", "warning"]
    code: str
    message: str


class ValidationSummary(ApiModel):
    ready: bool
    students_count: int
    enabled_seats_count: int
    hard_constraints_count: int
    issues: list[ApiIssue] = Field(default_factory=list)


class ResolvedGoalSummary(ApiModel):
    goal_id: str
    title: str
    description: str
    preset_name: str | None


class InspectClassResponse(VersionedResponse):
    class_name: str
    goal: ResolvedGoalSummary
    validation: ValidationSummary
    warnings: list[str] = Field(default_factory=list)


class GenerateClassResponse(VersionedResponse):
    class_name: str
    goal: ResolvedGoalSummary
    warnings: list[str] = Field(default_factory=list)
    candidate_set: CandidateSet
    summary: str | None = None
    plan_comparison_report: PlanComparisonReport | None = None
