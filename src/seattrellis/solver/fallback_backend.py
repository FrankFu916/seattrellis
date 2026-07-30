"""Seeded Python fallback solver with a cooperative wall-clock deadline."""

from __future__ import annotations

import random
import time

from seattrellis.history import avoid_recent_neighbors_cost
from seattrellis.io.validation import format_infeasible_diagnostic
from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.backend_common import individual_cost, solution_from_assignment
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.problem import (
    CompiledProblem,
    assignment_is_excluded,
    distance_for_rule,
    seat_indexes_adjacent,
)
from seattrellis.solver.result import SeatingSolution
from seattrellis.solver.soft_objectives import (
    SoftObjectiveContext,
    compile_soft_objectives,
    evaluate_soft_objectives,
)


class _FallbackDeadlineExceeded(RuntimeError):
    """Stop cooperative fallback work after the configured deadline."""


def solve_with_fallback(
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    """Solve with the built-in Python heuristic backend."""

    students = problem.students
    seats = problem.seats
    layout = problem.layout
    rules = problem.rules
    rng = random.Random(seed)
    objective_context = compile_soft_objectives(students, layout, rules, pair_history)
    attempts = max(40, len(students) * 12)
    deadline = time.monotonic() + time_limit_seconds
    best_assignment: dict[int, int] | None = None
    best_cost: float | None = None
    completed_attempts = 0
    stopped_by_time_limit = False

    try:
        for attempt in range(attempts):
            _raise_if_deadline_reached(deadline)
            assignment: dict[int, int] = {}
            used_seats: set[int] = set()
            success = True

            while len(assignment) < len(students):
                _raise_if_deadline_reached(deadline)
                choice = _choose_next_student(
                    problem,
                    assignment,
                    used_seats,
                    deadline,
                )
                if choice is None:
                    success = False
                    break
                student_index, candidates = choice

                def ranking_cost(seat_index: int) -> float:
                    _raise_if_deadline_reached(deadline)
                    cost = _fallback_candidate_cost(
                        student_index,
                        seat_index,
                        assignment,
                        problem,
                        history,
                        pair_history,
                        objective_context,
                    )
                    _raise_if_deadline_reached(deadline)
                    if attempt > 0:
                        cost += rng.random() * 25
                    return cost

                candidates = sorted(
                    candidates,
                    key=ranking_cost,
                )
                seat_index = (
                    candidates[0]
                    if attempt == 0
                    else rng.choice(candidates[: min(3, len(candidates))])
                )
                assignment[student_index] = seat_index
                used_seats.add(seat_index)

            _raise_if_deadline_reached(deadline)
            if (
                not success
                or not _full_assignment_valid(
                    assignment,
                    problem,
                )
                or assignment_is_excluded(
                    assignment,
                    problem.excluded_assignments,
                )
            ):
                continue
            completed_attempts += 1
            if best_assignment is None:
                best_assignment = dict(assignment)
            cost = _fallback_total_cost(
                assignment,
                problem,
                history,
                pair_history,
                deadline,
                objective_context,
            )
            if best_cost is None or cost < best_cost:
                best_assignment = dict(assignment)
                best_cost = cost
    except _FallbackDeadlineExceeded:
        stopped_by_time_limit = True

    if best_assignment is None:
        if stopped_by_time_limit:
            raise SeatTrellisSolveError(
                "Fallback solver did not find a feasible seating plan within "
                f"{time_limit_seconds:g} seconds. This is not proof that the "
                "problem is infeasible; try increasing --time-limit, reducing "
                "candidate count, or relaxing hard constraints."
            )
        raise SeatTrellisSolveError(format_infeasible_diagnostic(students, layout, rules))

    return solution_from_assignment(
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
            "soft_objective_warnings": list(objective_context.warnings),
            "mentor_pairs": [
                {
                    "mentor": pair.mentor_key,
                    "learner": pair.learner_key,
                    "recent_occurrences": pair.recent_occurrences,
                }
                for pair in objective_context.mentor_pairs
            ],
        },
        layout,
        rules,
        history,
        pair_history,
    )


def _choose_next_student(
    problem: CompiledProblem,
    assignment: dict[int, int],
    used_seats: set[int],
    deadline: float,
) -> tuple[int, list[int]] | None:
    best: tuple[int, list[int]] | None = None
    for student_index in range(len(problem.students)):
        _raise_if_deadline_reached(deadline)
        if student_index in assignment:
            continue
        candidates: list[int] = []
        for seat_index in range(len(problem.seats)):
            _raise_if_deadline_reached(deadline)
            if seat_index in used_seats:
                continue
            if _partial_assignment_valid(
                {**assignment, student_index: seat_index},
                problem,
            ):
                candidates.append(seat_index)
        if not candidates:
            return None
        if best is None or len(candidates) < len(best[1]):
            best = (student_index, candidates)
    return best


def _partial_assignment_valid(
    assignment: dict[int, int],
    problem: CompiledProblem,
) -> bool:
    compiled = problem.rules_compiled
    if len(set(assignment.values())) < len(assignment):
        return False
    for student_index, fixed_seat_index in compiled.fixed_seats.items():
        if student_index in assignment and assignment[student_index] != fixed_seat_index:
            return False
    for first_index, second_index in compiled.must_be_adjacent:
        if first_index in assignment and second_index in assignment:
            if not seat_indexes_adjacent(
                problem,
                assignment[first_index],
                assignment[second_index],
            ):
                return False
    for first_index, second_index in compiled.cannot_be_adjacent:
        if first_index in assignment and second_index in assignment:
            if seat_indexes_adjacent(
                problem,
                assignment[first_index],
                assignment[second_index],
            ):
                return False
    for first_index, second_index, rule in compiled.min_distance:
        if first_index in assignment and second_index in assignment:
            if (
                distance_for_rule(
                    problem,
                    assignment[first_index],
                    assignment[second_index],
                    rule,
                )
                < rule.distance
            ):
                return False
    return True


def _full_assignment_valid(
    assignment: dict[int, int],
    problem: CompiledProblem,
) -> bool:
    return _partial_assignment_valid(assignment, problem)


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
    return float(individual_cost(student, seat, layout, rules, history, fake_rng, min_row, max_row))


def _fallback_candidate_cost(
    student_index: int,
    seat_index: int,
    assignment: dict[int, int],
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    objective_context: SoftObjectiveContext,
) -> float:
    students = problem.students
    seats = problem.seats
    layout = problem.layout
    rules = problem.rules
    cost = _fallback_individual_cost(students[student_index], seats[seat_index], layout, rules, history)
    rule = rules.soft.avoid_recent_neighbors
    if rule.enabled and rule.weight:
        for assigned_student_index, assigned_seat_index in assignment.items():
            cost += avoid_recent_neighbors_cost(
                students[student_index].key,
                students[assigned_student_index].key,
                seats[seat_index],
                seats[assigned_seat_index],
                layout,
                rule,
                pair_history,
                adjacency_edges=problem.edges,
            )
    prospective = {
        students[index].key: seats[assigned_seat_index].seat_id
        for index, assigned_seat_index in assignment.items()
    }
    prospective[students[student_index].key] = seats[seat_index].seat_id
    cost += evaluate_soft_objectives(
        prospective,
        objective_context,
        rules,
    ).total_cost
    return cost


def _fallback_total_cost(
    assignment: dict[int, int],
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    deadline: float,
    objective_context: SoftObjectiveContext,
) -> float:
    students = problem.students
    seats = problem.seats
    layout = problem.layout
    rules = problem.rules
    min_row = min(seat.row for seat in seats)
    max_row = max(seat.row for seat in seats)
    rng = random.Random(rules.seed)
    cost = 0.0
    for student_index, seat_index in assignment.items():
        _raise_if_deadline_reached(deadline)
        cost += individual_cost(students[student_index], seats[seat_index], layout, rules, history, rng, min_row, max_row)

    if rules.soft.score_balance.enabled and rules.soft.score_balance.weight:
        for first_index, first_seat_index in assignment.items():
            _raise_if_deadline_reached(deadline)
            first_score = students[first_index].score
            if first_score is None:
                continue
            for second_index, second_seat_index in assignment.items():
                _raise_if_deadline_reached(deadline)
                if second_index <= first_index:
                    continue
                second_score = students[second_index].score
                if second_score is None:
                    continue
                if seat_indexes_adjacent(problem, first_seat_index, second_seat_index):
                    cost -= rules.soft.score_balance.weight * abs(float(first_score) - float(second_score))

    rule = rules.soft.avoid_recent_neighbors
    if rule.enabled and rule.weight:
        for first_index, first_seat_index in assignment.items():
            _raise_if_deadline_reached(deadline)
            for second_index, second_seat_index in assignment.items():
                _raise_if_deadline_reached(deadline)
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
                    adjacency_edges=problem.edges,
                )
    assignment_by_key = {
        students[student_index].key: seats[seat_index].seat_id
        for student_index, seat_index in assignment.items()
    }
    cost += evaluate_soft_objectives(
        assignment_by_key,
        objective_context,
        rules,
    ).total_cost
    return cost


def _raise_if_deadline_reached(deadline: float) -> None:
    if time.monotonic() >= deadline:
        raise _FallbackDeadlineExceeded
