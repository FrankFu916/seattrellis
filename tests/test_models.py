from __future__ import annotations

import pytest

from seattrellis.models import AdjacencyConfig, ClassroomLayout, SeatNode, Student
from seattrellis.models.rules import MinDistanceRule


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


def test_layout_rejects_duplicate_grid_positions() -> None:
    with pytest.raises(ValueError, match="Duplicate seat grid position"):
        ClassroomLayout(
            seats=[
                SeatNode(seat_id="A1", row=1, col=1),
                SeatNode(seat_id="A2", row=1, col=1),
            ]
        )


@pytest.mark.parametrize("coordinate", [float("nan"), float("inf"), float("-inf")])
def test_seat_rejects_non_finite_coordinates(coordinate: float) -> None:
    with pytest.raises(ValueError, match="finite number"):
        SeatNode(seat_id="A1", row=1, col=1, x=coordinate)


@pytest.mark.parametrize("distance", [0, -1, float("nan"), float("inf")])
def test_adjacency_rejects_invalid_max_distance(distance: float) -> None:
    with pytest.raises(ValueError, match="positive finite"):
        AdjacencyConfig(max_distance=distance)


def test_adjacency_rejects_negative_grid_deltas() -> None:
    with pytest.raises(ValueError, match="max_row_delta must be non-negative"):
        AdjacencyConfig(max_row_delta=-1)


@pytest.mark.parametrize("distance", [float("nan"), float("inf"), float("-inf")])
def test_min_distance_rule_rejects_non_finite_distance(distance: float) -> None:
    with pytest.raises(ValueError, match="positive finite"):
        MinDistanceRule(students=("S1", "S2"), distance=distance)
