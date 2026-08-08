"""SeatTrellis v2 parity corpus generator (M0 / plan 3.2).

Generates deterministic fixture inputs and golden outputs under fixtures/parity/.

Usage:
    python scripts/gen_parity_fixtures.py inputs  [--cases id1,id2]
    python scripts/gen_parity_fixtures.py goldens [--cases id1,id2]
    python scripts/gen_parity_fixtures.py all
    python scripts/gen_parity_fixtures.py verify [--cases id1,id2]

Golden contract (per plan 3.2): heuristic solutions are NOT required to match
seat-for-seat between Python and Rust; semantics, legality, scoring definitions
and quality thresholds must match. Timestamp fields are stripped so files are
byte-stable.
"""
from __future__ import annotations

import csv
import hashlib
import json
import random
import shutil
import subprocess
import sys
import time
import zlib
from dataclasses import is_dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "fixtures" / "parity"
INPUTS = FIXTURES / "inputs"
GOLDENS = FIXTURES / "goldens"

SOLVER_BACKEND = "fallback"
# Deterministic budget: large enough that the fallback never hits the
# wall-clock deadline (attempts = max(40, n*12) always completes), so solve
# goldens are byte-stable across runs. Wall-clock-terminated solves are
# nondeterministic by construction (M0-03 finding). 1800s covers p80 (960
# attempts, ~4 min on a Mac; slower CI runners need slack). A deadline-bound
# run is still detectable: `verify` skips such cases with a visible warning
# instead of reporting a false DIFF.
TIME_LIMIT = "1800"
NONDETERMINISTIC_KEYS = ("created_at", "wall_clock_seconds", "snapshot_id")

SOFT_WEIGHTS = {
    "vision_front": 20,
    "height_back": 1,
    "randomize": 1,
    "score_balance": 1,
    "fair_rotation": 10,
    "avoid_recent_neighbors": 10,
    "cooling": 5,
    "score_position": 1,
    "score_distribution": 1,
    "mentor_pairing": 10,
}


def cli_bin() -> str:
    cand = ROOT / ".venv" / "bin" / "seattrellis"
    if cand.exists():
        return str(cand)
    found = shutil.which("seattrellis")
    if found:
        return found
    raise SystemExit("seattrellis CLI not found; activate the project venv")


def case_seed(case_id: str) -> int:
    return 1000 + (zlib.crc32(case_id.encode("utf-8")) % 900000)


def write_json(path: Path, obj) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    # sort_keys canonicalizes dict insertion order, which is NOT stable across
    # processes (Python string-hash randomization changes set-iteration order
    # that feeds dict construction in the oracle). Without this, goldens would
    # not be byte-stable (M0-03 finding).
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def strip_nondeterministic(obj):
    """Remove timestamp fields so goldens are byte-stable across runs."""
    if isinstance(obj, dict):
        return {k: strip_nondeterministic(v) for k, v in obj.items() if k not in NONDETERMINISTIC_KEYS}
    if isinstance(obj, list):
        return [strip_nondeterministic(v) for v in obj]
    return obj


def to_plain(obj):
    """Convert dataclasses / sets / tuples / enums into JSON-safe structures."""
    if hasattr(obj, "model_dump"):  # pydantic BaseModel
        return to_plain(obj.model_dump())
    if is_dataclass(obj):
        return to_plain(vars(obj))
    if isinstance(obj, dict):
        return {str(k): to_plain(v) for k, v in sorted(obj.items(), key=lambda kv: str(kv[0]))}
    if isinstance(obj, (set, frozenset)):
        return sorted(to_plain(v) for v in obj)
    if isinstance(obj, (list, tuple)):
        return [to_plain(v) for v in obj]
    if hasattr(obj, "value"):  # enum
        return obj.value
    if isinstance(obj, float):
        return round(obj, 6)
    return obj


def run_cli(args: list[str], *, cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        [cli_bin(), *args],
        capture_output=True,
        text=True,
        cwd=str(cwd or ROOT),
    )


# --------------------------------------------------------------------------
# Case matrix (plan 3.2: scale / seat density / layout / rule density /
# hard-rule mix / soft objectives / history / rotation / data quality /
# migration / export / invalid inputs)
# --------------------------------------------------------------------------

CASES: list[dict] = [
    # --- scale x layout x seat density x rule density ---
    dict(id="p20-rect-exact-none", desc="20 students, 5x4 rect, seats==students, no rules",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[])),
    dict(id="p20-rect-extra-sparse", desc="20 students, 6x4 rect, extra seats, sparse rules",
         n=20, layout=dict(kind="rect", rows=6, cols=4, ratio="extra"),
         rules=dict(hard=["fixed", "adjacent"], soft=["vision_front", "height_back"])),
    dict(id="p20-aisle-none", desc="20 students, aisle layout (disabled middle column), no rules",
         n=20, layout=dict(kind="aisle", rows=5, cols=5, ratio="exact"),
         rules=dict(hard=[], soft=[])),
    dict(id="p40-rect-exact-sparse", desc="40 students, 8x5 rect, sparse rules",
         n=40, layout=dict(kind="rect", rows=8, cols=5, ratio="exact"),
         rules=dict(hard=["fixed", "adjacent", "min_distance"], soft=["vision_front", "score_balance"])),
    dict(id="p40-disabled-extra-dense", desc="40 students, disabled seats, extra seats, dense rules",
         n=40, layout=dict(kind="disabled", rows=8, cols=6, ratio="extra"),
         rules=dict(hard=["fixed", "adjacent", "min_distance", "groups"], soft=["vision_front", "height_back", "randomize", "score_balance"])),
    dict(id="p40-irregular-spare-sparse", desc="40 students, irregular (stair-step) layout, many spare seats",
         n=40, layout=dict(kind="irregular", rows=8, cols=6, ratio="spare"),
         rules=dict(hard=["fixed"], soft=["vision_front", "height_back"])),
    dict(id="p50-custom-adj-sparse", desc="50 students, custom adjacency incl. diagonal + edges",
         n=50, layout=dict(kind="custom_adj", rows=10, cols=5, ratio="exact"),
         rules=dict(hard=["fixed", "adjacent"], soft=["vision_front", "score_balance", "height_back"])),
    dict(id="p60-rect-exact-dense", desc="60 students, 12x5 rect, dense rules",
         n=60, layout=dict(kind="rect", rows=12, cols=5, ratio="exact"),
         rules=dict(hard=["fixed", "adjacent", "min_distance", "groups"], soft=["vision_front", "height_back", "randomize", "score_balance", "cooling"])),
    dict(id="p60-aisle-extra-sparse", desc="60 students, aisle layout, extra seats, sparse rules",
         n=60, layout=dict(kind="aisle", rows=10, cols=7, ratio="extra"),
         rules=dict(hard=["adjacent"], soft=["vision_front"])),
    dict(id="p80-rect-exact-dense", desc="80 students, 16x5 rect, dense rules (stress case)",
         n=80, layout=dict(kind="rect", rows=16, cols=5, ratio="exact"),
         rules=dict(hard=["fixed", "adjacent", "min_distance", "groups"], soft=["vision_front", "height_back", "randomize", "score_balance"])),
    # --- hard rules: individual and combined ---
    dict(id="hard-fixed-only", desc="hard: fixed_seats only",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["fixed"], soft=[])),
    dict(id="hard-adjacent-only", desc="hard: must_be_adjacent + cannot_be_adjacent",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["adjacent"], soft=[])),
    dict(id="hard-min-distance", desc="hard: min_distance (euclidean + graph)",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["min_distance"], soft=[])),
    dict(id="hard-groups", desc="hard: groups separate + together",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["groups"], soft=[])),
    dict(id="hard-combined", desc="hard: all rules combined",
         n=24, layout=dict(kind="rect", rows=6, cols=4, ratio="exact"),
         rules=dict(hard=["fixed", "adjacent", "min_distance", "groups"], soft=[])),
    # --- soft objectives: each individually + all combined ---
    dict(id="soft-vision-front", desc="soft: vision_front",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["vision_front"])),
    dict(id="soft-height-back", desc="soft: height_back",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["height_back"])),
    dict(id="soft-randomize", desc="soft: randomize",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["randomize"])),
    dict(id="soft-score-balance", desc="soft: score_balance",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["score_balance"])),
    dict(id="soft-fair-rotation", desc="soft: fair_rotation with short history",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["fair_rotation"]), history=3),
    dict(id="soft-avoid-recent-neighbors", desc="soft: avoid_recent_neighbors with history",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["avoid_recent_neighbors"]), history=3),
    dict(id="soft-cooling", desc="soft: cooling with history",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["cooling"]), history=3),
    dict(id="soft-score-position", desc="soft: score_position",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["score_position"])),
    dict(id="soft-score-distribution", desc="soft: score_distribution",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["score_distribution"])),
    dict(id="soft-mentor", desc="soft: mentor_pairing",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["mentor_pairing"])),
    dict(id="soft-all", desc="soft: all objectives enabled (dense)",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["vision_front", "height_back", "randomize", "score_balance",
                                   "fair_rotation", "avoid_recent_neighbors", "cooling",
                                   "score_position", "score_distribution", "mentor_pairing"]),
         history=3),
    # --- history depth ---
    dict(id="hist-short", desc="short history (3 periods) + fairness softs",
         n=30, layout=dict(kind="rect", rows=6, cols=5, ratio="exact"),
         rules=dict(hard=[], soft=["fair_rotation", "avoid_recent_neighbors"]), history=3),
    dict(id="hist-long", desc="long history (10 periods), 40 students",
         n=40, layout=dict(kind="rect", rows=8, cols=5, ratio="exact"),
         rules=dict(hard=[], soft=["fair_rotation", "cooling", "avoid_recent_neighbors"]), history=10),
    # --- rotation ---
    dict(id="rotation-3-periods", desc="rotation plan, 3 periods, 40 students",
         n=40, layout=dict(kind="rect", rows=8, cols=5, ratio="exact"),
         rules=dict(hard=["fixed"], soft=["fair_rotation", "height_back"]), rotation=3),
    # --- data quality ---
    dict(id="data-missing", desc="students with missing score/height/vision/tags",
         n=24, layout=dict(kind="rect", rows=6, cols=4, ratio="exact"),
         rules=dict(hard=["fixed"], soft=["vision_front", "height_back"]),
         profile="missing"),
    dict(id="data-unicode", desc="unicode / Chinese names, long fields",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["fixed"], soft=["vision_front"]),
         profile="unicode"),
    # --- schema migration ---
    dict(id="migration-ruleset", desc="schema migrate on a ruleset artifact",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["fixed"], soft=["vision_front"]), migrate="ruleset"),
    dict(id="migration-snapshot", desc="schema migrate on a snapshot artifact",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[]), migrate="snapshot"),
    # --- export metadata ---
    dict(id="export-metadata", desc="export all formats, record metadata",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["fixed"], soft=["vision_front", "height_back"]), export=True),
    # --- invalid inputs (record CLI exit behavior) ---
    dict(id="invalid-empty-students", desc="student file with no students",
         n=0, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[]), invalid="empty_students"),
    dict(id="invalid-students-gt-seats", desc="more students than seats",
         n=25, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[]), invalid="students_gt_seats"),
    dict(id="invalid-dup-student-id", desc="duplicate student_id",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[]), invalid="dup_student_id"),
    dict(id="invalid-unknown-rule", desc="unknown hard rule kind",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=["unknown"], soft=[]), invalid="unknown_rule"),
    dict(id="invalid-unknown-soft-objective", desc="unknown soft objective",
         n=20, layout=dict(kind="rect", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=["unknown_soft"]), invalid="unknown_soft"),
    dict(id="invalid-bad-adjacency-ref", desc="custom edge references missing seat",
         n=20, layout=dict(kind="bad_adj", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[]), invalid="bad_adjacency"),
    dict(id="invalid-empty-layout", desc="layout with no seats",
         n=20, layout=dict(kind="empty", rows=5, cols=4, ratio="exact"),
         rules=dict(hard=[], soft=[]), invalid="empty_layout"),
]


# --------------------------------------------------------------------------
# Input generators (deterministic: random.Random seeded per case)
# --------------------------------------------------------------------------

def gen_students(case: dict, case_dir: Path) -> Path:
    n = case["n"]
    profile = case.get("profile", "normal")
    seed = case_seed(case["id"])
    rng = random.Random(seed + 1)
    path = case_dir / "students.csv"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["student_id", "name", "gender", "height_cm", "score", "vision", "tags", "needs", "notes"])
        for i in range(1, n + 1):
            sid = f"STU{i:03d}"
            if profile == "unicode":
                name = f"学生{chr(0x4E00 + (i % 20))}{i:03d}"
            else:
                name = f"Student{i:03d}"
            gender = "F" if i % 2 else "M"
            if profile == "missing" and i % 5 == 0:
                height = ""
            else:
                height = rng.randint(130, 190)
            if profile == "missing" and i % 7 == 0:
                score = ""
            else:
                score = round(rng.uniform(50, 100), 1)
            if profile == "missing" and i % 3 == 0:
                vision = ""
            elif i % 5 == 0:
                vision = "0.6"
            elif i % 11 == 0:
                vision = "poor"
            else:
                vision = ""
            tags = "leader" if i in (1, n // 2) else ""
            needs = "vision_front" if i == 1 else ""
            notes = "特别关注" if (profile == "unicode" and i % 6 == 0) else ""
            w.writerow([sid, name, gender, height, score, vision, tags, needs, notes])
            if case.get("invalid") == "dup_student_id" and i == 1:
                # Duplicate the first row so the input actually contains a
                # repeated student identifier (the reader must reject it).
                w.writerow([sid, name, gender, height, score, vision, tags, needs, notes])
    return path


def _zone_for_row(r: int, rows: int) -> str:
    front = max(1, rows // 5)
    back = max(1, rows // 5)
    if r <= front:
        return "front"
    if r > rows - back:
        return "back"
    return "middle"


def gen_layout(case: dict, case_dir: Path) -> Path:
    lk = case["layout"]
    rows, cols = lk["rows"], lk["cols"]
    n = case["n"]
    kind = lk["kind"]
    seed = case_seed(case["id"])
    rng = random.Random(seed + 2)

    if kind == "empty":
        seats = []
    else:
        ratio = lk.get("ratio", "exact")
        target = n if ratio == "exact" else int(n * (1.2 if ratio == "extra" else 1.5))
        # Build the candidate grid first.
        grid: list[dict] = []
        for r in range(1, rows + 1):
            row_seats = cols
            if kind == "irregular":
                row_seats = max(2, cols - (r // 4))  # gentle stair-step
            for c in range(1, row_seats + 1):
                grid.append(dict(seat_id=f"R{r}C{c}", row=r, col=c, x=float(c), y=float(r)))
        # Disable seats per layout kind (deterministic).
        disabled_flags = []
        for entry in grid:
            if kind == "aisle" and entry["col"] == (cols // 2 + 1):
                disabled_flags.append(True)
            elif kind == "disabled" and rng.random() < 0.08:
                disabled_flags.append(True)
            else:
                disabled_flags.append(False)
        enabled_ids = [e["seat_id"] for e, d in zip(grid, disabled_flags) if not d]
        # Trim from the tail to reach the target seat count.
        if len(enabled_ids) > target:
            keep = set(enabled_ids[:target])
        else:
            keep = set(enabled_ids)
        seats = []
        for entry, disabled in zip(grid, disabled_flags):
            if disabled or entry["seat_id"] not in keep:
                continue
            r, c = entry["row"], entry["col"]
            row_seats_now = len({g["seat_id"] for g in grid if g["row"] == r})
            seats.append(dict(
                seat_id=entry["seat_id"],
                row=r,
                col=c,
                x=entry["x"],
                y=entry["y"],
                enabled=True,
                zone=_zone_for_row(r, rows),
                near_window=c == 1,
                near_door=c == row_seats_now,
                near_platform=r == 1,
                near_ac=False,
                tags=[],
                attributes={},
            ))

    adjacency = dict(
        include_horizontal=True,
        include_vertical=True,
        include_diagonal=kind == "custom_adj",
        max_row_delta=1,
        max_col_delta=1,
        max_distance=None,
        use_xy_distance=True,
        custom_edges=[],
    )
    if kind == "custom_adj" and seats:
        ids = [s["seat_id"] for s in seats]
        # Pick a few real seats for extra custom edges.
        pairs = [(ids[0], ids[1])] if len(ids) > 1 else []
        for r in range(2, min(rows, 6)):
            a, b = f"R{r}C1", f"R{r+1}C1"
            if a in ids and b in ids and (a, b) not in pairs:
                pairs.append((a, b))
        adjacency["custom_edges"] = [[a, b] for a, b in pairs]
    if kind == "bad_adj":
        adjacency["custom_edges"] = [["R1C1", "R99C99"]]

    layout = dict(
        layout_id=case["id"],
        name=f"Parity {case['id']}",
        seats=seats,
        adjacency=adjacency,
        metadata=dict(platform="front"),
    )
    if not case.get("invalid") and sum(1 for s in seats if s["enabled"]) < n:
        raise SystemExit(
            f"[{case['id']}] only {sum(1 for s in seats if s['enabled'])} enabled "
            f"seats for {n} students; fix the layout spec, do not ship an "
            "accidentally-infeasible corpus case."
        )
    path = case_dir / "classroom.json"
    write_json(path, layout)
    return path


def _first_enabled_seat(case_dir: Path) -> str:
    """First enabled seat of the generated layout, so fixed rules never pin a
    disabled seat (deterministic across `inputs`/`goldens`/`verify`)."""
    layout_path = case_dir / "classroom.json"
    if layout_path.exists():
        layout = read_json(layout_path)
        for seat in layout.get("seats", []):
            if seat.get("enabled", True):
                return seat["seat_id"]
    return "R1C1"


def gen_rules(case: dict, case_dir: Path) -> Path:
    n = max(case["n"], 12)
    prof = case["rules"]
    seed = case_seed(case["id"])
    hard: dict = {}
    if "fixed" in prof.get("hard", []):
        hard["fixed_seats"] = [{"student": "STU001", "seat_id": _first_enabled_seat(case_dir)}]
    if "adjacent" in prof.get("hard", []):
        hard["must_be_adjacent"] = [{"students": ["STU002", "STU003"]}]
        hard["cannot_be_adjacent"] = [{"students": ["STU004", "STU005"]}]
    if "min_distance" in prof.get("hard", []):
        hard["min_distance"] = [
            {"students": ["STU006", "STU007"], "distance": 2, "metric": "euclidean"},
            {"students": ["STU008", "STU009"], "distance": 1, "metric": "graph"},
        ]
    groups = []
    if "groups" in prof.get("hard", []):
        groups = [
            {"name": "Alpha", "students": ["STU001", "STU006", "STU011"], "separate": False},
            {"name": "Beta", "students": ["STU002", "STU007", "STU012"], "separate": True},
        ]
    if "unknown" in prof.get("hard", []):
        hard["unknown_rule"] = [{"student": "STU001"}]
    if case.get("invalid") == "unknown_rule":
        hard["teleport_students"] = [{"student": "STU001"}]

    soft: dict = {}
    for key in prof.get("soft", []):
        if key == "unknown_soft":
            soft["magic_seating"] = {"enabled": True, "weight": 5}
            continue
        soft[key] = {"enabled": True, "weight": SOFT_WEIGHTS.get(key, 1)}
        if key == "score_position":
            soft[key]["direction"] = "high_front"
        if key == "score_distribution":
            soft[key]["scope"] = "row"

    rules = dict(seed=seed, hard=hard, soft=soft)
    if groups:
        rules["groups"] = groups
    path = case_dir / "rules.json"
    write_json(path, rules)
    return path


def gen_history(case: dict, case_dir: Path, students: Path, layout: Path) -> list[Path]:
    periods = case.get("history", 0)
    if not periods:
        return []
    seed = case_seed(case["id"])
    hist_dir = case_dir / "history"
    hist_dir.mkdir(parents=True, exist_ok=True)
    outputs = []
    for p in range(1, periods + 1):
        # History periods use a minimal rule set (randomize only) and
        # deterministic per-period seeds, so they are reproducible.
        r = {"seed": seed + p * 7, "hard": {}, "soft": {"randomize": {"enabled": True, "weight": 1}}}
        rfile = hist_dir / f"_period-rules-{p:02d}.json"
        write_json(rfile, r)
        out = hist_dir / f"period-{p:02d}.snapshot.json"
        proc = run_cli([
            "solve", "--students", str(students), "--layout", str(layout),
            "--rules", str(rfile), "--output", str(out),
            "--backend", SOLVER_BACKEND, "--seed", str(seed + p * 7),
            "--time-limit", "1800",
        ])
        rfile.unlink(missing_ok=True)
        if proc.returncode != 0:
            raise SystemExit(f"[{case['id']}] history period {p} failed: {proc.stderr[:400]}")
        # Strip timestamps so inputs/ is byte-stable and re-checkable by `verify`.
        write_json(out, strip_nondeterministic(read_json(out)))
        outputs.append(out)
    return outputs


# --------------------------------------------------------------------------
# Golden generators
# --------------------------------------------------------------------------

def _solve_flags(case: dict, case_dir: Path):
    students = case_dir / "students.csv"
    layout = case_dir / "classroom.json"
    rules = case_dir / "rules.json"
    flags = [
        "--students", str(students), "--layout", str(layout),
        "--rules", str(rules), "--backend", SOLVER_BACKEND,
        "--seed", str(case_seed(case["id"])), "--time-limit", TIME_LIMIT,
    ]
    if case.get("history"):
        flags += ["--history-dir", str(case_dir / "history")]
    return flags


def gen_hard_constraints(case: dict, case_dir: Path, gold_dir: Path) -> bool:
    """Compiled hard rules via the Python API (resolve + compile)."""
    try:
        from seattrellis.io.json_files import load_layout, load_rules
        from seattrellis.io.students import read_students
        from seattrellis.solver.precompute import precompute_topology
        from seattrellis.solver.rule_compiler import compile_hard_rules, resolve_hard_rules
    except ImportError as exc:  # pragma: no cover
        print(f"    skip hard-constraints ({exc})")
        return False
    students = read_students(case_dir / "students.csv")
    layout = load_layout(case_dir / "classroom.json")
    rules = load_rules(case_dir / "rules.json")
    topology = precompute_topology(students, layout)
    resolved = resolve_hard_rules(students, layout, rules, topology=topology)
    compiled = compile_hard_rules(resolved)
    out = {
        "student_count": len(students),
        "seat_count": len(layout.seats),
        "enabled_seat_count": sum(1 for s in layout.seats if s.enabled),
        "ambiguous_student_references": list(resolved.student_references.ambiguous_references),
        "fixed_seats": {str(k): v for k, v in sorted(compiled.fixed_seats.items())},
        "must_be_adjacent": [list(t) for t in compiled.must_be_adjacent],
        "cannot_be_adjacent": [list(t) for t in compiled.cannot_be_adjacent],
        "min_distance": [
            {"first": int(a), "second": int(b), "rule": to_plain(md)}
            for (a, b, md) in compiled.min_distance
        ],
        "topology": {
            "edges": [list(e) for e in sorted(topology.edges)],
            "student_index_by_key": {k: v for k, v in sorted(topology.student_index_by_key.items())},
            "seat_index_by_id": {k: v for k, v in sorted(topology.seat_index_by_id.items())},
            "adjacent_pairs": [list(e) for e in sorted(topology.adjacent_seat_index_pairs)],
            "euclidean_distance_matrix": {
                "shape": [len(topology.euclidean_distance_matrix),
                          len(topology.euclidean_distance_matrix[0]) if topology.euclidean_distance_matrix else 0],
                "first_row": list(topology.euclidean_distance_matrix[0]) if topology.euclidean_distance_matrix else [],
            },
            "graph_distance_matrix": {
                "shape": [len(topology.graph_distance_matrix),
                          len(topology.graph_distance_matrix[0]) if topology.graph_distance_matrix else 0],
                "first_row": list(topology.graph_distance_matrix[0]) if topology.graph_distance_matrix else [],
            },
        },
    }
    write_json(gold_dir / "hard-constraints.json", strip_nondeterministic(out))
    return True


def gen_solve_goldens(case: dict, case_dir: Path, gold_dir: Path) -> None:
    seed = case_seed(case["id"])
    flags = _solve_flags(case, case_dir)

    # Single snapshot.
    snap = gold_dir / "snapshot.json"
    proc = run_cli(["solve", *flags, "--candidates", "1", "--output", str(snap)])
    if proc.returncode == 0 and snap.exists():
        write_json(snap, strip_nondeterministic(read_json(snap)))
    else:
        print(f"    snapshot failed: {proc.stderr[:300]}")

    # Candidate set + plan comparison report (small cases only; the
    # multi-candidate engine for large classes is M4-03 work, and a
    # candidates=3 golden triples the deterministic-budget runtime).
    if case["n"] <= 40 and not case.get("invalid"):
        cand = gold_dir / "candidates.json"
        report = gold_dir / "plan-report.json"
        proc = run_cli([
            "solve", *flags, "--candidates", "3",
            "--output", str(cand), "--report", str(report),
        ])
        if proc.returncode == 0 and cand.exists():
            write_json(cand, strip_nondeterministic(read_json(cand)))
            if report.exists():
                write_json(report, strip_nondeterministic(read_json(report)))
            write_json(gold_dir / "objective-breakdown.json", extract_breakdown(read_json(cand)))
        else:
            print(f"    candidates failed: {proc.stderr[:300]}")


def extract_breakdown(cand: dict) -> dict:
    out = {"schema_version": cand.get("schema_version"), "candidates": []}
    for c in cand.get("candidates", []):
        score = c.get("score", {})
        out["candidates"].append({
            "candidate_id": c.get("candidate_id"),
            "total_score": score.get("total"),
            "breakdown": score.get("breakdown"),
            "hard_constraint_violations": c.get("hard_constraint_violation_count"),
        })
    return out


def normalize_paths(text: str, case_dir: Path, gold_dir: Path | None = None) -> str:
    """Replace absolute paths in CLI output with stable placeholders.

    The corpus must be byte-stable when regenerated from a different checkout
    or temp dir (verify regenerates into a temp directory). CLI error/summary
    messages embed input paths, so they are normalized here.
    """
    for base, placeholder in (
        (case_dir, "<case_dir>"),
        (gold_dir, "<gold_dir>") if gold_dir is not None else (None, None),
        (INPUTS, "<inputs>"),
        (GOLDENS, "<goldens>"),
        (ROOT, "<repo>"),
    ):
        if base is not None:
            text = text.replace(str(base), placeholder)
    return text


def gen_history_reports(case: dict, case_dir: Path, gold_dir: Path) -> None:
    if not case.get("history"):
        return
    students = case_dir / "students.csv"
    layout = case_dir / "classroom.json"
    hist_dir = case_dir / "history"
    for cmd, name in (("history-report", "history-report.json"), ("pair-report", "pair-report.json")):
        out = gold_dir / name
        proc = run_cli([
            cmd, "--students", str(students), "--layout", str(layout),
            "--history-dir", str(hist_dir), "-o", str(out),
        ])
        if proc.returncode == 0 and out.exists():
            write_json(out, strip_nondeterministic(read_json(out)))
        else:
            print(f"    {cmd} failed: {proc.stderr[:300]}")


def gen_rotation(case: dict, case_dir: Path, gold_dir: Path) -> None:
    if not case.get("rotation"):
        return
    flags = _solve_flags(case, case_dir)
    out = gold_dir / "rotation-plan.json"
    proc = run_cli([
        "rotation-plan", *flags, "--periods", str(case["rotation"]),
        "--name", f"Parity {case['id']}", "--output", str(out),
    ])
    if proc.returncode == 0 and out.exists():
        write_json(out, strip_nondeterministic(read_json(out)))
    else:
        print(f"    rotation failed: {proc.stderr[:300]}")


def gen_migration(case: dict, case_dir: Path, gold_dir: Path) -> None:
    kind = case.get("migrate")
    if not kind:
        return
    source = case_dir / "rules.json" if kind == "ruleset" else gold_dir / "snapshot.json"
    if not source.exists():
        print(f"    migrate skipped: {source.name} missing")
        return
    out_target = gold_dir / f"migrated-{kind}.json"
    # 1) dry-run probe
    dry = run_cli(["schema", "migrate", "-i", str(source), "--dry-run"])
    # 2) real migration to a golden path
    real = run_cli(["schema", "migrate", "-i", str(source), "-o", str(out_target), "--no-backup"])
    result = {
        "artifact": kind,
        "source_basename": source.name,
        "dry_run": {
            "exit_code": dry.returncode,
            "stdout": normalize_paths(dry.stdout.strip(), case_dir, gold_dir),
            "stderr": normalize_paths(dry.stderr.strip(), case_dir, gold_dir),
        },
        "migrate": {
            "exit_code": real.returncode,
            "stdout": normalize_paths(real.stdout.strip(), case_dir, gold_dir),
            "stderr": normalize_paths(real.stderr.strip(), case_dir, gold_dir),
        },
        "output_exists": out_target.exists(),
    }
    if out_target.exists():
        result["output"] = strip_nondeterministic(read_json(out_target))
        if kind == "snapshot":
            out_target.unlink()  # keep goldens lean: contents already embedded
    write_json(gold_dir / "migration-result.json", result)


def gen_export_metadata(case: dict, case_dir: Path, gold_dir: Path) -> None:
    if not case.get("export"):
        return
    snap = gold_dir / "snapshot.json"
    if not snap.exists():
        print("    export skipped: snapshot missing")
        return
    formats = ["svg", "html", "print-html", "png", "pdf", "excel", "docx", "pptx"]
    # No export format is byte-stable: text formats embed a "generation time"
    # string, PDF/DOCX/PPTX carry timestamps, xlsx zip stores file mtimes (2s
    # granularity), and PNG's deflate stream depends on the platform zlib
    # (M0-03 findings). The golden therefore records the semantic contract
    # only: exit code and line counts for text formats. stderr/stdout are
    # deliberately NOT recorded: they vary across platforms (e.g. CJK font
    # fallback warnings on headless Linux). Byte-stability is an M5-04
    # export-parity item.
    meta = {}
    for fmt in formats:
        out = gold_dir / f"export.{fmt}"
        proc = run_cli(["export", "--snapshot", str(snap), "--format", fmt, "--output", str(out)])
        entry = {"exit_code": proc.returncode}
        if proc.returncode == 0 and out.exists():
            data = out.read_bytes()
            entry["content_embeds_generation_timestamp"] = True
            if fmt in ("svg", "html", "print-html"):
                entry["lines"] = data.decode("utf-8", errors="replace").count("\n") + 1
            out.unlink()  # keep goldens lean; metadata is the contract
        meta[fmt] = entry
    write_json(gold_dir / "export-metadata.json", meta)


def gen_invalid_result(case: dict, case_dir: Path, gold_dir: Path) -> None:
    if not case.get("invalid"):
        return
    flags = _solve_flags(case, case_dir)
    proc = run_cli(["solve", *flags, "--candidates", "1", "--output", str(gold_dir / "snapshot.json")])
    result = {
        "invalid_kind": case["invalid"],
        "command": "solve",
        "exit_code": proc.returncode,
        "stdout": normalize_paths(proc.stdout.strip()[:500], case_dir, gold_dir),
        "stderr_head": normalize_paths(proc.stderr.strip()[:600], case_dir, gold_dir),
    }
    write_json(gold_dir / "invalid-result.json", result)


# --------------------------------------------------------------------------
# Orchestration
# --------------------------------------------------------------------------

def gen_inputs(cases: list[dict]) -> None:
    for case in cases:
        cid = case["id"]
        case_dir = INPUTS / cid
        print(f"[inputs] {cid}")
        gen_students(case, case_dir)
        gen_layout(case, case_dir)
        gen_rules(case, case_dir)
        students = case_dir / "students.csv"
        layout = case_dir / "classroom.json"
        gen_history(case, case_dir, students, layout)


def gen_goldens(cases: list[dict]) -> None:
    for case in cases:
        cid = case["id"]
        case_dir = INPUTS / cid
        gold_dir = GOLDENS / cid
        # Regeneration is authoritative: drop stale golden files from earlier
        # corpus versions (e.g. a previous candidates threshold) so `verify`
        # never sees committed files that regeneration no longer produces.
        shutil.rmtree(gold_dir, ignore_errors=True)
        gold_dir.mkdir(parents=True, exist_ok=True)
        print(f"[goldens] {cid}")
        if case.get("invalid"):
            gen_invalid_result(case, case_dir, gold_dir)
            continue
        gen_hard_constraints(case, case_dir, gold_dir)
        gen_solve_goldens(case, case_dir, gold_dir)
        gen_history_reports(case, case_dir, gold_dir)
        gen_rotation(case, case_dir, gold_dir)
        gen_migration(case, case_dir, gold_dir)
        gen_export_metadata(case, case_dir, gold_dir)


def _deadline_bound(path: Path) -> bool:
    """True when a fresh snapshot was produced by a deadline-bound solve.

    On a slow machine the fallback can hit the wall-clock budget before its
    deterministic attempt count (max(40, n*12)) is exhausted; such a run is
    nondeterministic by construction and must not be byte-compared.
    """
    try:
        data = read_json(path)
    except (OSError, ValueError):
        return False
    metrics = data.get("metrics", {}) if isinstance(data, dict) else {}
    attempts = metrics.get("attempts")
    limit = metrics.get("attempt_limit")
    if isinstance(attempts, int) and isinstance(limit, int) and attempts < limit:
        return True
    return bool(metrics.get("stopped_by_time_limit"))


def compare_tree(committed: Path, fresh: Path, label: str) -> bool:
    """Byte-compare every file under `committed` with its regenerated twin.

    Fresh snapshots whose solve hit the wall-clock deadline are skipped with
    a visible warning: they are nondeterministic by construction, not drift.
    """
    ok = True
    for path in sorted(committed.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(committed)
        twin = fresh / rel
        if not twin.exists():
            print(f"    MISSING regen ({label}): {rel}")
            ok = False
            continue
        if rel.name.endswith(".snapshot.json") or rel.name == "snapshot.json":
            if _deadline_bound(twin):
                print(f"    SKIP ({label}): {rel} (solve hit the wall-clock "
                      "deadline; nondeterministic by construction)")
                continue
        if twin.read_bytes() != path.read_bytes():
            print(f"    DIFF ({label}): {rel}")
            ok = False
    return ok


def verify_goldens(cases: list[dict]) -> bool:
    """Regenerate into a temp dir and compare byte-for-byte after stripping."""
    import tempfile
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        tmp_root = Path(tmp)
        for case in cases:
            cid = case["id"]
            print(f"[verify] {cid}")
            tmp_case = tmp_root / cid
            gen_students(case, tmp_case)
            gen_layout(case, tmp_case)
            gen_rules(case, tmp_case)
            students = tmp_case / "students.csv"
            layout = tmp_case / "classroom.json"
            gen_history(case, tmp_case, students, layout)
            ok = compare_tree(INPUTS / cid, tmp_case, "inputs") and ok
            tmp_gold = tmp_root / "gold" / cid
            tmp_gold.mkdir(parents=True, exist_ok=True)
            if case.get("invalid"):
                gen_invalid_result(case, tmp_case, tmp_gold)
            else:
                gen_hard_constraints(case, tmp_case, tmp_gold)
                gen_solve_goldens(case, tmp_case, tmp_gold)
                gen_history_reports(case, tmp_case, tmp_gold)
                gen_rotation(case, tmp_case, tmp_gold)
                gen_migration(case, tmp_case, tmp_gold)
                gen_export_metadata(case, tmp_case, tmp_gold)
            ok = compare_tree(GOLDENS / cid, tmp_gold, "goldens") and ok
    return ok


def tree_hashes(root: Path) -> dict[str, dict[str, int | str]]:
    """Relative path -> {sha256, bytes} for every file under `root`."""
    out: dict[str, dict[str, int | str]] = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            rel = path.relative_to(root).as_posix()
            out[rel] = {
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "bytes": path.stat().st_size,
            }
    return out


def write_manifest() -> None:
    git = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, cwd=str(ROOT))
    commit = git.stdout.strip() if git.returncode == 0 else "unknown"
    ver = subprocess.run([cli_bin(), "doctor"], capture_output=True, text=True, cwd=str(ROOT))
    version = "unknown"
    for line in ver.stdout.splitlines() + ver.stderr.splitlines():
        if line.startswith("SeatTrellis"):
            version = line.split()[1]
            break
    manifest = {
        "corpus_name": "seattrellis-v2-parity",
        "corpus_version": "1.0.0",
        "source_commit": commit,
        "seattrellis_version": version,
        "python_version": sys.version.split()[0],
        "generator": "scripts/gen_parity_fixtures.py",
        "solver_backend": SOLVER_BACKEND,
        "pip_extras": "image,excel,pdf,docx,pptx",
        "golden_contract": (
            "Heuristic solutions are not required to match seat-for-seat between "
            "Python and Rust; semantics, legality, scoring definitions and quality "
            "thresholds must match. Timestamp fields are stripped for stability."
        ),
        "normalized_keys": list(NONDETERMINISTIC_KEYS),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "case_count": len(CASES),
        "input_hashes": tree_hashes(INPUTS),
        "golden_hashes": tree_hashes(GOLDENS),
        "cases": {
            c["id"]: {
                "description": c["desc"],
                "n_students": c["n"],
                "layout": c["layout"]["kind"],
                "seat_ratio": c["layout"].get("ratio", "exact"),
                "hard_rules": c["rules"]["hard"],
                "soft_objectives": c["rules"]["soft"],
                "history_periods": c.get("history", 0),
                "rotation_periods": c.get("rotation", 0),
                "profile": c.get("profile", "normal"),
                "migrate": c.get("migrate"),
                "export": c.get("export", False),
                "invalid": c.get("invalid"),
                "seed": case_seed(c["id"]),
            }
            for c in CASES
        },
    }
    write_json(FIXTURES / "MANIFEST.json", manifest)


def main(argv: list[str]) -> int:
    args = [a for a in argv if not a.startswith("--cases")]
    case_filter = None
    for i, a in enumerate(argv):
        if a == "--cases" and i + 1 < len(argv):
            case_filter = set(argv[i + 1].split(","))
    cases = [c for c in CASES if case_filter is None or c["id"] in case_filter]
    if not cases:
        print("no cases matched")
        return 2
    command = args[0] if args else "all"
    if command == "inputs":
        gen_inputs(cases)
    elif command == "goldens":
        gen_goldens(cases)
    elif command == "verify":
        ok = verify_goldens(cases)
        return 0 if ok else 1
    elif command == "all":
        gen_inputs(cases)
        gen_goldens(cases)
    else:
        print(f"unknown command: {command}")
        return 2
    write_manifest()
    print(f"done: {len(cases)} case(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
