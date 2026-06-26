from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

from seattrellis import cli
from seattrellis.io.json_files import (
    load_plan_comparison_report,
    load_seating_artifact,
)
from seattrellis.models.candidate import CandidatePlan, CandidateSet, PlanComparisonReport
from seattrellis.models.snapshot import SeatingSnapshot


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
    output_root = Path(output_dir)
    output_root.mkdir(parents=True, exist_ok=True)
    artifact_path = output_root / (
        "seattrellis.candidates.json"
        if candidate_count > 1
        else "seattrellis.snapshot.json"
    )
    report_path = output_root / "seattrellis.plan-report.json" if candidate_count > 1 else None

    written_path, summary = cli.solve_with_report(
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


def selected_candidate(
    result: WebSolveResult,
    candidate_id: str = "recommended",
) -> CandidatePlan | None:
    if not isinstance(result.artifact, CandidateSet):
        return None
    return result.artifact.get_candidate(candidate_id)


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
    output_path = output_root / f"seating.{_extension_for_format(normalized_format)}"
    return cli.export(
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
                "fair_rotation": _score_text(breakdown.fair_rotation_score.score),
                "recent_neighbors": _score_text(
                    breakdown.avoid_recent_neighbors_score.score
                ),
                "score_balance": _score_text(breakdown.score_balance_score.score),
                "diversity": _score_text(breakdown.diversity_score.score),
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
            "score": _score_text(dimension.score),
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


def _extension_for_format(output_format: str) -> str:
    if output_format in {"excel", "xlsx"}:
        return "xlsx"
    if output_format in {"html", "png"}:
        return output_format
    raise ValueError(f"Unsupported export format: {output_format}")


def _score_text(score: float | None) -> str:
    return "n/a" if score is None else f"{score:.1f}"
