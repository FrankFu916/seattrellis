#!/usr/bin/env python
"""Generate parity reference problems and Python fallback baseline results.

This is the Python side of the SeatTrellis Rust solver parity harness (S2). It
freezes what the pure-Python fallback solver produces for a small set of
fictional class problems (40/50/60 students) so that the future Rust solver
(``seattrellis_core``) can be compared against the same inputs on feasibility,
cost, and hard-constraint satisfaction.

Output
------
For every ``(size, profile)`` case the script writes
``benchmarks/reference/{size}-{profile}.json`` with a stable schema:

.. code-block:: json

    {
      "format": "seattrellis-parity-reference",
      "format_version": 1,
      "case": {"size", "profile", "template_id", "seed", "time_limit_seconds"},
      "problem": {  # CoreSolveRequest-compatible (see seattrellis_core::CoreSolveRequest)
        "api_version": 2,
        "student_count", "seat_positions", "edges",
        "fixed_seats", "must_be_adjacent", "cannot_be_adjacent",
        "min_distance": [{"students": [..], "distance": .., "metric": "euclidean"|"graph"}],
        "seed"
      },
      "problem_meta": {"students", "student_keys", "layout", "rules"},
      "python_reference": {
        "feasible", "solver_status", "total_cost",
        "assignment": {student_key: seat_id},
        "assignment_by_index": [[student_index, seat_index], ...],
        "solve_time_seconds", "attempts", "stopped_by_time_limit", "diagnostics"
      }
    }

The ``problem`` block deserializes directly into
``seattrellis_core::CoreSolveRequest``, so the Rust side can feed it unchanged.

Determinism and the time limit
------------------------------
The Python fallback solver keeps exploring randomized improvement attempts until
its attempt cap or its wall-clock deadline. A tight deadline (for example
``--time-limit 3.0``) therefore cuts the improvement loop off at a point that
depends on machine speed and load, which changes the recorded ``total_cost``
(measured: 40 students yields 60431.0 at 3.0 s but 59975.0 when the loop
finishes). To make the reference a pure function of ``(problem, seed)`` the
default runs every seeded attempt to completion and treats ``--time-limit`` as
a safety bound (300.0 s comfortably covers the built-in sizes; 60 students
finishes in roughly 90 s on a typical laptop). Use ``--quick`` for a fast
3.0 s smoke pass, which is *not* guaranteed to be machine independent.

Rules covered per case
----------------------
- fixed seats, must/cannot-be-adjacent, min-distance hard rules
- score-balance and mentor-pairing soft objectives (plus the ``daily`` preset
  vision/height/randomize soft goals)

Usage
-----
    python scripts/benchmark_parity.py                # deterministic reference
    python scripts/benchmark_parity.py --quick        # fast 3 s smoke pass
    python scripts/benchmark_parity.py --sizes 40,60 --skip-determinism-check

The comparison driver is importable:

    from benchmark_parity import compare_native
    report = compare_native("benchmarks/reference/40-parity.json",
                            "native-result.json")
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any

# Force the pure-Python fallback backend before seattrellis reads the
# environment, and pass backend="fallback" explicitly at every solve call.
os.environ["SEATTRELLIS_USE_ORTOOLS"] = "0"

from seattrellis import __version__  # noqa: E402
from seattrellis.application.room_templates import (  # noqa: E402
    build_room_from_template,
    recommend_room_template,
)
from seattrellis.models import (  # noqa: E402
    FixedSeatRule,
    HardRules,
    MentorPairingRule,
    MinDistanceRule,
    PairRule,
    RuleSet,
    WeightedRule,
)
from seattrellis.models.student import Student  # noqa: E402
from seattrellis.presets import get_preset  # noqa: E402
from seattrellis.solver import (  # noqa: E402
    SeatTrellisSolveError,
    compile_problem,
    solve_seating,
)

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

DEFAULT_SIZES = (40, 50, 60)
DEFAULT_PROFILE = "parity"
DEFAULT_SEED = 42
# Safety bound: large enough that the fallback finishes every seeded attempt for
# the built-in sizes, which is what makes the reference machine-independent.
DEFAULT_TIME_LIMIT = 300.0
QUICK_TIME_LIMIT = 3.0

NATIVE_API_VERSION = 2  # seattrellis_core::NATIVE_API_VERSION
FORMAT_NAME = "seattrellis-parity-reference"
FORMAT_VERSION = 1
SOLVER_BACKEND = "fallback"

# Problem shapes are resolved by ``compile_problem``; these key names match
# ``seattrellis_core::CoreSolveRequest`` (api_version, student_count,
# seat_positions, edges, fixed_seats, must_be_adjacent, cannot_be_adjacent,
# min_distance, seed).
PROBLEM_KEYS = (
    "api_version",
    "student_count",
    "seat_positions",
    "edges",
    "fixed_seats",
    "must_be_adjacent",
    "cannot_be_adjacent",
    "min_distance",
    "seed",
)


# ---------------------------------------------------------------------------
# Fictional problem generation
# ---------------------------------------------------------------------------


def make_students(count: int) -> list[Student]:
    """Return deterministic fictional students in the ``examples/`` style.

    Every field is generated from the student index only, so the cohort is a
    pure function of ``count``. Data is deliberately synthetic.
    """

    if count <= 0:
        raise ValueError("student count must be positive")
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
            notes="fictional parity-benchmark student",
        )
        for i in range(1, count + 1)
    ]


def _spread_indexes(length: int, count: int) -> list[int]:
    """Return ``count`` spread indexes over ``range(length)`` (deterministic)."""

    if count <= 0:
        return []
    if count == 1:
        return [0]
    return [round(index * (length - 1) / (count - 1)) for index in range(count)]


def build_rules(
    students: list[Student],
    layout: Any,
    seed: int,
) -> RuleSet:
    """Return a feasible, deterministic ruleset covering the parity goals.

    Soft rules start from the ``daily`` preset and add mentor pairing; without
    history snapshots the preset's fair-rotation and recent-neighbor objectives
    contribute exactly zero cost. Hard rules use a spread of fixed seats plus
    disjoint student pools for adjacent / separated / min-distance pairs so the
    constraints never contradict each other.
    """

    count = len(students)
    student_keys = [student.key for student in students]
    seat_ids = [seat.seat_id for seat in layout.enabled_seats]

    fixed_count = max(1, count // 20)
    fixed_indexes = _spread_indexes(count, fixed_count)
    fixed_set = set(fixed_indexes)
    fixed_seats = [
        FixedSeatRule(student=student_keys[index], seat_id=seat_ids[index])
        for index in fixed_indexes
    ]

    remaining = [student_keys[index] for index in range(count) if index not in fixed_set]
    must_be_adjacent = [
        PairRule(students=(remaining[0], remaining[1])),
        PairRule(students=(remaining[2], remaining[3])),
    ]

    pool = remaining[4:]
    cannot_count = min(len(pool) // 2, count // 4)
    min_distance_count = min(len(pool) // 2 - cannot_count, count // 10)
    cannot_be_adjacent = [
        PairRule(students=(pool[index], pool[-(index + 1)]))
        for index in range(cannot_count)
    ]
    min_distance = [
        MinDistanceRule(
            students=(pool[cannot_count + index], pool[-(cannot_count + index + 1)]),
            distance=2.0,
            metric="graph",
        )
        for index in range(min_distance_count)
    ]

    hard = HardRules(
        fixed_seats=fixed_seats,
        must_be_adjacent=must_be_adjacent,
        cannot_be_adjacent=cannot_be_adjacent,
        min_distance=min_distance,
    )

    soft = get_preset("daily").rules.soft.model_copy(deep=True)
    soft.score_balance = WeightedRule(enabled=True, weight=4)
    soft.mentor_pairing = MentorPairingRule(enabled=True, weight=12)

    return RuleSet(seed=seed, hard=hard, soft=soft)


# ---------------------------------------------------------------------------
# Reference payload construction
# ---------------------------------------------------------------------------


def build_problem_json(compiled: Any, seed: int) -> dict[str, Any]:
    """Compile a ``CoreSolveRequest``-compatible problem description.

    Seat positions use grid coordinates; adjacency edges, fixed seats, and rule
    pairs are resolved to the same student/seat indexes the solver used.
    """

    topology = compiled.topology
    return {
        "api_version": NATIVE_API_VERSION,
        "student_count": len(compiled.students),
        "seat_positions": [
            [float(seat.x), float(seat.y)] for seat in topology.seats
        ],
        "edges": [list(edge) for edge in sorted(topology.adjacent_seat_index_pairs)],
        "fixed_seats": [
            [student_index, seat_index]
            for student_index, seat_index in sorted(
                compiled.rules_compiled.fixed_seats.items()
            )
        ],
        "must_be_adjacent": [
            list(pair) for pair in compiled.rules_compiled.must_be_adjacent
        ],
        "cannot_be_adjacent": [
            list(pair) for pair in compiled.rules_compiled.cannot_be_adjacent
        ],
        "min_distance": [
            {
                "students": [first, second],
                "distance": rule.distance,
                "metric": rule.metric,
            }
            for first, second, rule in compiled.rules_compiled.min_distance
        ],
        "seed": seed,
    }


def _layout_metadata(layout: Any) -> dict[str, Any]:
    metadata = dict(layout.metadata or {})
    return {
        "template_id": metadata.get("template_id"),
        "capacity": metadata.get("capacity", len(layout.enabled_seats)),
        "rows": metadata.get("rows"),
        "seats_per_row": metadata.get("seats_per_row"),
        "aisles_after": metadata.get("aisles_after", []),
        "enabled_seat_count": len(layout.enabled_seats),
    }


def build_problem_meta(
    students: list[Student],
    layout: Any,
    rules: RuleSet,
    compiled: Any,
) -> dict[str, Any]:
    """Attach full (fictional) input data for reproducibility on the Rust side."""

    return {
        "students": [student.model_dump(mode="json") for student in students],
        "student_keys": [student.key for student in students],
        "layout": {
            **_layout_metadata(layout),
            "seat_ids": [seat.seat_id for seat in compiled.topology.seats],
        },
        "rules": rules.model_dump(mode="json"),
    }


def solve_reference(
    compiled: Any,
    students: list[Student],
    layout: Any,
    rules: RuleSet,
    seed: int,
    time_limit: float,
) -> dict[str, Any]:
    """Run the Python fallback solver and format the reference result.

    ``SeatTrellisSolveError`` (infeasible or timed out) is captured into
    ``diagnostics`` so every case still emits a well-formed reference record.
    """

    started = time.monotonic()
    try:
        solution = solve_seating(
            students,
            layout,
            rules,
            seed=seed,
            time_limit_seconds=time_limit,
            backend=SOLVER_BACKEND,
        )
    except SeatTrellisSolveError as exc:
        elapsed = time.monotonic() - started
        return {
            "feasible": False,
            "solver_status": "INFEASIBLE",
            "total_cost": None,
            "assignment": {},
            "assignment_by_index": [],
            "solve_time_seconds": round(elapsed, 4),
            "solver_backend": SOLVER_BACKEND,
            "attempts": None,
            "attempt_limit": None,
            "stopped_by_time_limit": None,
            "diagnostics": [str(exc)],
        }

    elapsed = time.monotonic() - started
    seat_index_by_id = compiled.topology.seat_index_by_id
    student_index_by_key = compiled.topology.student_index_by_key
    assignment_by_index = [
        [
            student_index_by_key[assignment.student_key],
            seat_index_by_id[assignment.seat_id],
        ]
        for assignment in solution.assignments
    ]
    assignment_by_index.sort(key=lambda pair: pair[0])

    return {
        "feasible": solution.solver_status == "FEASIBLE",
        "solver_status": solution.solver_status,
        "total_cost": solution.objective_value,
        "assignment": solution.assignment_map,
        "assignment_by_index": assignment_by_index,
        "solve_time_seconds": round(elapsed, 4),
        "solver_backend": solution.metrics.get("solver_backend_effective", SOLVER_BACKEND),
        "attempts": solution.metrics.get("attempts"),
        "attempt_limit": solution.metrics.get("attempt_limit"),
        "stopped_by_time_limit": bool(solution.metrics.get("stopped_by_time_limit")),
        "diagnostics": [],
    }


def build_case(
    size: int,
    profile: str,
    seed: int,
    time_limit: float,
) -> dict[str, Any]:
    """Build one full parity reference payload for a class size."""

    students = make_students(size)
    template = recommend_room_template(size)
    if template is None:
        raise ValueError(
            f"No built-in room template can seat {size} students; "
            "use a size up to 60."
        )
    layout = build_room_from_template(template)
    rules = build_rules(students, layout, seed)
    compiled = compile_problem(students, layout, rules)
    problem_json = build_problem_json(compiled, seed)
    reference = solve_reference(compiled, students, layout, rules, seed, time_limit)

    return {
        "format": FORMAT_NAME,
        "format_version": FORMAT_VERSION,
        "seattrellis_version": __version__,
        "solver_backend": SOLVER_BACKEND,
        "case": {
            "size": size,
            "profile": profile,
            "template_id": template.template_id,
            "seed": seed,
            "time_limit_seconds": time_limit,
        },
        "problem": problem_json,
        "problem_meta": build_problem_meta(students, layout, rules, compiled),
        "python_reference": reference,
    }


def _payload_signature(payload: dict[str, Any]) -> str:
    """Canonical JSON of everything except wall-clock solve time."""

    signature = json.loads(json.dumps(payload))
    signature["python_reference"].pop("solve_time_seconds", None)
    return json.dumps(signature, sort_keys=True, separators=(",", ":"))


# ---------------------------------------------------------------------------
# Native-vs-reference comparison driver
# ---------------------------------------------------------------------------


def _load_json(value: Any) -> dict[str, Any] | None:
    if value is None:
        return None
    if isinstance(value, dict):
        return value
    if isinstance(value, (str, Path)):
        path = Path(value)
        if not path.exists():
            raise FileNotFoundError(f"Result file not found: {path}")
        return json.loads(path.read_text(encoding="utf-8"))
    raise TypeError(f"Unsupported input: {type(value)!r}")


def _coerce_problem(problem_json: Any) -> tuple[dict[str, Any], dict[str, Any] | None]:
    """Return ``(problem, problem_meta)`` from a payload or a bare problem dict."""

    data = _load_json(problem_json) or {}
    if isinstance(data.get("problem"), dict):
        return data["problem"], data.get("problem_meta")
    return data, None


def _coerce_native(native_result_json: Any) -> dict[str, Any] | None:
    data = _load_json(native_result_json)
    if not data:
        return None
    if isinstance(data.get("result"), dict):
        return data["result"]
    return data


def _normalize_native_assignment(
    native: dict[str, Any],
    problem: dict[str, Any],
    problem_meta: dict[str, Any] | None,
) -> tuple[dict[int, int] | None, str | None]:
    """Return ``{student_index: seat_index}`` or ``(None, error)``.

    Accepts the native ``assignment`` as ``[[student_index, seat_index], ...]``
    or as ``{student_key: seat_id}`` (when the payload includes ``student_keys``
    and ordered ``seat_ids``).
    """

    raw = native.get("assignment")
    if raw is None:
        return None, "native result has no 'assignment'"
    if isinstance(raw, list):
        normalized: dict[int, int] = {}
        for pair in raw:
            if not isinstance(pair, (list, tuple)) or len(pair) != 2:
                return None, "assignment pairs must be [student_index, seat_index]"
            student_index, seat_index = int(pair[0]), int(pair[1])
            if student_index in normalized:
                return None, f"duplicate student index {student_index} in assignment"
            normalized[student_index] = seat_index
        return normalized, None
    if isinstance(raw, dict):
        if not problem_meta:
            return None, "student-key assignment requires a reference problem_meta"
        student_keys = problem_meta.get("student_keys") or []
        seat_ids = (problem_meta.get("layout") or {}).get("seat_ids") or []
        key_to_index = {key: index for index, key in enumerate(student_keys)}
        id_to_index = {seat_id: index for index, seat_id in enumerate(seat_ids)}
        normalized = {}
        for student_ref, seat_ref in raw.items():
            student_index = (
                int(student_ref)
                if str(student_ref).isdigit()
                else key_to_index.get(str(student_ref))
            )
            seat_index = (
                int(seat_ref)
                if str(seat_ref).isdigit()
                else id_to_index.get(str(seat_ref))
            )
            if student_index is None:
                return None, f"unknown student reference {student_ref!r}"
            if seat_index is None:
                return None, f"unknown seat reference {seat_ref!r}"
            normalized[student_index] = seat_index
        return normalized, None
    return None, "unsupported assignment format"


def _graph_distances(seat_count: int, edges: list[list[int]]) -> list[list[float]]:
    """BFS graph-distance matrix over the seat adjacency edges (matching Rust)."""

    adjacency: list[list[int]] = [[] for _ in range(seat_count)]
    for first, second in edges:
        adjacency[first].append(second)
        adjacency[second].append(first)
    matrix: list[list[float]] = []
    for source in range(seat_count):
        distances: list[float | None] = [None] * seat_count
        distances[source] = 0.0
        queue = [source]
        while queue:
            seat = queue.pop(0)
            for neighbor in adjacency[seat]:
                if distances[neighbor] is None:
                    distances[neighbor] = distances[seat] + 1.0
                    queue.append(neighbor)
        matrix.append([value if value is not None else float("inf") for value in distances])
    return matrix


def evaluate_hard_constraints(
    problem: dict[str, Any],
    assignment_by_student: dict[int, int] | None,
) -> dict[str, Any]:
    """Check a native assignment against every hard rule in ``problem``.

    This mirrors ``seattrellis_core``'s evaluation: uniqueness plus the four
    hard-rule families, using grid-coordinate Euclidean distance and graph-hop
    distance exactly as the core does.
    """

    student_count = problem.get("student_count", 0)
    seat_positions = problem.get("seat_positions", [])
    seat_count = len(seat_positions)
    edges = problem.get("edges", [])
    adjacency = {tuple(sorted(edge)) for edge in edges}
    graph_distances = _graph_distances(seat_count, edges)

    assignment = dict(assignment_by_student or {})

    def assigned(student_index: int) -> int | None:
        return assignment.get(student_index)

    def are_adjacent(first_seat: int, second_seat: int) -> bool:
        return tuple(sorted((first_seat, second_seat))) in adjacency

    def seats_meet_distance(
        first_student: int,
        second_student: int,
        distance: float,
        metric: str,
    ) -> bool:
        first_seat = assigned(first_student)
        second_seat = assigned(second_student)
        if first_seat is None or second_seat is None:
            return False
        if metric == "graph":
            value = graph_distances[first_seat][second_seat]
        else:  # euclidean
            first = seat_positions[first_seat]
            second = seat_positions[second_seat]
            value = ((first[0] - second[0]) ** 2 + (first[1] - second[1]) ** 2) ** 0.5
        return value >= distance

    used_seats = [seat for seat in assignment.values()]
    assignment_complete = len(assignment) == student_count
    assignment_unique = (
        len(used_seats) == len(set(used_seats))
        and all(0 <= seat < seat_count for seat in used_seats)
    )

    fixed_violations = [
        [student_index, seat_index]
        for student_index, seat_index in problem.get("fixed_seats", [])
        if assigned(student_index) != seat_index
    ]
    must_violations = [
        [first, second]
        for first, second in problem.get("must_be_adjacent", [])
        if assigned(first) is not None
        and assigned(second) is not None
        and not are_adjacent(assigned(first), assigned(second))  # type: ignore[arg-type]
    ]
    cannot_violations = [
        [first, second]
        for first, second in problem.get("cannot_be_adjacent", [])
        if assigned(first) is not None
        and assigned(second) is not None
        and are_adjacent(assigned(first), assigned(second))  # type: ignore[arg-type]
    ]
    min_distance_violations = [
        [student_index, seat_index]
        for rule in problem.get("min_distance", [])
        for student_index, seat_index in [rule["students"]]
        if not seats_meet_distance(
            student_index, seat_index, rule["distance"], rule.get("metric", "euclidean")
        )
    ]

    categories = {
        "fixed_seats": {"violations": fixed_violations, "satisfied": not fixed_violations},
        "must_be_adjacent": {"violations": must_violations, "satisfied": not must_violations},
        "cannot_be_adjacent": {"violations": cannot_violations, "satisfied": not cannot_violations},
        "min_distance": {"violations": min_distance_violations, "satisfied": not min_distance_violations},
    }
    hard_satisfied = (
        assignment_complete
        and assignment_unique
        and all(category["satisfied"] for category in categories.values())
    )
    return {
        "assignment_complete": assignment_complete,
        "assignment_unique": assignment_unique,
        "categories": categories,
        "hard_constraints_satisfied": hard_satisfied,
    }


def compare_native(
    problem_json: Any,
    native_result_json: Any,
) -> dict[str, Any]:
    """Return a per-item diff report between a native result and the reference.

    Parameters
    ----------
    problem_json:
        Either a full reference payload (a ``benchmarks/reference/*.json`` dict
        or file path, containing ``problem`` and ``python_reference``) or a bare
        ``CoreSolveRequest``-compatible problem dict.
    native_result_json:
        A native solver result dict or file path. Accepts ``feasible``,
        ``assignment`` (index pairs or key/seat-id dict), and ``cost`` or
        ``total_cost``. ``None``/empty yields the "not provided" placeholder.

    The returned report covers feasibility agreement, cost difference, and a
    hard-constraint audit of the native assignment. It never raises on an empty
    native result.
    """

    problem, problem_meta = _coerce_problem(problem_json)
    if not problem:
        return {
            "native_result_provided": False,
            "status": "problem-unavailable",
            "summary": "Reference problem JSON is empty; nothing to compare.",
        }
    missing = [key for key in PROBLEM_KEYS if key not in problem]
    if missing:
        return {
            "native_result_provided": False,
            "status": "problem-invalid",
            "missing_keys": missing,
            "summary": (
                "Reference problem JSON is missing CoreSolveRequest fields: "
                + ", ".join(missing)
                + "."
            ),
        }

    native = _coerce_native(native_result_json)
    if native is None:
        return {
            "native_result_provided": False,
            "status": "native-result-not-provided",
            "student_count": problem.get("student_count"),
            "summary": (
                "Native result not provided. Feed the same problem to the Rust "
                "solver and pass its output here to compare feasibility, cost, "
                "and hard-constraint satisfaction."
            ),
        }

    # Pull the embedded python_reference when the input is a full payload.
    if isinstance(problem_json, dict):
        python_ref = problem_json.get("python_reference")
    else:
        loaded = _load_json(problem_json) or {}
        python_ref = loaded.get("python_reference")

    python_feasible = (
        bool(python_ref.get("feasible")) if isinstance(python_ref, dict) else None
    )
    python_cost = (
        python_ref.get("total_cost") if isinstance(python_ref, dict) else None
    )
    native_feasible = bool(native.get("feasible"))
    native_cost = native.get("cost", native.get("total_cost"))
    native_cost = float(native_cost) if native_cost is not None else None

    assignment, assignment_error = _normalize_native_assignment(native, problem, problem_meta)
    hard_checks: dict[str, Any] | None = None
    if assignment_error is not None:
        hard_checks = {
            "assignment_complete": False,
            "assignment_unique": False,
            "categories": {},
            "hard_constraints_satisfied": False,
            "normalization_error": assignment_error,
        }
    else:
        hard_checks = evaluate_hard_constraints(problem, assignment)

    overlap: float | None = None
    python_assignment = (
        (python_ref or {}).get("assignment_by_index")
        if isinstance(python_ref, dict)
        else None
    )
    if (
        assignment is not None
        and isinstance(python_assignment, list)
        and python_assignment
    ):
        python_map = {pair[0]: pair[1] for pair in python_assignment}
        matched = sum(
            1 for student_index, seat_index in assignment.items()
            if python_map.get(student_index) == seat_index
        )
        overlap = matched / len(python_map)

    if python_feasible is not None and python_feasible != native_feasible:
        verdict = "FEASIBILITY_MISMATCH"
    elif not native_feasible:
        verdict = "BOTH_INFEASIBLE" if python_feasible is False else "NATIVE_INFEASIBLE"
    elif hard_checks and hard_checks.get("normalization_error"):
        verdict = "NATIVE_ASSIGNMENT_UNPARSEABLE"
    elif hard_checks and not hard_checks["hard_constraints_satisfied"]:
        verdict = "NATIVE_HARD_VIOLATION"
    elif (
        native_cost is not None
        and python_cost is not None
        and abs(native_cost - python_cost) > 1e-6
    ):
        verdict = "COST_DIFFERS"
    else:
        verdict = "MATCH"

    cost_difference = (
        python_cost - native_cost
        if python_cost is not None and native_cost is not None
        else None
    )

    return {
        "native_result_provided": True,
        "verdict": verdict,
        "feasibility": {
            "python": python_feasible,
            "native": native_feasible,
            "agreement": python_feasible is None or python_feasible == native_feasible,
        },
        "cost": {
            "python": python_cost,
            "native": native_cost,
            "difference": cost_difference,
            "cost_available": native_cost is not None,
        },
        "assignment": {
            "normalization_error": assignment_error,
            "hard_constraint_checks": hard_checks,
            "overlap_with_python": overlap,
        },
        "summary": _format_compare_summary(
            verdict,
            python_feasible,
            native_feasible,
            native_cost,
            python_cost,
            hard_checks,
            overlap,
        ),
    }


def _format_compare_summary(
    verdict: str,
    python_feasible: bool | None,
    native_feasible: bool,
    native_cost: float | None,
    python_cost: float | None,
    hard_checks: dict[str, Any] | None,
    overlap: float | None,
) -> str:
    lines = [f"Verdict: {verdict}"]
    lines.append(
        f"  feasibility: python={python_feasible} native={native_feasible}"
    )
    if native_cost is not None and python_cost is not None:
        lines.append(f"  cost: python={python_cost:g} native={native_cost:g} "
                     f"delta={python_cost - native_cost:g}")
    elif native_cost is None:
        lines.append("  cost: native cost not provided")
    if hard_checks:
        lines.append(
            "  hard constraints: "
            + (
                "satisfied"
                if hard_checks.get("hard_constraints_satisfied")
                else "VIOLATED"
            )
        )
    if overlap is not None:
        lines.append(f"  assignment overlap with python: {overlap:.1%}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate parity reference problems and Python fallback baselines."
    )
    parser.add_argument(
        "--sizes",
        default=",".join(str(size) for size in DEFAULT_SIZES),
        help="Comma-separated class sizes.",
    )
    parser.add_argument("--profile", default=DEFAULT_PROFILE, help="Reference profile name.")
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED, help="Deterministic seed.")
    parser.add_argument(
        "--time-limit",
        type=float,
        default=DEFAULT_TIME_LIMIT,
        help=(
            "Wall-clock safety bound in seconds per solve. The fallback runs "
            "every seeded attempt to completion, so this only guards against "
            "slow machines."
        ),
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Fast 3-second smoke pass; skips the determinism re-check. Results "
        "are deadline-capped and not guaranteed machine independent.",
    )
    parser.add_argument(
        "--skip-determinism-check",
        action="store_true",
        help="Generate once without re-running to verify determinism.",
    )
    parser.add_argument(
        "--output-dir",
        default=None,
        help="Output directory for reference JSON files "
        "(default: <repo>/benchmarks/reference).",
    )
    parser.add_argument(
        "--compare-native",
        default=None,
        help="Path to a native result JSON to compare against --reference.",
    )
    parser.add_argument(
        "--reference",
        default=None,
        help="Reference JSON to compare against (requires --compare-native).",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Print the full JSON payload for every case.",
    )
    parser.add_argument("--quiet", action="store_true", help="Print only the summary lines.")
    args = parser.parse_args()

    if args.quick:
        args.time_limit = QUICK_TIME_LIMIT
        args.skip_determinism_check = True
    if args.compare_native and not args.reference:
        parser.error("--compare-native requires --reference.")
    if args.reference and not args.compare_native:
        parser.error("--reference requires --compare-native.")
    if args.time_limit < 0.1:
        parser.error("--time-limit must be at least 0.1 seconds.")
    return args


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _default_output_dir() -> Path:
    return _repo_root() / "benchmarks" / "reference"


def main() -> int:
    args = _parse_args()

    # Comparison-only mode: no generation, just diff an existing reference
    # against a native solver output.
    if args.compare_native:
        report = compare_native(Path(args.reference), Path(args.compare_native))
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0

    sizes = [int(item.strip()) for item in args.sizes.split(",") if item.strip()]
    if not sizes:
        print("At least one size is required.", file=sys.stderr)
        return 2
    if args.time_limit >= DEFAULT_TIME_LIMIT:
        print(
            f"Note: --time-limit {args.time_limit:g}s is used as a safety bound; "
            "the fallback runs every seeded attempt to completion so the "
            "reference is machine independent.",
            file=sys.stderr,
        )

    output_dir = Path(args.output_dir) if args.output_dir else _default_output_dir()
    output_dir.mkdir(parents=True, exist_ok=True)

    def generate() -> dict[int, dict[str, Any]]:
        payloads: dict[int, dict[str, Any]] = {}
        for size in sizes:
            payloads[size] = build_case(
                size, args.profile, seed=args.seed, time_limit=args.time_limit
            )
        return payloads

    payloads = generate()

    if not args.skip_determinism_check:
        print("Verifying determinism (re-running every case) ...", file=sys.stderr)
        second = generate()
        mismatches = [
            size
            for size in sizes
            if _payload_signature(payloads[size]) != _payload_signature(second[size])
        ]
        if mismatches:
            print(
                "Determinism check FAILED for sizes "
                + ", ".join(str(size) for size in mismatches)
                + ". The fallback result changed between identical runs; "
                "increase --time-limit so every seeded attempt completes.",
                file=sys.stderr,
            )
            return 1
        print("Determinism check passed (same seed -> same output).", file=sys.stderr)

    written: list[Path] = []
    for size in sizes:
        payload = payloads[size]
        reference = payload["python_reference"]
        stopped = reference.get("stopped_by_time_limit")
        if stopped:
            print(
                f"WARNING: size {size} stopped at the time-limit "
                f"({args.time_limit:g}s); the reference is machine dependent. "
                "Raise --time-limit.",
                file=sys.stderr,
            )
        path = output_dir / f"{size}-{args.profile}.json"
        path.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        written.append(path)
        if not args.quiet:
            print(
                f"[{size}-{args.profile}] template={payload['case']['template_id']} "
                f"feasible={reference['feasible']} "
                f"cost={reference['total_cost']} "
                f"elapsed={reference['solve_time_seconds']}s "
                f"attempts={reference['attempts']} "
                f"stopped={bool(stopped)} -> {path}"
            )
        if args.verbose:
            print(json.dumps(payload, ensure_ascii=False, indent=2))

    print(f"Wrote {len(written)} reference file(s) to {output_dir}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
