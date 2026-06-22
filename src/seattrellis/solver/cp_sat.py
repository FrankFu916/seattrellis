from __future__ import annotations

import os
import random
from dataclasses import dataclass
from math import inf
from typing import Any

from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.history import SeatHistory
from seattrellis.models.rules import MinDistanceRule, PairRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment
from seattrellis.models.student import Student
from seattrellis.history import assignment_fairness_summary, fair_rotation_cost
from seattrellis.solver.adjacency import (
    SeatEdge,
    build_adjacency_edges,
    graph_distance,
    normalize_edge,
    seat_distance,
)
from seattrellis.io.validation import format_infeasible_diagnostic, validate_loaded_inputs
from seattrellis.solver.result import SeatingSolution
from seattrellis.optional import MissingOptionalDependencyError

cp_model = None
_cp_model_unavailable = False


class SeatTrellisSolveError(ValueError):
    """Raised when a seating problem cannot be solved or validated."""


@dataclass(frozen=True)
class _CompiledRules:
    fixed_seats: dict[int, int]
    must_be_adjacent: list[tuple[int, int]]
    cannot_be_adjacent: list[tuple[int, int]]
    min_distance: list[tuple[int, int, MinDistanceRule]]


def solve_seating(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet | None = None,
    *,
    history: SeatHistory | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
) -> SeatingSolution:
    """Solve a seating plan using CP-SAT, with a small deterministic fallback."""

    rules = rules or RuleSet()
    seed = rules.seed if seed is None else seed
    seats = sorted(layout.enabled_seats, key=lambda seat: (seat.row, seat.col, seat.seat_id))
    if not students:
        raise SeatTrellisSolveError("At least one student is required.")
    if len(students) > len(seats):
        raise SeatTrellisSolveError(
            f"Not enough enabled seats: {len(students)} students but only {len(seats)} enabled seats."
        )

    _validate_unique_students(students)
    validation_report = validate_loaded_inputs(students, layout, rules)
    if validation_report.errors:
        raise SeatTrellisSolveError(validation_report.format_failure(title="Input validation failed."))
    edges = build_adjacency_edges(layout)
    compiled = _compile_rules(students, seats, layout, rules, edges)

    cp_sat = _load_cp_model()
    if cp_sat is not None:
        return _solve_with_ortools(students, seats, layout, rules, compiled, edges, history, seed, time_limit_seconds)
    return _solve_with_fallback(students, seats, layout, rules, compiled, edges, history, seed)


def _load_cp_model():
    global cp_model, _cp_model_unavailable
    if cp_model is not None:
        return cp_model
    if _cp_model_unavailable:
        return None
    if os.environ.get("SEATTRELLIS_USE_ORTOOLS") not in {"1", "true", "TRUE", "yes", "YES"}:
        return None

    try:  # pragma: no cover - exercised when OR-Tools is installed and enabled.
        from ortools.sat.python import cp_model as loaded_cp_model
    except Exception as exc:  # pragma: no cover - local fallback path is tested.
        _cp_model_unavailable = True
        raise MissingOptionalDependencyError("OR-Tools solver", "solver") from exc
    cp_model = loaded_cp_model
    return cp_model


def _solve_with_ortools(
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    compiled: _CompiledRules,
    edges: set[SeatEdge],
    history: SeatHistory | None,
    seed: int,
    time_limit_seconds: float,
) -> SeatingSolution:
    model = cp_model.CpModel()
    x: dict[tuple[int, int], Any] = {}
    for student_index in range(len(students)):
        for seat_index in range(len(seats)):
            x[(student_index, seat_index)] = model.NewBoolVar(f"x_{student_index}_{seat_index}")

    for student_index in range(len(students)):
        model.AddExactlyOne(x[(student_index, seat_index)] for seat_index in range(len(seats)))
    for seat_index in range(len(seats)):
        model.AddAtMostOne(x[(student_index, seat_index)] for student_index in range(len(students)))

    for student_index, seat_index in compiled.fixed_seats.items():
        model.Add(x[(student_index, seat_index)] == 1)

    _add_pair_constraints(model, x, seats, compiled, layout, edges)
    objective_terms = _build_individual_objective_terms(x, students, seats, layout, rules, history, seed)
    objective_terms.extend(_build_score_balance_terms(model, x, students, seats, rules, edges))
    if objective_terms:
        model.Minimize(sum(coef * var for var, coef in objective_terms))

    solver = cp_model.CpSolver()
    solver.parameters.max_time_in_seconds = time_limit_seconds
    solver.parameters.random_seed = seed
    solver.parameters.num_search_workers = 1
    status = solver.Solve(model)
    if status not in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        raise SeatTrellisSolveError(format_infeasible_diagnostic(students, layout, rules))

    assignment_by_student: dict[int, int] = {}
    for student_index in range(len(students)):
        for seat_index in range(len(seats)):
            if solver.Value(x[(student_index, seat_index)]):
                assignment_by_student[student_index] = seat_index
                break

    return _solution_from_assignment(
        students,
        seats,
        assignment_by_student,
        "OPTIMAL" if status == cp_model.OPTIMAL else "FEASIBLE",
        float(solver.ObjectiveValue()) if objective_terms else None,
        {"solver": "ortools-cp-sat"},
        layout,
        rules,
        history,
    )


def _add_pair_constraints(
    model: Any,
    x: dict[tuple[int, int], Any],
    seats: list[SeatNode],
    compiled: _CompiledRules,
    layout: ClassroomLayout,
    edges: set[SeatEdge],
) -> None:
    for first_index, second_index in compiled.must_be_adjacent:
        for first_seat_index, first_seat in enumerate(seats):
            for second_seat_index, second_seat in enumerate(seats):
                if first_seat_index == second_seat_index:
                    continue
                edge = normalize_edge(first_seat.seat_id, second_seat.seat_id)
                if edge not in edges:
                    model.AddBoolOr(
                        [x[(first_index, first_seat_index)].Not(), x[(second_index, second_seat_index)].Not()]
                    )

    for first_index, second_index in compiled.cannot_be_adjacent:
        for first_seat_index, first_seat in enumerate(seats):
            for second_seat_index, second_seat in enumerate(seats):
                if first_seat_index == second_seat_index:
                    continue
                edge = normalize_edge(first_seat.seat_id, second_seat.seat_id)
                if edge in edges:
                    model.AddBoolOr(
                        [x[(first_index, first_seat_index)].Not(), x[(second_index, second_seat_index)].Not()]
                    )

    for first_index, second_index, rule in compiled.min_distance:
        for first_seat_index, first_seat in enumerate(seats):
            for second_seat_index, second_seat in enumerate(seats):
                if first_seat_index == second_seat_index:
                    continue
                distance = _distance_for_rule(layout, first_seat, second_seat, rule)
                if distance < rule.distance:
                    model.AddBoolOr(
                        [x[(first_index, first_seat_index)].Not(), x[(second_index, second_seat_index)].Not()]
                    )


def _build_individual_objective_terms(
    x: dict[tuple[int, int], Any],
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
    seed: int,
) -> list[tuple[Any, int]]:
    terms: list[tuple[Any, int]] = []
    rng = random.Random(seed)
    min_row = min(seat.row for seat in seats)
    max_row = max(seat.row for seat in seats)
    for student_index, student in enumerate(students):
        for seat_index, seat in enumerate(seats):
            coef = _individual_cost(student, seat, layout, rules, history, rng, min_row, max_row)
            if coef:
                terms.append((x[(student_index, seat_index)], coef))
    return terms


def _build_score_balance_terms(
    model: Any,
    x: dict[tuple[int, int], Any],
    students: list[Student],
    seats: list[SeatNode],
    rules: RuleSet,
    edges: set[SeatEdge],
) -> list[tuple[Any, int]]:
    soft = rules.soft.score_balance
    if not soft.enabled or soft.weight == 0:
        return []

    seat_index_by_id = {seat.seat_id: index for index, seat in enumerate(seats)}
    terms: list[tuple[Any, int]] = []
    for first_student_index, first_student in enumerate(students):
        if first_student.score is None:
            continue
        for second_student_index in range(first_student_index + 1, len(students)):
            second_student = students[second_student_index]
            if second_student.score is None:
                continue
            score_gap = int(round(abs(float(first_student.score) - float(second_student.score))))
            if score_gap == 0:
                continue
            for first_seat_id, second_seat_id in edges:
                for a_id, b_id in ((first_seat_id, second_seat_id), (second_seat_id, first_seat_id)):
                    a_index = seat_index_by_id[a_id]
                    b_index = seat_index_by_id[b_id]
                    pair_var = model.NewBoolVar(
                        f"score_pair_{first_student_index}_{second_student_index}_{a_index}_{b_index}"
                    )
                    model.Add(pair_var <= x[(first_student_index, a_index)])
                    model.Add(pair_var <= x[(second_student_index, b_index)])
                    model.Add(pair_var >= x[(first_student_index, a_index)] + x[(second_student_index, b_index)] - 1)
                    terms.append((pair_var, -soft.weight * score_gap))
    return terms


def _solve_with_fallback(
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    compiled: _CompiledRules,
    edges: set[SeatEdge],
    history: SeatHistory | None,
    seed: int,
) -> SeatingSolution:
    rng = random.Random(seed)
    attempts = max(40, len(students) * 12)
    best_assignment: dict[int, int] | None = None
    best_cost: float = inf

    for attempt in range(attempts):
        assignment: dict[int, int] = {}
        used_seats: set[int] = set()
        success = True

        while len(assignment) < len(students):
            choice = _choose_next_student(students, seats, layout, compiled, edges, assignment, used_seats)
            if choice is None:
                success = False
                break
            student_index, candidates = choice
            if attempt == 0:
                candidates = sorted(
                    candidates,
                    key=lambda idx: _fallback_individual_cost(students[student_index], seats[idx], layout, rules, history),
                )
                seat_index = candidates[0]
            else:
                candidates = sorted(
                    candidates,
                    key=lambda idx: _fallback_individual_cost(students[student_index], seats[idx], layout, rules, history)
                    + rng.random() * 25,
                )
                seat_index = rng.choice(candidates[: min(3, len(candidates))])
            assignment[student_index] = seat_index
            used_seats.add(seat_index)

        if not success or not _full_assignment_valid(assignment, seats, layout, compiled, edges):
            continue
        cost = _fallback_total_cost(assignment, students, seats, layout, rules, edges, history)
        if cost < best_cost:
            best_assignment = dict(assignment)
            best_cost = cost

    if best_assignment is None:
        raise SeatTrellisSolveError(format_infeasible_diagnostic(students, layout, rules))

    return _solution_from_assignment(
        students,
        seats,
        best_assignment,
        "FEASIBLE",
        best_cost,
        {"solver": "fallback-heuristic", "attempts": attempts},
        layout,
        rules,
        history,
    )


def _choose_next_student(
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    compiled: _CompiledRules,
    edges: set[SeatEdge],
    assignment: dict[int, int],
    used_seats: set[int],
) -> tuple[int, list[int]] | None:
    best: tuple[int, list[int]] | None = None
    for student_index in range(len(students)):
        if student_index in assignment:
            continue
        candidates = [
            seat_index
            for seat_index in range(len(seats))
            if seat_index not in used_seats
            and _partial_assignment_valid(
                {**assignment, student_index: seat_index}, seats, layout, compiled, edges
            )
        ]
        if not candidates:
            return None
        if best is None or len(candidates) < len(best[1]):
            best = (student_index, candidates)
    return best


def _partial_assignment_valid(
    assignment: dict[int, int],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    compiled: _CompiledRules,
    edges: set[SeatEdge],
) -> bool:
    if len(set(assignment.values())) < len(assignment):
        return False
    for student_index, fixed_seat_index in compiled.fixed_seats.items():
        if student_index in assignment and assignment[student_index] != fixed_seat_index:
            return False
    for first_index, second_index in compiled.must_be_adjacent:
        if first_index in assignment and second_index in assignment:
            if not _seat_indexes_adjacent(seats, assignment[first_index], assignment[second_index], edges):
                return False
    for first_index, second_index in compiled.cannot_be_adjacent:
        if first_index in assignment and second_index in assignment:
            if _seat_indexes_adjacent(seats, assignment[first_index], assignment[second_index], edges):
                return False
    for first_index, second_index, rule in compiled.min_distance:
        if first_index in assignment and second_index in assignment:
            first_seat = seats[assignment[first_index]]
            second_seat = seats[assignment[second_index]]
            if _distance_for_rule(layout, first_seat, second_seat, rule) < rule.distance:
                return False
    return True


def _full_assignment_valid(
    assignment: dict[int, int],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    compiled: _CompiledRules,
    edges: set[SeatEdge],
) -> bool:
    return _partial_assignment_valid(assignment, seats, layout, compiled, edges)


def _compile_rules(
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    edges: set[SeatEdge],
) -> _CompiledRules:
    student_refs = _student_reference_map(students)
    seat_index_by_id = {seat.seat_id: index for index, seat in enumerate(seats)}
    fixed: dict[int, int] = {}
    fixed_seats_seen: dict[int, int] = {}

    for rule in rules.hard.fixed_seats:
        student_index = _resolve_student(rule.student, student_refs)
        if rule.seat_id not in seat_index_by_id:
            raise SeatTrellisSolveError(f"Fixed seat {rule.seat_id!r} is unknown or disabled.")
        seat_index = seat_index_by_id[rule.seat_id]
        if student_index in fixed:
            raise SeatTrellisSolveError(f"Student {rule.student!r} is fixed to more than one seat.")
        if seat_index in fixed_seats_seen:
            raise SeatTrellisSolveError(f"Seat {rule.seat_id!r} is fixed to more than one student.")
        fixed[student_index] = seat_index
        fixed_seats_seen[seat_index] = student_index

    compiled = _CompiledRules(
        fixed_seats=fixed,
        must_be_adjacent=[_compile_pair(rule, student_refs) for rule in rules.hard.must_be_adjacent],
        cannot_be_adjacent=[_compile_pair(rule, student_refs) for rule in rules.hard.cannot_be_adjacent],
        min_distance=[
            (*_compile_pair(rule, student_refs), rule)
            for rule in rules.hard.min_distance
        ],
    )
    _validate_compiled_rule_conflicts(compiled, seats, layout, edges)
    return compiled


def _validate_compiled_rule_conflicts(
    compiled: _CompiledRules,
    seats: list[SeatNode],
    layout: ClassroomLayout,
    edges: set[SeatEdge],
) -> None:
    must_pairs = {_pair_key(first, second) for first, second in compiled.must_be_adjacent}
    cannot_pairs = {_pair_key(first, second) for first, second in compiled.cannot_be_adjacent}
    conflicts = must_pairs & cannot_pairs
    if conflicts:
        raise SeatTrellisSolveError(
            "Conflicting hard rules: the same student pair appears in both must_be_adjacent and cannot_be_adjacent."
        )

    fixed_by_student = compiled.fixed_seats
    for first_index, second_index in compiled.must_be_adjacent:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            first_seat = seats[fixed_by_student[first_index]]
            second_seat = seats[fixed_by_student[second_index]]
            if normalize_edge(first_seat.seat_id, second_seat.seat_id) not in edges:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats do not satisfy a must_be_adjacent rule."
                )
    for first_index, second_index in compiled.cannot_be_adjacent:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            first_seat = seats[fixed_by_student[first_index]]
            second_seat = seats[fixed_by_student[second_index]]
            if normalize_edge(first_seat.seat_id, second_seat.seat_id) in edges:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats violate a cannot_be_adjacent rule."
                )
    for first_index, second_index, rule in compiled.min_distance:
        if first_index in fixed_by_student and second_index in fixed_by_student:
            first_seat = seats[fixed_by_student[first_index]]
            second_seat = seats[fixed_by_student[second_index]]
            if _distance_for_rule(layout, first_seat, second_seat, rule) < rule.distance:
                raise SeatTrellisSolveError(
                    "Conflicting hard rules: fixed seats violate a min_distance rule."
                )


def _pair_key(first_index: int, second_index: int) -> tuple[int, int]:
    return (first_index, second_index) if first_index < second_index else (second_index, first_index)


def _compile_pair(rule: PairRule, student_refs: dict[str, int]) -> tuple[int, int]:
    first_index = _resolve_student(rule.students[0], student_refs)
    second_index = _resolve_student(rule.students[1], student_refs)
    if first_index == second_index:
        raise SeatTrellisSolveError("A pair rule must reference two different students.")
    return first_index, second_index


def _student_reference_map(students: list[Student]) -> dict[str, int]:
    refs: dict[str, int] = {}
    for index, student in enumerate(students):
        for value in (student.student_id, student.name):
            if not value:
                continue
            if value in refs and refs[value] != index:
                raise SeatTrellisSolveError(f"Ambiguous student reference: {value!r}.")
            refs[value] = index
    return refs


def _resolve_student(ref: str, refs: dict[str, int]) -> int:
    if ref not in refs:
        raise SeatTrellisSolveError(f"Unknown student reference: {ref!r}.")
    return refs[ref]


def _validate_unique_students(students: list[Student]) -> None:
    keys = [student.key for student in students]
    duplicates = sorted({key for key in keys if keys.count(key) > 1})
    if duplicates:
        raise SeatTrellisSolveError(f"Duplicate student identifiers: {', '.join(duplicates)}")


def _seat_indexes_adjacent(seats: list[SeatNode], first_index: int, second_index: int, edges: set[SeatEdge]) -> bool:
    if first_index == second_index:
        return False
    return normalize_edge(seats[first_index].seat_id, seats[second_index].seat_id) in edges


def _distance_for_rule(
    layout: ClassroomLayout,
    first_seat: SeatNode,
    second_seat: SeatNode,
    rule: MinDistanceRule,
) -> float:
    if rule.metric == "graph":
        return graph_distance(layout, first_seat.seat_id, second_seat.seat_id)
    return seat_distance(first_seat, second_seat)


def _individual_cost(
    student: Student,
    seat: SeatNode,
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
    rng: random.Random,
    min_row: int,
    max_row: int,
) -> int:
    cost = 0
    if rules.soft.vision_front.enabled and _needs_front(student):
        cost += rules.soft.vision_front.weight * (seat.row - min_row) * 100
    if rules.soft.height_back.enabled and student.height_cm is not None:
        front_penalty = max_row - seat.row
        cost += rules.soft.height_back.weight * int(round(float(student.height_cm))) * front_penalty
    if rules.soft.randomize.enabled:
        cost += rules.soft.randomize.weight * rng.randint(0, 100)
    cost += fair_rotation_cost(student, seat, layout, rules.soft.fair_rotation, history)
    return cost


def _fallback_individual_cost(
    student: Student,
    seat: SeatNode,
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
) -> float:
    fake_rng = random.Random(0)
    return float(_individual_cost(student, seat, layout, rules, history, fake_rng, seat.row, seat.row + 4))


def _fallback_total_cost(
    assignment: dict[int, int],
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    edges: set[SeatEdge],
    history: SeatHistory | None,
) -> float:
    min_row = min(seat.row for seat in seats)
    max_row = max(seat.row for seat in seats)
    rng = random.Random(rules.seed)
    cost = 0.0
    for student_index, seat_index in assignment.items():
        cost += _individual_cost(students[student_index], seats[seat_index], layout, rules, history, rng, min_row, max_row)

    if rules.soft.score_balance.enabled and rules.soft.score_balance.weight:
        for first_index, first_seat_index in assignment.items():
            first_score = students[first_index].score
            if first_score is None:
                continue
            for second_index, second_seat_index in assignment.items():
                if second_index <= first_index:
                    continue
                second_score = students[second_index].score
                if second_score is None:
                    continue
                if _seat_indexes_adjacent(seats, first_seat_index, second_seat_index, edges):
                    cost -= rules.soft.score_balance.weight * abs(float(first_score) - float(second_score))
    return cost


def _needs_front(student: Student) -> bool:
    values = [item.lower() for item in student.tags + student.needs]
    if student.vision is not None:
        values.append(str(student.vision).lower())
        try:
            return float(student.vision) < 1.0
        except (TypeError, ValueError):
            pass
    keywords = {
        "vision",
        "vision_front",
        "front",
        "poor",
        "low",
        "nearsighted",
        "short_sighted",
        "myopia",
        "视力",
        "近视",
        "靠前",
    }
    return any(value in keywords for value in values)


def _solution_from_assignment(
    students: list[Student],
    seats: list[SeatNode],
    assignment_by_student: dict[int, int],
    status: str,
    objective_value: float | None,
    metrics: dict[str, Any],
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
) -> SeatingSolution:
    assignments = [
        SeatAssignment(
            student_key=students[student_index].key,
            student_name=students[student_index].display_name,
            seat_id=seats[assignment_by_student[student_index]].seat_id,
        )
        for student_index in range(len(students))
    ]
    fairness = assignment_fairness_summary(assignments, students, layout, rules, history)
    if fairness:
        metrics = {**metrics, "fairness": fairness}
    return SeatingSolution(
        assignments=assignments,
        solver_status=status,
        objective_value=objective_value,
        metrics=metrics,
    )
