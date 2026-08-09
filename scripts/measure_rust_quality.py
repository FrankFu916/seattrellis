#!/usr/bin/env python
"""M3 6.6 quality gate: Rust solver vs OR-Tools normalized regret.

For every (size, profile) benchmark case the script computes the
normalized regret of the Rust solver against the Python OR-Tools backend:

    regret = (rust_total_cost - ortools_total_cost) / |ortools_total_cost|

The plan's gate (6.6): median regret <= 5%, P95 <= 15% on the standard
benchmark set. Positive regret means the Rust plan is more expensive
(worse); negative means better than OR-Tools.

Both sides solve the *same* compiled problem: OR-Tools via the Python
backend, Rust via the release CLI on the CoreSolveRequest built from the
same compiled problem (mirrors scripts/benchmark_parity.build_problem_json).
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))
CLI = ROOT / "target" / "release" / "seattrellis_cli"

from seattrellis.benchmarks import (  # noqa: E402
    benchmark_layout,
    benchmark_layout_shape,
    benchmark_rules,
    benchmark_students,
)
from seattrellis.presets import get_preset  # noqa: E402
from seattrellis.solver.ortools_backend import solve_with_ortools  # noqa: E402
from seattrellis.solver.problem import compile_problem  # noqa: E402


def core_student(py_student: dict[str, object]) -> dict[str, object]:
    """Map a Python student to the core Student shape (same as the diff
    harness)."""
    key = py_student.get("student_id") or py_student.get("name")
    vision = py_student.get("vision")
    return {
        "key": str(key),
        "display_name": py_student.get("name"),
        "height_cm": py_student.get("height_cm"),
        "score": py_student.get("score"),
        "vision": None if vision is None else str(vision),
        "tags": py_student.get("tags", []),
        "needs": py_student.get("needs", []),
    }


def build_problem(compiled: object, seed: int) -> dict[str, object]:
    """CoreSolveRequest for a compiled problem (mirrors
    benchmark_parity.build_problem_json)."""
    topology = compiled.topology
    rules_compiled = compiled.rules_compiled
    return {
        "api_version": 2,
        "student_count": len(compiled.students),
        "seat_positions": [
            [float(seat.x), float(seat.y)] for seat in topology.seats
        ],
        "edges": [list(edge) for edge in sorted(topology.adjacent_seat_index_pairs)],
        "fixed_seats": [
            [student_index, seat_index]
            for student_index, seat_index in sorted(rules_compiled.fixed_seats.items())
        ],
        "must_be_adjacent": [list(pair) for pair in rules_compiled.must_be_adjacent],
        "cannot_be_adjacent": [
            list(pair) for pair in rules_compiled.cannot_be_adjacent
        ],
        "min_distance": [
            {
                "students": [first, second],
                "distance": rule.distance,
                "metric": rule.metric,
            }
            for first, second, rule in rules_compiled.min_distance
        ],
        "seed": seed,
    }


def measure(
    size: int,
    profile: str,
    preset_name: str,
    ortools_time_limit: float,
) -> dict[str, object]:
    rows, cols = benchmark_layout_shape(size)
    students = benchmark_students(size)
    layout = benchmark_layout(rows, cols)
    base_rules = get_preset(preset_name).rules
    rules = benchmark_rules(profile, students, layout, base_rules)
    compiled = compile_problem(students, layout, rules)

    # OR-Tools reference (no history/pair-history in the benchmark cases).
    solution = solve_with_ortools(
        compiled,
        history=None,
        pair_history=None,
        seed=int(rules.seed),
        time_limit_seconds=ortools_time_limit,
        requested_backend="ortools",
    )
    if not solution.assignments or solution.objective_value is None:
        return {
            "case": f"{size}-{profile}",
            "ortools_feasible": False,
            "rust_feasible": None,
            "ortools_cost": None,
            "rust_cost": None,
            "regret": None,
            "ortools_status": solution.solver_status,
        }

    # Rust solver on the same problem (release CLI).
    problem = build_problem(compiled, int(rules.seed))
    problem["students"] = [core_student(s.model_dump()) for s in students]
    problem["rules"] = {"seed": rules.seed, "soft": rules.soft.model_dump()}
    with tempfile.TemporaryDirectory() as tmp:
        problem_file = Path(tmp) / "problem.json"
        output_file = Path(tmp) / "result.json"
        problem_file.write_text(json.dumps(problem), encoding="utf-8")
        proc = subprocess.run(
            [str(CLI), "solve", "--problem", str(problem_file), "--output", str(output_file)],
            capture_output=True,
            text=True,
            timeout=600,
        )
        if proc.returncode != 0 or not output_file.exists():
            return {
                "case": f"{size}-{profile}",
                "ortools_feasible": True,
                "rust_feasible": False,
                "ortools_cost": solution.total_cost,
                "rust_cost": None,
                "regret": None,
                "rust_error": proc.stderr.strip()[-200:],
            }
        result = json.loads(output_file.read_text(encoding="utf-8"))

    rust_feasible = bool(result.get("feasible"))
    rust_cost = result.get("total_cost")
    if not rust_feasible or rust_cost is None:
        return {
            "case": f"{size}-{profile}",
            "ortools_feasible": True,
            "rust_feasible": rust_feasible,
            "ortools_cost": solution.objective_value,
            "rust_cost": rust_cost,
            "regret": None,
            "rust_status": result.get("status"),
        }

    ortools_cost = solution.objective_value
    regret = (rust_cost - ortools_cost) / abs(ortools_cost)
    return {
        "case": f"{size}-{profile}",
        "ortools_feasible": True,
        "rust_feasible": True,
        "ortools_cost": ortools_cost,
        "rust_cost": rust_cost,
        "regret": regret,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", default="40,50,60", help="Comma-separated class sizes")
    parser.add_argument("--profiles", default="light,dense", help="Comma-separated profiles")
    parser.add_argument("--preset", default="daily")
    parser.add_argument("--ortools-time-limit", type=float, default=30.0)
    args = parser.parse_args()

    sizes = [int(item) for item in args.sizes.split(",")]
    profiles = [item for item in args.profiles.split(",") if item]

    rows = []
    for size in sizes:
        for profile in profiles:
            row = measure(size, profile, args.preset, args.ortools_time_limit)
            rows.append(row)
            regret = row["regret"]
            print(
                f"{row['case']:<12} ortools={row['ortools_cost']} "
                f"rust={row['rust_cost']} regret={regret if regret is None else round(regret * 100, 2)}%"
            )

    regrets = [row["regret"] for row in rows if row["regret"] is not None]
    if not regrets:
        print("no comparable cases; cannot evaluate the gate")
        return 1
    regrets_sorted = sorted(regrets)
    median = regrets_sorted[len(regrets_sorted) // 2]
    p95 = regrets_sorted[min(len(regrets_sorted) - 1, int(len(regrets_sorted) * 0.95))]
    print("-" * 60)
    print(f"cases: {len(rows)}  comparable: {len(regrets)}")
    print(f"median regret: {median * 100:.2f}%  (gate <= 5%)")
    print(f"P95 regret:    {p95 * 100:.2f}%  (gate <= 15%)")
    gate_ok = median <= 0.05 and p95 <= 0.15
    print(f"gate: {'PASS' if gate_ok else 'FAIL'}")
    return 0 if gate_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
