"""Serialization and presentation helpers for solver benchmark reports."""

from __future__ import annotations

import platform
import sys
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone

from seattrellis import __version__
from seattrellis.benchmarks import (
    BENCHMARK_DATASET_NAME,
    BENCHMARK_DATASET_VERSION,
    BENCHMARK_DEFAULT_SIZES,
    benchmark_profile_metadata,
)


BENCHMARK_PHASES = ("parse", "compile", "solve", "score", "serialization")


@dataclass(frozen=True)
class BenchmarkResult:
    case_id: str
    dataset_version: str
    size: int
    rows: int
    cols: int
    backend: str
    candidates: int
    time_limit_seconds: float
    ok: bool
    elapsed_seconds: float
    run_id: str = ""
    constraint_profile: str = "light"
    phase_elapsed_seconds: dict[str, float | None] = field(
        default_factory=lambda: {phase: None for phase in BENCHMARK_PHASES}
    )
    feasible: bool | None = None
    candidate_yield_rate: float = 0.0
    candidate_diversity: dict[str, object] = field(default_factory=dict)
    solve_attempts: int = 0
    feasible_candidates: int = 0
    serialized_bytes: int = 0
    failed_phase: str | None = None
    generated_candidates: int = 0
    recommended_candidate_id: str | None = None
    solver_backend: str | None = None
    solver_backend_effective: str | None = None
    solver_status: str | None = None
    error_type: str | None = None
    error: str | None = None


def build_payload(*, results: list[BenchmarkResult], preset_name: str) -> dict[str, object]:
    """Build the JSON benchmark report payload."""

    selected_profiles = sorted({result.constraint_profile for result in results})
    selected_candidate_counts = sorted({result.candidates for result in results})
    return {
        "benchmark_version": 1,
        "description": "Synthetic SeatTrellis solver benchmark. Data is fictional.",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "preset": preset_name,
        "profiles": [benchmark_profile_metadata(profile) for profile in selected_profiles],
        "candidate_counts": selected_candidate_counts,
        "phase_names": list(BENCHMARK_PHASES),
        "dataset": {
            "name": BENCHMARK_DATASET_NAME,
            "version": BENCHMARK_DATASET_VERSION,
            "default_sizes": list(BENCHMARK_DEFAULT_SIZES),
            "fictional": True,
        },
        "environment": {
            "seattrellis_version": __version__,
            "python": sys.version.split()[0],
            "platform": platform.platform(),
        },
        "summary": summarize_results(results),
        "results": [asdict(result) for result in results],
    }


def summarize_results(results: list[BenchmarkResult]) -> dict[str, object]:
    """Summarize benchmark results for dashboards and release notes."""

    total_cases = len(results)
    successful_cases = sum(1 for result in results if result.ok)
    feasible_cases = sum(1 for result in results if result.feasible is True)
    infeasible_cases = sum(1 for result in results if result.feasible is False)
    known_feasibility_cases = feasible_cases + infeasible_cases
    return {
        "total_cases": total_cases,
        "successful_cases": successful_cases,
        "failed_cases": total_cases - successful_cases,
        "success_rate": ratio(successful_cases, total_cases),
        "feasible_cases": feasible_cases,
        "infeasible_cases": infeasible_cases,
        "unknown_feasibility_cases": total_cases - known_feasibility_cases,
        "feasibility_rate": ratio(feasible_cases, known_feasibility_cases),
        "average_phase_seconds": {
            phase: average(
                [
                    value
                    for result in results
                    if (value := result.phase_elapsed_seconds.get(phase)) is not None
                ]
            )
            for phase in BENCHMARK_PHASES
        },
        "by_backend": [
            _summarize_backend(backend, [result for result in results if result.backend == backend])
            for backend in sorted({result.backend for result in results})
        ],
        "by_size": [
            _summarize_size(size, [result for result in results if result.size == size])
            for size in sorted({result.size for result in results})
        ],
    }


def render_markdown_report(payload: dict[str, object]) -> str:
    """Render a compact human-readable benchmark report."""

    dataset = payload["dataset"]
    environment = payload["environment"]
    summary = payload["summary"]
    results = payload["results"]
    assert isinstance(dataset, dict)
    assert isinstance(environment, dict)
    assert isinstance(summary, dict)
    assert isinstance(results, list)
    lines = [
        "# SeatTrellis benchmark report",
        "",
        f"- Dataset: `{dataset['name']}` / `{dataset['version']}`",
        f"- Preset: `{payload['preset']}`",
        "- Profiles: " + ", ".join(
            f"`{item['name']}`" for item in payload.get("profiles", [])
        ),
        "- Candidate counts: " + ", ".join(
            f"`{value}`" for value in payload.get("candidate_counts", [])
        ),
        f"- SeatTrellis: `{environment['seattrellis_version']}`",
        f"- Python: `{environment['python']}`",
        f"- Platform: `{environment['platform']}`",
        (
            f"- Cases: {summary['successful_cases']}/{summary['total_cases']} succeeded "
            f"({summary['success_rate']:.0%})"
        ),
        "",
        "## Backend summary",
        "",
        "| Backend | Success | Avg elapsed | Max elapsed | Effective backend |",
        "|---|---:|---:|---:|---|",
    ]
    for item in summary["by_backend"]:
        assert isinstance(item, dict)
        lines.append(
            "| {backend} | {successful}/{total} | {average} | {maximum} | {effective} |".format(
                backend=item["backend"],
                successful=item["successful_cases"],
                total=item["total_cases"],
                average=_format_seconds(item["average_elapsed_seconds"]),
                maximum=_format_seconds(item["max_elapsed_seconds"]),
                effective=", ".join(item["effective_backends"]) or "n/a",
            )
        )
    lines.extend(
        [
            "",
            "## Phase averages",
            "",
            "| Parse | Compile | Solve | Score | Serialization |",
            "|---:|---:|---:|---:|---:|",
            "| {parse} | {compile} | {solve} | {score} | {serialization} |".format(
                **{
                    phase: _format_seconds(summary["average_phase_seconds"][phase])
                    for phase in BENCHMARK_PHASES
                }
            ),
            "",
            "## Case results",
            "",
            "| Case | Profile | Size | Layout | Backend | Status | Elapsed | Candidates | Feasible | Diversity |",
            "|---|---|---:|---|---|---|---:|---:|---|---:|",
        ]
    )
    for item in sorted(
        results,
        key=lambda value: (
            value["size"],
            value["constraint_profile"],
            value["candidates"],
            value["backend"],
        ),
    ):
        assert isinstance(item, dict)
        status = item["solver_status"] if item["ok"] else f"ERROR:{item['error_type']}"
        diversity = item.get("candidate_diversity", {})
        assert isinstance(diversity, dict)
        lines.append(
            (
                "| {case_id} | {profile} | {size} | {rows}×{cols} | {backend} | "
                "{status} | {elapsed} | {generated}/{requested} | {feasible} | "
                "{diversity} |"
            ).format(
                case_id=item["case_id"],
                profile=item["constraint_profile"],
                size=item["size"],
                rows=item["rows"],
                cols=item["cols"],
                backend=item["backend"],
                status=status,
                elapsed=_format_seconds(item["elapsed_seconds"]),
                generated=item["generated_candidates"],
                requested=item["candidates"],
                feasible=_format_feasible(item.get("feasible")),
                diversity=_format_percent(diversity.get("mean_pairwise_distance_percent")),
            )
        )
    return "\n".join(lines)


def _summarize_backend(backend: str, results: list[BenchmarkResult]) -> dict[str, object]:
    successful = [result for result in results if result.ok]
    failed = [result for result in results if not result.ok]
    elapsed = [result.elapsed_seconds for result in successful]
    return {
        "backend": backend,
        "total_cases": len(results),
        "successful_cases": len(successful),
        "failed_cases": len(failed),
        "success_rate": ratio(len(successful), len(results)),
        "total_elapsed_seconds": round(sum(result.elapsed_seconds for result in results), 3),
        "average_elapsed_seconds": average(elapsed),
        "min_elapsed_seconds": min(elapsed) if elapsed else None,
        "max_elapsed_seconds": max(elapsed) if elapsed else None,
        "generated_candidates_total": sum(result.generated_candidates for result in successful),
        "feasible_cases": sum(result.feasible is True for result in results),
        "solver_statuses": sorted(
            {result.solver_status for result in successful if result.solver_status}
        ),
        "effective_backends": sorted(
            {
                result.solver_backend_effective
                for result in successful
                if result.solver_backend_effective
            }
        ),
        "failures": [
            {
                "case_id": result.case_id,
                "run_id": result.run_id,
                "error_type": result.error_type,
                "error": result.error,
            }
            for result in failed
        ],
    }


def _summarize_size(size: int, results: list[BenchmarkResult]) -> dict[str, object]:
    successful = [result for result in results if result.ok]
    fastest = min(successful, key=lambda result: result.elapsed_seconds) if successful else None
    first = results[0]
    return {
        "size": size,
        "rows": first.rows,
        "cols": first.cols,
        "total_cases": len(results),
        "successful_cases": len(successful),
        "failed_cases": len(results) - len(successful),
        "success_rate": ratio(len(successful), len(results)),
        "fastest_backend": fastest.backend if fastest else None,
        "fastest_elapsed_seconds": fastest.elapsed_seconds if fastest else None,
        "successful_backends": sorted({result.backend for result in successful}),
        "backend_elapsed_seconds": {
            backend: average(
                [
                    result.elapsed_seconds
                    for result in results
                    if result.backend == backend and result.ok
                ]
            )
            for backend in sorted({result.backend for result in results})
        },
        "backend_statuses": {
            backend: _one_or_many_statuses(
                [result for result in results if result.backend == backend]
            )
            for backend in sorted({result.backend for result in results})
        },
    }


def _one_or_many_statuses(results: list[BenchmarkResult]) -> str | list[str]:
    statuses = sorted(
        {
            result.solver_status
            if result.ok
            else f"ERROR:{result.error_type}"
            for result in results
        }
    )
    return statuses[0] if len(statuses) == 1 else statuses


def ratio(numerator: int, denominator: int) -> float:
    """Return a stable ratio while treating an empty sample as zero."""

    if denominator == 0:
        return 0.0
    return round(numerator / denominator, 4)


def average(values: list[float]) -> float | None:
    """Return the rounded mean, or ``None`` for an empty sample."""

    if not values:
        return None
    return round(sum(values) / len(values), 6)


def _format_seconds(value: object) -> str:
    if value is None:
        return "n/a"
    return f"{float(value):.6f}s"


def _format_percent(value: object) -> str:
    if value is None:
        return "n/a"
    return f"{float(value):.1f}%"


def _format_feasible(value: object) -> str:
    if value is None:
        return "n/a"
    return "yes" if value is True else "no"
