#!/usr/bin/env python
from __future__ import annotations

import argparse
import json
import platform
import sys
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

from seattrellis import __version__
from seattrellis.benchmarks import (
    BENCHMARK_DATASET_NAME,
    BENCHMARK_DATASET_VERSION,
    BENCHMARK_DEFAULT_SIZES,
    benchmark_case_id,
    benchmark_layout,
    benchmark_layout_shape,
    benchmark_students,
)
from seattrellis.presets import get_preset
from seattrellis.service import compute_solve
from seattrellis.service_types import SolveInput
from seattrellis.solver.backend import normalize_solver_backend


@dataclass(frozen=True)
class BenchmarkCase:
    case_id: str
    size: int
    rows: int
    cols: int
    backend: str
    candidates: int
    time_limit_seconds: float


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
    generated_candidates: int = 0
    recommended_candidate_id: str | None = None
    solver_backend: str | None = None
    solver_backend_effective: str | None = None
    solver_status: str | None = None
    error_type: str | None = None
    error: str | None = None


def main() -> None:
    args = _parse_args()
    sizes = _parse_sizes(args.sizes)
    backends = _parse_backends(args.backends)
    results = [
        run_case(case, preset_name=args.preset)
        for case in _cases(
            sizes=sizes,
            backends=backends,
            candidates=args.candidates,
            time_limit_seconds=args.time_limit,
        )
    ]
    payload = build_payload(results=results, preset_name=args.preset)
    text = json.dumps(payload, ensure_ascii=False, indent=2)
    if args.output:
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text + "\n", encoding="utf-8")
    if args.markdown_output:
        markdown_output = Path(args.markdown_output)
        markdown_output.parent.mkdir(parents=True, exist_ok=True)
        markdown_output.write_text(render_markdown_report(payload) + "\n", encoding="utf-8")
    print(text)


def build_payload(*, results: list[BenchmarkResult], preset_name: str) -> dict[str, object]:
    """Build the JSON benchmark report payload."""

    return {
        "benchmark_version": 1,
        "description": "Synthetic SeatTrellis solver benchmark. Data is fictional.",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "preset": preset_name,
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
    return {
        "total_cases": total_cases,
        "successful_cases": successful_cases,
        "failed_cases": total_cases - successful_cases,
        "success_rate": _ratio(successful_cases, total_cases),
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
            "## Case results",
            "",
            "| Case | Size | Layout | Backend | Status | Elapsed | Candidates |",
            "|---|---:|---|---|---|---:|---:|",
        ]
    )
    for item in sorted(results, key=lambda value: (value["size"], value["backend"])):
        assert isinstance(item, dict)
        status = item["solver_status"] if item["ok"] else f"ERROR:{item['error_type']}"
        lines.append(
            "| {case_id} | {size} | {rows}×{cols} | {backend} | {status} | {elapsed} | {candidates} |".format(
                case_id=item["case_id"],
                size=item["size"],
                rows=item["rows"],
                cols=item["cols"],
                backend=item["backend"],
                status=status,
                elapsed=_format_seconds(item["elapsed_seconds"]),
                candidates=item["generated_candidates"],
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
        "success_rate": _ratio(len(successful), len(results)),
        "total_elapsed_seconds": round(sum(result.elapsed_seconds for result in results), 3),
        "average_elapsed_seconds": _average(elapsed),
        "min_elapsed_seconds": min(elapsed) if elapsed else None,
        "max_elapsed_seconds": max(elapsed) if elapsed else None,
        "generated_candidates_total": sum(result.generated_candidates for result in successful),
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
        "success_rate": _ratio(len(successful), len(results)),
        "fastest_backend": fastest.backend if fastest else None,
        "fastest_elapsed_seconds": fastest.elapsed_seconds if fastest else None,
        "successful_backends": sorted({result.backend for result in successful}),
        "backend_elapsed_seconds": {
            result.backend: result.elapsed_seconds if result.ok else None for result in results
        },
        "backend_statuses": {
            result.backend: result.solver_status if result.ok else f"ERROR:{result.error_type}"
            for result in results
        },
    }


def _ratio(numerator: int, denominator: int) -> float:
    if denominator == 0:
        return 0.0
    return round(numerator / denominator, 4)


def _average(values: list[float]) -> float | None:
    if not values:
        return None
    return round(sum(values) / len(values), 3)


def _format_seconds(value: object) -> str:
    if value is None:
        return "n/a"
    return f"{float(value):.3f}s"


def run_case(case: BenchmarkCase, *, preset_name: str = "daily") -> BenchmarkResult:
    students = benchmark_students(case.size)
    layout = benchmark_layout(case.rows, case.cols)
    rules = get_preset(preset_name).rules
    started = time.perf_counter()
    try:
        output = compute_solve(
            SolveInput(
                students=students,
                layout=layout,
                rules=rules,
                candidate_count=case.candidates,
                time_limit_seconds=case.time_limit_seconds,
                backend=case.backend,
            )
        )
    except Exception as exc:
        return BenchmarkResult(
            case_id=case.case_id,
            dataset_version=BENCHMARK_DATASET_VERSION,
            size=case.size,
            rows=case.rows,
            cols=case.cols,
            backend=case.backend,
            candidates=case.candidates,
            time_limit_seconds=case.time_limit_seconds,
            ok=False,
            elapsed_seconds=round(time.perf_counter() - started, 3),
            error_type=exc.__class__.__name__,
            error=str(exc),
        )

    elapsed = round(time.perf_counter() - started, 3)
    candidate_set = output.candidate_set
    first_candidate = candidate_set.candidates[0]
    solver_backend = str(candidate_set.metadata.get("solver_backend", "unknown"))
    solver_backend_effective = first_candidate.snapshot.metrics.get("solver_backend_effective")
    return BenchmarkResult(
        case_id=case.case_id,
        dataset_version=BENCHMARK_DATASET_VERSION,
        size=case.size,
        rows=case.rows,
        cols=case.cols,
        backend=case.backend,
        candidates=case.candidates,
        time_limit_seconds=case.time_limit_seconds,
        ok=True,
        elapsed_seconds=elapsed,
        generated_candidates=len(candidate_set.candidates),
        recommended_candidate_id=candidate_set.recommended_candidate_id,
        solver_backend=solver_backend,
        solver_backend_effective=str(solver_backend_effective) if solver_backend_effective else None,
        solver_status=first_candidate.snapshot.solver_status,
    )


def _cases(
    *,
    sizes: Iterable[int],
    backends: Iterable[str],
    candidates: int,
    time_limit_seconds: float,
) -> Iterable[BenchmarkCase]:
    for size in sizes:
        rows, cols = benchmark_layout_shape(size)
        for backend in backends:
            yield BenchmarkCase(
                case_id=benchmark_case_id(size, rows, cols),
                size=size,
                rows=rows,
                cols=cols,
                backend=backend,
                candidates=candidates,
                time_limit_seconds=time_limit_seconds,
            )


def _parse_sizes(value: str) -> list[int]:
    sizes = [int(item.strip()) for item in value.split(",") if item.strip()]
    if not sizes:
        raise SystemExit("At least one size is required.")
    return sizes


def _parse_backends(value: str) -> list[str]:
    try:
        backends = [normalize_solver_backend(item) for item in value.split(",") if item.strip()]
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    if not backends:
        raise SystemExit("At least one backend is required.")
    return backends


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run synthetic SeatTrellis solver benchmarks.")
    parser.add_argument("--sizes", default="40,50,60", help="Comma-separated class sizes.")
    parser.add_argument("--backends", default="fallback,ortools", help="Comma-separated backends.")
    parser.add_argument("--candidates", type=int, default=1, help="Candidate count per case.")
    parser.add_argument("--time-limit", type=float, default=10.0, help="Seconds per solve.")
    parser.add_argument("--preset", default="daily", help="Preset name. Currently daily is used.")
    parser.add_argument("--output", default=None, help="Optional JSON report path.")
    parser.add_argument("--markdown-output", default=None, help="Optional Markdown summary path.")
    args = parser.parse_args()
    if args.candidates < 1:
        raise SystemExit("--candidates must be at least 1.")
    if args.time_limit < 0.1:
        raise SystemExit("--time-limit must be at least 0.1 seconds.")
    return args


if __name__ == "__main__":
    main()
