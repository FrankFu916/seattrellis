#!/usr/bin/env python
from __future__ import annotations

import argparse
import json
from pathlib import Path

from seattrellis.benchmark_report import (
    BENCHMARK_PHASES,
    BenchmarkResult,
    build_payload,
    render_markdown_report,
    summarize_results,
)
from seattrellis.benchmark_runner import (
    BenchmarkCase,
    _candidate_diversity,
    _cases,
    run_case,
)
from seattrellis.benchmarks import (
    BENCHMARK_DEFAULT_CANDIDATE_COUNTS,
    BENCHMARK_DEFAULT_PROFILES,
    BENCHMARK_DEFAULT_SIZES,
    normalize_benchmark_profile,
)
from seattrellis.solver.backend import normalize_solver_backend


def main() -> None:
    args = _parse_args()
    sizes = _parse_sizes(args.sizes)
    backends = _parse_backends(args.backends)
    profiles = _parse_profiles(args.profiles)
    candidate_counts = (
        _parse_candidate_counts(args.candidate_counts)
        if args.candidate_counts is not None
        else [args.candidates]
    )
    results = [
        run_case(case, preset_name=args.preset)
        for case in _cases(
            sizes=sizes,
            backends=backends,
            candidates=args.candidates,
            time_limit_seconds=args.time_limit,
            profiles=profiles,
            candidate_counts=candidate_counts,
            max_attempts=args.max_attempts,
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
    if not args.quiet:
        print(text)


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


def _parse_profiles(value: str) -> list[str]:
    try:
        profiles = [
            normalize_benchmark_profile(item)
            for item in value.split(",")
            if item.strip()
        ]
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    if not profiles:
        raise SystemExit("At least one benchmark profile is required.")
    return list(dict.fromkeys(profiles))


def _parse_candidate_counts(value: str) -> list[int]:
    try:
        counts = [int(item.strip()) for item in value.split(",") if item.strip()]
    except ValueError as exc:
        raise SystemExit("--candidate-counts must be comma-separated integers.") from exc
    if not counts:
        raise SystemExit("At least one candidate count is required.")
    invalid = [count for count in counts if not 1 <= count <= 20]
    if invalid:
        raise SystemExit("Candidate counts must be between 1 and 20.")
    return list(dict.fromkeys(counts))


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run synthetic SeatTrellis solver benchmarks.")
    parser.add_argument("--sizes", default="40,50,60", help="Comma-separated class sizes.")
    parser.add_argument("--backends", default="fallback,ortools", help="Comma-separated backends.")
    parser.add_argument("--candidates", type=int, default=1, help="Candidate count per case.")
    parser.add_argument(
        "--candidate-counts",
        default=None,
        help=(
            "Optional comma-separated candidate matrix, for example "
            + ",".join(str(value) for value in BENCHMARK_DEFAULT_CANDIDATE_COUNTS)
            + ". "
            "When set, it takes precedence over --candidates."
        ),
    )
    parser.add_argument(
        "--profiles",
        "--constraint-profiles",
        default="light",
        help=(
            "Comma-separated constraint profiles. Supported: "
            + ",".join(BENCHMARK_DEFAULT_PROFILES)
            + "."
        ),
    )
    parser.add_argument("--time-limit", type=float, default=10.0, help="Seconds per solve.")
    parser.add_argument(
        "--max-attempts",
        type=int,
        default=None,
        help="Optional cap on solve attempts per case, useful for scheduled reports.",
    )
    parser.add_argument("--preset", default="daily", help="Preset name. Currently daily is used.")
    parser.add_argument("--output", default=None, help="Optional JSON report path.")
    parser.add_argument("--markdown-output", default=None, help="Optional Markdown summary path.")
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Write requested report files without printing the JSON payload.",
    )
    args = parser.parse_args()
    if not 1 <= args.candidates <= 20:
        raise SystemExit("--candidates must be between 1 and 20.")
    if args.time_limit < 0.1:
        raise SystemExit("--time-limit must be at least 0.1 seconds.")
    if args.max_attempts is not None and args.max_attempts < 1:
        raise SystemExit("--max-attempts must be at least 1.")
    return args


if __name__ == "__main__":
    main()
