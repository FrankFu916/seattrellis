"""Versioned Pydantic contracts for the local Web API.

The models in this module deliberately have no dependency on FastAPI.  They
can be used by another local transport, by a desktop shell, or directly in
tests while preserving the same ``/api/v1`` contract.
"""

from __future__ import annotations

from math import isfinite
from typing import Any, Literal

try:
    from pydantic.v1 import (
        BaseModel,
        Field,
        StrictInt,
        StrictStr,
        root_validator,
        validator,
    )
except ImportError:  # pragma: no cover - Pydantic v1 installed directly.
    from pydantic import (
        BaseModel,
        Field,
        StrictInt,
        StrictStr,
        root_validator,
        validator,
    )

from seattrellis.editing_protocol import EditorStateEnvelope
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


class CandidateSummary(ApiModel):
    candidate_id: str
    recommended: bool
    total_score: float = Field(ge=0, le=100)
    hard_constraints_satisfied: bool
    warning_count: int = Field(ge=0)


class GenerateClassResponse(VersionedResponse):
    class_name: str
    goal: ResolvedGoalSummary
    warnings: list[str] = Field(default_factory=list)
    recommended_candidate_id: str
    candidates: list[CandidateSummary]
    editor: EditorStateEnvelope


class CreateLayoutDraftRequest(ApiModel):
    name: str = "Classroom"
    template_id: str | None = None
    layout: ClassroomLayout | None = None
    rows: int | None = Field(default=None, ge=1, le=50)
    columns: int | None = Field(default=None, ge=1, le=50)

    @validator("name", "template_id", pre=True)
    def clean_layout_text(cls, value: object, field: Any) -> object:
        if value is None and field.name == "template_id":
            return None
        if not isinstance(value, str):
            raise ValueError("must be a string.")
        text = value.strip()
        if not text:
            raise ValueError("cannot be empty.")
        return text

    @validator("rows", "columns", pre=True)
    def reject_boolean_dimensions(cls, value: object) -> object:
        if value is not None and (isinstance(value, bool) or not isinstance(value, int)):
            raise ValueError("must be an integer.")
        return value

    @root_validator(skip_on_failure=True)
    def choose_one_layout_source(cls, values: dict[str, Any]) -> dict[str, Any]:
        sources = [
            values.get("template_id") is not None,
            values.get("layout") is not None,
            values.get("rows") is not None or values.get("columns") is not None,
        ]
        if sum(sources) != 1:
            raise ValueError(
                "Choose one template, existing layout, or rows and columns."
            )
        if sources[2] and (
            values.get("rows") is None or values.get("columns") is None
        ):
            raise ValueError("Both rows and columns are required.")
        return values


class LayoutCellState(ApiModel):
    row: int = Field(ge=1)
    column: int = Field(ge=1)
    kind: Literal["seat", "aisle", "platform", "empty"]
    seat_id: str | None = None


class LayoutStateResponse(VersionedResponse):
    kind: Literal["seattrellis_layout_state"] = "seattrellis_layout_state"
    draft_id: str
    revision: int = Field(ge=0)
    name: str
    rows: int = Field(ge=1)
    columns: int = Field(ge=1)
    cells: list[LayoutCellState]
    undo_depth: int = Field(ge=0)
    redo_depth: int = Field(ge=0)
    usable_seat_count: int = Field(ge=0)


class LayoutOperationRequest(ApiModel):
    kind: Literal[
        "set_cell",
        "insert_row",
        "delete_row",
        "insert_column",
        "delete_column",
        "translate",
        "mirror_horizontal",
        "flip_vertical",
    ]
    payload: dict[str, StrictStr | StrictInt | None] = Field(default_factory=dict)


class LayoutCommandRequest(ApiModel):
    command_id: str
    draft_id: str
    base_revision: int = Field(ge=0)
    action: Literal["apply", "undo", "redo"]
    operation: LayoutOperationRequest | None = None

    @validator("command_id", "draft_id", pre=True)
    def clean_layout_identifier(cls, value: object) -> str:
        if not isinstance(value, str) or not value.strip():
            raise ValueError("must be a non-empty string.")
        return value.strip()

    @validator("base_revision", pre=True)
    def reject_boolean_revision(cls, value: object) -> object:
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError("must be an integer.")
        return value

    @root_validator(skip_on_failure=True)
    def action_matches_operation(cls, values: dict[str, Any]) -> dict[str, Any]:
        action = values.get("action")
        operation = values.get("operation")
        if action == "apply" and operation is None:
            raise ValueError("Apply commands require an operation.")
        if action in {"undo", "redo"} and operation is not None:
            raise ValueError(f"{action} commands cannot contain an operation.")
        return values


class CompiledLayoutResponse(VersionedResponse):
    draft_id: str
    revision: int = Field(ge=0)
    layout: ClassroomLayout


RosterFieldName = Literal[
    "student_id",
    "name",
    "gender",
    "height_cm",
    "score",
    "vision",
    "tags",
    "needs",
    "notes",
]


class RosterColumnItem(ApiModel):
    index: int = Field(ge=0)
    header: str


class RosterPreviewRow(ApiModel):
    row_number: int = Field(ge=2)
    cells: list[str | int | float | bool | None]


class RosterMappingItem(ApiModel):
    field: RosterFieldName
    column_index: int = Field(ge=0)

    @validator("column_index", pre=True)
    def reject_boolean_column_index(cls, value: object) -> object:
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError("column_index must be an integer.")
        return value


class RosterMappingIssueItem(ApiModel):
    code: str
    message: str
    field: RosterFieldName | None = None
    column_indices: list[int] = Field(default_factory=list)


class RosterDraftResponse(VersionedResponse):
    draft_id: str
    source_format: Literal["csv", "xlsx"]
    row_count: int = Field(ge=0)
    column_count: int = Field(ge=1)
    columns: list[RosterColumnItem]
    preview_rows: list[RosterPreviewRow]
    suggested_mapping: list[RosterMappingItem]
    mapping_issues: list[RosterMappingIssueItem]


class RosterUpdatePreviewRequest(ApiModel):
    mapping: list[RosterMappingItem]
    current_students: list[Student] = Field(default_factory=list)
    current_revision: int = Field(default=0, ge=0)
    mode: Literal["incremental", "replace"] = "incremental"
    updated_fields: list[Literal[
        "student_id",
        "name",
        "gender",
        "height_cm",
        "score",
        "vision",
        "tags",
        "needs",
        "notes",
        "attributes",
    ]] | None = None

    @validator("current_revision", pre=True)
    def reject_boolean_roster_revision(cls, value: object) -> object:
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError("current_revision must be an integer.")
        return value


class RosterFieldChangeItem(ApiModel):
    field: str
    before: Any = None
    after: Any = None


class RosterChangeItem(ApiModel):
    action: Literal["add", "update", "unchanged", "remove", "conflict"]
    match_method: Literal["student_id", "name", "new"]
    before: Student | None = None
    after: Student | None = None
    field_changes: list[RosterFieldChangeItem] = Field(default_factory=list)
    incoming_index: int | None = Field(default=None, ge=0)
    existing_index: int | None = Field(default=None, ge=0)


class RosterConflictItem(ApiModel):
    code: str
    message: str
    incoming_index: int | None = Field(default=None, ge=0)
    existing_indices: list[int] = Field(default_factory=list)


class RosterUpdatePreviewResponse(VersionedResponse):
    draft_id: str
    base_revision: int = Field(ge=0)
    mode: Literal["incremental", "replace"]
    can_apply: bool
    action_counts: dict[str, int]
    changes: list[RosterChangeItem]
    conflicts: list[RosterConflictItem]
    resulting_students: list[Student] | None = None


class ExportDraftRequest(ApiModel):
    """Export one editing draft as a downloadable file.

    The draft reflects the teacher's current plan, including any manual
    adjustments, so the downloaded file matches what is displayed.
    """

    draft_id: str
    format: Literal[
        "print-html",
        "html",
        "svg",
        "pptx",
        "png",
        "pdf",
        "docx",
        "excel",
    ]
    orientation: Literal["portrait", "landscape"] = "landscape"
    locale: Literal["zh", "en"] = "zh"
    show_student_ids: bool = False

    @validator("draft_id", pre=True)
    def clean_draft_identifier(cls, value: object) -> str:
        if not isinstance(value, str) or not value.strip():
            raise ValueError("draft_id must be a non-empty string.")
        return value.strip()
