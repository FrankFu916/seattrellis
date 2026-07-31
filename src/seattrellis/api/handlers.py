"""Transport-independent handlers behind the local ``/api/v1`` routes."""

from __future__ import annotations

import importlib.util
import os
import tempfile
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path

from seattrellis.api.drafts import EditorDraftNotFoundError, EditorDraftStore
from seattrellis.api.errors import ApiProblem
from seattrellis.api.models import (
    ApiErrorDetail,
    ApiIssue,
    CandidateSummary,
    CapabilitiesResponse,
    ExportDraftRequest,
    GenerateClassRequest,
    GenerateClassResponse,
    GenerateRotationPlanRequest,
    GenerateRotationPlanResponse,
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
from seattrellis.exporters import export_snapshot
from seattrellis.io.json_files import InputFileError
from seattrellis.io.validation import ValidationIssue
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.service_types import (
    CANVAS_EXPORT_FORMATS,
    ExportRequest,
    PageOptions,
    export_extension,
    RotationInput,
)
from seattrellis.service import compute_rotation_plan
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
            "rotation-plans",
            "layout-editing",
            "roster-mapping",
            "roster-update-preview",
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


def generate_rotation_plan(
    request: GenerateRotationPlanRequest,
) -> GenerateRotationPlanResponse:
    """Generate future periods through the shared class workflow boundary."""

    class_request = GenerateClassRequest(draft=request.draft, options=request.options)
    draft = _build_class_draft(class_request)
    try:
        readiness = inspect_class(draft)
        readiness.validation.raise_for_errors(title="Class setup is not ready.")
        output = compute_rotation_plan(
            RotationInput(
                students=list(draft.students),
                layout=draft.layout,
                rules=readiness.resolved_goal.rules,
                period_count=request.period_count,
                period_labels=request.period_labels,
                preset_name=readiness.resolved_goal.preset_name,
                history_snapshots=draft.history_snapshots,
                name=draft.name,
                seed=request.options.seed,
                time_limit_seconds=request.options.time_limit_seconds,
                backend=request.options.backend,
            )
        )
    except MissingOptionalDependencyError as exc:
        raise ApiProblem(
            status_code=503,
            code="feature_unavailable",
            message="The selected solver is not available in this installation.",
        ) from exc
    except (InputFileError, TypeError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="class_not_ready",
            message="The class setup is not ready to generate a rotation plan.",
        ) from exc
    except SeatTrellisSolveError as exc:
        raise ApiProblem(
            status_code=409,
            code="plan_not_found",
            message="No rotation plan was found with the current room and rules.",
        ) from exc
    return GenerateRotationPlanResponse(
        class_name=draft.name,
        goal=_goal_summary(readiness.resolved_goal),
        warnings=list(dict.fromkeys((*readiness.warnings, *output.plan.warnings))),
        rotation_plan=output.plan,
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


@dataclass(frozen=True)
class ExportArtifact:
    """One rendered export file before it is attached to the response."""

    data: bytes
    content_type: str
    filename: str


_EXPORT_CONTENT_TYPES: dict[str, str] = {
    "print-html": "text/html; charset=utf-8",
    "html": "text/html; charset=utf-8",
    "svg": "image/svg+xml; charset=utf-8",
    "pptx": (
        "application/vnd.openxmlformats-officedocument."
        "presentationml.presentation"
    ),
    "png": "image/png",
    "pdf": "application/pdf",
    "docx": (
        "application/vnd.openxmlformats-officedocument."
        "wordprocessingml.document"
    ),
    "excel": (
        "application/vnd.openxmlformats-officedocument."
        "spreadsheetml.sheet"
    ),
}

# Bilingual catalog copy for the browser workbench.  The application layer
# keeps single-language room and goal copy, so this endpoint owns the short
# teacher-facing translations for the local React client.
_ROOM_NAME_ZH: dict[str, str] = {
    "standard-30": "30 座教室",
    "standard-48": "48 座教室",
    "standard-60": "60 座教室",
}
_ROOM_DESCRIPTION: dict[str, tuple[str, str]] = {
    "standard-30": (
        "5 排 × 6 座，中央过道，适合小班。",
        "5 rows of 6 seats with a center aisle for a smaller class.",
    ),
    "standard-48": (
        "6 排 × 8 座，中央过道，适合常规班级。",
        "6 rows of 8 seats with a center aisle for a typical class.",
    ),
    "standard-60": (
        "6 排 × 10 座，中央过道，适合大班。",
        "6 rows of 10 seats with a center aisle for a larger class.",
    ),
}

_GOAL_COPY: dict[str, tuple[str, str, str, str]] = {
    # title_zh, title_en, description_zh, description_en
    "daily-rotation": (
        "日常轮换",
        "Daily rotation",
        "兼顾视力和身高需求，减少近期重复邻座，并适度轮换位置。",
        "Balance vision and height needs, vary recent neighbors, and rotate "
        "seats for everyday classroom use.",
    ),
    "quick-shuffle": (
        "快速打乱",
        "Quick shuffle",
        "不依赖成绩或历史记录，快速生成一组中性的随机座位方案。",
        "Create a neutral shuffle without relying on scores or saved history.",
    ),
    "fair-shuffle": (
        "公平轮换",
        "Fair shuffle",
        "优先参考历史座位，让每名学生逐步获得不同的位置和邻座。",
        "Use seating history to give each student a wider range of positions "
        "and neighbors over time.",
    ),
    "peer-support": (
        "邻座互助",
        "Peer support",
        "让成绩层次不同的学生在邻座范围内适度混合。",
        "Mix students from different score ranges across neighboring seats.",
    ),
}

_EXPORT_FORMAT_COPY: dict[str, tuple[str, str, str, str]] = {
    "print-html": (
        "打印版",
        "Print sheet",
        "适合 A4 打印或存为 PDF。",
        "Designed for A4 printing or saving as PDF.",
    ),
    "html": (
        "网页版",
        "HTML",
        "适合在浏览器中查看。",
        "Open in any browser.",
    ),
    "svg": (
        "SVG 矢量图",
        "SVG image",
        "矢量格式，方便继续编辑。",
        "Vector image that stays easy to edit.",
    ),
    "pptx": (
        "PowerPoint",
        "PowerPoint",
        "可编辑的幻灯片。",
        "An editable slide deck.",
    ),
    "png": (
        "PNG 图片",
        "PNG image",
        "适合截图和分享。",
        "A simple image for sharing.",
    ),
    "pdf": (
        "PDF",
        "PDF",
        "适合打印或分发。",
        "Best for printing and sharing.",
    ),
    "docx": (
        "Word",
        "Word",
        "可编辑的文档。",
        "An editable document.",
    ),
    "excel": (
        "Excel",
        "Excel",
        "可编辑的表格。",
        "An editable spreadsheet.",
    ),
}

_EXPORT_FORMAT_OPTIONAL_MODULE: dict[str, str | None] = {
    "print-html": None,
    "html": None,
    "svg": None,
    "pptx": "pptx",
    "png": "PIL",
    "pdf": "weasyprint",
    "docx": "docx",
    "excel": "openpyxl",
}


def catalogs() -> dict[str, list[dict[str, object]]]:
    """Return teacher-facing catalogs in the browser workbench contract.

    The response uses camelCase keys to match the committed React client and
    lists only export formats whose optional dependencies are installed, so a
    minimal workspace never advertises a download it cannot produce.
    """

    room_templates_items: list[dict[str, object]] = []
    for template in list_room_templates():
        name_zh = _ROOM_NAME_ZH.get(template.template_id, template.name)
        description = _ROOM_DESCRIPTION.get(
            template.template_id, (template.name, template.name)
        )
        room_templates_items.append(
            {
                "id": template.template_id,
                "name": {
                    "zh-CN": name_zh,
                    "en": template.name,
                },
                "description": {
                    "zh-CN": description[0],
                    "en": description[1],
                },
                "rows": template.rows,
                "columns": template.grid_columns,
            }
        )

    goal_items: list[dict[str, object]] = []
    for goal in list_teacher_goals():
        if goal.goal_id == "custom":
            continue
        copy = _GOAL_COPY.get(
            goal.goal_id, (goal.title, goal.title, goal.description, goal.description)
        )
        goal_items.append(
            {
                "id": goal.goal_id,
                "name": {"zh-CN": copy[0], "en": copy[1]},
                "description": {"zh-CN": copy[2], "en": copy[3]},
            }
        )

    format_items: list[dict[str, object]] = []
    for output_format, module in _EXPORT_FORMAT_OPTIONAL_MODULE.items():
        if module is not None and importlib.util.find_spec(module) is None:
            continue
        copy = _EXPORT_FORMAT_COPY.get(
            output_format, (output_format, output_format, "", "")
        )
        format_items.append(
            {
                "id": output_format,
                "name": {"zh-CN": copy[0], "en": copy[1]},
                "description": {"zh-CN": copy[2], "en": copy[3]},
            }
        )

    return {
        "roomTemplates": room_templates_items,
        "teacherGoals": goal_items,
        "exportFormats": format_items,
    }


def export_draft(
    request: ExportDraftRequest,
    *,
    draft_store: EditorDraftStore,
) -> ExportArtifact:
    """Render the current editing draft into one downloadable file."""

    try:
        snapshot = draft_store.snapshot(request.draft_id)
    except EditorDraftNotFoundError as exc:
        raise ApiProblem(
            status_code=404,
            code="editor_draft_not_found",
            message=(
                "This seating plan has expired or was already closed. "
                "Generate the plan again and retry the export."
            ),
        ) from exc

    template = "teacher" if request.show_student_ids else "public"
    extension = export_extension(request.format)
    # Canvas formats (SVG, PPTX) render at a fixed 16:9 size and reject page
    # options, while the print-oriented formats honor the requested page.
    page = (
        PageOptions()
        if request.format in CANVAS_EXPORT_FORMATS
        else PageOptions(orientation=request.orientation)
    )
    descriptor, temporary_path = tempfile.mkstemp(suffix=f".{extension}")
    os.close(descriptor)
    export_request = ExportRequest(
        output_format=request.format,
        output_path=Path(temporary_path),
        template=template,
        page=page,
        locale=request.locale,
    )
    try:
        try:
            export_snapshot(snapshot, request=export_request)
        except MissingOptionalDependencyError as exc:
            raise ApiProblem(
                status_code=503,
                code="feature_unavailable",
                message=(
                    "The selected export format is not available in this "
                    "installation. Choose another format or install the "
                    "required optional component."
                ),
            ) from exc
        except ValueError as exc:
            raise ApiProblem(
                status_code=422,
                code="export_rejected",
                message=str(exc) or "This export could not be prepared.",
            ) from exc
        data = Path(temporary_path).read_bytes()
    finally:
        try:
            os.unlink(temporary_path)
        except FileNotFoundError:
            pass

    return ExportArtifact(
        data=data,
        content_type=_EXPORT_CONTENT_TYPES.get(
            request.format, "application/octet-stream"
        ),
        filename=f"seating.{extension}",
    )
