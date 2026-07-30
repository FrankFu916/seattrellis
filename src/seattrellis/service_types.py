"""Request and response types for the in-memory service functions."""

from __future__ import annotations

from dataclasses import dataclass, field
from math import isfinite
from pathlib import Path
from typing import TYPE_CHECKING, Literal, Sequence

from seattrellis.models.candidate import (
    CandidateSet,
    HardConstraintSummary,
    PlanComparisonReport,
)
from seattrellis.models.history import FairnessReport, PairHistoryReport
from seattrellis.models.layout import ClassroomLayout
from seattrellis.io.project import ProjectPaths
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.io.validation import ValidationReport
from seattrellis.solver.backend import SolverBackend, normalize_solver_backend

if TYPE_CHECKING:
    from seattrellis.editing import EditingLockState, EditingOperation, EditingRecord


ExportTemplate = Literal["public", "teacher", "report"]
PageOrientation = Literal["portrait", "landscape"]
ExportLocale = Literal["zh", "en"]
CandidateScope = Literal["selected", "all"]

EXPORT_TEMPLATES = ("public", "teacher", "report")
PAGE_ORIENTATIONS = ("portrait", "landscape")
EXPORT_LOCALES = ("zh", "en")
CANDIDATE_SCOPES = ("selected", "all")
CANVAS_EXPORT_FORMATS = ("svg", "pptx")


@dataclass(frozen=True)
class PrivacyOptions:
    """Control which student fields may appear in an export."""

    hide_scores: bool = False
    hide_notes: bool = True
    hide_special_needs: bool = True
    anonymize: bool = False
    show_height: bool = True
    show_vision: bool = True

    @classmethod
    def for_template(cls, template: str) -> "PrivacyOptions":
        normalized = normalize_export_template(template)
        if normalized == "public":
            return cls(
                hide_scores=True,
                hide_notes=True,
                hide_special_needs=True,
                show_height=False,
                show_vision=False,
            )
        if normalized == "teacher":
            return cls(
                hide_scores=False,
                hide_notes=False,
                hide_special_needs=False,
                show_height=True,
                show_vision=True,
            )
        return cls(
            hide_scores=False,
            hide_notes=True,
            hide_special_needs=True,
            show_height=False,
            show_vision=False,
        )


@dataclass(frozen=True)
class PageOptions:
    """Print-page settings shared by HTML, PDF, and Word exporters."""

    orientation: PageOrientation = "portrait"
    scale: float = 1.0
    paper_size: str = "A4"
    margin_mm: float = 15.0

    def __post_init__(self) -> None:
        orientation = str(self.orientation).strip().lower()
        if orientation not in PAGE_ORIENTATIONS:
            supported = ", ".join(PAGE_ORIENTATIONS)
            raise ValueError(
                f"Unsupported page orientation {self.orientation!r}. "
                f"Supported orientations: {supported}."
            )
        if not isfinite(self.scale) or not 0.5 <= self.scale <= 2.0:
            raise ValueError("page scale must be a finite number between 0.5 and 2.0")
        paper_size = str(self.paper_size).strip().upper()
        if paper_size != "A4":
            raise ValueError("Only A4 paper size is currently supported.")
        if not isfinite(self.margin_mm) or not 5 <= self.margin_mm <= 30:
            raise ValueError(
                "page margin_mm must be a finite number between 5 and 30"
            )
        object.__setattr__(self, "orientation", orientation)
        object.__setattr__(self, "paper_size", paper_size)


@dataclass(frozen=True)
class ExportRequest:
    """Version-stable export options shared by application adapters."""

    output_format: str
    output_path: str | Path | None = None
    template: ExportTemplate = "public"
    privacy: PrivacyOptions | None = None
    page: PageOptions = field(default_factory=PageOptions)
    locale: ExportLocale = "zh"
    candidate_scope: CandidateScope = "selected"
    candidate_id: str | None = None

    def __post_init__(self) -> None:
        output_format = str(self.output_format).strip().lower()
        export_extension(output_format)
        template = normalize_export_template(self.template)
        locale = normalize_export_locale(self.locale)
        candidate_scope = str(self.candidate_scope).strip().lower()
        if candidate_scope not in CANDIDATE_SCOPES:
            supported = ", ".join(CANDIDATE_SCOPES)
            raise ValueError(
                f"Unsupported candidate scope {self.candidate_scope!r}. "
                f"Supported scopes: {supported}."
            )
        candidate_id = (
            str(self.candidate_id).strip() if self.candidate_id is not None else None
        )
        if candidate_id == "":
            raise ValueError("candidate_id cannot be empty.")
        object.__setattr__(self, "output_format", output_format)
        object.__setattr__(self, "template", template)
        object.__setattr__(self, "locale", locale)
        object.__setattr__(self, "candidate_scope", candidate_scope)
        object.__setattr__(self, "candidate_id", candidate_id)

    @property
    def resolved_privacy(self) -> PrivacyOptions:
        return self.privacy or PrivacyOptions.for_template(self.template)

    @property
    def resolved_output_path(self) -> Path:
        if self.output_path is not None:
            return Path(self.output_path)
        extension = export_extension(self.output_format)
        return Path("outputs") / f"seating.{extension}"


def normalize_export_template(template: str) -> ExportTemplate:
    normalized = str(template).strip().lower()
    if normalized not in EXPORT_TEMPLATES:
        supported = ", ".join(EXPORT_TEMPLATES)
        raise ValueError(
            f"Unsupported export template {template!r}. "
            f"Supported templates: {supported}."
        )
    return normalized  # type: ignore[return-value]


def normalize_export_locale(locale: str) -> ExportLocale:
    normalized = str(locale).strip().lower()
    if normalized not in EXPORT_LOCALES:
        supported = ", ".join(EXPORT_LOCALES)
        raise ValueError(
            f"Unsupported export locale {locale!r}. Supported locales: {supported}."
        )
    return normalized  # type: ignore[return-value]


@dataclass(frozen=True)
class SolveInput:
    """Pure in-memory solve request (no file paths)."""

    students: list[Student]
    layout: ClassroomLayout
    rules: RuleSet
    preset_name: str | None = None
    history_snapshots: list[SeatingSnapshot] | None = None
    candidate_count: int = 1
    seed: int | None = None
    time_limit_seconds: float = 3.0
    backend: SolverBackend = "auto"

    def __post_init__(self) -> None:
        object.__setattr__(self, "backend", normalize_solver_backend(self.backend))


@dataclass(frozen=True)
class SolveOutput:
    """Pure in-memory solve result."""

    candidate_set: CandidateSet
    preset_warnings: list[str] | None = None
    warnings: list[str] | None = None
    summary: str | None = None
    plan_comparison_report: PlanComparisonReport | None = None


@dataclass(frozen=True)
class ValidateInput:
    """Pure in-memory validation request."""

    students: list[Student]
    layout: ClassroomLayout
    rules: RuleSet
    strict: bool = False


@dataclass(frozen=True)
class ValidateOutput:
    """Pure in-memory validation result."""

    report: ValidationReport
    formatted: str


@dataclass(frozen=True)
class EditInput:
    """Pure in-memory manual editing request."""

    snapshot: SeatingSnapshot
    operations: Sequence[EditingOperation] = field(default_factory=tuple)
    locked_students: Sequence[str] = field(default_factory=tuple)
    locked_seats: Sequence[str] = field(default_factory=tuple)


@dataclass(frozen=True)
class EditOutput:
    """Pure in-memory manual editing result."""

    snapshot: SeatingSnapshot
    hard_constraints: HardConstraintSummary
    unseated_students: list[str]
    locked_students: list[str]
    locked_seats: list[str]
    operation_log: tuple[EditingRecord, ...]
    lock_state: EditingLockState


@dataclass(frozen=True)
class RepairInput:
    """Request a constrained re-solve from a manual seating draft."""

    snapshot: SeatingSnapshot
    affected_students: Sequence[str] = field(default_factory=tuple)
    locked_students: Sequence[str] = field(default_factory=tuple)
    locked_seats: Sequence[str] = field(default_factory=tuple)
    lock_state: EditingLockState | None = None
    reuse_saved_locks: bool = True
    history_snapshots: Sequence[SeatingSnapshot] = field(default_factory=tuple)
    seed: int | None = None
    time_limit_seconds: float = 3.0
    backend: SolverBackend = "auto"

    def __post_init__(self) -> None:
        if not isfinite(self.time_limit_seconds) or self.time_limit_seconds < 0.1:
            raise ValueError("time_limit_seconds must be a finite number >= 0.1")
        object.__setattr__(self, "backend", normalize_solver_backend(self.backend))


@dataclass(frozen=True)
class RepairOutput:
    """Result and trace data for a constrained re-solve."""

    snapshot: SeatingSnapshot
    hard_constraints: HardConstraintSummary
    locked_students: list[str]
    locked_seats: list[str]
    lock_state: EditingLockState
    mutable_students: list[str]
    fixed_assignments: dict[str, str]
    reserved_empty_seats: list[str]
    changed_students: list[str]


@dataclass(frozen=True)
class HistoryReportInput:
    """Pure in-memory history report request."""

    students: list[Student]
    layout: ClassroomLayout
    history_snapshots: list[SeatingSnapshot]


@dataclass(frozen=True)
class HistoryReportOutput:
    """Pure in-memory history report result."""

    report: FairnessReport
    formatted: str


@dataclass(frozen=True)
class PairReportInput:
    """Pure in-memory pair report request."""

    students: list[Student]
    layout: ClassroomLayout
    history_snapshots: list[SeatingSnapshot]
    top: int = 10
    within_distance: int = 2


@dataclass(frozen=True)
class PairReportOutput:
    """Pure in-memory pair report result."""

    report: PairHistoryReport
    formatted: str


@dataclass(frozen=True)
class ProjectInfoInput:
    """Pure in-memory project info request."""

    project: SeatTrellisProject
    paths: ProjectPaths


@dataclass(frozen=True)
class ProjectInfoOutput:
    """Pure in-memory project info result."""

    formatted: str


def export_extension(output_format: str) -> str:
    """Return the usual file extension for an export format."""
    normalized = output_format.lower()
    if normalized in {"excel", "xlsx"}:
        return "xlsx"
    if normalized in {"html", "png", "pdf", "docx", *CANVAS_EXPORT_FORMATS}:
        return normalized
    if normalized == "print-html":
        return "html"
    raise ValueError(f"Unsupported export format: {output_format}")


def score_text(score: float | None) -> str:
    """Format an optional score for display."""
    return "n/a" if score is None else f"{score:.1f}"
