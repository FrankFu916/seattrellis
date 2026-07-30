"""Application use cases for the default classroom planning workflow."""

from __future__ import annotations

from dataclasses import dataclass
from math import isfinite

from seattrellis.application.teacher_goals import (
    ResolvedTeacherGoal,
    TeacherGoalSelection,
    resolve_teacher_goal,
)
from seattrellis.io.validation import ValidationReport, validate_loaded_inputs
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.service import compute_solve
from seattrellis.service_types import SolveInput, SolveOutput
from seattrellis.solver.backend import (
    SolverBackend,
    normalize_solver_backend,
)


@dataclass(frozen=True)
class ClassDraft:
    """The domain data needed to inspect or generate one classroom plan."""

    name: str
    students: tuple[Student, ...]
    layout: ClassroomLayout
    goal: TeacherGoalSelection
    history_snapshots: tuple[SeatingSnapshot, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.name, str):
            raise TypeError("class name must be a string")
        name = self.name.strip()
        if not name:
            raise ValueError("class name cannot be empty")
        object.__setattr__(self, "name", name)
        object.__setattr__(self, "students", tuple(self.students))
        object.__setattr__(
            self,
            "history_snapshots",
            tuple(self.history_snapshots),
        )


@dataclass(frozen=True)
class GenerateOptions:
    """Advanced solve controls hidden from the default teacher workflow."""

    candidate_count: int | None = None
    seed: int | None = None
    time_limit_seconds: float = 3.0
    backend: SolverBackend = "auto"

    def __post_init__(self) -> None:
        if self.candidate_count is not None and not 1 <= self.candidate_count <= 20:
            raise ValueError("candidate_count must be between 1 and 20")
        if not isfinite(self.time_limit_seconds) or self.time_limit_seconds < 0.1:
            raise ValueError("time_limit_seconds must be a finite number >= 0.1")
        object.__setattr__(
            self,
            "backend",
            normalize_solver_backend(self.backend),
        )


@dataclass(frozen=True)
class ClassReadiness:
    """Validation and plain-language goal guidance for one class draft."""

    resolved_goal: ResolvedTeacherGoal
    validation: ValidationReport
    warnings: tuple[str, ...]

    @property
    def ready(self) -> bool:
        return self.validation.ok


def inspect_class(draft: ClassDraft) -> ClassReadiness:
    """Resolve the teacher goal and validate a class without running a solver."""

    resolved_goal = resolve_teacher_goal(
        draft.goal,
        students=draft.students,
        history_count=len(draft.history_snapshots),
    )
    validation = validate_loaded_inputs(
        list(draft.students),
        draft.layout,
        resolved_goal.rules,
    )
    warnings = tuple(dict.fromkeys((*validation.warnings, *resolved_goal.warnings)))
    return ClassReadiness(
        resolved_goal=resolved_goal,
        validation=validation,
        warnings=warnings,
    )


def generate_class_plan(
    draft: ClassDraft,
    *,
    options: GenerateOptions | None = None,
) -> SolveOutput:
    """Generate candidates through the existing in-memory service boundary."""

    resolved_options = options or GenerateOptions()
    readiness = inspect_class(draft)
    readiness.validation.raise_for_errors(title="Class setup is not ready.")
    candidate_count = (
        resolved_options.candidate_count
        if resolved_options.candidate_count is not None
        else readiness.resolved_goal.definition.default_candidate_count
    )
    return compute_solve(
        SolveInput(
            students=list(draft.students),
            layout=draft.layout,
            rules=readiness.resolved_goal.rules,
            preset_name=readiness.resolved_goal.preset_name,
            history_snapshots=list(draft.history_snapshots),
            candidate_count=candidate_count,
            seed=resolved_options.seed,
            time_limit_seconds=resolved_options.time_limit_seconds,
            backend=resolved_options.backend,
        )
    )
