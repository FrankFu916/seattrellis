from __future__ import annotations

from collections import deque
from math import dist, inf

from seattrellis.models.layout import ClassroomLayout, SeatNode

SeatEdge = tuple[str, str]


def normalize_edge(first: str, second: str) -> SeatEdge:
    if first == second:
        raise ValueError("Self adjacency edges are not allowed.")
    return tuple(sorted((first, second)))  # type: ignore[return-value]


def build_adjacency_edges(layout: ClassroomLayout) -> set[SeatEdge]:
    """Build an undirected adjacency graph for enabled seats."""

    config = layout.adjacency
    enabled = {seat.seat_id: seat for seat in layout.enabled_seats}
    seats = list(enabled.values())
    edges: set[SeatEdge] = set()

    for index, first in enumerate(seats):
        for second in seats[index + 1 :]:
            if _are_adjacent(first, second, layout):
                edges.add(normalize_edge(first.seat_id, second.seat_id))

    for first, second in config.custom_edges:
        if first in enabled and second in enabled and first != second:
            edges.add(normalize_edge(first, second))

    return edges


def adjacency_map(edges: set[SeatEdge]) -> dict[str, set[str]]:
    graph: dict[str, set[str]] = {}
    for first, second in edges:
        graph.setdefault(first, set()).add(second)
        graph.setdefault(second, set()).add(first)
    return graph


def seat_distance(first: SeatNode, second: SeatNode) -> float:
    return dist((float(first.x), float(first.y)), (float(second.x), float(second.y)))


def graph_distance(layout: ClassroomLayout, first_id: str, second_id: str) -> float:
    if first_id == second_id:
        return 0.0

    graph = adjacency_map(build_adjacency_edges(layout))
    queue: deque[tuple[str, int]] = deque([(first_id, 0)])
    seen = {first_id}
    while queue:
        seat_id, depth = queue.popleft()
        for neighbor in graph.get(seat_id, set()):
            if neighbor == second_id:
                return float(depth + 1)
            if neighbor not in seen:
                seen.add(neighbor)
                queue.append((neighbor, depth + 1))
    return inf


def _are_adjacent(first: SeatNode, second: SeatNode, layout: ClassroomLayout) -> bool:
    config = layout.adjacency
    if config.max_distance is not None:
        if config.use_xy_distance:
            return seat_distance(first, second) <= config.max_distance
        row_col_distance = dist((first.row, first.col), (second.row, second.col))
        return row_col_distance <= config.max_distance

    row_delta = abs(first.row - second.row)
    col_delta = abs(first.col - second.col)
    if row_delta == 0 and 0 < col_delta <= config.max_col_delta:
        return config.include_horizontal
    if col_delta == 0 and 0 < row_delta <= config.max_row_delta:
        return config.include_vertical
    if row_delta and col_delta:
        return (
            config.include_diagonal
            and row_delta <= config.max_row_delta
            and col_delta <= config.max_col_delta
        )
    return False
