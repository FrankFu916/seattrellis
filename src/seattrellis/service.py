"""SeatTrellis service layer — the single entry point for all business operations.

Architecture
------------

* **Category A — Pure compute functions** (in-memory models in/out, zero file I/O).
  These are the functions intended for future Tauri / REST / programmatic use.

* **Category B — Orchestration functions** (file paths in/out).
  These combine file I/O with Category A functions.  Their signatures are
  identical to the original ``cli.py`` public functions for full backward
  compatibility.

* **Category C — Shared utilities** (display formatting, format-to-extension
  mapping).  Consolidated here to eliminate duplication across cli, web, and
  exporters.
"""

from __future__ import annotations

import os
import sys
from math import isfinite
from pathlib import Path
from typing import Sequence

from seattrellis import __version__
from seattrellis.candidates import generate_candidate_set
from seattrellis.demo import create_demo_files
from seattrellis.exporters import export_snapshot
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
from seattrellis.scoring import build_plan_comparison_report
from seattrellis.service_types import (
    HistoryReportInput,
    HistoryReportOutput,
    PairReportInput,
    PairReportOutput,
    ProjectInfoInput,
    ProjectInfoOutput,
    SolveInput,
    SolveOutput,
    ValidateInput,
    ValidateOutput,
    export_extension,
    score_text,
)
from seattrellis.solver import SeatTrellisSolveError


# ============================================================================
# Category A — Pure compute functions (no file I/O)
# ============================================================================


def compute_solve(input: SolveInput) -> SolveOutput:
    """Core solve logic — pure in-memory computation, no file I/O."""
    if not 1 <= input.candidate_count <= 20:
        raise ValueError("candidate_count must be between 1 and 20")
    if not isfinite(input.time_limit_seconds) or input.time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")

    validation = validate_loaded_inputs(input.students, input.layout, input.rules)
    validation.raise_for_errors(title="Input validation failed.")

    snapshots = list(input.history_snapshots or [])
    preset_warnings = preset_context_warnings(
        None,  # preset object not available in pure path — passed via orchestrator
        input.students,
        history_count=len(snapshots),
        rules=input.rules,
    )

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
    )

    _apply_preset_metadata(
        candidate_set,
        preset_name=input.preset_name,
        rules_overlay=False,
        warnings=preset_warnings,
    )

    if input.candidate_count == 1:
        fairness = candidate_set.candidates[0].snapshot.metrics.get("fairness", {})
        summary = _format_solve_fairness_summary(fairness) if fairness else None
        summary = _append_warnings(summary, preset_warnings)
    else:
        summary = _format_candidate_set_summary(candidate_set)

    report = build_plan_comparison_report(candidate_set, history_snapshots=snapshots)

    return SolveOutput(
        candidate_set=candidate_set,
        preset_warnings=preset_warnings,
        summary=summary,
        plan_comparison_report=report,
    )


def compute_validate(input: ValidateInput) -> ValidateOutput:
    """Core validation logic — pure in-memory computation, no file I/O."""
    report = validate_loaded_inputs(input.students, input.layout, input.rules)
    report.raise_for_errors(strict=input.strict)
    return ValidateOutput(report=report, formatted=report.format_success())


def compute_history_report(input: HistoryReportInput) -> HistoryReportOutput:
    """Core history report — pure computation from in-memory models."""
    history = build_seat_history(input.students, input.layout, input.history_snapshots)
    report = build_fairness_report(history)
    return HistoryReportOutput(report=report, formatted=format_history_report(report))


def compute_pair_report(input: PairReportInput) -> PairReportOutput:
    """Core pair report — pure computation from in-memory models."""
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
    """Core project info — pure formatting from in-memory models."""
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


# ============================================================================
# Category B — Orchestration functions (file paths, backward-compatible)
# ============================================================================


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
) -> tuple[Path, str | None]:
    if not 1 <= candidate_count <= 20:
        raise ValueError("candidate_count must be between 1 and 20")
    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")

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
        )
    )
    candidate_set = result.candidate_set
    preset_warnings = result.preset_warnings or []
    summary = result.summary

    # Re-apply preset metadata with full context (rules_overlay, etc.)
    _apply_preset_metadata(
        candidate_set,
        preset_name=preset.name if preset is not None else None,
        rules_overlay=rules_path is not None and preset is not None,
        warnings=preset_warnings,
    )

    if candidate_count == 1:
        snapshot = candidate_set.candidates[0].snapshot
        path = write_json_model(snapshot, output_path)
        fairness = snapshot.metrics.get("fairness", {})
        summary = _format_solve_fairness_summary(fairness) if fairness else None
        summary = _append_warnings(summary, preset_warnings)
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
    output_format: str,
    output_path: str | Path | None = None,
    candidate_id: str | None = None,
    default_candidate_id: str = "recommended",
) -> Path:
    artifact = load_seating_artifact(snapshot_path)
    if isinstance(artifact, CandidateSet):
        candidate = artifact.get_candidate(candidate_id or default_candidate_id)
        snapshot = _snapshot_with_candidate_metadata(candidate)
    else:
        if candidate_id is not None:
            raise ValueError(
                "--candidate can only be used when --snapshot is a candidate set."
            )
        snapshot = artifact
    return export_snapshot(snapshot, output_format, output_path)


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
    for extra, import_name, pkg_name in [
        ("solver", "ortools", "ortools"),
        ("excel", "openpyxl", "openpyxl"),
        ("image", "PIL", "Pillow"),
        ("web", "streamlit", "streamlit"),
        ("pdf", "weasyprint", "weasyprint"),
        ("docx", "docx", "python-docx"),
    ]:
        try:
            __import__(import_name)
            extras_status.append((extra, "✅", pkg_name))
        except Exception:
            extras_status.append((extra, "❌", pkg_name))

    lines.append("")
    lines.append("  Optional extras:")
    for extra, status, pkg in extras_status:
        lines.append(f"    {status} {extra:8s} ({pkg})")

    examples_dir = Path(__file__).resolve().parents[2] / "examples"
    lines.append("")
    lines.append("  Examples:")
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

    ortools_env = os.environ.get("SEATTRELLIS_USE_ORTOOLS")
    lines.append("")
    lines.append(f"  SEATTRELLIS_USE_ORTOOLS: {ortools_env or '(not set)'}")

    lines.append("")
    lines.append("  Privacy: all examples/ use fictional data only.")
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


# ============================================================================
# Category C — Shared utilities
# ============================================================================


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
    if preset_name is None:
        return
    metadata = {
        "name": preset_name,
        "user_rules_overlay": rules_overlay,
    }
    candidate_set.metadata["preset"] = metadata
    for warning in warnings:
        if warning not in candidate_set.warnings:
            candidate_set.warnings.append(warning)
    for candidate in candidate_set.candidates:
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


def _friendly_error(exc: Exception) -> str:
    return str(exc) or exc.__class__.__name__


def _format_project_path(
    label: str, configured: str, resolved: Path | None
) -> str:
    if resolved is None:
        return f"- {label}: {configured} [not configured]"
    status = "exists" if resolved.exists() else "missing"
    return f"- {label}: {configured} -> {resolved} [{status}]"
