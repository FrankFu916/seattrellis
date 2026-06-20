from __future__ import annotations

from seattrellis.models import ClassroomLayout, RuleSet, SeatNode, Student
from seattrellis.solver import solve_seating


def test_solver_assigns_each_student_once_and_each_seat_at_most_once() -> None:
    students = [Student(student_id=f"S{i}", name=f"Student {i}") for i in range(1, 4)]
    layout = ClassroomLayout(
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
            SeatNode(seat_id="B1", row=2, col=1),
            SeatNode(seat_id="B2", row=2, col=2, enabled=False),
        ]
    )

    solution = solve_seating(students, layout, RuleSet(seed=1), seed=1)

    assert len(solution.assignments) == 3
    assert len({assignment.student_key for assignment in solution.assignments}) == 3
    assert len({assignment.seat_id for assignment in solution.assignments}) == 3
    assert "B2" not in {assignment.seat_id for assignment in solution.assignments}
