"""Transport-independent handlers behind the local ``/api/v1`` routes."""

from __future__ import annotations

import importlib.util
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
    ProjectArtifactCompareResponse,
    ProjectArtifactDiff,
    ProjectArtifactAssignmentChange,
    ProjectArtifactRequest,
    ProjectArtifactRestoreResponse,
    ProjectArtifactSummary,
    ProjectHistoryResponse,
    ProjectListResponse,
    ProjectMigrationRequest,
    ProjectMigrationResponse,
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
from seattrellis.schema_migration import migrate_json_file
from seattrellis.service_types import (
    CANVAS_EXPORT_FORMATS,
    ExportRequest,
    PageOptions,
    export_extension,
    RotationInput,
)
from seattrellis.service import compute_rotation_plan
from seattrellis.models.candidate import CandidatePlan, CandidateSet
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
            "project-rotation-save",
            "project-rotation-load",
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
    )


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
            )
        )
        # Candidate scores and student records are intentionally not returned
        # in this browsing response.
    return items


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
