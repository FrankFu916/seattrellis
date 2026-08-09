"""Python/Rust differential harness with the frozen v2 seven-status semantics.

M0-03 (see docs/SeatTrellis_v2.0.0_开发与发布总计划_修订版.md §四.1 and the
parity ledger §0): the harness classifies every run on both sides under the
single v2 SolveStatus vocabulary —

    SOLVED / PROVEN_INFEASIBLE / TIMEOUT / UNKNOWN / INVALID_INPUT /
    CANCELLED / INTERNAL_ERROR

Frozen rules:

- A Python error is NEVER recorded as INFEASIBLE (that was the old harness
  bug: `benchmark_parity.solve_reference` mapped every SeatTrellisSolveError
  to solver_status="INFEASIBLE").
- Heuristic exhaustion without a proof is UNKNOWN, not PROVEN_INFEASIBLE.
- Wall-clock exhaustion is TIMEOUT; input-validation failures are
  INVALID_INPUT; anything else is INTERNAL_ERROR.
- Any status mismatch exits non-zero. All classes pass -> exit 0.

Status classes exercised:

1. SOLVED class   — benchmark sizes (default 40/50/60) with full student +
                    rule data, and every non-invalid fixture case.
2. INVALID_INPUT  — the seven `invalid-*` cases of fixtures/parity (which the
                    Python CLI rejects by design).
3. TIMEOUT class  — a 60-student case with a near-zero time limit. The Rust
                    core has no time budget yet (ledger gap, M3-04), so a
                    mismatch here is a *documented* gap: it is reported loudly
                    but still fails the run, per the frozen mismatch policy.

Usage:
    python scripts/rust_python_diff.py                      # benchmark sizes
    python scripts/rust_python_diff.py --sizes 40,60        # subset
    python scripts/rust_python_diff.py --time-limit 3
    python scripts/rust_python_diff.py --fixtures           # fixture corpus classes
"""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / "target" / "release" / "seattrellis_cli"  # single root workspace since M1-01
PY_CLI = ROOT / ".venv" / "bin" / "seattrellis"
FIXTURES = ROOT / "fixtures" / "parity"
INPUTS = FIXTURES / "inputs"
GOLDENS = FIXTURES / "goldens"
SOLVER_BACKEND = "fallback"

sys.path.insert(0, str(Path(__file__).resolve().parent))
from benchmark_parity import (  # noqa: E402
    NATIVE_API_VERSION,
    build_case,
)

# --- frozen v2 SolveStatus vocabulary ---------------------------------------

STATUS_SOLVED = "SOLVED"
STATUS_PROVEN_INFEASIBLE = "PROVEN_INFEASIBLE"
STATUS_TIMEOUT = "TIMEOUT"
STATUS_UNKNOWN = "UNKNOWN"
STATUS_INVALID_INPUT = "INVALID_INPUT"
STATUS_CANCELLED = "CANCELLED"
STATUS_INTERNAL_ERROR = "INTERNAL_ERROR"

# Status classes where a Rust mismatch is a *documented* ledger gap (not a
# harness artifact). A mismatch still fails the run per M0-03.
# TIMEOUT was listed here until M3-04 landed --time-limit (PR #95); the
# benchmark TIMEOUT class now exercises the Rust wall-clock budget too.
KNOWN_RUST_GAPS: dict[str, str] = {}

# Case-level documented corpus gaps (case id -> ledger reference). These are
# real equivalence gaps, not harness artifacts: for each of them the Python
# load/resolver rejected the original input, so the harness sent a DEGRADED
# request (unknown rule kinds / bad adjacency references dropped) that the
# Rust core legitimately solves. The comparison is therefore not
# apples-to-apples and each case carries an explicit ledger reference.
# Without `--allow-documented-gaps` the run still fails on them (M0-03);
# with the flag (used by CI) only NEW mismatches fail the run.
DOCUMENTED_CORPUS_GAPS: dict[str, str] = {
    "invalid-unknown-rule": "ledger 附 M0: unknown rule kinds dropped by the degraded request; core serde ignores unknown fields",
    "invalid-unknown-soft-objective": "ledger 附 M0: unknown soft objectives dropped by the degraded request",
    "invalid-bad-adjacency-ref": "ledger 附 M0: CLI cannot express a bad-adjacency layout; degraded request solves",
}

INVALID_TOKENS = (
    "validation",
    "required",
    "require",
    "not enough",
    "unknown",
    "duplicate",
    "invalid",
    "unrecognized",
    "missing",
    "at least",
    "cannot seat",
    "more students",
)


def classify_python_cli(proc: subprocess.CompletedProcess) -> str:
    """Classify a Python CLI run under the v2 vocabulary (never INFEASIBLE)."""
    if proc.returncode == 0:
        return STATUS_SOLVED
    text = (proc.stderr + proc.stdout).lower()
    if "this is not proof" in text or "time limit" in text:
        return STATUS_TIMEOUT
    if "no feasible seating plan" in text:
        return STATUS_UNKNOWN
    if any(token in text for token in INVALID_TOKENS):
        return STATUS_INVALID_INPUT
    return STATUS_INTERNAL_ERROR


def classify_rust_cli(proc: subprocess.CompletedProcess, output: Path | None) -> str:
    """Classify a Rust CLI run under the v2 vocabulary.

    The core reports `feasible: bool` only; a zero-exit with feasible=false is
    greedy exhaustion, which the frozen semantics require to be UNKNOWN (the
    core cannot prove infeasibility yet).
    """
    if proc.returncode == 0 and output is not None and output.exists():
        try:
            data = json.loads(output.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return STATUS_INTERNAL_ERROR
        if data.get("feasible"):
            return STATUS_SOLVED
        return STATUS_UNKNOWN
    text = proc.stderr.lower()
    if any(token in text for token in INVALID_TOKENS):
        return STATUS_INVALID_INPUT
    return STATUS_INTERNAL_ERROR


# --- Rust request builders --------------------------------------------------

def _core_student(py_student: dict[str, object]) -> dict[str, object]:
    """Map a Python student dump to the core Student shape."""
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


def benchmark_request(case: dict[str, object]) -> dict[str, object]:
    """CoreSolveRequest for a benchmark case: index-space problem + students
    + soft rules (root-level hard fields come from ``problem``)."""
    problem = case["problem"]
    meta = case["problem_meta"]
    request = dict(problem)
    request["students"] = [_core_student(s) for s in meta["students"]]
    rules = json.loads(json.dumps(meta["rules"]))
    # The core reads only `seed` + `soft` from the rules document; hard
    # constraints travel in the root-level index fields emitted by
    # build_problem_json.
    request["rules"] = {"seed": rules.get("seed", 42), "soft": rules.get("soft", {})}
    return request


def fixture_to_request(case_dir: Path) -> tuple[dict[str, object], list[str]]:
    """Convert a fixtures/parity input case into a CoreSolveRequest.

    Known hard rules are resolved to index constraints through the same
    Python resolver the corpus generator uses, so the problem the Rust core
    sees is exactly the problem the Python oracle solved. Unknown rule kinds
    cannot be represented by the core DTO: they are dropped (the core serde
    ignores unknown fields the same way) and the drop is recorded in the
    returned notes. Inputs the pydantic loaders or the resolver reject
    (invalid-* cases) fall back to a minimal degraded request built from the
    raw files that still carries the malformed shape.
    """
    from seattrellis.io.json_files import load_layout, load_rules
    from seattrellis.io.students import read_students
    from seattrellis.solver.precompute import precompute_topology
    from seattrellis.solver.rule_compiler import compile_hard_rules, resolve_hard_rules

    notes: list[str] = []
    try:
        students = read_students(case_dir / "students.csv")
        layout = load_layout(case_dir / "classroom.json")
        rules = load_rules(case_dir / "rules.json")
        topology = precompute_topology(students, layout)
        resolved = resolve_hard_rules(students, layout, rules, topology=topology)
        compiled = compile_hard_rules(resolved)
    except Exception as exc:  # invalid-input cases reject during load/resolve
        notes.append(f"Python load/resolver rejected the input "
                     f"({exc.__class__.__name__}); sending a degraded minimal request")
        return degraded_request_raw(case_dir), notes
    request: dict[str, object] = {
        "api_version": NATIVE_API_VERSION,
        "student_count": len(students),
        "seat_positions": [[float(s.x), float(s.y)] for s in topology.seats],
        "edges": [list(e) for e in sorted(topology.adjacent_seat_index_pairs)],
        "fixed_seats": [[i, j] for i, j in sorted(compiled.fixed_seats.items())],
        "must_be_adjacent": [list(p) for p in compiled.must_be_adjacent],
        "cannot_be_adjacent": [list(p) for p in compiled.cannot_be_adjacent],
        "min_distance": [
            {"students": [a, b], "distance": r.distance, "metric": r.metric}
            for (a, b, r) in compiled.min_distance
        ],
        "seed": rules.seed,
        "students": [_core_student(s.model_dump(mode="json")) for s in students],
        "rules": {
            "seed": rules.seed,
            "soft": rules.model_dump(mode="json").get("soft", {}),
        },
    }
    return request, notes


def degraded_request_raw(case_dir: Path) -> dict[str, object]:
    """Minimal CoreSolveRequest from the raw fixture files, bypassing the
    pydantic loaders (which reject the malformed inputs by design).

    Keeps student_count, seat positions and the student records (including
    duplicates) so the Rust side still sees the malformed shape; adjacency and
    hard rules are dropped and the drop is recorded via the caller's notes.
    """
    layout = json.loads((case_dir / "classroom.json").read_text(encoding="utf-8"))
    seats = [s for s in layout.get("seats", []) if s.get("enabled", True)]
    rules = json.loads((case_dir / "rules.json").read_text(encoding="utf-8"))
    students: list[dict[str, object]] = []
    with (case_dir / "students.csv").open(newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            vision = row.get("vision") or None
            students.append({
                "key": str(row.get("student_id") or row.get("name") or ""),
                "display_name": row.get("name") or None,
                "height_cm": _num_or_none(row.get("height_cm")),
                "score": _num_or_none(row.get("score")),
                "vision": str(vision) if vision else None,
                "tags": _split_tags(row.get("tags")),
                "needs": _split_tags(row.get("needs")),
            })
    seed = rules.get("seed", 42)
    return {
        "api_version": NATIVE_API_VERSION,
        "student_count": len(students),
        "seat_positions": [[float(s["x"]), float(s["y"])] for s in seats],
        "seed": seed,
        "students": students,
        "rules": {"seed": seed, "soft": rules.get("soft", {})},
    }


def _num_or_none(raw: str | None) -> float | None:
    if raw is None or raw.strip() == "":
        return None
    try:
        return float(raw)
    except ValueError:
        return None


def _split_tags(raw: str | None) -> list[str]:
    if raw is None or raw.strip() == "":
        return []
    return [t for t in (part.strip() for part in raw.split(",")) if t]


def run_rust(
    request: dict[str, object],
    tmp: Path,
    time_limit: float | None = None,
) -> tuple[str, str, dict[str, str]]:
    """Run the Rust CLI on a request; return (status, detail, notes).

    ``time_limit`` is forwarded to ``--time-limit`` so the TIMEOUT class
    exercises the M3-04 wall-clock budget (previously a documented gap).
    """
    problem_file = tmp / "rust-problem.json"
    output_file = tmp / "rust-result.json"
    problem_file.write_text(json.dumps(request), encoding="utf-8")
    cmd = [str(CLI), "solve", "--problem", str(problem_file), "--output", str(output_file)]
    if time_limit is not None:
        cmd += ["--time-limit", str(time_limit)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    status = classify_rust_cli(proc, output_file)
    detail = ""
    if proc.returncode == 0 and output_file.exists():
        try:
            data = json.loads(output_file.read_text(encoding="utf-8"))
            detail = f"feasible={data.get('feasible')} seated={len(data.get('assignment', []))}"
        except (OSError, ValueError):
            detail = proc.stderr.strip()[:200]
    else:
        detail = proc.stderr.strip()[:200]
    return status, detail, {}


# --- status classes ---------------------------------------------------------

def run_benchmark_classes(sizes: list[int], time_limit: float) -> list[tuple[str, str, str, str, list[str]]]:
    """(case_id, python_status, rust_status, detail, notes) for SOLVED +
    TIMEOUT classes built from benchmark_parity cases."""
    rows: list[tuple[str, str, str, str, list[str]]] = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        for size in sizes:
            case = build_case(size, "daily", seed=42, time_limit=time_limit)
            py_ref = case["python_reference"]
            py_status = py_ref.get("status", STATUS_INTERNAL_ERROR)
            rust = run_rust(benchmark_request(case), tmp_path, time_limit=time_limit)
            rows.append((f"bench-{size}", py_status, rust[0], rust[1], []))
        # TIMEOUT class: same size but the minimum allowed budget (the
        # fallback enforces time_limit_seconds >= 0.1); 60 students exceed it.
        case = build_case(max(sizes), "daily", seed=42, time_limit=0.1)
        py_ref = case["python_reference"]
        py_status = py_ref.get("status", STATUS_INTERNAL_ERROR)
        rust = run_rust(benchmark_request(case), tmp_path, time_limit=0.1)
        rows.append((f"bench-{max(sizes)}-timeout-0.1", py_status, rust[0], rust[1], []))
    return rows


def run_fixture_classes() -> list[tuple[str, str, str, str, list[str]]]:
    """(case_id, python_status, rust_status, detail, notes) for every case in
    fixtures/parity: non-invalid cases are the SOLVED class, invalid-* cases
    are the INVALID_INPUT class."""
    rows: list[tuple[str, str, str, str, list[str]]] = []
    if not CLI.exists():
        raise SystemExit(f"Rust CLI not found: {CLI}; build it first (cargo build --release --manifest-path native/Cargo.toml)")
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        for case_dir in sorted(INPUTS.iterdir()):
            if not case_dir.is_dir():
                continue
            cid = case_dir.name
            flags = [
                str(PY_CLI), "solve",
                "--students", str(case_dir / "students.csv"),
                "--layout", str(case_dir / "classroom.json"),
                "--rules", str(case_dir / "rules.json"),
                "--backend", SOLVER_BACKEND, "--seed", "42",
                "--time-limit", "3",
            ]
            if (case_dir / "history").is_dir():
                flags += ["--history-dir", str(case_dir / "history")]
            py_proc = subprocess.run(flags, capture_output=True, text=True)
            py_status = classify_python_cli(py_proc)
            request, notes = fixture_to_request(case_dir)
            rust_status, detail, _ = run_rust(request, tmp_path)
            rows.append((cid, py_status, rust_status, detail, notes))
    return rows


# --- reporting --------------------------------------------------------------

def report(rows: list[tuple[str, str, str, str, list[str]]], allow_documented: bool = False) -> int:
    mismatches = 0
    documented = 0
    print(f"{'case':<42} {'python':<16} {'rust':<16} match")
    print("-" * 92)
    for cid, py_status, rust_status, detail, notes in rows:
        # M1-03 frozen semantics: a legal incumbent found within the budget
        # beats a timeout, so Python TIMEOUT + Rust SOLVED is a match (the
        # Rust solver simply finished inside the budget). Since M3-04 the
        # Rust side carries its own --time-limit for the TIMEOUT class.
        match = py_status == rust_status or (
            py_status == STATUS_TIMEOUT and rust_status == STATUS_SOLVED
        )
        if not match:
            mismatches += 1
        flag = "OK " if match else "MISMATCH"
        print(f"{cid:<42} {py_status:<16} {rust_status:<16} {flag}")
        if detail:
            print(f"    rust detail: {detail}")
        for note in notes:
            print(f"    note: {note}")
        if not match and py_status in KNOWN_RUST_GAPS:
            print(f"    documented gap: {KNOWN_RUST_GAPS[py_status]}")
        if not match and rust_status in KNOWN_RUST_GAPS:
            print(f"    documented gap: {KNOWN_RUST_GAPS[rust_status]}")
        if not match and cid in DOCUMENTED_CORPUS_GAPS:
            documented += 1
            print(f"    documented corpus gap: {DOCUMENTED_CORPUS_GAPS[cid]}")
    print("-" * 92)
    new_mismatches = mismatches - documented
    print(
        f"cases: {len(rows)}  mismatches: {mismatches} "
        f"(documented: {documented}, new: {new_mismatches})"
    )
    if allow_documented:
        # CI mode: exactly the documented corpus gaps are tolerated; any
        # other mismatch still fails the run.
        return 1 if new_mismatches else 0
    # Strict mode (default, M0-03): any mismatch fails the run.
    return 1 if mismatches else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", default="40,50,60")
    parser.add_argument("--time-limit", type=float, default=3.0)
    parser.add_argument("--fixtures", action="store_true", help="run the fixtures/parity status classes")
    parser.add_argument(
        "--allow-documented-gaps",
        action="store_true",
        help="CI mode: tolerate exactly the case-level documented corpus gaps "
        "(DOCUMENTED_CORPUS_GAPS); any new mismatch still fails the run",
    )
    args = parser.parse_args()

    if not PY_CLI.exists():
        raise SystemExit(f"Python CLI not found: {PY_CLI}; activate the project venv")

    if args.fixtures:
        rows = run_fixture_classes()
    else:
        sizes = [int(item) for item in args.sizes.split(",")]
        rows = run_benchmark_classes(sizes, args.time_limit)
    return report(rows, allow_documented=args.allow_documented_gaps)


if __name__ == "__main__":
    raise SystemExit(main())
