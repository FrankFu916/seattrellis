from __future__ import annotations

from typing import Any

try:
    from pydantic.v1 import BaseModel, Field, root_validator, validator
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import BaseModel, Field, root_validator, validator


class SeatNode(BaseModel):
    """A physical or logical seat node in the classroom graph."""

    seat_id: str
    row: int
    col: int
    x: float | None = None
    y: float | None = None
    enabled: bool = True
    zone: str | None = None
    near_window: bool = False
    near_door: bool = False
    near_platform: bool = False
    near_ac: bool = False
    tags: list[str] = Field(default_factory=list)
    attributes: dict[str, Any] = Field(default_factory=dict)

    @validator("seat_id")
    def clean_seat_id(cls, value: str) -> str:
        value = str(value).strip()
        if not value:
            raise ValueError("seat_id cannot be empty.")
        return value

    @validator("row", "col")
    def positive_grid_position(cls, value: int) -> int:
        if value < 1:
            raise ValueError("row and col must be positive integers.")
        return value

    @root_validator(skip_on_failure=True)
    def default_coordinates(cls, values: dict[str, Any]) -> dict[str, Any]:
        if values.get("x") is None:
            values["x"] = float(values.get("col", 0))
        if values.get("y") is None:
            values["y"] = float(values.get("row", 0))
        return values


class AdjacencyConfig(BaseModel):
    """Rules for generating a seat adjacency graph."""

    include_horizontal: bool = True
    include_vertical: bool = False
    include_diagonal: bool = False
    max_row_delta: int = 1
    max_col_delta: int = 1
    max_distance: float | None = None
    use_xy_distance: bool = True
    custom_edges: list[tuple[str, str]] = Field(default_factory=list)

    @validator("custom_edges", pre=True)
    def normalize_edges(cls, value: Any) -> list[tuple[str, str]]:
        if value is None:
            return []
        normalized: list[tuple[str, str]] = []
        for edge in value:
            if isinstance(edge, dict):
                first = edge.get("a") or edge.get("from") or edge.get("seat_a")
                second = edge.get("b") or edge.get("to") or edge.get("seat_b")
            else:
                first, second = edge
            normalized.append((str(first), str(second)))
        return normalized


class ClassroomLayout(BaseModel):
    """A classroom layout represented by seat nodes and adjacency settings."""

    layout_id: str = "default-layout"
    name: str = "Classroom"
    seats: list[SeatNode]
    adjacency: AdjacencyConfig = Field(default_factory=AdjacencyConfig)
    metadata: dict[str, Any] = Field(default_factory=dict)

    @root_validator(skip_on_failure=True)
    def require_unique_seat_ids(cls, values: dict[str, Any]) -> dict[str, Any]:
        seats = values.get("seats") or []
        if not seats:
            raise ValueError("Classroom layout must contain at least one seat.")
        seat_ids = [seat.seat_id for seat in seats]
        duplicates = sorted({seat_id for seat_id in seat_ids if seat_ids.count(seat_id) > 1})
        if duplicates:
            raise ValueError(f"Duplicate seat_id: {', '.join(duplicates)}")
        enabled_ids = {seat.seat_id for seat in seats if seat.enabled}
        disabled_ids = {seat.seat_id for seat in seats if not seat.enabled}
        for first, second in values.get("adjacency", AdjacencyConfig()).custom_edges:
            missing = [seat_id for seat_id in (first, second) if seat_id not in seat_ids]
            if missing:
                raise ValueError(f"custom_edges reference unknown seat_id values: {', '.join(missing)}")
            disabled = [seat_id for seat_id in (first, second) if seat_id in disabled_ids]
            if disabled:
                raise ValueError(f"custom_edges reference disabled seat_id values: {', '.join(disabled)}")
            if first == second:
                raise ValueError("custom_edges cannot connect a seat to itself.")
        if not enabled_ids:
            raise ValueError("Classroom layout must contain at least one enabled seat.")
        return values

    @property
    def enabled_seats(self) -> list[SeatNode]:
        return [seat for seat in self.seats if seat.enabled]

    def seat_by_id(self, seat_id: str) -> SeatNode:
        for seat in self.seats:
            if seat.seat_id == seat_id:
                return seat
        raise KeyError(f"Unknown seat_id: {seat_id}")
