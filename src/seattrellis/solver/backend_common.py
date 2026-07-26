"""Shared helpers for solver backend implementations."""

from __future__ import annotations

import random
from typing import Any

from seattrellis.history import assignment_fairness_summary, fair_rotation_cost
from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment
from seattrellis.models.student import Student, student_needs_front
from seattrellis.solver.result import SeatingSolution


def individual_cost(
    student: Student,
    seat: SeatNode,
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
    rng: random.Random,
    min_row: int,
    max_row: int,
) -> int:
    """Cost contribution for one student-seat assignment."""

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


def solution_from_assignment(
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
    """Build a SeatingSolution from student-index to seat-index assignments."""

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
