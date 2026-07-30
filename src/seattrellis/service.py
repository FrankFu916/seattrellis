"""Business operations shared by the CLI and Web interface.

``compute_*`` functions work with in-memory models. The remaining public
functions handle file loading, output paths, and command-oriented formatting.
"""

from __future__ import annotations

import sys
from copy import deepcopy
from dataclasses import replace
from datetime import datetime, timezone
from importlib.metadata import PackageNotFoundError, version
from math import isfinite
from pathlib import Path
from typing import Sequence

from seattrellis import __version__
from seattrellis.candidates import generate_candidate_set
from seattrellis.demo import create_demo_files
from seattrellis.editing import (
    EditingLockState,
    EditingOperation,
    EditingSession,
    lock_state_from_snapshot,
    snapshot_with_lock_state,
)
from seattrellis.exporters import export_candidate_report_html, export_snapshot
from seattrellis.history import (
    build_fairness_report,
    build_pair_history,
    build_pair_history_report,
    build_seat_history,
    format_history_report,
    format_pair_history_report,
    load_history_snapshots,
)
from seattrellis.io.json_files import (
    InputFileError,
    load_layout,
    load_seating_artifact,
    write_json_model,
)
from seattrellis.io.project import (
    ProjectPaths,
    find_latest_project_artifact,
    load_project_paths,
    write_project,
)
from seattrellis.io.students import read_students
from seattrellis.io.validation import (
    ValidationReport,
    validate_files,
    validate_loaded_inputs,
)
from seattrellis.models.candidate import CandidatePlan, CandidateSet, MultiSolveOptions
from seattrellis.models.history import FairnessReport, PairHistoryReport
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import (
    export_preset,
    format_preset,
    format_preset_list,
    get_preset,
    load_rules_with_preset,
    preset_context_warnings,
)
from seattrellis.repair import compile_repair_context, format_repair_solve_failure
from seattrellis.scoring import build_plan_comparison_report, evaluate_hard_constraints
from seattrellis.service_types import (
    EditInput,
    EditOutput,
    ExportRequest,
    HistoryReportInput,
    HistoryReportOutput,
    PairReportInput,
    PairReportOutput,
    ProjectInfoInput,
    ProjectInfoOutput,
    RepairInput,
    RepairOutput,
    SolveInput,
    SolveOutput,
    ValidateInput,
    ValidateOutput,
    export_extension,
    score_text,
)
from seattrellis.solver import SeatTrellisSolveError, solve_seating
from seattrellis.solver.backend import (
    SOLVER_BACKENDS,
    normalize_solver_backend,
    resolve_solver_backend,
    solver_backend_environment_summary,
)


# In-memory operations


def compute_solve(input: SolveInput) -> SolveOutput:
    """Solve from models already loaded in memory."""
    if not 1 <= input.candidate_count <= 20:
        raise ValueError("candidate_count must be between 1 and 20")
    if not isfinite(input.time_limit_seconds) or input.time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")

    validation = validate_loaded_inputs(input.students, input.layout, input.rules)
    validation.raise_for_errors(title="Input validation failed.")

    snapshots = list(input.history_snapshots or [])
    preset = get_preset(input.preset_name) if input.preset_name is not None else None
    preset_warnings = preset_context_warnings(
        preset,
        input.students,
        history_count=len(snapshots),
        rules=input.rules,
    )
    runtime_warnings = _dedupe_warnings([*validation.warnings, *preset_warnings])

    seat_history = build_seat_history(input.students, input.layout, snapshots)
    pair_rule = input.rules.soft.avoid_recent_neighbors
    pair_history = build_pair_history(
        input.students,
        input.layout,
        snapshots,
        lookback=pair_rule.lookback,
        within_distance=pair_rule.within_distance,
    )

    options = MultiSolveOptions(
        candidate_count=input.candidate_count,
        seed=input.rules.seed if input.seed is None else input.seed,
    )
    candidate_set = generate_candidate_set(
        input.students,
        input.layout,
        input.rules,
        history=seat_history,
        pair_history=pair_history,
        history_snapshots=snapshots,
        options=options,
        time_limit_seconds=input.time_limit_seconds,
        backend=input.backend,
    )

    _apply_preset_metadata(
        candidate_set,
        preset_name=input.preset_name,
        rules_overlay=False,
        warnings=runtime_warnings,
    )

    if input.candidate_count == 1:
        fairness = candidate_set.candidates[0].snapshot.metrics.get("fairness", {})
        summary = _format_solve_fairness_summary(fairness) if fairness else None
        summary = _append_warnings(summary, runtime_warnings)
    else:
        summary = _format_candidate_set_summary(candidate_set)

    report = build_plan_comparison_report(candidate_set, history_snapshots=snapshots)

    return SolveOutput(
        candidate_set=candidate_set,
        preset_warnings=preset_warnings,
        warnings=runtime_warnings,
        summary=summary,
        plan_comparison_report=report,
    )


def compute_validate(input: ValidateInput) -> ValidateOutput:
    """Validate models already loaded in memory."""
    report = validate_loaded_inputs(input.students, input.layout, input.rules)
    report.raise_for_errors(strict=input.strict)
    return ValidateOutput(report=report, formatted=report.format_success())


def compute_edit(input: EditInput) -> EditOutput:
    """Apply manual editing commands to a loaded snapshot."""
    saved_lock_state = lock_state_from_snapshot(input.snapshot)
    explicit_lock_state = EditingLockState.from_values(
        locked_students=input.locked_students,
        locked_seats=input.locked_seats,
    )
    initial_lock_state = EditingLockState.from_values(
        locked_students=(
            *saved_lock_state.locked_students,
            *explicit_lock_state.locked_students,
        ),
        locked_seats=(
            *saved_lock_state.locked_seats,
            *explicit_lock_state.locked_seats,
        ),
    )
    session = EditingSession.from_snapshot(
        input.snapshot,
        locked_students=initial_lock_state.locked_students,
        locked_seats=initial_lock_state.locked_seats,
    )
    summary = session.hard_constraint_summary()
    for operation in input.operations:
        summary = session.apply(operation)

    lock_state = session.lock_state
    result = EditOutput(
        snapshot=snapshot_with_lock_state(session.current_snapshot(), lock_state),
        hard_constraints=summary,
        unseated_students=session.unseated_students(),
        locked_students=sorted(session.locked_students),
        locked_seats=sorted(session.locked_seats),
        operation_log=session.operation_log,
        lock_state=lock_state,
    )
    return replace(
        result,
        snapshot=_snapshot_with_edit_metadata(result.snapshot, result),
    )


def compute_repair(input: RepairInput) -> RepairOutput:
    """Re-solve a draft while preserving requested local anchors."""

    input_lock_state = input.lock_state or EditingLockState()
    explicit_lock_state = EditingLockState.from_values(
        locked_students=input.locked_students,
        locked_seats=input.locked_seats,
    )
    context = compile_repair_context(
        input.snapshot,
        affected_students=input.affected_students,
        locked_students=(
            *input_lock_state.locked_students,
            *explicit_lock_state.locked_students,
        ),
        locked_seats=(
            *input_lock_state.locked_seats,
            *explicit_lock_state.locked_seats,
        ),
        reuse_saved_locks=input.reuse_saved_locks,
    )
    seed = input.snapshot.seed if input.seed is None else input.seed
    history_snapshots = list(input.history_snapshots)
    history = build_seat_history(
        input.snapshot.students,
        input.snapshot.layout,
        history_snapshots,
    )
    pair_rule = input.snapshot.rules.soft.avoid_recent_neighbors
    pair_history = build_pair_history(
        input.snapshot.students,
        input.snapshot.layout,
        history_snapshots,
        lookback=pair_rule.lookback,
        within_distance=pair_rule.within_distance,
    )
    try:
        solution = solve_seating(
            input.snapshot.students,
            context.solver_layout,
            context.solver_rules,
            history=history,
            pair_history=pair_history,
            seed=seed,
            time_limit_seconds=input.time_limit_seconds,
            backend=input.backend,
        )
    except SeatTrellisSolveError as exc:
        if (
            context.locked_students
            or context.locked_seats
            or context.requested_affected_students
        ):
            raise SeatTrellisSolveError(
                format_repair_solve_failure(context, exc)
            ) from exc
        raise
    metadata = dict(input.snapshot.metadata)
    previous_repair = metadata.get("repair")
    if isinstance(previous_repair, dict):
        repair_history = metadata.get("repair_history", [])
        if not isinstance(repair_history, list):
            repair_history = []
        metadata["repair_history"] = [*repair_history, previous_repair]
    metadata["repair"] = {
        "requested_affected_students": context.requested_affected_students,
        "effective_affected_students": context.effective_affected_students,
        "closure_added_students": context.closure_added_students,
        "locked_students": context.locked_students,
        "locked_seats": context.locked_seats,
        "mutable_students": context.mutable_students,
        "fixed_assignments": context.fixed_assignments,
        "temporary_fixed_assignments": context.temporary_fixed_assignments,
        "reserved_empty_seats": context.reserved_empty_seats,
        "reuse_saved_locks": input.reuse_saved_locks,
        "history_count": len(history_snapshots),
        "solver_backend": solution.metrics.get("solver_backend_effective"),
    }
    snapshot = solution.to_snapshot(
        students=input.snapshot.students,
        layout=input.snapshot.layout,
        rules=input.snapshot.rules,
        seed=seed,
        metadata=metadata,
    )
    lock_state = EditingLockState.from_values(
        locked_students=context.locked_students,
        locked_seats=context.locked_seats,
    )
    snapshot = snapshot_with_lock_state(snapshot, lock_state)
    metadata = dict(snapshot.metadata)
    repair_constraints = evaluate_hard_constraints(
        snapshot.assignments,
        snapshot.students,
        context.solver_layout,
        context.solver_rules,
    )
    if not repair_constraints.satisfied:
        raise SeatTrellisSolveError(
            "Repair solver returned a snapshot that violates repair anchors."
        )
    hard_constraints = EditingSession.from_snapshot(snapshot).hard_constraint_summary()
    if not hard_constraints.satisfied:
        raise SeatTrellisSolveError(
            "Repair solver returned a snapshot that violates hard constraints."
        )
    original_assignments = {
        assignment.student_key: assignment.seat_id
        for assignment in input.snapshot.assignments
    }
    changed_students = sorted(
        assignment.student_key
        for assignment in snapshot.assignments
        if original_assignments.get(assignment.student_key) != assignment.seat_id
    )
    metadata["repair"]["changed_students"] = changed_students
    metadata["repair"]["anchor_constraints_satisfied"] = repair_constraints.satisfied
    if hasattr(snapshot, "model_copy"):
        snapshot = snapshot.model_copy(  # type: ignore[attr-defined,assignment]
            update={"metadata": metadata}
        )
    else:
        snapshot = snapshot.copy(update={"metadata": metadata})
    snapshot = _snapshot_with_repair_provenance(snapshot)
    return RepairOutput(
        snapshot=snapshot,
        hard_constraints=hard_constraints,
        locked_students=context.locked_students,
        locked_seats=context.locked_seats,
        lock_state=lock_state,
        mutable_students=context.mutable_students,
        fixed_assignments=context.fixed_assignments,
        reserved_empty_seats=context.reserved_empty_seats,
        changed_students=changed_students,
    )


def compute_history_report(input: HistoryReportInput) -> HistoryReportOutput:
    """Build a seating-history report from loaded snapshots."""
    history = build_seat_history(input.students, input.layout, input.history_snapshots)
    report = build_fairness_report(history)
    return HistoryReportOutput(report=report, formatted=format_history_report(report))


def compute_pair_report(input: PairReportInput) -> PairReportOutput:
    """Build a pair-history report from loaded snapshots."""
    if input.top <= 0:
        raise ValueError("top must be positive.")
    if input.within_distance <= 0:
        raise ValueError("within_distance must be positive.")
    pair_history = build_pair_history(
        input.students,
        input.layout,
        input.history_snapshots,
        within_distance=input.within_distance,
    )
    report = build_pair_history_report(pair_history, top=input.top)
    return PairReportOutput(
        report=report,
        formatted=format_pair_history_report(report, top=input.top),
    )


def compute_project_info(input: ProjectInfoInput) -> ProjectInfoOutput:
    """Format project information from a loaded project and resolved paths."""
    lines = [
        f"Project: {input.project.name}",
        f"Project file: {input.paths.project_file}",
        f"Schema version: {input.project.schema_version}",
        "",
        "Paths:",
        _format_project_path("students", input.project.students, input.paths.students),
        _format_project_path("layout", input.project.layout, input.paths.layout),
        _format_project_path("rules", input.project.rules, input.paths.rules),
    ]
    if input.project.history_dir is None:
        lines.append("- history_dir: not configured")
    else:
        lines.append(
            _format_project_path(
                "history_dir", input.project.history_dir, input.paths.history_dir
            )
        )
    lines.extend(
        [
            _format_project_path(
                "outputs_dir", input.project.outputs_dir, input.paths.outputs_dir
            ),
            "",
            "Defaults:",
            f"- candidates: {input.project.default_candidates}",
            f"- candidate: {input.project.default_candidate}",
            f"- export format: {input.project.default_export_format}",
        ]
    )
    return ProjectInfoOutput(formatted="\n".join(lines))


# File-oriented operations


def solve(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    output_path: str | Path = "outputs/latest.snapshot.json",
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    time_limit_seconds: float = 3.0,
    candidate_count: int = 1,
    seed: int | None = None,
    report_path: str | Path | None = None,
    backend: str = "auto",
) -> Path:
    path, _summary = solve_with_report(
        students_path=students_path,
        layout_path=layout_path,
        rules_path=rules_path,
        preset_name=preset_name,
        output_path=output_path,
        history_paths=history_paths,
        history_dir=history_dir,
        time_limit_seconds=time_limit_seconds,
        candidate_count=candidate_count,
        seed=seed,
        report_path=report_path,
        backend=backend,
    )
    return path


def solve_with_report(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    output_path: str | Path = "outputs/latest.snapshot.json",
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    time_limit_seconds: float = 3.0,
    candidate_count: int = 1,
    seed: int | None = None,
    report_path: str | Path | None = None,
    backend: str = "auto",
) -> tuple[Path, str | None]:
    if not 1 <= candidate_count <= 20:
        raise ValueError("candidate_count must be between 1 and 20")
    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")
    backend = normalize_solver_backend(backend)

    students = read_students(students_path)
    layout = load_layout(layout_path)
    rules, preset = load_rules_with_preset(
        rules_path=rules_path,
        preset_name=preset_name,
    )
    history_snapshots = load_history_snapshots(
        history_paths=history_paths, history_dir=history_dir
    )

    result = compute_solve(
        SolveInput(
            students=students,
            layout=layout,
            rules=rules,
            preset_name=preset.name if preset is not None else preset_name,
            history_snapshots=history_snapshots,
            candidate_count=candidate_count,
            seed=seed,
            time_limit_seconds=time_limit_seconds,
            backend=backend,
        )
    )
    candidate_set = result.candidate_set
    preset_warnings = result.preset_warnings or []
    runtime_warnings = result.warnings or preset_warnings
    summary = result.summary

    # Re-apply preset metadata with full context (rules_overlay, etc.)
    _apply_preset_metadata(
        candidate_set,
        preset_name=preset.name if preset is not None else None,
        rules_overlay=rules_path is not None and preset is not None,
        warnings=runtime_warnings,
    )

    if candidate_count == 1:
        snapshot = candidate_set.candidates[0].snapshot
        path = write_json_model(snapshot, output_path)
        fairness = snapshot.metrics.get("fairness", {})
        summary = _format_solve_fairness_summary(fairness) if fairness else None
        summary = _append_warnings(summary, runtime_warnings)
    else:
        path = write_json_model(candidate_set, output_path)
        summary = _format_candidate_set_summary(candidate_set)

    if report_path is not None:
        report = build_plan_comparison_report(
            candidate_set, history_snapshots=history_snapshots
        )
        report_output = write_json_model(report, report_path)
        report_line = f"Full report written to {report_output}"
        summary = f"{summary}\n\n{report_line}" if summary else report_line

    return path, summary


def export(
    *,
    snapshot_path: str | Path,
    output_format: str | None = None,
    output_path: str | Path | None = None,
    candidate_id: str | None = None,
    default_candidate_id: str = "recommended",
    request: ExportRequest | None = None,
) -> Path:
    if request is None:
        if output_format is None:
            raise ValueError("output_format is required when request is not provided.")
        request = ExportRequest(
            output_format=output_format,
            output_path=output_path,
            candidate_id=candidate_id,
        )
    else:
        if output_format is not None and output_format.lower() != request.output_format:
            raise ValueError("output_format conflicts with request.output_format.")
        if output_path is not None and Path(output_path) != request.resolved_output_path:
            raise ValueError("output_path conflicts with request.output_path.")
        if candidate_id is not None and candidate_id != request.candidate_id:
            raise ValueError("candidate_id conflicts with request.candidate_id.")

    artifact = load_seating_artifact(snapshot_path)
    if request.candidate_scope == "all":
        if not isinstance(artifact, CandidateSet):
            raise ValueError("candidate_scope='all' requires a candidate set artifact.")
        if request.candidate_id is not None:
            raise ValueError("candidate_id cannot be combined with candidate_scope='all'.")
        if request.output_format not in {"html", "print-html"}:
            raise ValueError(
                "candidate_scope='all' currently supports only html and print-html exports."
            )
        report = build_plan_comparison_report(artifact)
        return export_candidate_report_html(
            artifact,
            report,
            request.resolved_output_path,
            page=request.page,
            locale=request.locale,
        )

    selected_candidate: CandidatePlan | None = None
    if isinstance(artifact, CandidateSet):
        selected_candidate = artifact.get_candidate(
            request.candidate_id or default_candidate_id
        )
        snapshot = _snapshot_with_candidate_metadata(selected_candidate)
    else:
        if request.candidate_id is not None:
            raise ValueError(
                "--candidate can only be used when --snapshot is a candidate set."
            )
        snapshot = artifact
    return export_snapshot(
        snapshot,
        candidate=selected_candidate,
        request=request,
    )


def edit_snapshot(
    *,
    snapshot_path: str | Path,
    output_path: str | Path = "outputs/edited.snapshot.json",
    operations: Sequence[EditingOperation],
    candidate_id: str | None = None,
    default_candidate_id: str = "recommended",
    locked_students: Sequence[str] | None = None,
    locked_seats: Sequence[str] | None = None,
    strict: bool = False,
) -> tuple[Path, str]:
    """Apply manual edit commands to a snapshot or selected candidate."""
    operations = list(operations)
    if not operations:
        raise ValueError("At least one editing operation is required.")
    candidate_id = str(candidate_id).strip() if candidate_id is not None else None
    if candidate_id == "":
        raise ValueError("candidate_id cannot be empty.")

    artifact = load_seating_artifact(snapshot_path)
    if isinstance(artifact, CandidateSet):
        selected_candidate = artifact.get_candidate(candidate_id or default_candidate_id)
        snapshot = _snapshot_with_candidate_metadata(selected_candidate)
    else:
        if candidate_id is not None:
            raise ValueError("--candidate can only be used when editing a candidate set.")
        snapshot = artifact

    result = compute_edit(
        EditInput(
            snapshot=snapshot,
            operations=operations,
            locked_students=tuple(locked_students or ()),
            locked_seats=tuple(locked_seats or ()),
        )
    )
    if strict and not result.hard_constraints.satisfied:
        raise ValueError(_format_edit_strict_failure(result))

    path = write_json_model(result.snapshot, output_path)
    return path, _format_edit_summary(result)


def repair_snapshot(
    *,
    snapshot_path: str | Path,
    output_path: str | Path = "outputs/repaired.snapshot.json",
    candidate_id: str | None = None,
    default_candidate_id: str = "recommended",
    affected_students: Sequence[str] = (),
    locked_students: Sequence[str] = (),
    locked_seats: Sequence[str] = (),
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    reuse_saved_locks: bool = True,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    backend: str = "auto",
) -> tuple[Path, str]:
    """Re-solve a snapshot or selected candidate while preserving draft locks."""

    candidate_id = str(candidate_id).strip() if candidate_id is not None else None
    if candidate_id == "":
        raise ValueError("candidate_id cannot be empty.")
    backend = normalize_solver_backend(backend)

    artifact = load_seating_artifact(snapshot_path)
    if isinstance(artifact, CandidateSet):
        selected_candidate = artifact.get_candidate(candidate_id or default_candidate_id)
        snapshot = _snapshot_with_candidate_metadata(selected_candidate)
    else:
        if candidate_id is not None:
            raise ValueError("--candidate can only be used when repairing a candidate set.")
        snapshot = artifact

    history_snapshots = load_history_snapshots(
        history_paths=history_paths,
        history_dir=history_dir,
    )

    result = compute_repair(
        RepairInput(
            snapshot=snapshot,
            affected_students=affected_students,
            locked_students=locked_students,
            locked_seats=locked_seats,
            reuse_saved_locks=reuse_saved_locks,
            history_snapshots=history_snapshots,
            seed=seed,
            time_limit_seconds=time_limit_seconds,
            backend=backend,
        )
    )
    path = write_json_model(result.snapshot, output_path)
    return path, _format_repair_summary(result)


def run_doctor() -> str:
    """Check the environment and return a diagnostic report."""
    lines: list[str] = []
    lines.append("=" * 52)
    lines.append("  SeatTrellis Doctor")
    lines.append("=" * 52)
    lines.append(f"  Version:      {__version__}")
    lines.append(f"  Python:       {sys.version.split()[0]}")
    lines.append(f"  Executable:   {sys.executable}")
    lines.append(f"  Platform:     {sys.platform}")

    extras_status: list[tuple[str, str, str]] = []
    for extra, package_name in [
        ("solver", "ortools"),
        ("excel", "openpyxl"),
        ("image", "Pillow"),
        ("web", "streamlit"),
        ("pdf", "weasyprint"),
        ("docx", "python-docx"),
    ]:
        try:
            version(package_name)
            extras_status.append((extra, "✅", package_name))
        except PackageNotFoundError:
            extras_status.append((extra, "❌", package_name))

    lines.append("")
    lines.append("  Optional extras (installed packages):")
    for extra, status, pkg in extras_status:
        lines.append(f"    {status} {extra:8s} ({pkg})")

    examples_dir = Path.cwd() / "examples"
    source_examples_dir = Path(__file__).resolve().parents[2] / "examples"
    if not examples_dir.is_dir() and source_examples_dir.is_dir():
        examples_dir = source_examples_dir
    lines.append("")
    lines.append(f"  Examples:     {examples_dir}")
    for fname in [
        "students.csv",
        "classroom.json",
        "rules.json",
        "project.seattrellis.json",
    ]:
        path = examples_dir / fname
        status = "✅" if path.exists() else "❌"
        lines.append(f"    {status} {fname}")

    outputs_dir = Path.cwd() / "outputs"
    lines.append("")
    lines.append(f"  Outputs dir:  {outputs_dir}")
    lines.append(
        f"    {'✅ exists' if outputs_dir.is_dir() else '⚠️  does not exist yet'}"
    )

    backend_env = solver_backend_environment_summary()
    requested_backend = "auto"
    effective_backend = resolve_solver_backend(requested_backend)
    lines.append("")
    lines.append("  Solver backend:")
    lines.append(f"    Default request: {requested_backend}")
    lines.append(f"    Effective default: {effective_backend}")
    lines.append(f"    Supported: {', '.join(SOLVER_BACKENDS)}")
    lines.append(f"    SEATTRELLIS_BACKEND: {backend_env['SEATTRELLIS_BACKEND']}")
    lines.append(f"    SEATTRELLIS_USE_ORTOOLS: {backend_env['SEATTRELLIS_USE_ORTOOLS']}")
    try:
        native_version = version("seattrellis-native")
    except PackageNotFoundError:
        native_line = "not installed"
    else:
        native_line = (
            f"installed ({native_version}; compatibility is checked only "
            "when selected)"
        )
    lines.append(f"    Native extension: {native_line}")

    lines.append("")
    lines.append(
        "  Privacy: bundled demo data is fictional; keep real classroom data private."
    )
    lines.append("=" * 52)
    return "\n".join(lines)


def run_validate(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    strict: bool = False,
) -> str:
    history_snapshots = load_history_snapshots(
        history_paths=history_paths,
        history_dir=history_dir,
    )
    report = validate_files(
        students_path=students_path,
        layout_path=layout_path,
        rules_path=rules_path,
        preset_name=preset_name,
        history_count=len(history_snapshots),
    )
    report.raise_for_errors(strict=strict)
    return report.format_success()


def run_history_report(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    output_path: str | Path | None = None,
) -> str:
    students = read_students(students_path)
    layout = load_layout(layout_path)
    snapshots = load_history_snapshots(
        history_paths=history_paths, history_dir=history_dir
    )
    result = compute_history_report(
        HistoryReportInput(students=students, layout=layout, history_snapshots=snapshots)
    )
    if output_path is not None:
        write_json_model(result.report, output_path)
    return result.formatted


def run_pair_report(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    output_path: str | Path | None = None,
    top: int = 10,
    within_distance: int = 2,
) -> str:
    if top <= 0:
        raise ValueError("top must be positive.")
    if within_distance <= 0:
        raise ValueError("within_distance must be positive.")
    students = read_students(students_path)
    layout = load_layout(layout_path)
    snapshots = load_history_snapshots(
        history_paths=history_paths, history_dir=history_dir
    )
    result = compute_pair_report(
        PairReportInput(
            students=students,
            layout=layout,
            history_snapshots=snapshots,
            top=top,
            within_distance=within_distance,
        )
    )
    if output_path is not None:
        write_json_model(result.report, output_path)
    return result.formatted


def init_demo(
    output_dir: str | Path = ".", *, overwrite: bool = False
) -> dict[str, Path]:
    return create_demo_files(output_dir, overwrite=overwrite)


def project_init(
    *,
    project_path: str | Path = "seattrellis.project.json",
    name: str = "SeatTrellis Project",
    students: str = "students.csv",
    layout: str = "classroom.json",
    rules: str = "rules.json",
    history_dir: str | None = None,
    outputs_dir: str = "outputs",
    candidates: int = 5,
    force: bool = False,
) -> Path:
    project = SeatTrellisProject(
        name=name,
        students=students,
        layout=layout,
        rules=rules,
        history_dir=history_dir,
        outputs_dir=outputs_dir,
        default_candidates=candidates,
    )
    return write_project(project, project_path, overwrite=force)


def project_info(
    *, project_path: str | Path = "seattrellis.project.json"
) -> str:
    project, paths = load_project_paths(project_path)
    return compute_project_info(
        ProjectInfoInput(project=project, paths=paths)
    ).formatted


def project_validate(
    *,
    project_path: str | Path = "seattrellis.project.json",
    strict: bool = False,
) -> str:
    _project, paths = load_project_paths(
        project_path,
        require_inputs=True,
        require_history=True,
    )
    return run_validate(
        students_path=paths.students,
        layout_path=paths.layout,
        rules_path=paths.rules,
        strict=strict,
    )


def project_solve(
    *,
    project_path: str | Path = "seattrellis.project.json",
    candidate_count: int | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    output_path: str | Path | None = None,
    report_path: str | Path | None = None,
    backend: str = "auto",
) -> tuple[Path, str | None]:
    project, paths = load_project_paths(
        project_path,
        require_inputs=True,
        require_history=True,
        create_outputs=True,
    )
    count = project.default_candidates if candidate_count is None else candidate_count
    if not 1 <= count <= 20:
        raise ValueError("candidates must be between 1 and 20.")
    if output_path is None:
        filename = (
            "latest.snapshot.json" if count == 1 else "latest.candidates.json"
        )
        output_path = paths.outputs_dir / filename
    return solve_with_report(
        students_path=paths.students,
        layout_path=paths.layout,
        rules_path=paths.rules,
        output_path=output_path,
        history_dir=paths.history_dir,
        time_limit_seconds=time_limit_seconds,
        candidate_count=count,
        seed=seed,
        report_path=report_path,
        backend=backend,
    )


def project_edit(
    *,
    project_path: str | Path = "seattrellis.project.json",
    snapshot_path: str | Path | None = None,
    candidate_id: str | None = None,
    operations: Sequence[EditingOperation],
    output_path: str | Path | None = None,
    strict: bool = False,
) -> tuple[Path, str]:
    project, paths = load_project_paths(project_path, create_outputs=True)
    selected_snapshot = (
        Path(snapshot_path)
        if snapshot_path is not None
        else find_latest_project_artifact(paths.outputs_dir)
    )
    if output_path is None:
        output_path = _edited_snapshot_output_path(selected_snapshot, paths.outputs_dir)
    return edit_snapshot(
        snapshot_path=selected_snapshot,
        output_path=output_path,
        operations=operations,
        candidate_id=candidate_id,
        default_candidate_id=project.default_candidate,
        strict=strict,
    )


def project_repair(
    *,
    project_path: str | Path = "seattrellis.project.json",
    snapshot_path: str | Path | None = None,
    candidate_id: str | None = None,
    affected_students: Sequence[str] = (),
    locked_students: Sequence[str] = (),
    locked_seats: Sequence[str] = (),
    reuse_saved_locks: bool = True,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    backend: str = "auto",
    output_path: str | Path | None = None,
) -> tuple[Path, str]:
    """Re-solve the latest or selected project artifact with draft locks."""

    project, paths = load_project_paths(
        project_path,
        require_history=True,
        create_outputs=True,
    )
    selected_snapshot = (
        Path(snapshot_path)
        if snapshot_path is not None
        else find_latest_project_artifact(paths.outputs_dir)
    )
    if output_path is None:
        output_path = _repaired_snapshot_output_path(selected_snapshot, paths.outputs_dir)
    return repair_snapshot(
        snapshot_path=selected_snapshot,
        output_path=output_path,
        candidate_id=candidate_id,
        default_candidate_id=project.default_candidate,
        affected_students=affected_students,
        locked_students=locked_students,
        locked_seats=locked_seats,
        history_dir=paths.history_dir,
        reuse_saved_locks=reuse_saved_locks,
        seed=seed,
        time_limit_seconds=time_limit_seconds,
        backend=backend,
    )


def project_export(
    *,
    project_path: str | Path = "seattrellis.project.json",
    snapshot_path: str | Path | None = None,
    output_format: str | None = None,
    candidate_id: str | None = None,
    output_path: str | Path | None = None,
) -> Path:
    project, paths = load_project_paths(project_path, create_outputs=True)
    selected_snapshot = (
        Path(snapshot_path)
        if snapshot_path is not None
        else find_latest_project_artifact(paths.outputs_dir)
    )
    selected_format = output_format or project.default_export_format
    if output_path is None:
        output_path = (
            paths.outputs_dir / f"seating.{export_extension(selected_format)}"
        )
    return export(
        snapshot_path=selected_snapshot,
        output_format=selected_format,
        output_path=output_path,
        candidate_id=candidate_id,
        default_candidate_id=project.default_candidate,
    )


# Shared formatting helpers


# ---------------------------------------------------------------------------
# Display / formatting helpers (moved from cli.py private functions)
# ---------------------------------------------------------------------------


def _solve_output_label(summary: str | None) -> str:
    return "Candidate set" if summary and summary.startswith("Generated ") else "Snapshot"


def _format_candidate_set_summary(candidate_set: CandidateSet) -> str:
    lines = [
        f"Generated {len(candidate_set.candidates)} candidate seating plans.",
        "",
        f"Recommended: {candidate_set.recommended_candidate_id}",
        "",
        "Candidate summary:",
    ]
    ranked = sorted(
        candidate_set.candidates,
        key=lambda candidate: (-candidate.total_score, candidate.candidate_id),
    )
    for candidate in ranked:
        breakdown = candidate.score.breakdown
        lines.append(
            f"- {candidate.candidate_id}: total {candidate.total_score:.1f} | "
            f"fair rotation {_dimension_rating(breakdown.fair_rotation_score.rating)} | "
            "neighbor repetition "
            f"{_neighbor_rating(breakdown.avoid_recent_neighbors_score.rating)} | "
            f"score balance {_dimension_rating(breakdown.score_balance_score.rating)}"
        )
    if candidate_set.warnings:
        lines.append("")
        lines.append("Warnings:")
        lines.extend(f"- {warning}" for warning in candidate_set.warnings)
    return "\n".join(lines)


def _apply_preset_metadata(
    candidate_set: CandidateSet,
    *,
    preset_name: str | None,
    rules_overlay: bool,
    warnings: Sequence[str],
) -> None:
    metadata = None
    if preset_name is not None:
        metadata = {
            "name": preset_name,
            "user_rules_overlay": rules_overlay,
        }
        candidate_set.metadata["preset"] = metadata
    for warning in warnings:
        if warning not in candidate_set.warnings:
            candidate_set.warnings.append(warning)
    for candidate in candidate_set.candidates:
        if metadata is not None:
            candidate.snapshot.metadata["preset"] = metadata
        if warnings:
            candidate.snapshot.metadata["warnings"] = list(warnings)


def _append_warnings(
    summary: str | None, warnings: Sequence[str]
) -> str | None:
    if not warnings:
        return summary
    warning_text = "\n".join(
        ["Warnings:", *(f"- {warning}" for warning in warnings)]
    )
    return f"{summary}\n\n{warning_text}" if summary else warning_text


def _dedupe_warnings(warnings: Sequence[str]) -> list[str]:
    deduped: list[str] = []
    for warning in warnings:
        if warning not in deduped:
            deduped.append(warning)
    return deduped


def _dimension_rating(rating: str) -> str:
    return rating.replace("_", " ")


def _neighbor_rating(rating: str) -> str:
    if rating == "high":
        return "low"
    if rating == "low":
        return "high"
    return _dimension_rating(rating)


def _snapshot_with_candidate_metadata(candidate: CandidatePlan) -> SeatingSnapshot:
    metadata = dict(candidate.snapshot.metadata)
    metadata["candidate"] = {
        "candidate_id": candidate.candidate_id,
        "total_score": candidate.total_score,
        "hard_constraints_satisfied": candidate.hard_constraints_satisfied,
        "score_breakdown": {
            "fair_rotation_score": candidate.score.breakdown.fair_rotation_score.score,
            "avoid_recent_neighbors_score": candidate.score.breakdown.avoid_recent_neighbors_score.score,
            "score_balance_score": candidate.score.breakdown.score_balance_score.score,
            "height_preference_score": candidate.score.breakdown.height_preference_score.score,
            "vision_preference_score": candidate.score.breakdown.vision_preference_score.score,
            "diversity_score": candidate.score.breakdown.diversity_score.score,
            "stability_score": candidate.score.breakdown.stability_score.score,
        },
    }
    if hasattr(candidate.snapshot, "model_copy"):
        return candidate.snapshot.model_copy(update={"metadata": metadata})  # type: ignore[attr-defined,return-value]
    return candidate.snapshot.copy(update={"metadata": metadata})


def _snapshot_with_edit_metadata(
    snapshot: SeatingSnapshot,
    result: EditOutput,
) -> SeatingSnapshot:
    metadata = dict(snapshot.metadata)
    candidate = metadata.pop("candidate", None)
    if isinstance(candidate, dict):
        metadata["source_candidate"] = candidate
    repair = metadata.pop("repair", None)
    if isinstance(repair, dict):
        metadata["source_repair"] = repair

    prior_edit = metadata.pop("manual_edit", None)
    prior_operations: list[object] = []
    prior_operation_count = 0
    if isinstance(prior_edit, dict):
        stored_operations = prior_edit.get("operations")
        if isinstance(stored_operations, list):
            prior_operations = deepcopy(stored_operations)
        stored_count = prior_edit.get("operation_count")
        if isinstance(stored_count, int) and stored_count >= 0:
            prior_operation_count = stored_count
        else:
            prior_operation_count = len(prior_operations)

    new_operations = [
        {
            "kind": record.operation.kind,
            "payload": deepcopy(record.operation.payload),
        }
        for record in result.operation_log
    ]
    source_solution = metadata.get("source_solution")
    if not isinstance(source_solution, dict) or snapshot.solver_status != "MANUAL_DRAFT":
        metadata["source_solution"] = {
            "created_at": snapshot.created_at.isoformat(),
            "solver_status": snapshot.solver_status,
            "objective_value": snapshot.objective_value,
            "metrics": deepcopy(snapshot.metrics),
        }

    operation_count = prior_operation_count + len(new_operations)
    edited_at = datetime.now(timezone.utc)
    metadata["manual_edit"] = {
        "edited_at": edited_at.isoformat(),
        "operation_count": operation_count,
        "operations": [*prior_operations, *new_operations],
        "locked_students": list(result.locked_students),
        "locked_seats": list(result.locked_seats),
        "unseated_students": list(result.unseated_students),
        "hard_constraints_satisfied": result.hard_constraints.satisfied,
        "violation_count": result.hard_constraints.violation_count,
    }
    current_metrics = {
        "manual_edit": {
            "operation_count": operation_count,
            "hard_constraints_satisfied": result.hard_constraints.satisfied,
            "violation_count": result.hard_constraints.violation_count,
        }
    }
    updates = {
        "created_at": edited_at,
        "metadata": metadata,
        "solver_status": "MANUAL_DRAFT",
        "objective_value": None,
        "metrics": current_metrics,
    }
    if hasattr(snapshot, "model_copy"):
        return snapshot.model_copy(  # type: ignore[attr-defined,return-value]
            update=updates
        )
    return snapshot.copy(update=updates)


def _snapshot_with_repair_provenance(snapshot: SeatingSnapshot) -> SeatingSnapshot:
    """Keep candidate origin without retaining stale pre-repair score metadata."""

    metadata = dict(snapshot.metadata)
    candidate = metadata.pop("candidate", None)
    if isinstance(candidate, dict):
        metadata["source_candidate"] = candidate
    manual_edit = metadata.pop("manual_edit", None)
    if isinstance(manual_edit, dict):
        metadata["source_manual_edit"] = manual_edit

    source_candidate = metadata.get("source_candidate")
    repair = metadata.get("repair")
    if isinstance(source_candidate, dict) and isinstance(repair, dict):
        repair = dict(repair)
        repair["source_candidate_id"] = source_candidate.get("candidate_id")
        metadata["repair"] = repair
    if hasattr(snapshot, "model_copy"):
        return snapshot.model_copy(  # type: ignore[attr-defined,return-value]
            update={"metadata": metadata}
        )
    return snapshot.copy(update={"metadata": metadata})


def _edited_snapshot_output_path(selected_snapshot: Path, outputs_dir: Path) -> Path:
    name = selected_snapshot.name
    for suffix in (".candidates.json", ".snapshot.json", ".json"):
        if name.endswith(suffix):
            return outputs_dir / f"{name.removesuffix(suffix)}.edited.snapshot.json"
    return outputs_dir / f"{selected_snapshot.stem}.edited.snapshot.json"


def _repaired_snapshot_output_path(selected_snapshot: Path, outputs_dir: Path) -> Path:
    name = selected_snapshot.name
    for suffix in (".candidates.json", ".snapshot.json", ".json"):
        if name.endswith(suffix):
            return outputs_dir / f"{name.removesuffix(suffix)}.repaired.snapshot.json"
    return outputs_dir / f"{selected_snapshot.stem}.repaired.snapshot.json"


def _format_solve_fairness_summary(fairness: object) -> str | None:
    if not isinstance(fairness, dict):
        return None
    history_count = fairness.get("history_count", 0)
    enabled_rules = fairness.get("enabled_rules", [])
    if not enabled_rules:
        message = fairness.get("message")
        if message:
            return f"Fairness: {message}"
        return (
            f"Fairness: history snapshots={history_count}, no active fairness rules."
        )
    fair_cost = fairness.get("fair_rotation_cost")
    neighbor_cost = fairness.get("avoid_recent_neighbors_cost")
    cost_parts = []
    if fair_cost is not None:
        cost_parts.append(f"fair_rotation_cost={fair_cost}")
    if neighbor_cost is not None:
        cost_parts.append(f"avoid_recent_neighbors_cost={neighbor_cost}")
    suffix = (
        ", ".join(cost_parts) if cost_parts else f"enabled_rules={enabled_rules}"
    )
    return f"Fairness: history snapshots={history_count}, {suffix}."


def _format_edit_summary(result: EditOutput) -> str:
    hard = result.hard_constraints
    lines = [
        "Manual edit summary:",
        f"- operations: {len(result.operation_log)}",
        f"- unseated students: {_format_preview(result.unseated_students)}",
        f"- locked students: {_format_preview(result.locked_students)}",
        f"- locked seats: {_format_preview(result.locked_seats)}",
        (
            f"- hard constraints: {'satisfied' if hard.satisfied else 'not satisfied'} "
            f"({hard.violation_count} violation(s))"
        ),
    ]
    if hard.violations:
        lines.append("Violations:")
        lines.extend(f"- {violation}" for violation in hard.violations[:10])
        if len(hard.violations) > 10:
            lines.append(f"- ... {len(hard.violations) - 10} more")
    return "\n".join(lines)


def _format_edit_strict_failure(result: EditOutput) -> str:
    violations = "\n".join(
        f"- {violation}" for violation in result.hard_constraints.violations[:10]
    )
    return (
        "Manual edits did not satisfy hard constraints; no snapshot was written."
        + (f"\n{violations}" if violations else "")
    )


def _format_repair_summary(result: RepairOutput) -> str:
    hard = result.hard_constraints
    lines = [
        "Repair summary:",
        f"- mutable students: {_format_preview(result.mutable_students)}",
        f"- fixed assignments: {len(result.fixed_assignments)}",
        f"- changed students: {_format_preview(result.changed_students)}",
        f"- locked students: {_format_preview(result.locked_students)}",
        f"- locked seats: {_format_preview(result.locked_seats)}",
        f"- reserved empty seats: {_format_preview(result.reserved_empty_seats)}",
        (
            f"- hard constraints: {'satisfied' if hard.satisfied else 'not satisfied'} "
            f"({hard.violation_count} violation(s))"
        ),
    ]
    return "\n".join(lines)


def _format_preview(values: Sequence[str]) -> str:
    if not values:
        return "none"
    preview = ", ".join(values[:5])
    if len(values) > 5:
        return f"{preview}, ... ({len(values)} total)"
    return preview


def _friendly_error(exc: Exception) -> str:
    return str(exc) or exc.__class__.__name__


def _format_project_path(
    label: str, configured: str, resolved: Path | None
) -> str:
    if resolved is None:
        return f"- {label}: {configured} [not configured]"
    status = "exists" if resolved.exists() else "missing"
    return f"- {label}: {configured} -> {resolved} [{status}]"
