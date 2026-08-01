"""OR-Tools CP-SAT solver backend."""

from __future__ import annotations

import random
from typing import Any

from seattrellis.history import avoid_recent_neighbors_cost
from seattrellis.io.validation import format_infeasible_diagnostic
from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet, effective_neighbor_rule
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.solver.adjacency import SeatEdge
from seattrellis.solver.backend_common import individual_cost, solution_from_assignment
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.problem import CompiledProblem, distance_for_rule
from seattrellis.solver.result import SeatingSolution

cp_model = None
_cp_model_unavailable = False
_cp_model_import_error: Exception | None = None


def solve_with_ortools(
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    """Solve with OR-Tools CP-SAT."""

    _load_cp_model()
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

    _add_pair_constraints(model, x, problem)
    for excluded in problem.excluded_assignments:
        model.Add(sum(x[(student_index, seat_index)] for student_index, seat_index in excluded.items()) <= len(students) - 1)
    objective_terms = _build_individual_objective_terms(x, students, seats, layout, rules, history, seed)
    objective_terms.extend(_build_pair_objective_terms(model, x, students, seats, layout, rules, edges, pair_history))
    objective_terms.extend(_build_score_balance_terms(model, x, problem))
    if objective_terms:
        model.Minimize(sum(coef * var for var, coef in objective_terms))

    solver = cp_model.CpSolver()
    solver.parameters.max_time_in_seconds = time_limit_seconds
    solver.parameters.random_seed = seed
    solver.parameters.num_search_workers = 1
    status = solver.Solve(model)
    if status not in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        raise SeatTrellisSolveError(
            format_ortools_failure(
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

    return solution_from_assignment(
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


def _load_cp_model():
    global cp_model, _cp_model_import_error, _cp_model_unavailable
    if cp_model is not None:
        return cp_model
    if _cp_model_unavailable:
        raise MissingOptionalDependencyError(
            "OR-Tools solver",
            "solver",
        ) from _cp_model_import_error
    try:  # pragma: no cover - exercised when OR-Tools is installed and enabled.
        from ortools.sat.python import cp_model as loaded_cp_model
    except Exception as exc:  # pragma: no cover - local fallback path is tested.
        _cp_model_unavailable = True
        _cp_model_import_error = exc
        raise MissingOptionalDependencyError("OR-Tools solver", "solver") from exc
    cp_model = loaded_cp_model
    _cp_model_unavailable = False
    _cp_model_import_error = None
    return cp_model


def _add_pair_constraints(
    model: Any,
    x: dict[tuple[int, int], Any],
    problem: CompiledProblem,
) -> None:
    seats = problem.seats
    compiled = problem.rules_compiled
    for first_index, second_index in compiled.must_be_adjacent:
        for first_seat_index in range(len(seats)):
            for second_seat_index in range(len(seats)):
                if first_seat_index == second_seat_index:
                    continue
                if not problem.topology.seats_are_adjacent(
                    first_seat_index,
                    second_seat_index,
                ):
                    model.AddBoolOr(
                        [x[(first_index, first_seat_index)].Not(), x[(second_index, second_seat_index)].Not()]
                    )

    for first_index, second_index in compiled.cannot_be_adjacent:
        for first_seat_index in range(len(seats)):
            for second_seat_index in range(len(seats)):
                if first_seat_index == second_seat_index:
                    continue
                if problem.topology.seats_are_adjacent(
                    first_seat_index,
                    second_seat_index,
                ):
                    model.AddBoolOr(
                        [x[(first_index, first_seat_index)].Not(), x[(second_index, second_seat_index)].Not()]
                    )

    for first_index, second_index, rule in compiled.min_distance:
        for first_seat_index in range(len(seats)):
            for second_seat_index in range(len(seats)):
                if first_seat_index == second_seat_index:
                    continue
                distance = distance_for_rule(
                    problem,
                    first_seat_index,
                    second_seat_index,
                    rule,
                )
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
            coef = individual_cost(student, seat, layout, rules, history, rng, min_row, max_row)
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
    rule = effective_neighbor_rule(rules)
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
    problem: CompiledProblem,
) -> list[tuple[Any, int]]:
    students = problem.students
    rules = problem.rules
    soft = rules.soft.score_balance
    if not soft.enabled or soft.weight == 0:
        return []

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
            for first_seat_index, second_seat_index in problem.topology.adjacent_seat_index_pairs:
                for a_index, b_index in (
                    (first_seat_index, second_seat_index),
                    (second_seat_index, first_seat_index),
                ):
                    pair_var = model.NewBoolVar(
                        f"score_pair_{first_student_index}_{second_student_index}_{a_index}_{b_index}"
                    )
                    model.Add(pair_var <= x[(first_student_index, a_index)])
                    model.Add(pair_var <= x[(second_student_index, b_index)])
                    model.Add(pair_var >= x[(first_student_index, a_index)] + x[(second_student_index, b_index)] - 1)
                    terms.append((pair_var, -soft.weight * score_gap))
    return terms


def format_ortools_failure(
    *,
    status: int,
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    time_limit_seconds: float,
) -> str:
    status_name = ortools_status_name(status)
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


def ortools_status_name(status: int) -> str:
    for name in ("OPTIMAL", "FEASIBLE", "INFEASIBLE", "MODEL_INVALID", "UNKNOWN"):
        if getattr(cp_model, name, None) == status:
            return name
    return f"status {status}"
