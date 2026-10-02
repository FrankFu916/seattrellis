#!/usr/bin/env python3
"""Experimental, deliberately bounded CP-SAT soft-objective parity checks.

No production registration, Python runtime dependency, or universal-optimality
claim. Unsupported active goals are rejected rather than silently omitted.
"""
from __future__ import annotations

import argparse
from collections import Counter, deque
from dataclasses import dataclass
from fractions import Fraction
import hashlib
from itertools import combinations, permutations
import json
import math
from pathlib import Path
import platform
import re
import subprocess
import sys
import time

import google.protobuf
import ortools
from ortools.sat.python import cp_model

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
SOFT_NAMES = (
    "vision_front", "height_back", "randomize", "score_balance", "score_position",
    "score_distribution", "mentor_pairing", "fair_rotation", "avoid_recent_neighbors", "cooling",
)
SUPPORTED = {"vision_front", "height_back", "score_position", "score_distribution", "fair_rotation"}
SAFE_INTEGER = 2**60  # Conservative headroom below CP-SAT signed-int64 limits.
VISION_MARKERS = {"vision", "vision_front", "front", "poor", "low", "nearsighted",
                  "short_sighted", "myopia", "视力", "近视", "靠前"}
CATEGORIES = {"front", "back", "middle", "side", "corner", "near_window", "near_door",
              "near_platform", "near_ac"}
RUST_FLOAT_SYNTAX = re.compile(
    r"[+-]?(?:inf(?:inity)?|nan|(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?)")


def require(condition: bool, message: str):
    if not condition:
        raise ValueError(message)


def ranks(students: list[dict]) -> dict[int, Fraction]:
    scored = sorted((float(s["score"]), s["key"], i) for i, s in enumerate(students)
                    if s.get("score") is not None)
    if len(scored) < 2 or scored[0][0] == scored[-1][0]:
        return {}
    result = {}
    start = 0
    while start < len(scored):
        end = start + 1
        while end < len(scored) and scored[end][0] == scored[start][0]:
            end += 1
        for _, _, i in scored[start:end]:
            result[i] = Fraction(start + end - 1, 2 * (len(scored) - 1))
        start = end
    return result


def needs_front(student: dict) -> bool:
    vision = student.get("vision")
    values = [item.lower() for item in student.get("tags", []) + student.get("needs", [])]
    if vision is not None:
        lowered = vision.lower()
        values.append(lowered)
        # Python float accepts whitespace, underscores and Unicode digits;
        # Rust f64::parse accepts none of them. Parsing precedence matters:
        # rejected numeric-looking strings must still consult vision tags.
        if RUST_FLOAT_SYNTAX.fullmatch(lowered):
            return float(lowered) < 1.0
    return bool(VISION_MARKERS.intersection(values))


def seat_categories(seat: dict, seats: list[dict]) -> set[str]:
    rows = sorted({s["row"] for s in seats})
    cols = sorted({s["col"] for s in seats})
    zone = (seat.get("zone") or "").strip().lower().replace("-", "_").replace(" ", "_")
    result = {zone} if zone in CATEGORIES else set()
    if zone not in {"front", "back", "middle"}:
        result.add("middle" if len(rows) == 1 else
                   "front" if seat["row"] == rows[0] else
                   "back" if seat["row"] == rows[-1] else "middle")
    if seat["col"] in {cols[0], cols[-1]}:
        result.add("side")
    if seat["row"] in {rows[0], rows[-1]} and seat["col"] in {cols[0], cols[-1]}:
        result.add("corner")
    for category in ("near_window", "near_door", "near_platform", "near_ac"):
        if seat.get(category):
            result.add(category)
    return result


def rotation_cost(student: dict, seat: dict, request: dict, rule: dict) -> int:
    history = request.get("history")
    if not history or history["history_count"] == 0:
        return 0
    student_history = history["students"].get(student["key"])
    if student_history is None:
        return 0
    relevant = seat_categories(seat, request["layout"]["seats"]).intersection(
        rule.get("avoid_repeating_categories", ["front", "back", "side", "corner",
                                              "near_window", "near_door", "near_ac"]))
    records = student_history.get("records", [])
    lookback = rule.get("lookback", 4)
    if lookback is not None and lookback <= 0:
        recent = []
    elif all(record.get("period_index") is not None for record in records):
        first = 1 if lookback is None else history["history_count"] - lookback + 1
        recent = [record for record in records if first <= record["period_index"] <= history["history_count"]]
    else:
        recent = records if lookback is None else records[-lookback:]
    counts = Counter(category for record in recent for category in record.get("categories", []))
    total = 0
    for category in relevant:
        assigned = student_history.get("category_counts", {}).get(category, 0)
        minimum = min(s.get("category_counts", {}).get(category, 0)
                      for s in history["students"].values())
        total += counts[category] * 100 + max(assigned - minimum, 0) * 25 - (10 if assigned == minimum else 0)
    return rule.get("weight", 10) * total


@dataclass
class Compiled:
    unary: list[list[Fraction]]
    percentiles: dict[int, Fraction]
    distribution_buckets: list[int] | None
    distribution_capacity: int
    distribution_weight: int
    scale: int
    integer_unary: list[list[int]]
    rank_units: dict[int, int]
    distribution_coefficient: int
    absolute_objective_bound: int


def compile_objective(request: dict, *, scale_multiplier: int = 1) -> Compiled:
    require(isinstance(scale_multiplier, int) and not isinstance(scale_multiplier, bool)
            and scale_multiplier > 0, "scale multiplier must be a positive integer")
    require(request.get("api_version") == 2, "only explicit v2 requests are supported")
    students, seats = request["students"], request["layout"]["seats"]
    n, m = request["student_count"], len(seats)
    require(isinstance(n, int) and not isinstance(n, bool), "student_count must be a JSON integer")
    require(n == len(students) and 1 <= n <= m <= 80, "invalid prototype student/seat capacity")
    require(len({s["key"] for s in students}) == n, "student keys must be unique")
    require(len({s["seat_id"] for s in seats}) == m, "seat identifiers must be unique")
    require(all(s.get("enabled", True) is True for s in seats), "disabled/nonboolean layout seats unsupported")
    require(len(request["seat_positions"]) == m, "layout and positions must be aligned")
    require(all(len(p) == 2 and all(isinstance(v, (int, float)) and not isinstance(v, bool)
                                  and math.isfinite(v) for v in p)
                for p in request["seat_positions"]), "coordinates must be finite")
    validate_distance_domain(request)
    for seat in seats:
        require(all(isinstance(seat[k], int) and not isinstance(seat[k], bool)
                    and -(2**31) <= seat[k] < 2**31 for k in ("row", "col")), "row/col must fit i32")
    soft = request["rules"]["soft"]
    require(set(soft) == set(SOFT_NAMES), "all ten goals must explicitly specify enabled; unknown goals rejected")
    for name, rule in soft.items():
        require(isinstance(rule.get("enabled"), bool), f"{name}: explicit enabled required")
        weight = rule.get("weight", 10 if name == "fair_rotation" else 1)
        require(isinstance(weight, int) and not isinstance(weight, bool) and 0 <= weight < 2**31,
                f"{name}: weight must be a nonnegative i32")
        require(not rule["enabled"] or weight <= 1_000_000,
                f"{name}: enabled weight exceeds production limit 1000000")
        require(not rule["enabled"] or name in SUPPORTED, f"unsupported enabled soft rule: {name}")
    require(request.get("pair_history") is None, "pair history unsupported; no pair goal implemented")
    for student in students:
        for name in ("score", "height_cm"):
            value = student.get(name)
            require(value is None or (isinstance(value, (int, float)) and not isinstance(value, bool)
                                      and math.isfinite(value)), f"{name} must be a finite JSON number or null")
        require(student.get("score") is None or abs(student["score"]) <= 1e9,
                "score outside production range [-1e9, 1e9]")
        require(student.get("height_cm") is None or 0 <= student["height_cm"] <= 300,
                "height outside production range [0, 300]")
        require(student.get("vision") is None or isinstance(student["vision"], str),
                "vision must be a JSON string or null, matching the Rust student DTO")
        require(all(isinstance(value, list) and all(isinstance(item, str) for item in value)
                    for value in [student.get("tags", []), student.get("needs", [])]),
                "tags and needs must contain JSON strings")
    history = request.get("history")
    if history:
        require(isinstance(history["history_count"], int) and 0 <= history["history_count"] < 2**31,
                "history_count must be nonnegative i32")
        for hist in history["students"].values():
            require(all(isinstance(v, int) and 0 <= v < 2**31
                        for v in hist.get("category_counts", {}).values()), "category count outside i32")
    active = {name for name, rule in soft.items() if rule["enabled"] and rule.get("weight", 10 if name == "fair_rotation" else 1)}
    rows = sorted({s["row"] for s in seats})
    row_ranks = {row: Fraction(i, len(rows) - 1) if len(rows) > 1 else Fraction(1, 2)
                 for i, row in enumerate(rows)}
    percentiles = ranks(students)
    unary = [[Fraction() for _ in seats] for _ in students]
    direction = soft["score_position"].get("direction", "high_front")
    require(direction in {"high_front", "high_back"}, "unsupported score_position direction")
    for i, student in enumerate(students):
        for j, seat in enumerate(seats):
            if "vision_front" in active and needs_front(student):
                unary[i][j] += soft["vision_front"].get("weight", 1) * (seat["row"] - rows[0]) * 100
            if "height_back" in active and student.get("height_cm") is not None:
                # Python round and Rust round_half_even agree on finite, in-range
                # binary64 heights. Saturating Rust ranges are explicitly rejected.
                height = round(student["height_cm"])
                require(abs(height) < 2**63, "height requires Rust saturation; unsupported")
                unary[i][j] += soft["height_back"].get("weight", 1) * height * (rows[-1] - seat["row"])
            if "score_position" in active and i in percentiles:
                target = percentiles[i] if direction == "high_back" else 1 - percentiles[i]
                unary[i][j] += (100 * soft["score_position"].get("weight", 1)
                                * abs(target - row_ranks[seat["row"]]) / len(percentiles))
            if "fair_rotation" in active:
                unary[i][j] += rotation_cost(student, seat, request, soft["fair_rotation"])
    buckets = None
    capacity = distribution_weight = 0
    if "score_distribution" in active:
        require(n == m and len(percentiles) == n,
                "distribution requires full occupancy and a nonconstant score for every student")
        scope = soft["score_distribution"].get("scope", "row")
        require(scope in {"row", "group"}, "unsupported score_distribution scope")
        labels = [s["row"] if scope == "row" else s.get("group_id") for s in seats]
        require(None not in labels, "group distribution requires group_id on every seat")
        unique = sorted(set(labels))
        require(len(unique) == 2 and labels.count(unique[0]) == labels.count(unique[1]),
                "distribution prototype supports exactly two equal-capacity populated buckets")
        buckets = [unique.index(label) for label in labels]
        capacity = n // 2
        distribution_weight = soft["score_distribution"].get("weight", 1)
    rank_scale = math.lcm(*(p.denominator for p in percentiles.values())) if percentiles else 1
    rank_units = {i: int(p * rank_scale) for i, p in percentiles.items()}
    distribution_unit = Fraction(100 * distribution_weight, capacity * rank_scale) if buckets else Fraction()
    scale = math.lcm(*(c.denominator for row in unary for c in row), distribution_unit.denominator) * scale_multiplier
    require(scale <= SAFE_INTEGER, "integer scaling exceeds conservative int64 limit")
    integer_unary = [[int(c * scale) for c in row] for row in unary]
    distribution_coefficient = int(distribution_unit * scale)
    gap_bound = sum(rank_units.values())
    # Also bound all Boolean coefficients (rather than relying on exactly-one)
    # because CP-SAT validates expression domains before propagating constraints.
    absolute_bound = sum(abs(c) for row in integer_unary for c in row) + abs(distribution_coefficient) * gap_bound
    require(absolute_bound <= SAFE_INTEGER, "scaled objective may overflow int64; unsupported")
    return Compiled(unary, percentiles, buckets, capacity, distribution_weight,
                    scale, integer_unary, rank_units, distribution_coefficient, absolute_bound)


def objective(compiled: Compiled, assignment: tuple[int, ...]) -> Fraction:
    cost = sum((compiled.unary[i][s] for i, s in enumerate(assignment)), Fraction())
    if compiled.distribution_buckets is not None:
        totals = [Fraction(), Fraction()]
        for i, seat in enumerate(assignment):
            totals[compiled.distribution_buckets[seat]] += compiled.percentiles[i]
        cost += 100 * compiled.distribution_weight * abs(totals[0] - totals[1]) / compiled.distribution_capacity
    return cost


def hard_pairs(request: dict):
    must = [tuple(pair) for pair in request.get("must_be_adjacent", [])]
    cannot = [tuple(pair) for pair in request.get("cannot_be_adjacent", [])]
    indices = {student["key"]: i for i, student in enumerate(request["students"])}
    for group in request["rules"].get("groups", []):
        members = list(dict.fromkeys(indices[key] for key in group["students"]))
        if group.get("together"):
            must.extend(combinations(members, 2))
        if group.get("separate"):
            cannot.extend(combinations(members, 2))
    return must, cannot


def graph_distances(request: dict) -> list[list[float]]:
    m = len(request["seat_positions"])
    neighbors = [set() for _ in range(m)]
    for a, b in request["edges"]:
        require(0 <= a < m and 0 <= b < m and a != b, "invalid adjacency edge")
        neighbors[a].add(b)
        neighbors[b].add(a)
    result = []
    for source in range(m):
        distances = [math.inf] * m
        distances[source] = 0
        queue = deque([source])
        while queue:
            current = queue.popleft()
            for other in neighbors[current]:
                if math.isinf(distances[other]):
                    distances[other] = distances[current] + 1
                    queue.append(other)
        result.append(distances)
    return result


def validate_distance_domain(request: dict):
    for rule in request["min_distance"]:
        distance = rule["distance"]
        require(isinstance(distance, (int, float)) and not isinstance(distance, bool)
                and math.isfinite(distance) and distance > 0, "distance must be positive and finite")
        require(rule["metric"] in {"graph", "euclidean"}, "unsupported distance metric")
        if rule["metric"] == "euclidean":
            # math.dist and Rust hypot can differ by one ULP. This experiment
            # therefore supports only a bounded integer domain whose squared
            # comparisons are exact, instead of accepting ambiguous boundaries.
            require(distance == int(distance) and distance <= 3_000_000,
                    "Euclidean prototype requires an integer threshold <= 3000000")
            require(all(coordinate == int(coordinate) and abs(coordinate) <= 1_000_000
                        for position in request["seat_positions"] for coordinate in position),
                    "Euclidean prototype requires integer coordinates within +/-1000000")


def distance_satisfied(request: dict, distances, rule: dict, a: int, b: int) -> bool:
    if rule["metric"] == "graph":
        return distances[a][b] >= rule["distance"]
    ax, ay = map(int, request["seat_positions"][a])
    bx, by = map(int, request["seat_positions"][b])
    return (ax - bx)**2 + (ay - by)**2 >= int(rule["distance"])**2


def legal(request: dict, assignment: tuple[int, ...]) -> bool:
    validate_distance_domain(request)
    if len(set(assignment)) != len(assignment):
        return False
    if any(assignment[i] != s for i, s in request["fixed_seats"]):
        return False
    edges = {tuple(sorted(edge)) for edge in request["edges"]}
    must, cannot = hard_pairs(request)
    if any(tuple(sorted((assignment[a], assignment[b]))) not in edges for a, b in must):
        return False
    if any(tuple(sorted((assignment[a], assignment[b]))) in edges for a, b in cannot):
        return False
    distances = graph_distances(request)
    return all(distance_satisfied(request, distances, rule, assignment[rule["students"][0]],
                                  assignment[rule["students"][1]])
               for rule in request["min_distance"])


def cp_run(request: dict, *, forced_assignment: tuple[int, ...] | None = None) -> dict:
    started = time.perf_counter()  # Includes validation, compilation and model building.
    compiled = compile_objective(request)
    budget = request.get("time_limit_seconds", 2.0)
    require(math.isfinite(budget) and budget > 0, "budget must be positive and finite")
    n, m = request["student_count"], len(request["seat_positions"])
    model = cp_model.CpModel()
    x = [[model.new_bool_var(f"x_{i}_{s}") for s in range(m)] for i in range(n)]
    positions = [model.new_int_var(0, m - 1, f"p_{i}") for i in range(n)]
    for i in range(n):
        model.add_exactly_one(x[i])
        model.add(positions[i] == sum(s * x[i][s] for s in range(m)))
    for s in range(m):
        model.add_at_most_one(x[i][s] for i in range(n))
    for i, s in request["fixed_seats"]:
        model.add(positions[i] == s)
    if forced_assignment is not None:
        require(len(forced_assignment) == n and all(0 <= seat < m for seat in forced_assignment),
                "invalid forced CP probe")
        for i, seat in enumerate(forced_assignment):
            model.add(positions[i] == seat)
    directed = sorted({pair for a, b in request["edges"] for pair in ((a, b), (b, a))})
    must, cannot = hard_pairs(request)
    for a, b in must:
        model.add_allowed_assignments([positions[a], positions[b]], directed)
    for a, b in cannot:
        model.add_forbidden_assignments([positions[a], positions[b]], directed)
    distances = graph_distances(request)
    for rule in request["min_distance"]:
        a, b = rule["students"]
        allowed = [(s, t) for s in range(m) for t in range(m)
                   if distance_satisfied(request, distances, rule, s, t)]
        model.add_allowed_assignments([positions[a], positions[b]], allowed)
    expression = sum(compiled.integer_unary[i][s] * x[i][s] for i in range(n) for s in range(m))
    if compiled.distribution_buckets is not None:
        bound = sum(compiled.rank_units.values())
        gap = model.new_int_var(-bound, bound, "bucket_score_gap")
        absolute = model.new_int_var(0, bound, "absolute_bucket_score_gap")
        model.add(gap == sum(compiled.rank_units[i] * (1 if compiled.distribution_buckets[s] == 0 else -1)
                             * x[i][s] for i in range(n) for s in range(m)))
        model.add_abs_equality(absolute, gap)
        expression += compiled.distribution_coefficient * absolute
    model.minimize(expression)
    error = model.validate()
    require(not error, f"CP-SAT rejected model: {error}")
    model_seconds = time.perf_counter() - started
    if model_seconds >= budget:
        return {"status": "MODEL_BUDGET_EXHAUSTED", "assignment": [], "model_seconds": model_seconds,
                "solve_seconds": 0, "operation_seconds": model_seconds}
    solver = cp_model.CpSolver()
    solver.parameters.num_search_workers = 1
    solver.parameters.random_seed = request["seed"]
    solver.parameters.max_time_in_seconds = budget - model_seconds
    solve_started = time.perf_counter()
    status = solver.solve(model)
    solve_seconds = time.perf_counter() - solve_started
    assignment = tuple(solver.value(p) for p in positions) if status in (cp_model.OPTIMAL, cp_model.FEASIBLE) else ()
    exact = objective(compiled, assignment) if assignment else None
    # ObjectiveValue is a double; recover an exact integer from chosen variables.
    integer_cost = int(exact * compiled.scale) if exact is not None else None
    return {"status": solver.status_name(status), "assignment": list(assignment),
            "scale": compiled.scale, "integer_objective": integer_cost,
            "objective_fraction": str(exact) if exact is not None else None,
            "objective": float(exact) if exact is not None else None,
            "solver_objective": solver.objective_value if assignment else None,
            "best_bound": solver.best_objective_bound,
            "model_seconds": model_seconds, "solve_seconds": solve_seconds,
            "operation_seconds": time.perf_counter() - started,
            "absolute_integer_objective_bound": compiled.absolute_objective_bound,
            "optimal_scope": "only the five implemented goals within documented domain; not all ten SeatTrellis goals"}


def base_case(n: int = 4, columns: int = 2) -> dict:
    seats = [{"seat_id": f"S{i}", "row": i // columns, "col": i % columns, "enabled": True}
             for i in range(n)]
    return {"api_version": 2, "student_count": n, "seed": 42, "time_limit_seconds": 2.0,
            "students": [{"key": f"P{i}", "score": float(i * 10)} for i in range(n)],
            "seat_positions": [[s["col"], s["row"]] for s in seats],
            "layout": {"layout_id": "soft-prototype", "seats": seats},
            "edges": [[i, j] for i in range(n) for j in range(i + 1, n)
                      if abs(seats[i]["row"] - seats[j]["row"]) + abs(seats[i]["col"] - seats[j]["col"]) == 1],
            "fixed_seats": [], "must_be_adjacent": [], "cannot_be_adjacent": [], "min_distance": [],
            "rules": {"seed": 42, "groups": [], "soft": {name: {"enabled": False, "weight": 1}
                                                          for name in SOFT_NAMES}}}


def cases() -> dict[str, dict]:
    result = {}
    request = base_case()
    request["rules"]["soft"]["vision_front"].update(enabled=True, weight=3)
    request["rules"]["soft"]["height_back"].update(enabled=True, weight=2)
    for student, height in zip(request["students"], [150.5, 151.5, 178.5, 175.0]):
        student["height_cm"] = height
    request["students"][0]["vision"] = "0.8"
    request["students"][1]["needs"] = ["近视"]
    request["students"][2].update(vision="1.2", tags=["poor"])  # Numeric value takes priority.
    result["vision-height-half-even"] = request
    request = base_case(6, 3)
    request["rules"]["soft"]["vision_front"].update(enabled=True, weight=2)
    for student, vision, tags in zip(request["students"],
                                     ["0_8", "1_0", " 0.8 ", "０.８", "+8e-1", "+NaN"],
                                     [["poor"], ["poor"], [], [], ["poor"], ["poor"]]):
        student.update(vision=vision, tags=tags)
    result["vision-rust-numeric-grammar"] = request
    request = base_case()
    request["rules"]["soft"]["vision_front"].update(enabled=True, weight=2)
    for student, vision in zip(request["students"], ["-inf", "Infinity", "NaN", ".8"]):
        student.update(vision=vision, tags=["poor"])
    result["vision-rust-special-values"] = request
    for direction in ("high_front", "high_back"):
        request = base_case(6, 2)
        request["rules"]["soft"]["score_position"].update(enabled=True, weight=3, direction=direction)
        result[f"position-{direction}"] = request
    request = base_case()
    request["students"][0]["score"] = None
    request["students"][1]["score"] = 20.0
    request["students"][2]["score"] = 20.0
    request["rules"]["soft"]["score_position"].update(enabled=True, weight=7)
    result["position-ties-missing-score"] = request
    request = base_case()
    request["students"][0]["score"] = None
    del request["students"][1]["score"]
    request["rules"]["soft"]["score_position"].update(enabled=True, weight=2)
    result["position-null-and-omitted-scores"] = request
    request = base_case(6, 2)
    request["students"] = request["students"][:4]
    request["student_count"] = 4
    request["rules"]["soft"]["score_position"].update(enabled=True, weight=2)
    result["position-unused-seats"] = request
    request = base_case()
    for student in request["students"]:
        student["score"] = 50.0
    request["rules"]["soft"]["score_position"].update(enabled=True, weight=2)
    result["position-unavailable-constant-scores"] = request
    request = base_case()
    for i, seat in enumerate(request["layout"]["seats"]):
        seat.update(row=0, col=i)
    request["seat_positions"] = [[i, 0] for i in range(4)]
    request["edges"] = [[0, 1], [1, 2], [2, 3]]
    request["rules"]["soft"]["score_position"].update(enabled=True, weight=2)
    result["position-single-row"] = request
    request = base_case()
    for student, score in zip(request["students"], [10.0, 10.0, 50.0, 90.0]):
        student["score"] = score
    request["rules"]["soft"]["score_distribution"].update(enabled=True, weight=4, scope="row")
    result["distribution-two-rows-ties"] = request
    request = base_case(6, 2)
    for i, seat in enumerate(request["layout"]["seats"]):
        seat["group_id"] = "a" if i % 2 == 0 else "b"
    request["rules"]["soft"]["score_distribution"].update(enabled=True, weight=5, scope="group")
    result["distribution-two-groups"] = request
    request = base_case(6, 3)
    request["fixed_seats"] = [[0, 0]]
    request["must_be_adjacent"] = [[1, 2]]
    request["cannot_be_adjacent"] = [[0, 5]]
    request["min_distance"] = [{"students": [0, 5], "distance": 2, "metric": "euclidean"},
                               {"students": [0, 5], "distance": 2, "metric": "graph"}]
    request["rules"]["groups"] = [{"name": "separate", "students": ["P0", "P5"], "separate": True},
                                    {"name": "together", "students": ["P1", "P2"], "together": True}]
    for name, weight in [("vision_front", 2), ("height_back", 1), ("score_position", 3),
                         ("score_distribution", 4), ("fair_rotation", 2)]:
        request["rules"]["soft"][name].update(enabled=True, weight=weight)
    request["rules"]["soft"]["fair_rotation"].update(lookback=2, avoid_repeating_categories=["front", "back", "side"])
    for i, student in enumerate(request["students"]):
        student.update(height_cm=150 + i * 5, vision="poor" if i < 2 else "good")
    request["history"] = {"history_count": 4, "students": {
        s["key"]: {"category_counts": {"front": i % 3, "back": (i + 1) % 3, "side": i % 2},
                   "records": [{"period_index": 1, "categories": ["front", "side"]},
                               {"period_index": 3, "categories": ["back"]}]}
        for i, s in enumerate(request["students"])}}
    result["mixed-hard-all-five-indexed-history"] = request
    for legacy in (False, True):
        request = base_case()
        request["rules"]["soft"]["fair_rotation"].update(enabled=True, weight=2, lookback=1,
                                                        avoid_repeating_categories=["front", "back", "corner"])
        request["layout"]["seats"][0].update(zone=" Near-Window ", near_door=True)
        request["history"] = {"history_count": 3, "students": {
            s["key"]: {"category_counts": {}, "records":
                       [{"categories": ["front"]}, {"categories": ["back"]}] if legacy else []}
            for s in request["students"]}}
        result["rotation-legacy-occurrence-window" if legacy else "rotation-negative-compensation"] = request
    request = base_case()
    request["rules"]["soft"]["fair_rotation"].update(
        enabled=True, weight=2, lookback=None,
        avoid_repeating_categories=["front", "back", "near_window", "near_door"])
    request["layout"]["seats"][0].update(zone=" Near-Window ", near_door=True)
    request["layout"]["seats"][1]["zone"] = "Back"  # Explicit category overrides inferred front.
    request["history"] = {"history_count": 3, "students": {
        s["key"]: {"category_counts": {"front": i % 2, "back": (i + 1) % 2,
                                       "near_window": i, "near_door": 3 - i},
                   "records": [{"period_index": 1, "categories": ["near_window", "front"]},
                               {"period_index": 3, "categories": ["near_door", "back"]}]}
        for i, s in enumerate(request["students"])}}
    result["rotation-active-zone-normalization-all-history"] = request
    request = base_case()
    request["rules"]["soft"]["score_position"].update(enabled=True)
    request["rules"]["groups"] = [{"name": "triangle", "students": ["P0", "P1", "P2"], "together": True}]
    result["mixed-infeasible-triangle"] = request
    return result


def rejection_controls() -> list[dict]:
    checks = []
    for name in sorted(set(SOFT_NAMES) - SUPPORTED):
        request = base_case()
        request["rules"]["soft"][name]["enabled"] = True
        checks.append((f"unsupported-{name}", request, {}))
    request = base_case(6, 2)
    request["rules"]["soft"]["score_distribution"]["enabled"] = True
    checks.append(("three-bucket-rms-unsupported", request, {}))
    request = base_case()
    request["students"][0]["score"] = None
    request["rules"]["soft"]["score_distribution"]["enabled"] = True
    checks.append(("missing-distribution-score", request, {}))
    request = base_case()
    request["layout"]["seats"][0]["row"] = 1
    request["rules"]["soft"]["score_distribution"]["enabled"] = True
    checks.append(("unequal-distribution-capacities", request, {}))
    request = base_case()
    request["rules"]["soft"]["score_distribution"].update(enabled=True, scope="group")
    checks.append(("missing-group-identifier", request, {}))
    request = base_case()
    request["students"] = request["students"][:3]
    request["student_count"] = 3
    request["rules"]["soft"]["score_distribution"]["enabled"] = True
    checks.append(("distribution-unused-seats", request, {}))
    request = base_case()
    request["rules"]["soft"]["height_back"].update(enabled=True, weight=1_000_000)
    for student in request["students"]:
        student["height_cm"] = 300
    request["layout"]["seats"][0]["row"] = -(2**31)
    request["layout"]["seats"][1]["row"] = 2**31 - 1
    checks.append(("integer-objective-overflow", request, {}))
    request = base_case()
    request["rules"]["soft"]["height_back"]["enabled"] = True
    request["students"][0]["height_cm"] = 1e300
    checks.append(("saturating-height-unsupported", request, {}))
    request = base_case()
    request["rules"]["soft"]["vision_front"].update(enabled=True, weight=-1)
    checks.append(("negative-weight", request, {}))
    request = base_case()
    request["rules"]["soft"]["vision_front"].update(enabled=True, weight=1_000_001)
    checks.append(("enabled-weight-outside-production-range", request, {}))
    checks.append(("negative-scaling", base_case(), {"scale_multiplier": -1}))
    checks.append(("zero-scaling", base_case(), {"scale_multiplier": 0}))
    checks.append(("scaling-overflow", base_case(), {"scale_multiplier": 2**64}))
    request = base_case()
    del request["rules"]["soft"]["randomize"]
    checks.append(("implicit-default-randomize-rejected", request, {}))
    for field, value, label in [
        ("score", "0_8", "string-score"), ("score", True, "boolean-score"),
        ("score", float("nan"), "nan-score"), ("score", float("inf"), "infinite-score"),
        ("score", 1e10, "score-outside-production-range"),
        ("height_cm", "150.5", "string-height"), ("height_cm", float("nan"), "nan-height"),
        ("height_cm", 301, "height-outside-production-range"),
        ("vision", 0.8, "numeric-vision-wrong-dto-type"),
    ]:
        request = base_case()
        request["students"][0][field] = value
        checks.append((label, request, {}))
    request = base_case()
    request["seat_positions"] = [[0, 0], [56.13681341631508, 26.274160852293527], [1000, 0], [0, 1000]]
    request["min_distance"] = [{"students": [0, 1], "metric": "euclidean", "distance": 61.98123384565982}]
    checks.append(("euclidean-one-ulp-boundary-rejected", request, {}))
    request = base_case()
    request["seat_positions"][1] = [1.5, 0]
    request["min_distance"] = [{"students": [0, 1], "metric": "euclidean", "distance": 1}]
    checks.append(("fractional-euclidean-coordinate-rejected", request, {}))
    reports = []
    for name, request, options in checks:
        try:
            compile_objective(request, **options)
        except ValueError as error:
            reports.append({"name": name, "rejected": True, "reason": str(error)})
        else:
            raise RuntimeError(f"rejection control unexpectedly accepted: {name}")
    return reports


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "results.json")
    parser.add_argument("--write-cases", action="store_true")
    args = parser.parse_args()
    require(ortools.__version__ == "9.15.6755" and google.protobuf.__version__ == "6.33.5",
            "use pinned OR-Tools 9.15.6755 and protobuf 6.33.5")
    controls = rejection_controls()
    exhausted_request = base_case()
    exhausted_request["time_limit_seconds"] = 1e-9
    exhausted = cp_run(exhausted_request)
    require(exhausted["status"] == "MODEL_BUDGET_EXHAUSTED" and not exhausted["assignment"]
            and exhausted["solve_seconds"] == 0, "model construction must consume the shared budget")
    results = []
    documents = cases()
    for name, request in documents.items():
        if args.write_cases:
            directory = ROOT / "cases"
            directory.mkdir(exist_ok=True)
            (directory / f"{name}.json").write_text(json.dumps(request, ensure_ascii=False, indent=2) + "\n")
        assignments = list(permutations(range(len(request["seat_positions"])), request["student_count"]))
        process = subprocess.run([str(args.oracle.resolve())],
                                 input=json.dumps({"request": request, "assignments": assignments}) + "\n",
                                 text=True, capture_output=True, check=True, timeout=30)
        oracle = json.loads(process.stdout)
        compiled = compile_objective(request)
        candidates = []
        max_difference = 0.0
        for assignment, score in zip(assignments, oracle["scores"], strict=True):
            exact = objective(compiled, assignment)
            difference = abs(float(exact) - score["total_cost"])
            max_difference = max(max_difference, difference)
            require(difference <= 1e-9, f"Rust objective mismatch in {name}: {assignment}: {exact} vs {score}")
            valid = legal(request, assignment)
            require(valid == score["legal"], f"Rust hard-rule mismatch: {name}/{assignment}")
            if valid:
                candidates.append((exact, assignment))
        cp = cp_run(request)
        response = oracle["rust_response"]
        fixed_cp_probes = []
        if candidates:
            optimum = min(cost for cost, _ in candidates)
            require(cp["status"] == "OPTIMAL", f"small prototype did not prove optimum: {name}/{cp}")
            chosen = tuple(cp["assignment"])
            require(legal(request, chosen) and objective(compiled, chosen) == optimum,
                    f"CP-SAT disagrees with exhaustive optimum: {name}")
            chosen_index = assignments.index(chosen)
            require(oracle["scores"][chosen_index]["legal"], "CP assignment rejected by Rust evaluator")
            require(response["feasible"], f"Rust did not return a feasible small assignment: {name}")
            rust_assignment = tuple(dict(response["assignment"])[i] for i in range(request["student_count"]))
            rust_exact = objective(compiled, rust_assignment)
            require(legal(request, rust_assignment), "Rust response failed independent hard validation")
            require(abs(response["total_cost"] - float(rust_exact)) <= 1e-9,
                    "production Rust cost disagrees with independent objective")
            require(abs(oracle["fixed_probe"]["total_cost"] - float(rust_exact)) <= 1e-9,
                    "Rust fixed-probe total disagrees with scoring oracle")
            rust_gap = float(rust_exact - optimum)
            require(rust_gap >= -1e-9, "Rust beat exhaustive optimum; validation bug")
            # Probe the actual CP model at nonoptimal assignments as well. Its
            # ObjectiveValue must equal the independently scored integer cost;
            # merely recomputing a chosen assignment would miss model mistakes.
            ordered = sorted(candidates)
            for index in sorted({0, len(ordered) // 2, len(ordered) - 1}):
                exact_cost, assignment = ordered[index]
                fixed = cp_run(request, forced_assignment=assignment)
                expected_integer = int(exact_cost * compiled.scale)
                require(fixed["status"] == "OPTIMAL" and tuple(fixed["assignment"]) == assignment,
                        f"fixed CP assignment did not solve: {name}/{assignment}")
                require(fixed["solver_objective"] == float(expected_integer),
                        f"CP model objective disagrees with independent score: {name}/{assignment}")
                fixed_cp_probes.append({"assignment": list(assignment), "exact_cost": str(exact_cost),
                                        "expected_integer_cost": expected_integer,
                                        "solver_objective": fixed["solver_objective"]})
        else:
            optimum = rust_gap = None
            require(cp["status"] == "INFEASIBLE", "CP incorrectly returned feasibility")
            require(response["status"] == "ProvenInfeasible", "Rust did not prove exhaustive small infeasibility")
        results.append({"case": name,
                        "document_sha256": hashlib.sha256(json.dumps(request, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
                        "permutations_checked": len(assignments), "legal_assignments": len(candidates),
                        "maximum_rust_float_difference": max_difference,
                        "exhaustive_optimum": str(optimum) if optimum is not None else None,
                        "cp_sat": cp, "rust_response": response,
                        "rust_solve_seconds": oracle["rust_solve_seconds"],
                        "rust_cost_above_exhaustive_optimum": rust_gap,
                        "fixed_cp_objective_probes": fixed_cp_probes})
    provenance = {"python": sys.version, "platform": platform.platform(), "ortools": ortools.__version__,
                  "protobuf": google.protobuf.__version__, "seed": 42, "cp_workers": 1,
                  "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                  "rust_source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
                  "oracle_binary_sha256": sha256(args.oracle),
                  "source_sha256": {str(path.relative_to(REPO)): sha256(path) for path in
                                    [Path(__file__), ROOT / "requirements.lock.txt", ROOT / "rust-oracle/src/main.rs",
                                     ROOT / "rust-oracle/Cargo.toml", ROOT / "rust-oracle/Cargo.lock",
                                     REPO / "crates/seattrellis-core/src/cost.rs",
                                     REPO / "crates/seattrellis-core/src/objectives.rs",
                                     REPO / "crates/seattrellis-core/src/models.rs",
                                     REPO / "crates/seattrellis-core/src/engine.rs"]}}
    report = {"purpose": "small-case objective/hard-rule equivalence, not a throughput benchmark",
              "provenance": provenance, "supported_goals": sorted(SUPPORTED),
              "exact_integer_scaling_error": 0,
              "rust_binary64_comparison_tolerance": 1e-9,
              "rejection_controls": controls, "preparation_budget_control": exhausted, "cases": results,
              "total_fixed_cp_objective_probes": sum(len(c["fixed_cp_objective_probes"]) for c in results),
              "total_permutations_checked": sum(c["permutations_checked"] for c in results)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"passed": True, "cases": len(results), "permutations": report["total_permutations_checked"],
                      "rejection_controls": len(controls), "report": str(args.output)}))


if __name__ == "__main__":
    main()
