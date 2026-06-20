from __future__ import annotations

from seattrellis.io.json_files import load_layout
from seattrellis.solver.adjacency import build_adjacency_edges, normalize_edge


def test_adjacency_uses_enabled_horizontal_seats() -> None:
    layout = load_layout("tests/fixtures/classroom.json")
    edges = build_adjacency_edges(layout)

    assert normalize_edge("A1", "A2") in edges
    assert normalize_edge("B1", "B2") in edges
    assert normalize_edge("B2", "B3") in edges
    assert normalize_edge("A2", "A3") not in edges
