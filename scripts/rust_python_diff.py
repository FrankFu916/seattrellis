"""Full-data differential between the Rust solver and the Python fallback.

The reference `problem` blocks in benchmarks/reference/ are structural only
(no student or rule data), so they cannot exercise the cost functions.  This
script rebuilds each case with the full student records and the daily-rotation
soft rules, runs both the Rust solver (seattrellis_cli) and the Python
fallback, and reports feasibility, hard-constraint validity, best cost, and
solve time so gaps between the two implementations are visible.

Usage:
    python scripts/rust_python_diff.py [--size 40,50,60] [--time-limit 3]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from benchmark_parity import build_case, evaluate_hard_constraints, solve_reference  # noqa: E402

CLI = Path(__file__).resolve().parent.parent / "native" / "target" / "release" / "seattrellis_cli"


def _core_student(py_student: dict[str, object]) -> dict[str, object]:
    """Map a Python student dump to the core Student shape (key + display_name)."""
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


def rust_request(case: dict[str, object]) -> dict[str, object]:
    """Build a CoreSolveRequest that carries the full student + rule data."""
    problem = case["problem"]
    meta = case["problem_meta"]
    request = dict(problem)
    request["students"] = [_core_student(s) for s in meta["students"]]
    # Rust core reads only `seed` + `soft` from the rules document.
    rules = json.loads(json.dumps(meta["rules"]))
    request["rules"] = {"seed": rules.get("seed", 42), "soft": rules.get("soft", {})}
    return request


def run_rust(request: dict[str, object], tmp_path: Path) -> dict[str, object]:
    problem_file = tmp_path / "rust-problem.json"
    output_file = tmp_path / "rust-result.json"
    problem_file.write_text(json.dumps(request), encoding="utf-8")
    result = subprocess.run(
        [str(CLI), "solve", "--problem", str(problem_file), "--output", str(output_file)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Rust CLI failed: {result.stderr[:300]}")
    return json.loads(output_file.read_text(encoding="utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", default="40,50,60")
    parser.add_argument("--time-limit", type=float, default=3.0)
    args = parser.parse_args()
    sizes = [int(item) for item in args.size.split(",")]

    tmp = Path("/tmp/seattrellis-diff")
    tmp.mkdir(parents=True, exist_ok=True)

    for size in sizes:
        case = build_case(size, "daily", seed=42, time_limit=args.time_limit)
        request = rust_request(case)
        rust_start = time.monotonic()
        rust = run_rust(request, tmp)
        rust_seconds = time.monotonic() - rust_start

        py = case["python_reference"]
        print(f"=== {size} 人 (time-limit {args.time_limit:g}s) ===")
        print(f"  Python: feasible={py['feasible']} cost={py.get('total_cost')} "
              f"seated={len(py.get('assignment_by_index', []))} time={py.get('solve_time_seconds')}")
        print(f"  Rust:   feasible={rust['feasible']} cost={rust.get('total_cost')} "
              f"seated={len(rust.get('assignment', []))} time={rust_seconds:.2f}s "
              f"hard_ok={rust.get('hard_constraints_satisfied')}")
        match = py["feasible"] == rust["feasible"]
        print(f"  feasibility match: {match}")
        if not match:
            print("  !! feasibility mismatch between Rust and Python")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
