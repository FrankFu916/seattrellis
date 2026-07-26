"""Stable solver indexes and reusable classroom topology data."""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from math import inf
from typing import Literal, Sequence

from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.student import Student
from seattrellis.solver.adjacency import SeatEdge, build_adjacency_edges, seat_distance

SeatIndexEdge = tuple[int, int]
DistanceMetric = Literal["euclidean", "graph"]
DistanceMatrix = tuple[tuple[float, ...], ...]


@dataclass(frozen=True)
class CompiledTopology:
    """Index spaces, adjacency, and distances shared by solver backends."""

    seats: list[SeatNode]
    edges: set[SeatEdge]
    student_index_by_key: dict[str, int]
    seat_index_by_id: dict[str, int]
    adjacent_seat_index_pairs: frozenset[SeatIndexEdge]
    adjacency_by_seat_index: tuple[frozenset[int], ...]
    euclidean_distance_matrix: DistanceMatrix
    graph_distance_matrix: DistanceMatrix

    def seats_are_adjacent(self, first_index: int, second_index: int) -> bool:
        """Return whether two indexed seats share an adjacency edge."""

        if first_index == second_index:
            return False
        return second_index in self.adjacency_by_seat_index[first_index]

    def distance(
        self,
        first_index: int,
        second_index: int,
        metric: DistanceMetric,
    ) -> float:
        """Return a precomputed distance between two indexed seats."""

        matrix = (
            self.graph_distance_matrix
            if metric == "graph"
            else self.euclidean_distance_matrix
        )
        return matrix[first_index][second_index]


def precompute_topology(
    students: Sequence[Student],
    layout: ClassroomLayout,
) -> CompiledTopology:
    """Build stable indexes and all reusable seat topology exactly once."""

    seats = sorted(
        layout.enabled_seats,
        key=lambda seat: (seat.row, seat.col, seat.seat_id),
    )
    student_index_by_key = {
        student.key: index for index, student in enumerate(students)
    }
    seat_index_by_id = {seat.seat_id: index for index, seat in enumerate(seats)}
    edges = build_adjacency_edges(layout)
    adjacent_pairs = _compile_index_edges(edges, seat_index_by_id)
    adjacency = _build_index_adjacency(len(seats), adjacent_pairs)
    return CompiledTopology(
        seats=seats,
        edges=edges,
        student_index_by_key=student_index_by_key,
        seat_index_by_id=seat_index_by_id,
        adjacent_seat_index_pairs=adjacent_pairs,
        adjacency_by_seat_index=adjacency,
        euclidean_distance_matrix=_build_euclidean_distance_matrix(seats),
        graph_distance_matrix=_build_graph_distance_matrix(adjacency),
    )


def _compile_index_edges(
    edges: set[SeatEdge],
    seat_index_by_id: dict[str, int],
) -> frozenset[SeatIndexEdge]:
    pairs: set[SeatIndexEdge] = set()
    for first_id, second_id in edges:
        first_index = seat_index_by_id[first_id]
        second_index = seat_index_by_id[second_id]
        pairs.add(
            (first_index, second_index)
            if first_index < second_index
            else (second_index, first_index)
        )
    return frozenset(pairs)


def _build_index_adjacency(
    seat_count: int,
    edges: frozenset[SeatIndexEdge],
) -> tuple[frozenset[int], ...]:
    adjacency: list[set[int]] = [set() for _ in range(seat_count)]
    for first_index, second_index in edges:
        adjacency[first_index].add(second_index)
        adjacency[second_index].add(first_index)
    return tuple(frozenset(neighbors) for neighbors in adjacency)


def _build_euclidean_distance_matrix(seats: Sequence[SeatNode]) -> DistanceMatrix:
    return tuple(
        tuple(seat_distance(first, second) for second in seats)
        for first in seats
    )


def _build_graph_distance_matrix(
    adjacency: tuple[frozenset[int], ...],
) -> DistanceMatrix:
    rows: list[tuple[float, ...]] = []
    for source_index in range(len(adjacency)):
        distances = [inf] * len(adjacency)
        distances[source_index] = 0.0
        queue: deque[int] = deque([source_index])
        while queue:
            seat_index = queue.popleft()
            next_distance = distances[seat_index] + 1.0
            for neighbor_index in adjacency[seat_index]:
                if distances[neighbor_index] != inf:
                    continue
                distances[neighbor_index] = next_distance
                queue.append(neighbor_index)
        rows.append(tuple(distances))
    return tuple(rows)
