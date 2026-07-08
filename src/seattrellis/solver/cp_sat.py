from __future__ import annotations

import random
import time
from math import inf, isfinite
from typing import Any, Mapping, Sequence

from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment
from seattrellis.models.student import Student, student_needs_front
from seattrellis.history import assignment_fairness_summary, avoid_recent_neighbors_cost, fair_rotation_cost
from seattrellis.solver.adjacency import (
    SeatEdge,
    normalize_edge,
)
from seattrellis.io.validation import format_infeasible_diagnostic
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.problem import (
    CompiledProblem,
    CompiledRules,
    assignment_is_excluded,
    compile_problem,
    distance_for_rule,
    seat_indexes_adjacent,
)
from seattrellis.solver.result import SeatingSolution
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.solver.backend import normalize_solver_backend, resolve_solver_backend
from seattrellis.solver.native import require_native_core

cp_model = None
_cp_model_unavailable = False


def solve_seating(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet | None = None,
    *,
    history: SeatHistory | None = None,
    pair_history: PairHistory | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    excluded_assignments: Sequence[Mapping[str, str]] | None = None,
    backend: str = "auto",
) -> SeatingSolution:
    """Solve a seating plan using CP-SAT, with a small deterministic fallback."""

    if not isfinite(time_limit_seconds) or time_limit_seconds < 0.1:
        raise ValueError("time_limit_seconds must be a finite number >= 0.1")
    rules = rules or RuleSet()
    seed = rules.seed if seed is None else seed
    problem = compile_problem(
        students,
        layout,
        rules,
        excluded_assignments=excluded_assignments or [],
    )

    requested_backend = normalize_solver_backend(backend)
    effective_backend = resolve_solver_backend(requested_backend)
    if effective_backend == "ortools":
        _load_cp_model()
        return _solve_with_ortools(
            problem,
            history,
            pair_history,
            seed,
            time_limit_seconds,
            requested_backend,
        )
    if effective_backend == "native":
        return _solve_with_native(
            problem,
            history,
            pair_history,
            seed,
            time_limit_seconds,
            requested_backend,
        )
    return _solve_with_fallback(
        problem,
        history,
        pair_history,
        seed,
        time_limit_seconds,
        requested_backend,
    )


def _solve_with_native(
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    native_core = require_native_core()
    solution = _solve_with_fallback(
        problem,
        history,
        pair_history,
        seed,
        time_limit_seconds,
        requested_backend,
    )
    assignment_pairs = [
        (
            problem.student_index_by_key[assignment.student_key],
            problem.seat_index_by_id[assignment.seat_id],
        )
        for assignment in solution.assignments
    ]
    if not native_core.assignment_is_unique(len(problem.students), len(problem.seats), assignment_pairs):
        raise SeatTrellisSolveError("Native hard-constraint verification failed: assignment is not unique.")
    solution.metrics.update(
        {
            "solver": "native-spike+fallback-heuristic",
            "solver_backend_effective": "native",
            "native_core": {
                "module": "seattrellis_native",
                "version": getattr(native_core, "__version__", None),
                "validated_unique_assignment": True,
            },
        }
    )
    return solution


def _load_cp_model():
    global cp_model, _cp_model_unavailable
    if cp_model is not None:
        return cp_model
    if _cp_model_unavailable:
        return None
    try:  # pragma: no cover - exercised when OR-Tools is installed and enabled.
        from ortools.sat.python import cp_model as loaded_cp_model
    except Exception as exc:  # pragma: no cover - local fallback path is tested.
        _cp_model_unavailable = True
        raise MissingOptionalDependencyError("OR-Tools solver", "solver") from exc
    cp_model = loaded_cp_model
    return cp_model


def _solve_with_ortools(
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    students = problem.students
    seats = problem.seats
    layout = problem.layout
    rules = problem.rules
    compiled = problem.rules_compiled
    edges = problem.edges
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
    for excluded in problem.excluded_assignments:
        model.Add(sum(x[(student_index, seat_index)] for student_index, seat_index in excluded.items()) <= len(students) - 1)
    objective_terms = _build_individual_objective_terms(x, students, seats, layout, rules, history, seed)
    objective_terms.extend(_build_pair_objective_terms(model, x, students, seats, layout, rules, edges, pair_history))
    objective_terms.extend(_build_score_balance_terms(model, x, students, seats, rules, edges))
    if objective_terms:
        model.Minimize(sum(coef * var for var, coef in objective_terms))

    solver = cp_model.CpSolver()
    solver.parameters.max_time_in_seconds = time_limit_seconds
    solver.parameters.random_seed = seed
    solver.parameters.num_search_workers = 1
    status = solver.Solve(model)
    if status not in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        raise SeatTrellisSolveError(
            _format_ortools_failure(
                status=status,
                students=students,
                layout=layout,
                rules=rules,
                time_limit_seconds=time_limit_seconds,
            )
        )

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
        {
            "solver": "ortools-cp-sat",
            "solver_backend_requested": requested_backend,
            "solver_backend_effective": "ortools",
            "time_limit_seconds": time_limit_seconds,
        },
        layout,
        rules,
        history,
        pair_history,
    )


def _add_pair_constraints(
    model: Any,
    x: dict[tuple[int, int], Any],
    seats: list[SeatNode],
    compiled: CompiledRules,
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
                distance = distance_for_rule(layout, first_seat, second_seat, rule)
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


def _build_pair_objective_terms(
    model: Any,
    x: dict[tuple[int, int], Any],
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    edges: set[SeatEdge],
    pair_history: PairHistory | None,
) -> list[tuple[Any, int]]:
    rule = rules.soft.avoid_recent_neighbors
    if not rule.enabled or rule.weight == 0:
        return []

    terms: list[tuple[Any, int]] = []
    for first_student_index, first_student in enumerate(students):
        for second_student_index in range(first_student_index + 1, len(students)):
            second_student = students[second_student_index]
            for first_seat_index, first_seat in enumerate(seats):
                for second_seat_index, second_seat in enumerate(seats):
                    if first_seat_index == second_seat_index:
                        continue
                    cost = avoid_recent_neighbors_cost(
                        first_student.key,
                        second_student.key,
                        first_seat,
                        second_seat,
                        layout,
                        rule,
                        pair_history,
                        adjacency_edges=edges,
                    )
                    if not cost:
                        continue
                    pair_var = model.NewBoolVar(
                        "recent_neighbor_"
                        f"{first_student_index}_{second_student_index}_{first_seat_index}_{second_seat_index}"
                    )
                    model.Add(pair_var <= x[(first_student_index, first_seat_index)])
                    model.Add(pair_var <= x[(second_student_index, second_seat_index)])
                    model.Add(
                        pair_var
                        >= x[(first_student_index, first_seat_index)]
                        + x[(second_student_index, second_seat_index)]
                        - 1
                    )
                    terms.append((pair_var, cost))
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
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    students = problem.students
    seats = problem.seats
    layout = problem.layout
    rules = problem.rules
    compiled = problem.rules_compiled
    edges = problem.edges
    rng = random.Random(seed)
    attempts = max(40, len(students) * 12)
    deadline = time.monotonic() + time_limit_seconds
    best_assignment: dict[int, int] | None = None
    best_cost: float = inf
    completed_attempts = 0
    stopped_by_time_limit = False

    for attempt in range(attempts):
        if attempt > 0 and time.monotonic() >= deadline:
            stopped_by_time_limit = True
            break
        assignment: dict[int, int] = {}
        used_seats: set[int] = set()
        success = True

        while len(assignment) < len(students):
            if attempt > 0 and time.monotonic() >= deadline:
                stopped_by_time_limit = True
                success = False
                break
            choice = _choose_next_student(students, seats, layout, compiled, edges, assignment, used_seats)
            if choice is None:
                success = False
                break
            student_index, candidates = choice
            if attempt == 0:
                candidates = sorted(
                    candidates,
                    key=lambda idx: _fallback_candidate_cost(
                        student_index,
                        idx,
                        assignment,
                        students,
                        seats,
                        layout,
                        rules,
                        edges,
                        history,
                        pair_history,
                    ),
                )
                seat_index = candidates[0]
            else:
                candidates = sorted(
                    candidates,
                    key=lambda idx: _fallback_candidate_cost(
                        student_index,
                        idx,
                        assignment,
                        students,
                        seats,
                        layout,
                        rules,
                        edges,
                        history,
                        pair_history,
                    )
                    + rng.random() * 25,
                )
                seat_index = rng.choice(candidates[: min(3, len(candidates))])
            assignment[student_index] = seat_index
            used_seats.add(seat_index)

        if (
            not success
            or not _full_assignment_valid(assignment, seats, layout, compiled, edges)
            or assignment_is_excluded(assignment, problem.excluded_assignments)
        ):
            continue
        completed_attempts += 1
        cost = _fallback_total_cost(assignment, students, seats, layout, rules, edges, history, pair_history)
        if cost < best_cost:
            best_assignment = dict(assignment)
            best_cost = cost

    if best_assignment is None:
        if stopped_by_time_limit:
            raise SeatTrellisSolveError(
                "Fallback solver did not find a feasible seating plan within "
                f"{time_limit_seconds:g} seconds. This is not proof that the "
                "problem is infeasible; try increasing --time-limit, reducing "
                "candidate count, or relaxing hard constraints."
            )
        raise SeatTrellisSolveError(format_infeasible_diagnostic(students, layout, rules))

    return _solution_from_assignment(
        students,
        seats,
        best_assignment,
        "FEASIBLE",
        best_cost,
        {
            "solver": "fallback-heuristic",
            "solver_backend_requested": requested_backend,
            "solver_backend_effective": "fallback",
            "attempts": completed_attempts,
            "attempt_limit": attempts,
            "stopped_by_time_limit": stopped_by_time_limit,
            "time_limit_seconds": time_limit_seconds,
        },
        layout,
        rules,
        history,
        pair_history,
    )


def _format_ortools_failure(
    *,
    status: int,
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    time_limit_seconds: float,
) -> str:
    status_name = _ortools_status_name(status)
    if status_name == "INFEASIBLE":
        return format_infeasible_diagnostic(students, layout, rules)
    if status_name == "MODEL_INVALID":
        return (
            "OR-Tools rejected the internal CP-SAT model as invalid. "
            "Please report this with the input files that reproduce it."
        )
    return (
        "OR-Tools did not find a feasible seating plan within "
        f"{time_limit_seconds:g} seconds (status: {status_name}). "
        "This is not proof that the problem is infeasible. Try increasing "
        "--time-limit, using --backend fallback, reducing candidate count, "
        "or relaxing soft rules that make the model large."
    )


def _ortools_status_name(status: int) -> str:
    for name in ("OPTIMAL", "FEASIBLE", "INFEASIBLE", "MODEL_INVALID", "UNKNOWN"):
        if getattr(cp_model, name, None) == status:
            return name
    return f"status {status}"


def _choose_next_student(
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    compiled: CompiledRules,
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
    compiled: CompiledRules,
    edges: set[SeatEdge],
) -> bool:
    if len(set(assignment.values())) < len(assignment):
        return False
    for student_index, fixed_seat_index in compiled.fixed_seats.items():
        if student_index in assignment and assignment[student_index] != fixed_seat_index:
            return False
    for first_index, second_index in compiled.must_be_adjacent:
        if first_index in assignment and second_index in assignment:
            if not seat_indexes_adjacent(seats, assignment[first_index], assignment[second_index], edges):
                return False
    for first_index, second_index in compiled.cannot_be_adjacent:
        if first_index in assignment and second_index in assignment:
            if seat_indexes_adjacent(seats, assignment[first_index], assignment[second_index], edges):
                return False
    for first_index, second_index, rule in compiled.min_distance:
        if first_index in assignment and second_index in assignment:
            first_seat = seats[assignment[first_index]]
            second_seat = seats[assignment[second_index]]
            if distance_for_rule(layout, first_seat, second_seat, rule) < rule.distance:
                return False
    return True


def _full_assignment_valid(
    assignment: dict[int, int],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    compiled: CompiledRules,
    edges: set[SeatEdge],
) -> bool:
    return _partial_assignment_valid(assignment, seats, layout, compiled, edges)


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
    if rules.soft.vision_front.enabled and student_needs_front(student):
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
    enabled = layout.enabled_seats
    min_row = min(s.row for s in enabled) if enabled else seat.row
    max_row = max(s.row for s in enabled) if enabled else seat.row
    return float(_individual_cost(student, seat, layout, rules, history, fake_rng, min_row, max_row))


def _fallback_candidate_cost(
    student_index: int,
    seat_index: int,
    assignment: dict[int, int],
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    edges: set[SeatEdge],
    history: SeatHistory | None,
    pair_history: PairHistory | None,
) -> float:
    cost = _fallback_individual_cost(students[student_index], seats[seat_index], layout, rules, history)
    rule = rules.soft.avoid_recent_neighbors
    if not rule.enabled or rule.weight == 0:
        return cost
    for assigned_student_index, assigned_seat_index in assignment.items():
        cost += avoid_recent_neighbors_cost(
            students[student_index].key,
            students[assigned_student_index].key,
            seats[seat_index],
            seats[assigned_seat_index],
            layout,
            rule,
            pair_history,
            adjacency_edges=edges,
        )
    return cost


def _fallback_total_cost(
    assignment: dict[int, int],
    students: list[Student],
    seats: list[SeatNode],
    layout: ClassroomLayout,
    rules: RuleSet,
    edges: set[SeatEdge],
    history: SeatHistory | None,
    pair_history: PairHistory | None,
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
                if seat_indexes_adjacent(seats, first_seat_index, second_seat_index, edges):
                    cost -= rules.soft.score_balance.weight * abs(float(first_score) - float(second_score))

    rule = rules.soft.avoid_recent_neighbors
    if rule.enabled and rule.weight:
        for first_index, first_seat_index in assignment.items():
            for second_index, second_seat_index in assignment.items():
                if second_index <= first_index:
                    continue
                cost += avoid_recent_neighbors_cost(
                    students[first_index].key,
                    students[second_index].key,
                    seats[first_seat_index],
                    seats[second_seat_index],
                    layout,
                    rule,
                    pair_history,
                    adjacency_edges=edges,
                )
    return cost


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
    pair_history: PairHistory | None,
) -> SeatingSolution:
    assignments = [
        SeatAssignment(
            student_key=students[student_index].key,
            student_name=students[student_index].display_name,
            seat_id=seats[assignment_by_student[student_index]].seat_id,
        )
        for student_index in range(len(students))
    ]
    pair_history = pair_history or (history.pair_history if history is not None else None)
    fairness = assignment_fairness_summary(assignments, students, layout, rules, history, pair_history)
    if fairness:
        metrics = {**metrics, "fairness": fairness}
    return SeatingSolution(
        assignments=assignments,
        solver_status=status,
        objective_value=objective_value,
        metrics=metrics,
    )
