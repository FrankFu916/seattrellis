from __future__ import annotations

import json
from dataclasses import dataclass
from math import isfinite
from pathlib import Path
from typing import Any, Sequence

from seattrellis.service import (
    export,
    export_extension,
    project_export,
    project_info,
    project_solve,
    project_validate,
    score_text,
    solve_with_report,
)
from seattrellis.io.json_files import (
    InputFileError,
    load_layout,
    load_plan_comparison_report,
    load_seating_artifact,
    load_snapshot,
)
from seattrellis.io.project import load_project_paths
from seattrellis.io.students import read_students
from seattrellis.models.candidate import CandidatePlan, CandidateSet, PlanComparisonReport
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.presets import resolve_rules_with_preset


@dataclass(frozen=True)
class WebSolveResult:
    artifact_path: Path
    artifact: SeatingSnapshot | CandidateSet
    report_path: Path | None = None
    report: PlanComparisonReport | None = None
    summary: str | None = None

    @property
    def is_candidate_set(self) -> bool:
        return isinstance(self.artifact, CandidateSet)

    @property
    def warnings(self) -> tuple[str, ...]:
        if isinstance(self.artifact, CandidateSet):
            return tuple(self.artifact.warnings)
        warnings = self.artifact.metadata.get("warnings", [])
        if isinstance(warnings, list):
            return tuple(str(warning) for warning in warnings)
        return ()


@dataclass(frozen=True)
class RulesPreview:
    rules: RuleSet
    preset_name: str | None
    overlay_applied: bool
    json_bytes: bytes


@dataclass(frozen=True)
class HistorySnapshotQuality:
    snapshot: str
    assignments: int
    covered_students: int
    coverage_percent: float
    missing_students: tuple[str, ...]
    unknown_students: tuple[str, ...]
    unknown_seats: tuple[str, ...]
    disabled_seats: tuple[str, ...]
    layout_matches: bool


@dataclass(frozen=True)
class HistoryQualityReport:
    snapshot_count: int
    student_count: int
    average_coverage_percent: float
    complete_snapshot_count: int
    snapshots: tuple[HistorySnapshotQuality, ...]
    warnings: tuple[str, ...]

    def rows(self) -> list[dict[str, object]]:
        return [
            {
                "snapshot": item.snapshot,
                "assignments": item.assignments,
                "coverage": f"{item.coverage_percent:.1f}%",
                "missing_students": len(item.missing_students),
                "unknown_students": len(item.unknown_students),
                "unknown_seats": len(item.unknown_seats),
                "disabled_seats": len(item.disabled_seats),
                "layout_matches": item.layout_matches,
            }
            for item in self.snapshots
        ]


def parse_rules_overlay(data: bytes | str) -> dict[str, Any]:
    try:
        text = data.decode("utf-8") if isinstance(data, bytes) else data
    except UnicodeDecodeError as exc:
        raise InputFileError(f"Rules overlay must be UTF-8 JSON: {exc}") from exc
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise InputFileError(
            f"Invalid rules JSON: line {exc.lineno}, column {exc.colno}: {exc.msg}"
        ) from exc
    if not isinstance(payload, dict):
        raise InputFileError("Invalid rules JSON: top-level value must be an object.")
    return payload


def build_rules_preview(
    *,
    rules_data: dict[str, Any] | None = None,
    preset_name: str | None = None,
) -> RulesPreview:
    rules, preset = resolve_rules_with_preset(
        rules_data=rules_data,
        preset_name=preset_name,
        source="<Web rules overlay>",
    )
    if hasattr(rules, "model_dump"):
        payload = rules.model_dump(mode="json")  # type: ignore[attr-defined]
    else:
        payload = json.loads(rules.json())
    json_bytes = (
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n"
    ).encode("utf-8")
    return RulesPreview(
        rules=rules,
        preset_name=preset.name if preset is not None else None,
        overlay_applied=rules_data is not None,
        json_bytes=json_bytes,
    )


def analyze_history_files(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    history_paths: Sequence[str | Path],
) -> HistoryQualityReport:
    students = read_students(students_path)
    layout = load_layout(layout_path)
    snapshots = [load_snapshot(path) for path in history_paths]
    return analyze_history_quality(students, layout, snapshots)


def analyze_history_quality(
    students: Sequence[Student],
    layout: ClassroomLayout,
    snapshots: Sequence[SeatingSnapshot],
) -> HistoryQualityReport:
    current_students = {student.key for student in students}
    current_seats = {seat.seat_id: seat for seat in layout.seats}
    current_layout_signature = _layout_signature(layout)
    rows: list[HistorySnapshotQuality] = []
    warnings: list[str] = []

    for index, snapshot in enumerate(snapshots, start=1):
        snapshot_name = str(
            snapshot.metadata.get("snapshot_id")
            or snapshot.metadata.get("name")
            or f"snapshot-{index}"
        )
        assigned_students = {
            assignment.student_key for assignment in snapshot.assignments
        }
        covered = current_students & assigned_students
        missing_students = tuple(sorted(current_students - assigned_students))
        unknown_students = tuple(sorted(assigned_students - current_students))
        unknown_seats = tuple(
            sorted(
                {
                    assignment.seat_id
                    for assignment in snapshot.assignments
                    if assignment.seat_id not in current_seats
                }
            )
        )
        disabled_seats = tuple(
            sorted(
                {
                    assignment.seat_id
                    for assignment in snapshot.assignments
                    if assignment.seat_id in current_seats
                    and not current_seats[assignment.seat_id].enabled
                }
            )
        )
        snapshot_layout_signature = _layout_signature(snapshot.layout)
        layout_matches = snapshot_layout_signature == current_layout_signature
        coverage = (
            100.0 * len(covered) / len(current_students)
            if current_students
            else 100.0
        )
        row = HistorySnapshotQuality(
            snapshot=snapshot_name,
            assignments=len(snapshot.assignments),
            covered_students=len(covered),
            coverage_percent=coverage,
            missing_students=missing_students,
            unknown_students=unknown_students,
            unknown_seats=unknown_seats,
            disabled_seats=disabled_seats,
            layout_matches=layout_matches,
        )
        rows.append(row)
        if missing_students:
            warnings.append(
                f"{snapshot_name}: missing {len(missing_students)} current students."
            )
        if unknown_students:
            warnings.append(
                f"{snapshot_name}: contains {len(unknown_students)} students "
                "not in the current list."
            )
        if unknown_seats:
            warnings.append(
                f"{snapshot_name}: references unknown seats: "
                f"{', '.join(unknown_seats)}."
            )
        if disabled_seats:
            warnings.append(
                f"{snapshot_name}: references disabled seats: "
                f"{', '.join(disabled_seats)}."
            )
        if not layout_matches:
            warnings.append(f"{snapshot_name}: layout differs from the current layout.")

    average_coverage = (
        sum(item.coverage_percent for item in rows) / len(rows) if rows else 0.0
    )
    complete_count = sum(
        item.coverage_percent == 100.0
        and not item.unknown_students
        and not item.unknown_seats
        and not item.disabled_seats
        and item.layout_matches
        for item in rows
    )
    return HistoryQualityReport(
        snapshot_count=len(rows),
        student_count=len(current_students),
        average_coverage_percent=average_coverage,
        complete_snapshot_count=complete_count,
        snapshots=tuple(rows),
        warnings=tuple(warnings),
    )


def _layout_signature(layout: ClassroomLayout) -> str:
    if hasattr(layout, "model_dump"):
        payload = layout.model_dump(mode="json")  # type: ignore[attr-defined]
    else:
        payload = json.loads(layout.json())
    return json.dumps(payload, ensure_ascii=False, sort_keys=True)


def solve_for_web(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    output_dir: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    candidate_count: int = 1,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
) -> WebSolveResult:
    if not 1 <= candidate_count <= 20:
        raise ValueError("candidate_count must be between 1 and 20")
    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")
    # Guard against bare string being iterated as characters.
    if isinstance(history_paths, str):
        raise TypeError(
            "history_paths must be a sequence of paths, not a bare string. "
            "Wrap it in a list: [path] instead of path."
        )
    output_root = Path(output_dir)
    output_root.mkdir(parents=True, exist_ok=True)
    artifact_path = output_root / (
        "seattrellis.candidates.json"
        if candidate_count > 1
        else "seattrellis.snapshot.json"
    )
    report_path = output_root / "seattrellis.plan-report.json" if candidate_count > 1 else None

    written_path, summary = solve_with_report(
        students_path=students_path,
        layout_path=layout_path,
        rules_path=rules_path,
        preset_name=preset_name,
        output_path=artifact_path,
        history_paths=history_paths,
        history_dir=history_dir,
        time_limit_seconds=time_limit_seconds,
        candidate_count=candidate_count,
        seed=seed,
        report_path=report_path,
    )
    artifact = load_seating_artifact(written_path)
    report = (
        load_plan_comparison_report(report_path)
        if report_path is not None and report_path.exists()
        else None
    )
    return WebSolveResult(
        artifact_path=written_path,
        artifact=artifact,
        report_path=report_path if report_path is not None and report_path.exists() else None,
        report=report,
        summary=summary,
    )


def project_info_for_web(*, project_path: str | Path) -> str:
    return project_info(project_path=project_path)


def project_validate_for_web(*, project_path: str | Path, strict: bool = False) -> str:
    return project_validate(project_path=project_path, strict=strict)


def project_solve_for_web(
    *,
    project_path: str | Path,
    candidate_count: int | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
) -> WebSolveResult:
    if candidate_count is not None and not 1 <= candidate_count <= 20:
        raise ValueError("candidate_count must be between 1 and 20")
    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")
    project, paths = load_project_paths(
        project_path,
        require_inputs=True,
        require_history=True,
        create_outputs=True,
    )
    count = project.default_candidates if candidate_count is None else candidate_count
    artifact_path = paths.outputs_dir / (
        "latest.snapshot.json" if count == 1 else "latest.candidates.json"
    )
    report_path = paths.outputs_dir / "latest.plan-report.json" if count > 1 else None

    written_path, summary = project_solve(
        project_path=project_path,
        candidate_count=candidate_count,
        seed=seed,
        time_limit_seconds=time_limit_seconds,
        output_path=artifact_path,
        report_path=report_path,
    )
    artifact = load_seating_artifact(written_path)
    report = (
        load_plan_comparison_report(report_path)
        if report_path is not None and report_path.exists()
        else None
    )
    return WebSolveResult(
        artifact_path=written_path,
        artifact=artifact,
        report_path=report_path if report_path is not None and report_path.exists() else None,
        report=report,
        summary=summary,
    )


def project_export_for_web(
    result: WebSolveResult,
    *,
    project_path: str | Path,
    output_format: str,
    output_dir: str | Path,
    candidate_id: str | None = None,
) -> Path:
    output_root = Path(output_dir)
    output_root.mkdir(parents=True, exist_ok=True)
    normalized_format = output_format.lower()
    output_path = output_root / f"project-seating.{export_extension(normalized_format)}"
    return project_export(
        project_path=project_path,
        snapshot_path=result.artifact_path,
        output_format=normalized_format,
        candidate_id=candidate_id,
        output_path=output_path,
    )


def selected_candidate(
    result: WebSolveResult,
    candidate_id: str = "recommended",
) -> CandidatePlan | None:
    if not isinstance(result.artifact, CandidateSet):
        return None
    try:
        return result.artifact.get_candidate(candidate_id)
    except ValueError:
        return None


def selected_snapshot(
    result: WebSolveResult,
    candidate_id: str = "recommended",
) -> SeatingSnapshot:
    candidate = selected_candidate(result, candidate_id)
    if candidate is not None:
        return candidate.snapshot
    if isinstance(result.artifact, SeatingSnapshot):
        return result.artifact
    raise ValueError("No seating snapshot is available.")


def export_for_web(
    result: WebSolveResult,
    *,
    output_format: str,
    output_dir: str | Path,
    candidate_id: str = "recommended",
) -> Path:
    output_root = Path(output_dir)
    output_root.mkdir(parents=True, exist_ok=True)
    normalized_format = output_format.lower()
    output_path = output_root / f"seating.{export_extension(normalized_format)}"
    return export(
        snapshot_path=result.artifact_path,
        output_format=normalized_format,
        output_path=output_path,
        candidate_id=candidate_id if result.is_candidate_set else None,
    )


def candidate_summary_rows(candidate_set: CandidateSet) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for candidate in sorted(
        candidate_set.candidates,
        key=lambda item: (-item.total_score, item.candidate_id),
    ):
        breakdown = candidate.score.breakdown
        rows.append(
            {
                "candidate_id": candidate.candidate_id,
                "recommended": candidate.candidate_id == candidate_set.recommended_candidate_id,
                "total_score": round(candidate.total_score, 1),
                "hard_constraints": "ok"
                if breakdown.hard_constraint_summary.satisfied
                else "violations",
                "fair_rotation": score_text(breakdown.fair_rotation_score.score),
                "recent_neighbors": score_text(
                    breakdown.avoid_recent_neighbors_score.score
                ),
                "score_balance": score_text(breakdown.score_balance_score.score),
                "diversity": score_text(breakdown.diversity_score.score),
            }
        )
    return rows


def score_breakdown_rows(candidate: CandidatePlan) -> list[dict[str, object]]:
    breakdown = candidate.score.breakdown
    dimensions = [
        ("fair_rotation", breakdown.fair_rotation_score),
        ("recent_neighbors", breakdown.avoid_recent_neighbors_score),
        ("score_balance", breakdown.score_balance_score),
        ("height", breakdown.height_preference_score),
        ("vision", breakdown.vision_preference_score),
        ("diversity", breakdown.diversity_score),
        ("stability", breakdown.stability_score),
    ]
    return [
        {
            "dimension": name,
            "status": dimension.status,
            "score": score_text(dimension.score),
            "weight": dimension.weight,
            "rating": dimension.rating,
        }
        for name, dimension in dimensions
    ]


def assignment_rows(snapshot: SeatingSnapshot) -> list[dict[str, object]]:
    return [
        {
            "student_key": assignment.student_key,
            "student_name": assignment.student_name,
            "seat_id": assignment.seat_id,
        }
        for assignment in snapshot.assignments
    ]


# ---------------------------------------------------------------------------
# Demo helpers (v0.4.0)
# ---------------------------------------------------------------------------

_EXAMPLES_DIR = Path(__file__).resolve().parents[3] / "examples"


def demo_paths() -> dict[str, Path | None]:
    """Return paths to the standard demo files, if they exist."""
    students_csv = _EXAMPLES_DIR / "students.csv"
    students_xlsx = _EXAMPLES_DIR / "students.xlsx"
    layout = _EXAMPLES_DIR / "classroom.json"
    history_dir = _EXAMPLES_DIR / "history"

    return {
        "students_csv": students_csv if students_csv.exists() else None,
        "students_xlsx": students_xlsx if students_xlsx.exists() else None,
        "layout": layout if layout.exists() else None,
        "history_dir": history_dir if history_dir.is_dir() else None,
    }


def load_demo_layout() -> ClassroomLayout:
    """Load the demo classroom layout."""
    layout_path = _EXAMPLES_DIR / "classroom.json"
    if not layout_path.exists():
        raise InputFileError(
            f"Demo layout not found: {layout_path}\n"
            f"Run `seattrellis init-demo` first."
        )
    from seattrellis.io.json_files import load_layout

    return load_layout(layout_path)


def load_demo_snapshot() -> SeatingSnapshot | None:
    """Load a demo snapshot if one exists."""
    snapshot_path = _EXAMPLES_DIR / "history" / "week1.snapshot.json"
    if not snapshot_path.exists():
        return None
    from seattrellis.io.json_files import load_snapshot

    return load_snapshot(snapshot_path)
