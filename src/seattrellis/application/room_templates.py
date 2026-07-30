"""Standard classroom layouts for the teacher-facing workflow.

The application layer describes a small set of familiar rooms without
exposing the layout JSON format.  Aisles are retained as disabled grid nodes
so renderers can show the physical gap and adjacency cannot bridge it.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

from seattrellis.models import AdjacencyConfig, ClassroomLayout, SeatNode


@dataclass(frozen=True, slots=True)
class RoomTemplate:
    """A reusable rectangular classroom description.

    ``aisles_after`` contains logical seat positions.  For example, ``(3,)``
    places an aisle after the third seat in every row.  A template's capacity
    counts enabled seats only; aisle cells never consume a student place.
    """

    template_id: str
    rows: int
    seats_per_row: int
    aisles_after: tuple[int, ...]
    name: str

    def __post_init__(self) -> None:
        template_id = _clean_text(self.template_id, "template_id")
        name = _clean_text(self.name, "name")
        rows = _positive_int(self.rows, "rows")
        seats_per_row = _positive_int(self.seats_per_row, "seats_per_row")
        aisles_after = _normalize_aisles_after(self.aisles_after, seats_per_row)

        object.__setattr__(self, "template_id", template_id)
        object.__setattr__(self, "name", name)
        object.__setattr__(self, "rows", rows)
        object.__setattr__(self, "seats_per_row", seats_per_row)
        object.__setattr__(self, "aisles_after", aisles_after)

    @property
    def capacity(self) -> int:
        """Return the number of enabled student seats."""

        return self.rows * self.seats_per_row

    @property
    def grid_columns(self) -> int:
        """Return the physical grid width, including aisle cells."""

        return self.seats_per_row + len(self.aisles_after)


def list_room_templates() -> tuple[RoomTemplate, ...]:
    """Return the built-in rooms in ascending capacity order."""

    return STANDARD_ROOM_TEMPLATES


def get_room_template(template_id: str | int) -> RoomTemplate:
    """Return a built-in room by ID or capacity alias.

    Accepted examples include ``"standard-48"``, ``"48-seat"``, and ``48``.
    Unknown IDs raise ``KeyError`` so callers can distinguish lookup failures
    from malformed room dimensions.
    """

    if isinstance(template_id, bool) or not isinstance(template_id, (str, int)):
        raise TypeError("template_id must be a string or integer capacity.")
    normalized = str(template_id).strip().lower().replace("_", "-")
    if not normalized:
        raise ValueError("template_id cannot be empty.")
    try:
        return _TEMPLATE_ALIASES[normalized]
    except KeyError as exc:
        available = ", ".join(_TEMPLATES_BY_ID)
        raise KeyError(
            f"Unknown room template {template_id!r}. Available templates: {available}."
        ) from exc


def recommend_room_template(student_count: int) -> RoomTemplate | None:
    """Return the smallest built-in room that can hold ``student_count``.

    Counts above the largest built-in room return ``None`` so the user
    interface can offer the custom room builder.  Non-positive counts are
    invalid because a class must contain at least one student.
    """

    count = _positive_int(student_count, "student_count")
    return next(
        (template for template in STANDARD_ROOM_TEMPLATES if template.capacity >= count),
        None,
    )


def build_standard_room(
    rows: int,
    seats_per_row: int,
    *,
    aisles_after: int | Sequence[int] | None = None,
    layout_id: str | None = None,
    name: str = "Standard classroom",
) -> ClassroomLayout:
    """Build a rectangular room with optional full-length aisles.

    Each aisle position means "after this logical seat number".  The aisle is
    represented by one disabled :class:`SeatNode` per row.  Horizontal
    adjacency is enabled, but the disabled cells split each row into separate
    banks so adjacent pairs never cross an aisle.
    """

    normalized_rows = _positive_int(rows, "rows")
    normalized_seats = _positive_int(seats_per_row, "seats_per_row")
    normalized_aisles = _normalize_aisles_after(aisles_after, normalized_seats)
    room_id = (
        _clean_text(layout_id, "layout_id")
        if layout_id is not None
        else f"standard-{normalized_rows}x{normalized_seats}"
    )
    room_name = _clean_text(name, "name")

    return _build_room(
        rows=normalized_rows,
        seats_per_row=normalized_seats,
        aisles_after=normalized_aisles,
        layout_id=room_id,
        name=room_name,
        template_id=None,
    )


def build_room_from_template(
    template: str | int | RoomTemplate,
    *,
    layout_id: str | None = None,
    name: str | None = None,
) -> ClassroomLayout:
    """Build a classroom from a built-in ID or a ``RoomTemplate`` instance."""

    if isinstance(template, RoomTemplate):
        selected = template
    else:
        selected = get_room_template(template)

    room_id = (
        _clean_text(layout_id, "layout_id")
        if layout_id is not None
        else selected.template_id
    )
    room_name = _clean_text(name, "name") if name is not None else selected.name
    return _build_room(
        rows=selected.rows,
        seats_per_row=selected.seats_per_row,
        aisles_after=selected.aisles_after,
        layout_id=room_id,
        name=room_name,
        template_id=selected.template_id,
    )


def _build_room(
    *,
    rows: int,
    seats_per_row: int,
    aisles_after: tuple[int, ...],
    layout_id: str,
    name: str,
    template_id: str | None,
) -> ClassroomLayout:
    seats: list[SeatNode] = []
    last_grid_column = seats_per_row + len(aisles_after)

    for row in range(1, rows + 1):
        grid_column = 1
        for logical_column in range(1, seats_per_row + 1):
            seats.append(
                SeatNode(
                    seat_id=f"R{row}C{grid_column}",
                    row=row,
                    col=grid_column,
                    zone=_seat_zone(row, rows),
                    near_platform=row == 1,
                    near_window=grid_column == 1,
                    near_door=row == rows and grid_column == last_grid_column,
                    attributes={"logical_column": logical_column},
                )
            )
            grid_column += 1

            if logical_column in aisles_after:
                # A disabled cell is more useful than an implicit coordinate
                # gap: layout editors and exporters can render the aisle while
                # the solver automatically excludes it from adjacency.
                seats.append(
                    SeatNode(
                        seat_id=f"AISLE-R{row}C{grid_column}",
                        row=row,
                        col=grid_column,
                        enabled=False,
                        zone="aisle",
                        tags=["aisle"],
                        attributes={"cell_type": "aisle"},
                    )
                )
                grid_column += 1

    metadata: dict[str, object] = {
        "room_type": "standard",
        "capacity": rows * seats_per_row,
        "rows": rows,
        "seats_per_row": seats_per_row,
        "aisles_after": list(aisles_after),
        "front": "row-1",
    }
    if template_id is not None:
        metadata["template_id"] = template_id

    return ClassroomLayout(
        layout_id=layout_id,
        name=name,
        seats=seats,
        adjacency=AdjacencyConfig(
            include_horizontal=True,
            include_vertical=False,
            include_diagonal=False,
            max_row_delta=1,
            max_col_delta=1,
        ),
        metadata=metadata,
    )


def _normalize_aisles_after(
    value: int | Sequence[int] | None,
    seats_per_row: int,
) -> tuple[int, ...]:
    if value is None:
        return ()
    raw_positions: Sequence[int]
    if isinstance(value, bool):
        raise TypeError("aisles_after must contain integer seat positions.")
    if isinstance(value, int):
        raw_positions = (value,)
    elif isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
        raw_positions = value
    else:
        raise TypeError("aisles_after must be an integer or a sequence of integers.")

    positions: list[int] = []
    for position in raw_positions:
        if isinstance(position, bool) or not isinstance(position, int):
            raise TypeError("aisles_after must contain integer seat positions.")
        if position < 1 or position >= seats_per_row:
            raise ValueError(
                "Each aisle position must be at least 1 and less than seats_per_row."
            )
        positions.append(position)
    if len(set(positions)) != len(positions):
        raise ValueError("aisles_after cannot contain duplicate positions.")
    return tuple(sorted(positions))


def _positive_int(value: int, field_name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{field_name} must be an integer.")
    if value <= 0:
        raise ValueError(f"{field_name} must be positive.")
    return value


def _clean_text(value: str, field_name: str) -> str:
    if not isinstance(value, str):
        raise TypeError(f"{field_name} must be a string.")
    cleaned = value.strip()
    if not cleaned:
        raise ValueError(f"{field_name} cannot be empty.")
    return cleaned


def _seat_zone(row: int, row_count: int) -> str:
    if row == 1:
        return "front"
    if row == row_count:
        return "back"
    return "middle"


# Instantiate templates after the validation helpers are defined because the
# frozen dataclass validates itself during module import.
ROOM_TEMPLATE_30 = RoomTemplate(
    template_id="standard-30",
    name="30-seat classroom",
    rows=5,
    seats_per_row=6,
    aisles_after=(3,),
)
ROOM_TEMPLATE_48 = RoomTemplate(
    template_id="standard-48",
    name="48-seat classroom",
    rows=6,
    seats_per_row=8,
    aisles_after=(4,),
)
ROOM_TEMPLATE_60 = RoomTemplate(
    template_id="standard-60",
    name="60-seat classroom",
    rows=6,
    seats_per_row=10,
    aisles_after=(5,),
)

STANDARD_ROOM_TEMPLATES: tuple[RoomTemplate, ...] = (
    ROOM_TEMPLATE_30,
    ROOM_TEMPLATE_48,
    ROOM_TEMPLATE_60,
)

_TEMPLATES_BY_ID = {
    template.template_id: template for template in STANDARD_ROOM_TEMPLATES
}
_TEMPLATE_ALIASES = {
    alias: template
    for template in STANDARD_ROOM_TEMPLATES
    for alias in (
        str(template.capacity),
        f"{template.capacity}-seat",
        f"{template.capacity}-seats",
        template.template_id,
    )
}
