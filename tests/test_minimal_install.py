from __future__ import annotations

import subprocess

from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import students_from_records
from seattrellis.solver import solve_seating


def test_cli_help_runs_in_minimal_install() -> None:
    result = subprocess.run(["seattrellis", "--help"], check=False, text=True, capture_output=True)

    assert result.returncode == 0
    assert "SeatTrellis" in result.stdout


def test_fallback_solver_handles_json_layout_and_rules_without_optional_extras() -> None:
    students = students_from_records(
        [
            {"student_id": "STU001", "name": "Student001"},
            {"student_id": "STU002", "name": "Student002"},
            {"student_id": "STU003", "name": "Student003"},
            {"student_id": "STU004", "name": "Student004"},
        ]
    )
    layout = load_layout("tests/fixtures/classroom.json")
    rules = load_rules("tests/fixtures/rules.json")

    solution = solve_seating(students, layout, rules, seed=rules.seed)

    assert len(solution.assignments) == len(students)
    assert solution.metrics["solver"] == "fallback-heuristic"
