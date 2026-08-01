"""Transport-independent handlers behind the local ``/api/v1`` routes."""

from __future__ import annotations

import importlib.util
import csv
import html
import io
import os
import tempfile
from copy import deepcopy
from collections.abc import Iterable
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Literal

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
    ProjectArtifactItem,
    ProjectArtifactOperation,
    ProjectArtifactProvenance,
    ProjectArtifactCompareResponse,
    ProjectArtifactDiff,
    ProjectArtifactAssignmentChange,
    ProjectArtifactRequest,
    ProjectArtifactRestoreResponse,
    ProjectArtifactSummary,
    ProjectHistoryResponse,
    ProjectListResponse,
    ProjectMigrationRequest,
    ProjectMigrationRestoreRequest,
    ProjectMigrationRestoreResponse,
    ProjectMigrationChange,
    ProjectMigrationReferenceCheck,
    ProjectMigrationResponse,
    ProjectGroupRegisterRequest,
    ProjectRotationLoadRequest,
    ProjectRotationLoadResponse,
    ProjectRotationSaveRequest,
    ProjectRotationSaveResponse,
    ProjectPathRequest,
    ProjectPrivacyResponse,
    ProjectRestoreResponse,
    PrivacyFindingItem,
    RecentProjectItem,
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
from seattrellis.io.json_files import (
    InputFileError,
    load_candidate_set,
    load_rotation_plan,
    load_seating_artifact,
    read_json,
    write_json_model,
)
from seattrellis.io.project import ProjectPaths, load_project_paths
from seattrellis.io.validation import ValidationIssue
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.project_bundle import (
    list_recent_projects,
    pack_project,
    restore_project_bundle as restore_bundle,
    scan_project_privacy,
)
from seattrellis.schema_migration import (
    merge_normalized_data,
    migrate_json_file,
    parse_migratable_artifact,
    restore_json_backup,
)
from seattrellis.service_types import (
    CANVAS_EXPORT_FORMATS,
    ExportRequest,
    PageOptions,
    export_extension,
    RotationInput,
)
from seattrellis.service import compute_rotation_plan
from seattrellis.models.candidate import CandidatePlan, CandidateSet
from seattrellis.models.rotation import RotationPlan
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.scoring import score_snapshot
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
            "project-workspace",
            "project-migration",
            "project-migration-restore",
            "project-rotation-save",
            "project-rotation-load",
            "project-group-register",
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


def list_projects(*, root: str = ".", limit: int = 20) -> ProjectListResponse:
    """Return recent projects without reading student records into the response."""

    if not 1 <= limit <= 100:
        raise ApiProblem(
            status_code=422,
            code="invalid_project_limit",
            message="The project list limit must be between 1 and 100.",
        )
    try:
        directory = Path(root).expanduser().resolve()
        projects = list_recent_projects(directory, limit=limit)
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_directory_unavailable",
            message="The selected project directory could not be read.",
        ) from exc
    return ProjectListResponse(
        root=str(directory),
        projects=[
            RecentProjectItem(
                name=item.name,
                path=str(item.path),
                modified_at=item.modified_at,
            )
            for item in projects
        ],
    )


def project_history(request: ProjectPathRequest) -> ProjectHistoryResponse:
    """Return history/output metadata while keeping student data server-side."""

    try:
        project, paths = load_project_paths(request.project_path)
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_unavailable",
            message="The selected project could not be opened.",
        ) from exc

    warnings: list[str] = []
    history_paths = (
        sorted(paths.history_dir.glob("*.json"), key=_path_sort_key, reverse=True)
        if paths.history_dir is not None and paths.history_dir.is_dir()
        else []
    )
    if paths.history_dir is None:
        warnings.append("This project does not configure a history directory.")
    elif not paths.history_dir.exists():
        warnings.append("The configured history directory is not available.")

    output_paths = (
        sorted(paths.outputs_dir.glob("*.json"), key=_path_sort_key, reverse=True)
        if request.include_outputs and paths.outputs_dir.is_dir()
        else []
    )
    history = _artifact_items(history_paths, warnings)
    outputs = _artifact_items(output_paths, warnings)
    return ProjectHistoryResponse(
        project_name=project.name,
        project_path=str(paths.project_file),
        history=history,
        outputs=outputs,
        warnings=list(dict.fromkeys(warnings)),
    )


def project_artifact_compare(
    request: ProjectArtifactRequest,
) -> ProjectArtifactCompareResponse:
    """Compare two validated project artifacts without returning student data."""

    try:
        _project, paths = load_project_paths(request.project_path)
        left_path = _resolve_project_artifact(paths, request.artifact_path)
        if request.compare_to_path is None:
            raise InputFileError("compare_to_path is required for artifact comparison.")
        right_path = _resolve_project_artifact(paths, request.compare_to_path)
        if left_path == right_path:
            raise InputFileError("An artifact cannot be compared with itself.")
        left_kind, left_snapshot, left_created_at = _snapshot_for_artifact(left_path)
        right_kind, right_snapshot, right_created_at = _snapshot_for_artifact(right_path)
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_artifact_unavailable",
            message="The selected project artifacts could not be compared.",
        ) from exc

    left_summary = _artifact_summary(
        left_path,
        kind=left_kind,
        snapshot=left_snapshot,
        created_at=left_created_at,
    )
    right_summary = _artifact_summary(
        right_path,
        kind=right_kind,
        snapshot=right_snapshot,
        created_at=right_created_at,
    )
    left_assignments = {item.student_key: item.seat_id for item in left_snapshot.assignments}
    right_assignments = {item.student_key: item.seat_id for item in right_snapshot.assignments}
    all_students = set(left_assignments) | set(right_assignments)
    assignment_details: list[ProjectArtifactAssignmentChange] = []
    for index, student_key in enumerate(sorted(all_students), start=1):
        before_seat_id = left_assignments.get(student_key)
        after_seat_id = right_assignments.get(student_key)
        if before_seat_id == after_seat_id:
            continue
        change = (
            "seated"
            if before_seat_id is None
            else "unseated"
            if after_seat_id is None
            else "moved"
        )
        assignment_details.append(
            ProjectArtifactAssignmentChange(
                student_ref=f"student-{index}",
                change=change,
                before_seat_id=before_seat_id,
                after_seat_id=after_seat_id,
            )
        )
    return ProjectArtifactCompareResponse(
        left=left_summary,
        right=right_summary,
        diff=ProjectArtifactDiff(
            assignment_changes=sum(
                left_assignments.get(student) != right_assignments.get(student)
                for student in all_students
            ),
            roster_added=len(set(right_assignments) - set(left_assignments)),
            roster_removed=len(set(left_assignments) - set(right_assignments)),
            layout_changed=(
                left_snapshot.layout.model_dump(mode="json")
                != right_snapshot.layout.model_dump(mode="json")
            ),
            rules_changed=(
                left_snapshot.rules.model_dump(mode="json")
                != right_snapshot.rules.model_dump(mode="json")
            ),
            solver_status_changed=left_snapshot.solver_status != right_snapshot.solver_status,
            assignment_details=assignment_details,
        ),
    )


def project_artifact_restore(
    request: ProjectArtifactRequest,
) -> ProjectArtifactRestoreResponse:
    """Restore an artifact as a new output snapshot without overwriting history."""

    try:
        _project, paths = load_project_paths(request.project_path)
        source_path = _resolve_project_artifact(paths, request.artifact_path)
        kind, snapshot, _created_at = _snapshot_for_artifact(source_path)
        if kind == "rotation_plan":
            raise InputFileError(
                "Select a snapshot or candidate set inside a rotation plan before restoring it."
            )
        paths.outputs_dir.mkdir(parents=True, exist_ok=True)
        metadata = deepcopy(snapshot.metadata)
        metadata["restored_from"] = source_path.name
        metadata["restored_at"] = datetime.now(timezone.utc).isoformat()
        restored = snapshot.model_copy(update={"metadata": metadata})
        target = _next_restored_path(paths.outputs_dir, source_path.stem)
        write_json_model(restored, target)
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_artifact_restore_failed",
            message="The selected project artifact could not be restored.",
        ) from exc
    return ProjectArtifactRestoreResponse(
        project_path=str(paths.project_file),
        source_artifact=str(source_path),
        restored_artifact=str(target),
    )


def project_migration_preview(
    request: ProjectMigrationRequest,
) -> ProjectMigrationResponse:
    """Validate a project artifact and describe a safe migration target."""

    return _migrate_project_artifact(request, dry_run=True)


def project_migration_apply(
    request: ProjectMigrationRequest,
) -> ProjectMigrationResponse:
    """Write a migrated artifact, preserving the source unless explicitly in-place."""

    return _migrate_project_artifact(request, dry_run=False)


def project_migration_restore(
    request: ProjectMigrationRestoreRequest,
) -> ProjectMigrationRestoreResponse:
    """Restore an in-place migration backup without allowing arbitrary paths."""

    try:
        _project, paths = load_project_paths(request.project_path)
        source_path = _resolve_project_artifact(paths, request.source_path)
        backup_path = _resolve_project_migration_backup(paths, request.backup_path)
        expected_prefix = f"{source_path.name}.bak"
        if backup_path.parent != source_path.parent or not backup_path.name.startswith(
            expected_prefix
        ):
            raise InputFileError(
                "The migration backup does not belong to the selected project artifact."
            )
        artifact, backup_model = parse_migratable_artifact(
            read_json(backup_path), backup_path
        )
        safety_backup = restore_json_backup(backup_path, source_path)
        restored_artifact, restored_model = parse_migratable_artifact(
            read_json(source_path), source_path
        )
        if restored_artifact != artifact:
            raise InputFileError("The restored backup has a different artifact type.")
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_migration_restore_failed",
            message="The migration backup could not be restored.",
        ) from exc
    return ProjectMigrationRestoreResponse(
        project_path=str(paths.project_file),
        source_path=str(source_path),
        backup_path=str(backup_path),
        safety_backup_path=str(safety_backup) if safety_backup is not None else None,
        artifact=artifact,
        schema_version=getattr(
            restored_model,
            "schema_version",
            getattr(backup_model, "schema_version"),
        ),
        restored_valid=True,
    )


def project_rotation_save(
    request: ProjectRotationSaveRequest,
    *,
    draft_store: EditorDraftStore,
) -> ProjectRotationSaveResponse:
    """Save all current rotation drafts without exposing them to the client."""

    try:
        _project, paths = load_project_paths(request.project_path)
        snapshots: list[SeatingSnapshot] = []
        for source_period, draft_id in zip(
            request.rotation_plan.periods,
            request.draft_ids,
            strict=True,
        ):
            snapshot = draft_store.snapshot(draft_id)
            _ensure_rotation_draft_matches_source(snapshot, source_period.snapshot)
            metadata = dict(snapshot.metadata)
            metadata["project_persistence"] = {
                "source": "react_rotation_editor",
                "rotation_plan": request.rotation_plan.name,
                "period": source_period.period,
                "draft_id": draft_id,
            }
            snapshots.append(snapshot.model_copy(update={"metadata": metadata}))

        saved_at = datetime.now(timezone.utc)
        periods = [
            source_period.model_copy(update={"snapshot": snapshot})
            for source_period, snapshot in zip(
                request.rotation_plan.periods,
                snapshots,
                strict=True,
            )
        ]
        plan = request.rotation_plan.model_copy(
            update={
                "periods": periods,
                "metadata": {
                    **request.rotation_plan.metadata,
                    "saved_at": saved_at.isoformat(),
                    "saved_from": "react_rotation_editor",
                },
            }
        )
        paths.outputs_dir.mkdir(parents=True, exist_ok=True)
        output_path = _next_rotation_output_path(
            paths.outputs_dir,
            request.output_name,
        )
        write_json_model(plan, output_path)
    except EditorDraftNotFoundError as exc:
        raise ApiProblem(
            status_code=409,
            code="rotation_draft_expired",
            message="One of the rotation periods has expired. Generate the rotation again before saving.",
        ) from exc
    except (InputFileError, OSError, TypeError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_rotation_save_failed",
            message="The edited rotation plan could not be saved to this project.",
        ) from exc
    return ProjectRotationSaveResponse(
        project_path=str(paths.project_file),
        output_path=str(output_path),
        period_count=plan.period_count,
        saved_at=saved_at,
    )


def project_rotation_load(
    request: ProjectRotationLoadRequest,
    *,
    draft_store: EditorDraftStore,
) -> ProjectRotationLoadResponse:
    """Recreate editable drafts from a saved rotation artifact."""

    try:
        _project, paths = load_project_paths(request.project_path)
        artifact_path = _resolve_project_artifact(paths, request.artifact_path)
        plan = load_rotation_plan(artifact_path)
        period_editors: list[EditorStateEnvelope] = []
        for period in plan.periods:
            period_score = score_snapshot(period.snapshot)
            period_candidate = CandidatePlan(
                candidate_id=f"period-{period.period}",
                snapshot=period.snapshot,
                score=period_score,
                hard_constraints_satisfied=(
                    period_score.breakdown.hard_constraint_summary.satisfied
                ),
                metadata={
                    "source": "project_rotation_load",
                    "rotation_plan": plan.name,
                    "rotation_period": period.period,
                },
            )
            period_editors.append(
                draft_store.create(
                    CandidateSet(
                        candidates=[period_candidate],
                        recommended_candidate_id=period_candidate.candidate_id,
                        metadata={
                            "source": "project_rotation_load",
                            "artifact_path": str(artifact_path),
                            "period_count": plan.period_count,
                        },
                    )
                )
            )
    except (InputFileError, OSError, TypeError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_rotation_load_failed",
            message="The saved rotation plan could not be opened for editing.",
        ) from exc
    return ProjectRotationLoadResponse(
        project_path=str(paths.project_file),
        artifact_path=str(artifact_path),
        rotation_plan=plan,
        editor=period_editors[0],
        period_editors=period_editors,
    )


def project_group_register(request: ProjectGroupRegisterRequest) -> ProjectDownloadArtifact:
    """Render a printable or tabular register from a saved rotation plan."""

    try:
        _project, paths = load_project_paths(request.project_path)
        artifact_path = _resolve_project_artifact(paths, request.artifact_path)
        plan = load_rotation_plan(artifact_path)
        rows = _group_register_rows(plan, locale=request.locale)
        if request.format == "csv":
            data = _render_group_register_csv(rows, locale=request.locale)
            content_type = "text/csv; charset=utf-8"
            suffix = "csv"
        else:
            data = _render_group_register_html(
                plan.name,
                rows,
                locale=request.locale,
            )
            content_type = "text/html; charset=utf-8"
            suffix = "html"
    except (InputFileError, OSError, TypeError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_group_register_failed",
            message="The selected rotation plan could not produce a group register.",
        ) from exc
    return ProjectDownloadArtifact(
        data=data,
        filename=f"group-register.{suffix}",
        content_type=content_type,
    )


def _group_register_rows(
    plan: RotationPlan,
    *,
    locale: Literal["zh", "en"],
) -> list[dict[str, str]]:
    """Build rows while retaining empty, missing, and unseated members."""

    periods = plan.periods
    group_names = sorted(
        {
            group.name
            for period in periods
            for group in period.snapshot.rules.groups
        }
    )
    rows: list[dict[str, str]] = []
    if not group_names:
        return [
            {
                "period": "",
                "group": "没有命名小组" if locale == "zh" else "No named groups",
                "student_id": "",
                "student": "",
                "seat": "",
                "status": "empty",
            }
        ]
    for period in periods:
        groups = {group.name: group for group in period.snapshot.rules.groups}
        student_by_key = {student.key: student for student in period.snapshot.students}
        seat_by_student = {
            assignment.student_key: assignment.seat_id
            for assignment in period.snapshot.assignments
        }
        for name in group_names:
            group = groups.get(name)
            members = list(group.students) if group is not None else []
            if not members:
                rows.append(
                    {
                        "period": period.label,
                        "group": name,
                        "student_id": "",
                        "student": "",
                        "seat": "",
                        "status": "empty",
                    }
                )
                continue
            for student_key in members:
                student = student_by_key.get(student_key)
                if student is None:
                    status = "missing"
                    display_name = ""
                    seat = ""
                else:
                    seat = seat_by_student.get(student_key, "")
                    status = "seated" if seat else "unseated"
                    display_name = student.display_name
                rows.append(
                    {
                        "period": period.label,
                        "group": name,
                        "student_id": student_key,
                        "student": display_name,
                        "seat": seat,
                        "status": status,
                    }
                )
    return rows


def _render_group_register_csv(
    rows: list[dict[str, str]],
    *,
    locale: Literal["zh", "en"],
) -> bytes:
    headers = (
        ["期次", "小组", "学生编号", "姓名", "座位", "状态"]
        if locale == "zh"
        else ["Period", "Group", "Student ID", "Student", "Seat", "Status"]
    )
    fields = ["period", "group", "student_id", "student", "seat", "status"]
    output = io.StringIO(newline="")
    writer = csv.writer(output)
    writer.writerow(headers)
    for row in rows:
        writer.writerow(
            [
                _group_register_status_label(row[field], locale=locale)
                if field == "status"
                else row[field]
                for field in fields
            ]
        )
    return output.getvalue().encode("utf-8-sig")


def _group_register_status_label(
    status: str,
    *,
    locale: Literal["zh", "en"],
) -> str:
    labels = {
        "zh": {
            "seated": "已入座",
            "unseated": "未入座",
            "missing": "名单中不存在",
            "empty": "空组",
        },
        "en": {
            "seated": "Seated",
            "unseated": "Unseated",
            "missing": "Missing from roster",
            "empty": "Empty group",
        },
    }
    return labels[locale].get(status, status)


def _group_register_cell(
    row: dict[str, str],
    field: str,
    *,
    locale: Literal["zh", "en"],
) -> str:
    value = row[field]
    if field == "status":
        value = _group_register_status_label(value, locale=locale)
    return html.escape(value)


def _render_group_register_html(
    plan_name: str,
    rows: list[dict[str, str]],
    *,
    locale: Literal["zh", "en"],
) -> bytes:
    title = "小组登记表" if locale == "zh" else "Group register"
    headers = (
        ["期次", "小组", "学生编号", "姓名", "座位", "状态"]
        if locale == "zh"
        else ["Period", "Group", "Student ID", "Student", "Seat", "Status"]
    )
    fields = ["period", "group", "student_id", "student", "seat", "status"]
    table_rows = "".join(
        "<tr>"
        + "".join(
            f"<td>{_group_register_cell(row, field, locale=locale)}</td>"
            for field in fields
        )
        + "</tr>"
        for row in rows
    )
    header_row = "".join(f"<th>{html.escape(label)}</th>" for label in headers)
    document = f"""<!doctype html>
<html lang="{locale}">
<head><meta charset="utf-8"><title>{html.escape(title)} · {html.escape(plan_name)}</title>
<style>
body {{ font: 14px -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; margin: 32px; color: #1f2933; }}
h1 {{ margin-bottom: 4px; }}
p {{ color: #52606d; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid #cbd5e1; padding: 7px 9px; text-align: left; }}
th {{ background: #eef2f6; }}
@media print {{ body {{ margin: 10mm; }} }}
</style></head>
<body><h1>{html.escape(title)}</h1><p>{html.escape(plan_name)}</p>
<table><thead><tr>{header_row}</tr></thead><tbody>{table_rows}</tbody></table>
</body></html>"""
    return document.encode("utf-8")


def _ensure_rotation_draft_matches_source(
    current: SeatingSnapshot,
    source: SeatingSnapshot,
) -> None:
    """Prevent a draft from being saved under the wrong rotation period."""

    if {student.key for student in current.students} != {
        student.key for student in source.students
    }:
        raise InputFileError("A rotation editing draft has a different roster.")
    if current.layout.model_dump(mode="json") != source.layout.model_dump(mode="json"):
        raise InputFileError("A rotation editing draft has a different layout.")


def _next_rotation_output_path(
    outputs_dir: Path,
    output_name: str | None,
) -> Path:
    """Choose a new rotation artifact without overwriting an existing output."""

    base_name = output_name or "rotation-plan.json"
    candidate = outputs_dir / base_name
    if not candidate.exists():
        return candidate
    stem = candidate.stem
    suffix = candidate.suffix or ".json"
    index = 2
    while True:
        candidate = outputs_dir / f"{stem}-{index}{suffix}"
        if not candidate.exists():
            return candidate
        index += 1


def _migrate_project_artifact(
    request: ProjectMigrationRequest,
    *,
    dry_run: bool,
) -> ProjectMigrationResponse:
    try:
        _project, paths = load_project_paths(request.project_path)
        source_path = (
            paths.project_file
            if request.artifact_path is None
            else _resolve_project_artifact(paths, request.artifact_path)
        )
        if not source_path.is_file():
            raise InputFileError("The selected project artifact does not exist.")
        source_data = read_json(source_path)
        _artifact, source_model = parse_migratable_artifact(source_data, source_path)
        normalized_data = source_model.model_dump(mode="json")
        change_count, changes = _migration_change_summary(
            source_data,
            merge_normalized_data(source_data, normalized_data),
        )
        output_path = _migration_output_path(
            paths,
            source_path,
            in_place=request.in_place,
        )
        result = migrate_json_file(
            source_path,
            output=output_path,
            in_place=request.in_place,
            dry_run=dry_run,
            create_backup=True,
        )
        after_valid = (
            None
            if result.dry_run
            else _validate_migration_output(result.output_path)
        )
        reference_checks = (
            _project_reference_checks(source_model, source_path)
            if isinstance(source_model, SeatTrellisProject)
            else []
        )
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_migration_failed",
            message="The selected project artifact could not be migrated.",
        ) from exc
    return ProjectMigrationResponse(
        project_path=str(paths.project_file),
        source_path=str(source_path),
        artifact=result.artifact,
        schema_version=result.schema_version,
        output_path=str(result.output_path) if result.output_path is not None else None,
        backup_path=str(result.backup_path) if result.backup_path is not None else None,
        dry_run=result.dry_run,
        before_valid=True,
        after_valid=after_valid,
        rollback_available=(
            result.backup_path is not None or not request.in_place
        ),
        change_count=change_count,
        changes=changes,
        reference_checks=reference_checks,
    )


def _project_reference_checks(
    project: object,
    project_path: Path,
) -> list[ProjectMigrationReferenceCheck]:
    """Check project references without reading their contents into the API."""

    checks: list[ProjectMigrationReferenceCheck] = []
    for field, expected in (
        ("students", "file"),
        ("layout", "file"),
        ("rules", "file"),
        ("history_dir", "directory"),
        ("outputs_dir", "directory"),
    ):
        configured = getattr(project, field, None)
        if configured is None:
            continue
        configured_path = str(configured)
        resolved = (project_path.parent / configured_path).resolve()
        if not resolved.exists():
            status = "missing"
        elif expected == "file" and not resolved.is_file():
            status = "wrong_type"
        elif expected == "directory" and not resolved.is_dir():
            status = "wrong_type"
        else:
            status = "ok"
        checks.append(
            ProjectMigrationReferenceCheck(
                field=field,  # type: ignore[arg-type]
                path=configured_path,
                expected=expected,  # type: ignore[arg-type]
                status=status,  # type: ignore[arg-type]
            )
        )
    return checks


def _migration_output_path(
    paths: ProjectPaths,
    source_path: Path,
    *,
    in_place: bool,
) -> Path | None:
    if in_place:
        return None
    if source_path == paths.project_file:
        directory = source_path.parent
    else:
        directory = paths.outputs_dir
    directory.mkdir(parents=True, exist_ok=True)
    stem = source_path.name.removesuffix(".json")
    candidate = directory / f"{stem}.migrated.json"
    suffix = 2
    while candidate.exists():
        candidate = directory / f"{stem}.migrated-{suffix}.json"
        suffix += 1
    return candidate


def _validate_migration_output(path: Path | None) -> bool:
    """Reparse a written artifact so the API can report post-write validity."""

    if path is None or not path.is_file():
        raise InputFileError("The migrated artifact was not written.")
    parse_migratable_artifact(read_json(path), path)
    return True


def _migration_change_summary(
    before: object,
    after: object,
    *,
    limit: int = 200,
) -> tuple[int, list[ProjectMigrationChange]]:
    """Compare normalized JSON without returning any original values."""

    count = 0
    changes: list[ProjectMigrationChange] = []

    def visit(left: object, right: object, path: str) -> None:
        nonlocal count
        if isinstance(left, dict) and isinstance(right, dict):
            keys = sorted(set(left) | set(right))
            for key in keys:
                child_path = f"{path}.{key}" if path else str(key)
                if key not in left:
                    record(child_path, "added", None, _json_type(right[key]))
                elif key not in right:
                    record(child_path, "removed", _json_type(left[key]), None)
                else:
                    visit(left[key], right[key], child_path)
            return
        if isinstance(left, list) and isinstance(right, list):
            shared = min(len(left), len(right))
            for index in range(shared):
                visit(left[index], right[index], f"{path}[{index}]")
            for index in range(shared, len(left)):
                record(f"{path}[{index}]", "removed", _json_type(left[index]), None)
            for index in range(shared, len(right)):
                record(f"{path}[{index}]", "added", None, _json_type(right[index]))
            return
        if left != right:
            record(path or "$", "changed", _json_type(left), _json_type(right))

    def record(
        path: str,
        change: Literal["added", "removed", "changed"],
        before_type: str | None,
        after_type: str | None,
    ) -> None:
        nonlocal count
        count += 1
        if len(changes) < limit:
            changes.append(
                ProjectMigrationChange(
                    path=path,
                    change=change,
                    before_type=before_type,
                    after_type=after_type,
                )
            )

    visit(before, after, "")
    return count, changes


def _json_type(value: object) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return "unknown"


def _resolve_project_artifact(paths: ProjectPaths, requested: str) -> Path:
    """Resolve an artifact only when it remains inside the project workspace."""

    candidate = Path(requested).expanduser().resolve()
    allowed_directories = [
        directory
        for directory in (
            paths.history_dir,
            paths.outputs_dir,
        )
        if directory is not None
    ]
    if candidate == paths.project_file:
        return candidate
    if not candidate.is_file() or not any(
        candidate.parent == directory or directory in candidate.parents
        for directory in allowed_directories
    ):
        raise InputFileError(
            "The artifact must be a file inside the project history or outputs directory."
        )
    return candidate


def _resolve_project_migration_backup(paths: ProjectPaths, requested: str) -> Path:
    """Resolve a migration backup inside the selected project workspace."""

    candidate = Path(requested).expanduser().resolve()
    allowed_directories = [paths.root]
    if paths.history_dir is not None:
        allowed_directories.append(paths.history_dir)
    allowed_directories.append(paths.outputs_dir)
    inside_workspace = any(
        candidate.parent == directory or directory in candidate.parents
        for directory in allowed_directories
    )
    if not candidate.is_file() or not inside_workspace:
        raise InputFileError(
            "The migration backup must be a file inside the project workspace."
        )
    if ".bak" not in candidate.name:
        raise InputFileError("The selected file is not a migration backup.")
    return candidate


def _snapshot_for_artifact(
    path: Path,
) -> tuple[
    Literal["snapshot", "candidate_set", "rotation_plan", "unknown"],
    SeatingSnapshot,
    datetime | None,
]:
    data = read_json(path)
    kind = data.get("kind")
    if kind == "candidate_set":
        artifact = load_candidate_set(path)
        return "candidate_set", artifact.get_candidate("recommended").snapshot, artifact.created_at
    if kind == "rotation_plan":
        artifact = load_rotation_plan(path)
        return "rotation_plan", artifact.periods[0].snapshot, artifact.created_at
    if kind in {None, "snapshot"}:
        artifact = load_seating_artifact(path)
        if not isinstance(artifact, SeatingSnapshot):
            raise InputFileError(f"Expected a snapshot artifact: {path}")
        return "snapshot", artifact, artifact.created_at
    raise InputFileError(f"Unsupported project artifact kind: {kind}")


def _artifact_summary(
    path: Path,
    *,
    kind: Literal["snapshot", "candidate_set", "rotation_plan", "unknown"],
    snapshot: SeatingSnapshot,
    created_at: datetime | None,
) -> ProjectArtifactSummary:
    return ProjectArtifactSummary(
        name=path.name,
        path=str(path),
        kind=kind,
        created_at=created_at,
        student_count=len(snapshot.students),
        assignment_count=len(snapshot.assignments),
        enabled_seat_count=len(snapshot.layout.enabled_seats),
        solver_status=snapshot.solver_status,
    )


def _next_restored_path(outputs_dir: Path, source_stem: str) -> Path:
    clean_stem = source_stem.removesuffix(".snapshot")
    base = outputs_dir / f"restored-{clean_stem}.snapshot.json"
    if not base.exists():
        return base
    index = 2
    while True:
        candidate = outputs_dir / f"restored-{clean_stem}-{index}.snapshot.json"
        if not candidate.exists():
            return candidate
        index += 1


def project_privacy(request: ProjectPathRequest) -> ProjectPrivacyResponse:
    """Scan a project using the same conservative bundle policy as the CLI."""

    try:
        report = scan_project_privacy(
            request.project_path,
            include_outputs=request.include_outputs,
        )
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_unavailable",
            message="The selected project could not be scanned.",
        ) from exc
    return ProjectPrivacyResponse(
        project_path=str(Path(request.project_path).expanduser().resolve()),
        files_scanned=report.files_scanned,
        safe_for_public_sharing=report.safe_for_public_sharing,
        findings=[
            PrivacyFindingItem(file=item.file, fields=list(item.fields))
            for item in report.findings
        ],
    )


@dataclass(frozen=True)
class ProjectBundleArtifact:
    """A project bundle ready for a local HTTP download."""

    data: bytes
    filename: str


@dataclass(frozen=True)
class ProjectDownloadArtifact:
    """A generated local download with an explicit content type."""

    data: bytes
    filename: str
    content_type: str


def pack_project_for_web(request: ProjectPathRequest) -> ProjectBundleArtifact:
    """Create a bundle in a temporary directory and return only its bytes."""

    project_file = Path(request.project_path).expanduser().resolve()
    filename = _bundle_filename(project_file)
    try:
        with tempfile.TemporaryDirectory(prefix="seattrellis-bundle-") as directory:
            result = pack_project(
                project_file,
                Path(directory) / filename,
                include_outputs=request.include_outputs,
            )
            data = result.path.read_bytes()
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_bundle_failed",
            message="The project backup could not be created.",
        ) from exc
    return ProjectBundleArtifact(data=data, filename=filename)


def restore_project_bundle_file(
    bundle_path: str | Path,
    output_dir: str | Path,
    *,
    overwrite: bool = False,
) -> ProjectRestoreResponse:
    """Restore a bundle through the same path-safe implementation as the CLI."""

    try:
        project_path = restore_bundle(bundle_path, output_dir, overwrite=overwrite)
    except (InputFileError, OSError, ValueError) as exc:
        raise ApiProblem(
            status_code=422,
            code="project_restore_failed",
            message="The project backup could not be restored to that folder.",
        ) from exc
    return ProjectRestoreResponse(
        project_path=str(project_path),
        output_dir=str(Path(output_dir).expanduser().resolve()),
    )


def _artifact_items(
    paths: Iterable[Path],
    warnings: list[str],
) -> list[ProjectArtifactItem]:
    items: list[ProjectArtifactItem] = []
    for path in paths:
        try:
            data = read_json(path)
            stat = path.stat()
        except (InputFileError, OSError):
            warnings.append(f"Could not read project artifact {path.name}.")
            continue
        kind = data.get("kind")
        if kind is None and "assignments" in data:
            kind = "snapshot"
        if kind not in {"snapshot", "candidate_set", "rotation_plan"}:
            kind = "unknown"
        periods = data.get("periods")
        students = data.get("students")
        first_period = periods[0] if isinstance(periods, list) and periods else None
        first_snapshot = (
            first_period.get("snapshot")
            if isinstance(first_period, dict)
            else None
        )
        if not isinstance(students, list) and isinstance(first_snapshot, dict):
            students = first_snapshot.get("students")
        created_at = _parse_artifact_datetime(data.get("created_at"))
        provenance = _artifact_provenance(data, kind=kind)
        operation_history, operation_history_truncated = _artifact_operation_history(
            data,
            kind=kind,
        )
        items.append(
            ProjectArtifactItem(
                name=path.name,
                path=str(path),
                kind=kind,
                modified_at=datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc),
                created_at=created_at,
                size_bytes=stat.st_size,
                student_count=len(students) if isinstance(students, list) else None,
                period_count=len(periods) if isinstance(periods, list) else None,
                provenance=provenance,
                operation_history=operation_history,
                operation_history_truncated=operation_history_truncated,
            )
        )
        # Candidate scores and student records are intentionally not returned
        # in this browsing response.
    return items


_SAFE_EDIT_OPERATION_KINDS = frozenset(
    {
        "swap_students",
        "move_student",
        "batch_move",
        "seat_student",
        "unseat_student",
        "lock_student",
        "unlock_student",
        "lock_seat",
        "unlock_seat",
    }
)


def _artifact_operation_history(
    data: dict[str, object],
    *,
    kind: str,
) -> tuple[list[ProjectArtifactOperation], bool]:
    """Return a bounded, identifier-free summary of editing commands."""

    entries: list[ProjectArtifactOperation] = []
    truncated = False
    for metadata in _artifact_metadata_values(data, kind=kind):
        manual_edit = metadata.get("manual_edit")
        if not isinstance(manual_edit, dict):
            manual_edit = metadata.get("source_manual_edit")
        if not isinstance(manual_edit, dict):
            continue
        commands = manual_edit.get("commands")
        if isinstance(commands, list):
            for command in commands:
                if not isinstance(command, dict):
                    continue
                if len(entries) >= 100:
                    truncated = True
                    break
                action = command.get("action")
                safe_action = action if action in {"apply", "undo", "redo"} else "unknown"
                operations = command.get("operations")
                kinds: list[str] = []
                if isinstance(operations, list):
                    for operation in operations:
                        if not isinstance(operation, dict):
                            continue
                        operation_kind = operation.get("kind")
                        safe_kind = (
                            operation_kind
                            if isinstance(operation_kind, str)
                            and operation_kind in _SAFE_EDIT_OPERATION_KINDS
                            else "other"
                        )
                        if safe_kind not in kinds and len(kinds) < 5:
                            kinds.append(safe_kind)
                entries.append(
                    ProjectArtifactOperation(
                        sequence=len(entries) + 1,
                        action=safe_action,  # type: ignore[arg-type]
                        operation_count=(
                            min(len(operations), 100)
                            if isinstance(operations, list)
                            else 0
                        ),
                        operation_kinds=kinds,
                    )
                )
            continue

        operations = manual_edit.get("operations")
        if isinstance(operations, list):
            if len(entries) >= 100:
                truncated = True
                continue
            kinds = []
            for operation in operations:
                if not isinstance(operation, dict):
                    continue
                operation_kind = operation.get("kind")
                safe_kind = (
                    operation_kind
                    if isinstance(operation_kind, str)
                    and operation_kind in _SAFE_EDIT_OPERATION_KINDS
                    else "other"
                )
                if safe_kind not in kinds and len(kinds) < 5:
                    kinds.append(safe_kind)
            entries.append(
                ProjectArtifactOperation(
                    sequence=len(entries) + 1,
                    action="apply",
                    operation_count=min(len(operations), 100),
                    operation_kinds=kinds,
                )
            )
    return entries, truncated


def _artifact_metadata_values(
    data: dict[str, object],
    *,
    kind: str,
) -> list[dict[str, object]]:
    """Collect nested metadata locally for provenance and timeline summaries."""

    metadata_values: list[dict[str, object]] = []
    top_metadata = data.get("metadata")
    if isinstance(top_metadata, dict):
        metadata_values.append(top_metadata)
    if kind == "rotation_plan":
        periods = data.get("periods")
        if isinstance(periods, list):
            for period in periods:
                if not isinstance(period, dict):
                    continue
                snapshot = period.get("snapshot")
                if isinstance(snapshot, dict) and isinstance(snapshot.get("metadata"), dict):
                    metadata_values.append(snapshot["metadata"])
    elif kind == "candidate_set":
        candidates = data.get("candidates")
        if isinstance(candidates, list):
            for candidate in candidates:
                if not isinstance(candidate, dict):
                    continue
                snapshot = candidate.get("snapshot")
                if isinstance(snapshot, dict) and isinstance(snapshot.get("metadata"), dict):
                    metadata_values.append(snapshot["metadata"])
    return metadata_values


def _artifact_provenance(
    data: dict[str, object],
    *,
    kind: str,
) -> ProjectArtifactProvenance | None:
    """Summarize artifact origin without returning student-sensitive metadata.

    Older history files do not carry provenance.  They remain visible with an
    ``unknown`` source, while newer generated, edited, rotated, and restored
    artifacts expose a small stable summary.  Nested period/candidate metadata
    is inspected locally and reduced to counts only.
    """

    metadata_values = _artifact_metadata_values(data, kind=kind)

    parent_name: str | None = None
    operation_count = 0
    has_operation_count = False
    source: str | None = None
    for metadata in metadata_values:
        restored_from = metadata.get("restored_from")
        if parent_name is None and isinstance(restored_from, str) and restored_from.strip():
            parent_name = Path(restored_from).name
        persistence = metadata.get("project_persistence")
        if isinstance(persistence, dict):
            artifact_path = persistence.get("artifact_path")
            if parent_name is None and isinstance(artifact_path, str) and artifact_path.strip():
                parent_name = Path(artifact_path).name
            if persistence.get("source") == "react_rotation_editor":
                source = "rotation_edit"
        if metadata.get("saved_from") == "react_rotation_editor":
            source = "rotation_edit"
        manual_edit = metadata.get("manual_edit")
        if not isinstance(manual_edit, dict):
            manual_edit = metadata.get("source_manual_edit")
        if isinstance(manual_edit, dict):
            count = manual_edit.get("operation_count")
            if isinstance(count, int) and not isinstance(count, bool) and count >= 0:
                operation_count += count
                has_operation_count = True
            if source != "rotation_edit":
                source = "manual_edit"

    if parent_name is not None:
        source = "restored"
    if source is None:
        # A solver status or candidate metadata marks a file as generated even
        # when it predates the explicit provenance fields.
        if data.get("solver_status") or data.get("kind") in {"candidate_set", "rotation_plan"}:
            source = "generated"
        else:
            source = "unknown"

    return ProjectArtifactProvenance(
        source=source,  # type: ignore[arg-type]
        parent_name=parent_name,
        operation_count=min(operation_count, 100_000) if has_operation_count else None,
    )


def _path_sort_key(path: Path) -> int:
    try:
        return path.stat().st_mtime_ns
    except OSError:
        return 0


def _parse_artifact_datetime(value: object) -> datetime | None:
    if not isinstance(value, str) or not value.strip():
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return parsed.replace(tzinfo=timezone.utc) if parsed.tzinfo is None else parsed


def _bundle_filename(project_file: Path) -> str:
    name = project_file.name
    for suffix in (".seattrellis.json", ".project.json", ".json"):
        if name.endswith(suffix):
            return f"{name[:-len(suffix)]}.seattrellis.zip"
    return f"{name}.seattrellis.zip"


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
    *,
    draft_store: EditorDraftStore | None = None,
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
    editor_store = draft_store or EditorDraftStore()
    period_editors: list[EditorStateEnvelope] = []
    for period in output.plan.periods:
        period_score = score_snapshot(period.snapshot)
        period_candidate = CandidatePlan(
            candidate_id=f"period-{period.period}",
            snapshot=period.snapshot,
            score=period_score,
            hard_constraints_satisfied=(
                period_score.breakdown.hard_constraint_summary.satisfied
            ),
            metadata={
                "rotation_plan": output.plan.name,
                "rotation_period": period.period,
            },
        )
        period_editors.append(
            editor_store.create(
                CandidateSet(
                    candidates=[period_candidate],
                    recommended_candidate_id=period_candidate.candidate_id,
                    metadata={
                        "source": "rotation_plan",
                        "period_count": output.plan.period_count,
                    },
                )
            )
        )
    return GenerateRotationPlanResponse(
        class_name=draft.name,
        goal=_goal_summary(readiness.resolved_goal),
        warnings=list(dict.fromkeys((*readiness.warnings, *output.plan.warnings))),
        rotation_plan=output.plan,
        editor=period_editors[0],
        period_editors=period_editors,
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
                hard_rules=request.draft.goal.hard_rules,
                rules_overlay=request.draft.goal.rules_overlay,
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
