#!/usr/bin/env python3
"""Solver performance regression gate (plan §6.6 item 7).

Records release-mode wall-clock baselines for planted-feasible instances
(n = 40/50/60/80, a separate frozen Python-generated corpus) and
asserts a run stays within the registered baseline (+10% tolerance) plus
an absolute interactive bound. The baseline JSON is committed so CI can
detect large timing regressions; the +10% margin does not guarantee immunity
to different hardware or runner noise, while a real algorithmic regression (the kind
that took n=80 from 8s to 0.41s) breaks the bound by a wide margin.

Usage:
    python scripts/bench_solver.py --record [--output benchmarks/solver-baseline.json]
    python scripts/bench_solver.py --check  [--baseline benchmarks/solver-baseline.json]
"""

from __future__ import annotations

import argparse
import hashlib
import platform
import os
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BASELINE = ROOT / "benchmarks" / "solver-baseline.json"
CLI = ROOT / "target" / "release" / "seattrellis"
SIZES = (40, 50, 60, 80)
RUNS_PER_SIZE = 5
# Absolute interactive bounds (ms): a solver that exceeds these on CI-class
# hardware is unusable regardless of the baseline drift.
ABSOLUTE_BOUNDS_MS = {40: 1500, 50: 2500, 60: 3500, 80: 6000}
TOLERANCE = 1.10


class Lcg:
    """Frozen SplitMix64 generator for this benchmark corpus."""

    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFFFFFFFFFFFFFF

    def next(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
        value = self.state
        value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
        value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
        return value ^ (value >> 31)

    def below(self, bound: int) -> int:
        return self.next() % bound


def grid_positions(count: int) -> list[list[float]]:
    columns = max(4, int(count**0.5))
    rows = (count + columns - 1) // columns
    return [[float(col * 1.1), float(row * 1.1)] for row in range(rows) for col in range(columns)][:count]


def grid_edges(positions: list[list[float]]) -> list[list[int]]:
    edges = []
    for first in range(len(positions)):
        for second in range(first + 1, len(positions)):
            dx = abs(positions[first][0] - positions[second][0])
            dy = abs(positions[first][1] - positions[second][1])
            if (dx <= 1.15 and dy <= 0.05) or (dy <= 1.15 and dx <= 0.05):
                edges.append([first, second])
    return edges


def adjacent(edges: list[list[int]], first: int, second: int) -> bool:
    return [first, second] in edges or [second, first] in edges


def planted_request(count: int, seed: int) -> dict:
    """Frozen planted-feasible corpus; distinct from the Rust long-run corpus."""
    positions = grid_positions(count)
    edges = grid_edges(positions)
    rng = Lcg(seed ^ (count << 32))

    assignment = list(range(count))
    for index in range(count - 1, 0, -1):
        swap_with = rng.below(index + 1)
        assignment[index], assignment[swap_with] = assignment[swap_with], assignment[index]
    seat_of = lambda student: assignment[student]  # noqa: E731

    fixed = []
    for _ in range(max(1, min(4, count // 20))):
        student = rng.below(count)
        if not any(s == student for s, _ in fixed):
            fixed.append([student, seat_of(student)])

    must, cannot, min_distance = [], [], []
    for first in range(count):
        for second in range(first + 1, count):
            roll = rng.below(48)
            first_seat, second_seat = seat_of(first), seat_of(second)
            if roll == 0 and adjacent(edges, first_seat, second_seat):
                must.append([first, second])
            elif roll == 1 and not adjacent(edges, first_seat, second_seat):
                cannot.append([first, second])
            elif roll == 2:
                dx = positions[first_seat][0] - positions[second_seat][0]
                dy = positions[first_seat][1] - positions[second_seat][1]
                if (dx * dx + dy * dy) ** 0.5 >= 1.2:
                    min_distance.append({"students": [first, second], "distance": 1.1, "metric": "euclidean"})

    return {
        "api_version": 2,
        "student_count": count,
        "seat_positions": positions,
        "edges": edges,
        "fixed_seats": fixed,
        "must_be_adjacent": must,
        "cannot_be_adjacent": cannot,
        "min_distance": min_distance,
        "seed": seed,
        # Preserve the historical effective hard-only workload. An empty
        # object now correctly enables model defaults, including randomize.
        "rules": {"seed": seed, "soft": {
            name: {"enabled": False, "weight": 1}
            for name in ("vision_front", "height_back", "randomize", "score_balance",
                         "score_position", "score_distribution", "mentor_pairing",
                         "fair_rotation", "avoid_recent_neighbors", "cooling")
        }},
    }


def median_solve_ms(count: int, seed: int, runs: int) -> float:
    request = planted_request(count, seed)
    with tempfile_dir() as tmp:
        problem = Path(tmp) / "problem.json"
        solution = Path(tmp) / "solution.json"
        problem.write_text(json.dumps(request), encoding="utf-8")
        timings = []
        for _ in range(runs):
            started = time.monotonic()
            result = subprocess.run(
                [str(CLI), "solve", "--problem", str(problem), "--output", str(solution)],
                capture_output=True,
                text=True,
                timeout=30,
            )
            elapsed = (time.monotonic() - started) * 1000.0
            if result.returncode != 0:
                raise SystemExit(f"solve failed for n={count}: {result.stderr.strip()}")
            response = json.loads(solution.read_text(encoding="utf-8"))
            if response.get("status") != "Solved":
                raise SystemExit(f"benchmark expected a valid solution, got {response.get('status')}")
            timings.append(elapsed)
        return statistics.median(timings)


def tempfile_dir() -> object:
    import tempfile

    return tempfile.TemporaryDirectory()


def corpus_hash() -> str:
    encoded = json.dumps([planted_request(count, 42) for count in SIZES], sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(encoded.encode()).hexdigest()


def metadata() -> dict:
    def output(command):
        return subprocess.run(command, capture_output=True, text=True, timeout=10).stdout.strip()
    return {"corpus_sha256": corpus_hash(), "seed": 42, "runs_per_size": RUNS_PER_SIZE,
            "corpus_revision": "planted-hard-v1-explicit-soft",
            "measurement": "release CLI wall-clock including atomic response-file output; status checked after timing",
            "rustc": output(["rustc", "--version"]), "commit": output(["git", "rev-parse", "HEAD"]),
            "git_dirty": bool(output(["git", "status", "--porcelain"])),
            "platform": platform.platform(), "machine": platform.machine(), "cpu_count": os.cpu_count(),
            "binary_sha256": hashlib.sha256(CLI.read_bytes()).hexdigest()}


def measure_all() -> dict:
    results = {}
    for count in SIZES:
        results[str(count)] = {"median_ms": round(median_solve_ms(count, 42, RUNS_PER_SIZE), 2)}
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", action="store_true", help="measure and write the baseline")
    parser.add_argument("--check", action="store_true", help="measure and compare against the baseline")
    parser.add_argument("--output", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    args = parser.parse_args()
    if args.record == args.check:
        parser.error("exactly one of --record / --check is required")
    if not CLI.exists():
        raise SystemExit(f"release CLI not found: {CLI}; build it with cargo build --release -p seattrellis")

    measured = measure_all()

    if args.record:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        document = {
            "schema_version": 1,
            "tool": "scripts/bench_solver.py",
            "note": "median wall-clock of the release CLI solve on planted-feasible instances",
            "sizes_ms": measured,
            "metadata": metadata(),
        }
        args.output.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
        print(f"baseline recorded: {args.output}")
        for size, timing in measured.items():
            print(f"  n={size}: {timing['median_ms']} ms")
        return 0

    if not args.baseline.is_file():
        raise SystemExit(f"baseline not found: {args.baseline}; run with --record first")
    baseline_document = json.loads(args.baseline.read_text(encoding="utf-8"))
    recorded_hash = baseline_document.get("metadata", {}).get("corpus_sha256")
    if recorded_hash and recorded_hash != corpus_hash():
        raise SystemExit("benchmark corpus changed; review the workload before recording a new baseline")
    if not recorded_hash:
        print("Legacy baseline has no machine/compiler metadata; comparison is a timing gate, not a controlled experiment")
    print(json.dumps(metadata(), sort_keys=True))
    baseline = baseline_document["sizes_ms"]

    failures = []
    for size, timing in measured.items():
        expected = baseline[size]["median_ms"]
        bound = ABSOLUTE_BOUNDS_MS[int(size)]
        within_tolerance = timing["median_ms"] <= expected * TOLERANCE
        within_absolute = timing["median_ms"] <= bound
        status = "OK" if within_tolerance and within_absolute else "REGRESSION"
        print(f"  n={size}: {timing['median_ms']} ms (baseline {expected} ms, bound {bound} ms) {status}")
        if not within_tolerance or not within_absolute:
            failures.append(size)
    if failures:
        print(f"PERFORMANCE REGRESSION at sizes: {', '.join(failures)}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
