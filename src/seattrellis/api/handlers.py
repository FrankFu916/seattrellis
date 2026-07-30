"""Transport-independent handlers behind the local ``/api/v1`` routes."""

from __future__ import annotations

from collections.abc import Iterable

from seattrellis.api.drafts import EditorDraftStore
from seattrellis.api.errors import ApiProblem
from seattrellis.api.models import (
    ApiErrorDetail,
    ApiIssue,
    CandidateSummary,
    CapabilitiesResponse,
    GenerateClassRequest,
    GenerateClassResponse,
    HealthResponse,
    InspectClassResponse,
    ResolvedGoalSummary,
    RoomTemplateItem,
    RoomTemplatesResponse,
    SolverBackendCapability,
    TeacherGoalItem,
    TeacherGoalsResponse,
    ValidationSummary,
)
from seattrellis.application.class_workflow import (
    ClassDraft,
    GenerateOptions,
    generate_class_plan,
    inspect_class,
)
from seattrellis.application.room_templates import (
    build_room_from_template,
    list_room_templates,
)
from seattrellis.application.teacher_goals import (
    ResolvedTeacherGoal,
    TeacherGoalSelection,
    list_teacher_goals,
)
from seattrellis.io.json_files import InputFileError
from seattrellis.io.validation import ValidationIssue
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.registry import registered_solver_backends


def health() -> HealthResponse:
    """Return a dependency-free liveness response."""

    return HealthResponse()


def capabilities() -> CapabilitiesResponse:
    """Describe features clients may rely on in API version 1."""

    backends = [
        SolverBackendCapability(
            name=backend.name,
            strategy=backend.capabilities.strategy,
            supports_history=backend.capabilities.supports_history,
            supports_candidate_exclusions=(
                backend.capabilities.supports_candidate_exclusions
            ),
            supports_seed=backend.capabilities.supports_seed,
            supports_time_limit=backend.capabilities.supports_time_limit,
            requires_optional_dependency=(
                backend.capabilities.requires_optional_dependency
            ),
            experimental=backend.capabilities.experimental,
        )
        for backend in registered_solver_backends()
    ]
    return CapabilitiesResponse(
        features=[
            "class-inspection",
            "class-generation",
            "room-templates",
            "teacher-goals",
        ],
        solver_backends=backends,
        limits={
            "candidate_count_min": 1,
            "candidate_count_max": 20,
            "time_limit_seconds_min": 0.1,
        },
    )


def room_templates() -> RoomTemplatesResponse:
    """Return built-in room definitions without exposing internal layout JSON."""

    return RoomTemplatesResponse(
        room_templates=[
            RoomTemplateItem(
                template_id=template.template_id,
                name=template.name,
                rows=template.rows,
                seats_per_row=template.seats_per_row,
                aisles_after=list(template.aisles_after),
                capacity=template.capacity,
                grid_columns=template.grid_columns,
            )
            for template in list_room_templates()
        ]
    )


def teacher_goals() -> TeacherGoalsResponse:
    """Return teacher-facing goals in their application-defined order."""

    return TeacherGoalsResponse(
        teacher_goals=[
            TeacherGoalItem(
                goal_id=goal.goal_id,
                title=goal.title,
                description=goal.description,
                default_candidate_count=goal.default_candidate_count,
                requires_custom_rules=goal.preset_name is None,
            )
            for goal in list_teacher_goals()
        ]
    )


def inspect_class_request(request: GenerateClassRequest) -> InspectClassResponse:
    """Resolve and validate a class draft without invoking a solver."""

    draft = _build_class_draft(request)
    try:
        readiness = inspect_class(draft)
    except (TypeError, ValueError) as exc:
        raise _invalid_class_draft_problem() from exc

    issues = [_safe_validation_issue(issue) for issue in readiness.validation.issues]
    warnings = _dedupe(
        issue.message for issue in issues if issue.level == "warning"
    )
    warnings = _dedupe((*warnings, *readiness.resolved_goal.warnings))
    return InspectClassResponse(
        class_name=draft.name,
        goal=_goal_summary(readiness.resolved_goal),
        validation=ValidationSummary(
            ready=readiness.ready,
            students_count=readiness.validation.students_count,
            enabled_seats_count=readiness.validation.enabled_seats_count,
            hard_constraints_count=readiness.validation.hard_constraints_count,
            issues=issues,
        ),
        warnings=list(warnings),
    )


def generate_class(
    request: GenerateClassRequest,
    *,
    draft_store: EditorDraftStore | None = None,
) -> GenerateClassResponse:
    """Generate candidates through the existing class workflow use case."""

    inspection = inspect_class_request(request)
    if not inspection.validation.ready:
        error_details = [
            ApiErrorDetail(code=issue.code, message=issue.message, field="draft")
            for issue in inspection.validation.issues
            if issue.level == "error"
        ]
        raise ApiProblem(
            status_code=422,
            code="class_not_ready",
            message="The class setup is not ready to generate a seating plan.",
            details=error_details,
        )

    draft = _build_class_draft(request)
    options = GenerateOptions(
        candidate_count=request.options.candidate_count,
        seed=request.options.seed,
        time_limit_seconds=request.options.time_limit_seconds,
        backend=request.options.backend,
    )
    try:
        output = generate_class_plan(draft, options=options)
    except MissingOptionalDependencyError as exc:
        raise ApiProblem(
            status_code=503,
            code="feature_unavailable",
            message=(
                "The selected solver is not available in this installation. "
                "Choose another solver or install the required optional component."
            ),
        ) from exc
    except InputFileError as exc:
        # Validation is normally handled above.  Keep the boundary private in
        # case application state changes between inspection and generation.
        raise ApiProblem(
            status_code=422,
            code="class_not_ready",
            message="The class setup is not ready to generate a seating plan.",
        ) from exc
    except SeatTrellisSolveError as exc:
        raise ApiProblem(
            status_code=409,
            code="plan_not_found",
            message=(
                "No seating plan was found with the current room and rules. "
                "Review the constraints or try a different solver setting."
            ),
        ) from exc

    warnings = _dedupe((*inspection.warnings, *(output.warnings or ())))
    resolved_store = draft_store or EditorDraftStore()
    candidate_set = output.candidate_set
    return GenerateClassResponse(
        class_name=draft.name,
        goal=inspection.goal,
        warnings=list(warnings),
        recommended_candidate_id=candidate_set.recommended_candidate_id,
        candidates=[
            CandidateSummary(
                candidate_id=candidate.candidate_id,
                recommended=(
                    candidate.candidate_id == candidate_set.recommended_candidate_id
                ),
                total_score=candidate.total_score,
                hard_constraints_satisfied=candidate.hard_constraints_satisfied,
                warning_count=len(candidate.warnings),
            )
            for candidate in candidate_set.candidates
        ],
        editor=resolved_store.create(candidate_set),
    )


def _build_class_draft(request: GenerateClassRequest) -> ClassDraft:
    room = request.draft.room
    try:
        layout = (
            room.layout
            if room.layout is not None
            else build_room_from_template(
                room.template_id or "",
                layout_id=room.layout_id,
                name=room.name,
            )
        )
        return ClassDraft(
            name=request.draft.name,
            students=tuple(request.draft.students),
            layout=layout,
            goal=TeacherGoalSelection(
                goal_id=request.draft.goal.goal_id,  # type: ignore[arg-type]
                custom_rules=request.draft.goal.custom_rules,
            ),
            history_snapshots=tuple(request.draft.history_snapshots),
        )
    except (KeyError, TypeError, ValueError) as exc:
        raise _invalid_class_draft_problem() from exc


def _invalid_class_draft_problem() -> ApiProblem:
    return ApiProblem(
        status_code=422,
        code="invalid_class_draft",
        message=(
            "The class draft contains an unsupported room, goal, or option. "
            "Review the class setup and try again."
        ),
    )


def _goal_summary(resolved_goal: ResolvedTeacherGoal) -> ResolvedGoalSummary:
    definition = resolved_goal.definition
    return ResolvedGoalSummary(
        goal_id=definition.goal_id,
        title=definition.title,
        description=definition.description,
        preset_name=resolved_goal.preset_name,
    )


def _safe_validation_issue(issue: ValidationIssue) -> ApiIssue:
    """Translate diagnostics without returning student or rule identifiers."""

    message = issue.message
    if message.startswith("Not enough enabled seats"):
        code = "room_capacity"
        safe_message = "The selected room does not have enough enabled seats."
    elif message == "Classroom layout has no enabled seats.":
        code = "room_has_no_seats"
        safe_message = "The selected room does not contain any enabled seats."
    elif message.startswith("Duplicate student identifiers"):
        code = "duplicate_student_identifier"
        safe_message = "Each student must have a unique ID or name."
    elif "without student_id" in message:
        code = "missing_student_id"
        safe_message = (
            "Some students have no student ID, so their names will be used "
            "as internal identifiers."
        )
    elif "not implemented" in message or "model-only" in message:
        code = "rule_capability"
        safe_message = (
            "One configured rule is not supported by the selected application version."
        )
    else:
        code = "class_rule"
        safe_message = (
            "One or more classroom rules refer to unavailable students or seats."
        )
    return ApiIssue(level=issue.level, code=code, message=safe_message)


def _dedupe(values: Iterable[str]) -> tuple[str, ...]:
    return tuple(dict.fromkeys(value for value in values if value))
