"""Experimental Rust native backend adapter."""

from __future__ import annotations

from typing import Any

from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.fallback_backend import solve_with_fallback
from seattrellis.solver.native import (
    EXPECTED_NATIVE_API_VERSION,
    evaluate_native_problem,
    require_native_core,
)
from seattrellis.solver.problem import CompiledProblem
from seattrellis.solver.result import SeatingSolution


def solve_with_native(
    problem: CompiledProblem,
    history: SeatHistory | None,
    pair_history: PairHistory | None,
    seed: int,
    time_limit_seconds: float,
    requested_backend: str,
) -> SeatingSolution:
    """Run the experimental native backend.

    The v1.4 native backend keeps Python fallback search in place and uses one
    coarse Rust call for graph precomputation, hard-rule verification and a
    scoring cross-check. This proves the boundary without making Rust a
    production solver yet.
    """

    native_core = require_native_core()
    solution = solve_with_fallback(
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
    native_result = evaluate_native_problem(
        native_core,
        _native_evaluation_request(problem, assignment_pairs),
    )
    if not native_result["assignment_unique"]:
        raise SeatTrellisSolveError(
            "Native assignment-structure validation failed: "
            "the assignment is not complete and unique."
        )
    if not native_result["hard_constraints_satisfied"]:
        codes = ", ".join(str(code) for code in native_result["violation_codes"])
        raise SeatTrellisSolveError(
            "Native hard-constraint validation failed after solving"
            + (f": {codes}." if codes else ".")
        )
    solution.metrics.update(
        {
            "solver": "fallback-heuristic+native-validator",
            "solver_backend_effective": "fallback",
            "solver_validation_backend": "native",
            "native_core": {
                "module": "seattrellis_native",
                "version": getattr(native_core, "__version__", None),
                "api_version": getattr(native_core, "NATIVE_API_VERSION", None),
                "role": "post-solve-constraint-validator",
                "dto_api_version": native_result["api_version"],
                "validated_unique_assignment": True,
                "validated_hard_constraints": True,
                "checked_rule_count": native_result["checked_rule_count"],
                "violation_count": native_result["violation_count"],
                "graph_distance_seat_count": len(
                    native_result["graph_distance_matrix"]
                ),
                "peer_mixing_gap_sum": native_result["peer_mixing_gap_sum"],
                "peer_mixing_pair_count": native_result[
                    "peer_mixing_pair_count"
                ],
                "peer_mixing_mean_gap": native_result["peer_mixing_mean_gap"],
            },
        }
    )
    return solution


def _native_evaluation_request(
    problem: CompiledProblem,
    assignment_pairs: list[tuple[int, int]],
) -> dict[str, Any]:
    """Build the versioned, identity-free DTO consumed by the Rust spike."""

    compiled = problem.rules_compiled
    return {
        "api_version": EXPECTED_NATIVE_API_VERSION,
        "student_count": len(problem.students),
        "seat_positions": [
            [
                float(seat.x if seat.x is not None else seat.col),
                float(seat.y if seat.y is not None else seat.row),
            ]
            for seat in problem.seats
        ],
        "edges": [
            list(edge)
            for edge in sorted(problem.topology.adjacent_seat_index_pairs)
        ],
        "assignments": [list(pair) for pair in assignment_pairs],
        "fixed_seats": [
            [student_index, seat_index]
            for student_index, seat_index in sorted(compiled.fixed_seats.items())
        ],
        "must_be_adjacent": [list(pair) for pair in compiled.must_be_adjacent],
        "cannot_be_adjacent": [list(pair) for pair in compiled.cannot_be_adjacent],
        "min_distance": [
            {
                "students": [first_student, second_student],
                "distance": rule.distance,
                "metric": rule.metric,
            }
            for first_student, second_student, rule in compiled.min_distance
        ],
        "student_scores": [
            float(student.score) if student.score is not None else None
            for student in problem.students
        ],
    }
