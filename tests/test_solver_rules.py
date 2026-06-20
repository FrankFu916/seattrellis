from __future__ import annotations

from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import read_students
from seattrellis.solver import solve_seating
from seattrellis.solver.adjacency import build_adjacency_edges, normalize_edge


def test_fixed_adjacent_and_not_adjacent_rules_are_enforced() -> None:
    students = read_students("tests/fixtures/students.csv")
    layout = load_layout("tests/fixtures/classroom.json")
    rules = load_rules("tests/fixtures/rules.json")

    solution = solve_seating(students, layout, rules, seed=rules.seed)
    assignment = solution.assignment_map
    edges = build_adjacency_edges(layout)

    assert assignment["STU001"] == "A1"
    assert normalize_edge(assignment["STU002"], assignment["STU003"]) in edges
    assert normalize_edge(assignment["STU001"], assignment["STU004"]) not in edges


def test_disabled_irregular_seat_is_not_used() -> None:
    students = read_students("tests/fixtures/students.csv")
    layout = load_layout("tests/fixtures/classroom.json")
    rules = load_rules("tests/fixtures/rules.json")

    solution = solve_seating(students, layout, rules, seed=rules.seed)

    assert "A3" not in {assignment.seat_id for assignment in solution.assignments}
