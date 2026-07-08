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
from seattrellis.models import ClassroomLayout, RuleSet, SeatNode, Student
from seattrellis.presets import get_preset
from seattrellis.service import compute_solve
from seattrellis.service_types import SolveInput
from seattrellis.solver.backend import normalize_solver_backend


@dataclass(frozen=True)
class BenchmarkCase:
    size: int
    rows: int
    cols: int
    backend: str
    candidates: int
    time_limit_seconds: float


@dataclass(frozen=True)
class BenchmarkResult:
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
    payload = {
        "benchmark_version": 1,
        "description": "Synthetic SeatTrellis solver benchmark. Data is fictional.",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "preset": args.preset,
        "environment": {
            "seattrellis_version": __version__,
            "python": sys.version.split()[0],
            "platform": platform.platform(),
        },
        "results": [asdict(result) for result in results],
    }
    text = json.dumps(payload, ensure_ascii=False, indent=2)
    if args.output:
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text + "\n", encoding="utf-8")
    print(text)


def run_case(case: BenchmarkCase, *, preset_name: str = "daily") -> BenchmarkResult:
    students = _students(case.size)
    layout = _layout(case.rows, case.cols)
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
        rows, cols = _layout_shape(size)
        for backend in backends:
            yield BenchmarkCase(
                size=size,
                rows=rows,
                cols=cols,
                backend=backend,
                candidates=candidates,
                time_limit_seconds=time_limit_seconds,
            )


def _layout_shape(size: int) -> tuple[int, int]:
    if size <= 40:
        return 5, 8
    if size <= 50:
        return 5, 10
    return 6, 10


def _students(count: int) -> list[Student]:
    return [
        Student(
            student_id=f"STU{i:03d}",
            name=f"Student{i:03d}",
            gender="F" if i % 2 else "M",
            height_cm=float(145 + (i * 7) % 42),
            score=float(55 + (i * 11) % 45),
            vision="poor" if i % 13 == 0 else None,
            tags=["leader"] if i % 17 == 0 else [],
            needs=["vision_front"] if i % 19 == 0 else [],
        )
        for i in range(1, count + 1)
    ]


def _layout(rows: int, cols: int) -> ClassroomLayout:
    seats: list[SeatNode] = []
    for row in range(1, rows + 1):
        zone = "front" if row == 1 else "back" if row == rows else "middle"
        for col in range(1, cols + 1):
            seats.append(
                SeatNode(
                    seat_id=f"R{row}C{col}",
                    row=row,
                    col=col,
                    x=float(col),
                    y=float(row),
                    zone=zone,
                    near_window=col == 1,
                    near_door=col == cols,
                    near_platform=row == 1,
                    near_ac=row == rows and col in {cols - 1, cols},
                )
            )
    return ClassroomLayout(layout_id=f"benchmark-{rows}x{cols}", seats=seats)


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
    args = parser.parse_args()
    if args.candidates < 1:
        raise SystemExit("--candidates must be at least 1.")
    if args.time_limit < 0.1:
        raise SystemExit("--time-limit must be at least 0.1 seconds.")
    return args


if __name__ == "__main__":
    main()
