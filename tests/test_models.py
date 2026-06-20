from __future__ import annotations

import pytest

from seattrellis.models import ClassroomLayout, SeatNode, Student


def test_student_requires_name_or_id() -> None:
    with pytest.raises(ValueError):
        Student()


def test_student_allows_optional_fields_and_attributes() -> None:
    student = Student(name="Student001", tags="leader, math", attributes={"custom": "x"})
    assert student.key == "Student001"
    assert student.tags == ["leader", "math"]
    assert student.attributes["custom"] == "x"


def test_seat_node_defaults_coordinates() -> None:
    seat = SeatNode(seat_id="A1", row=2, col=3)
    assert seat.x == 3.0
    assert seat.y == 2.0


def test_layout_rejects_duplicate_seat_ids() -> None:
    with pytest.raises(ValueError):
        ClassroomLayout(
            seats=[
                SeatNode(seat_id="A1", row=1, col=1),
                SeatNode(seat_id="A1", row=1, col=2),
            ]
        )
