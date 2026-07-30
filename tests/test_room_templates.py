from __future__ import annotations

from dataclasses import FrozenInstanceError

import pytest

from seattrellis.application.room_templates import (
    ROOM_TEMPLATE_30,
    RoomTemplate,
    build_room_from_template,
    build_standard_room,
    get_room_template,
    list_room_templates,
    recommend_room_template,
)
from seattrellis.solver.adjacency import build_adjacency_edges, normalize_edge


def test_builtin_templates_have_fixed_capacities_and_central_aisles() -> None:
    templates = list_room_templates()

    assert [template.capacity for template in templates] == [30, 48, 60]
    assert [(template.rows, template.seats_per_row) for template in templates] == [
        (5, 6),
        (6, 8),
        (6, 10),
    ]
    assert [template.aisles_after for template in templates] == [(3,), (4,), (5,)]


def test_room_template_is_immutable_and_validates_dimensions() -> None:
    with pytest.raises(FrozenInstanceError):
        ROOM_TEMPLATE_30.rows = 4  # type: ignore[misc]

    with pytest.raises(ValueError, match="rows must be positive"):
        RoomTemplate("invalid", 0, 6, (3,), "Invalid")
    with pytest.raises(ValueError, match="less than seats_per_row"):
        RoomTemplate("invalid", 2, 6, (6,), "Invalid")
    with pytest.raises(ValueError, match="duplicate"):
        RoomTemplate("invalid", 2, 6, (3, 3), "Invalid")


@pytest.mark.parametrize(
    ("template_id", "expected_capacity"),
    [
        ("standard-30", 30),
        ("STANDARD_48", 48),
        ("60-seat", 60),
        (60, 60),
    ],
)
def test_get_room_template_accepts_ids_and_capacity_aliases(
    template_id: str | int,
    expected_capacity: int,
) -> None:
    assert get_room_template(template_id).capacity == expected_capacity


def test_get_room_template_rejects_unknown_or_malformed_ids() -> None:
    with pytest.raises(KeyError, match="Unknown room template"):
        get_room_template("standard-36")
    with pytest.raises(ValueError, match="cannot be empty"):
        get_room_template("  ")
    with pytest.raises(TypeError, match="string or integer"):
        get_room_template(True)


@pytest.mark.parametrize(
    ("student_count", "expected_capacity"),
    [(1, 30), (30, 30), (31, 48), (48, 48), (49, 60), (60, 60)],
)
def test_recommend_room_template_uses_smallest_sufficient_room(
    student_count: int,
    expected_capacity: int,
) -> None:
    recommendation = recommend_room_template(student_count)

    assert recommendation is not None
    assert recommendation.capacity == expected_capacity


def test_recommend_room_template_returns_none_above_largest_room() -> None:
    assert recommend_room_template(61) is None


@pytest.mark.parametrize("student_count", [0, -1])
def test_recommend_room_template_rejects_non_positive_counts(student_count: int) -> None:
    with pytest.raises(ValueError, match="student_count must be positive"):
        recommend_room_template(student_count)


def test_build_room_from_template_keeps_aisles_as_disabled_nodes() -> None:
    layout = build_room_from_template("standard-30")

    assert layout.layout_id == "standard-30"
    assert len(layout.enabled_seats) == 30
    aisle_nodes = [seat for seat in layout.seats if not seat.enabled]
    assert len(aisle_nodes) == 5
    assert all(seat.zone == "aisle" for seat in aisle_nodes)
    assert {(seat.row, seat.col) for seat in aisle_nodes} == {
        (row, 4) for row in range(1, 6)
    }
    assert len({seat.seat_id for seat in layout.seats}) == len(layout.seats)
    assert len({(seat.row, seat.col) for seat in layout.seats}) == len(layout.seats)
    assert layout.metadata["template_id"] == "standard-30"


def test_standard_room_adjacency_does_not_cross_an_aisle() -> None:
    layout = build_standard_room(2, 6, aisles_after=3)
    edges = build_adjacency_edges(layout)

    assert normalize_edge("R1C2", "R1C3") in edges
    assert normalize_edge("R1C3", "R1C5") not in edges
    assert normalize_edge("R1C5", "R1C6") in edges
    assert all("AISLE" not in seat_id for edge in edges for seat_id in edge)


def test_standard_room_supports_multiple_aisles_and_custom_identity() -> None:
    layout = build_standard_room(
        2,
        6,
        aisles_after=(4, 2),
        layout_id="room-204",
        name="Room 204",
    )

    assert layout.layout_id == "room-204"
    assert layout.name == "Room 204"
    assert len(layout.enabled_seats) == 12
    assert [(seat.row, seat.col) for seat in layout.seats if not seat.enabled] == [
        (1, 3),
        (1, 6),
        (2, 3),
        (2, 6),
    ]
    assert layout.metadata["aisles_after"] == [2, 4]


@pytest.mark.parametrize(
    ("rows", "seats_per_row", "aisles_after", "error_type", "message"),
    [
        (0, 6, None, ValueError, "rows must be positive"),
        (2, 0, None, ValueError, "seats_per_row must be positive"),
        (2, 6, 0, ValueError, "at least 1"),
        (2, 6, 6, ValueError, "less than seats_per_row"),
        (2, 6, (2, 2), ValueError, "duplicate"),
        (2, 6, (2, 3.5), TypeError, "integer seat positions"),
    ],
)
def test_build_standard_room_rejects_invalid_dimensions(
    rows: int,
    seats_per_row: int,
    aisles_after: int | tuple[int | float, ...] | None,
    error_type: type[Exception],
    message: str,
) -> None:
    with pytest.raises(error_type, match=message):
        build_standard_room(  # type: ignore[arg-type]
            rows,
            seats_per_row,
            aisles_after=aisles_after,
        )


def test_build_room_from_custom_template_allows_identity_overrides() -> None:
    template = RoomTemplate(
        template_id="compact-12",
        name="Compact classroom",
        rows=3,
        seats_per_row=4,
        aisles_after=(2,),
    )

    layout = build_room_from_template(template, layout_id="class-7", name="Class 7")

    assert layout.layout_id == "class-7"
    assert layout.name == "Class 7"
    assert len(layout.enabled_seats) == 12
    assert layout.metadata["template_id"] == "compact-12"
