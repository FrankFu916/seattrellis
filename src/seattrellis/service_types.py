"""Typed request/response contracts for the SeatTrellis service layer.

These frozen dataclasses define the API boundary in a language-agnostic way.
They use only plain Python types and Pydantic models — no file paths, no I/O.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

from seattrellis.models.candidate import CandidateSet, PlanComparisonReport
from seattrellis.models.history import FairnessReport, PairHistoryReport
from seattrellis.models.layout import ClassroomLayout
from seattrellis.io.project import ProjectPaths
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.io.validation import ValidationReport


# ---------------------------------------------------------------------------
# Solve
# ---------------------------------------------------------------------------


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


@dataclass(frozen=True)
class SolveOutput:
    """Pure in-memory solve result."""

    candidate_set: CandidateSet
    preset_warnings: list[str] | None = None
    summary: str | None = None
    plan_comparison_report: PlanComparisonReport | None = None


# ---------------------------------------------------------------------------
# Validate
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# History report
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Pair report
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Project info
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ProjectInfoInput:
    """Pure in-memory project info request."""

    project: SeatTrellisProject
    paths: ProjectPaths


@dataclass(frozen=True)
class ProjectInfoOutput:
    """Pure in-memory project info result."""

    formatted: str


# ---------------------------------------------------------------------------
# Shared utilities (no internal deps — safe to import from anywhere)
# ---------------------------------------------------------------------------


def export_extension(output_format: str) -> str:
    """Canonical format-to-file-extension mapping.

    Consolidates the three previously-duplicated implementations in
    ``cli._export_extension``, ``workflow._extension_for_format``, and
    ``exporters._extension_for_format``.
    """
    normalized = output_format.lower()
    if normalized in {"excel", "xlsx"}:
        return "xlsx"
    if normalized in {"html", "png", "pdf", "docx"}:
        return normalized
    if normalized == "print-html":
        return "html"
    raise ValueError(f"Unsupported export format: {output_format}")


def score_text(score: float | None) -> str:
    """Canonical score display formatter.

    Consolidates ``workflow._score_text`` and ``components._score_cell``.
    """
    return "n/a" if score is None else f"{score:.1f}"
