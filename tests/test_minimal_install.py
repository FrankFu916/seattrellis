from __future__ import annotations

import subprocess

from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import students_from_records
from seattrellis.solver import solve_seating


def test_cli_help_runs_in_minimal_install() -> None:
    result = subprocess.run(["seattrellis", "--help"], check=False, text=True, capture_output=True)

    assert result.returncode == 0
    assert "SeatTrellis" in result.stdout


def test_csv_validate_and_html_export_run_in_minimal_install(tmp_path) -> None:
    validate_result = subprocess.run(
        [
            "seattrellis",
            "validate",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--rules",
            "examples/rules.json",
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert validate_result.returncode == 0, validate_result.stderr
    assert "Validation passed." in validate_result.stdout

    history_result = subprocess.run(
        [
            "seattrellis",
            "history-report",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--history-dir",
            "examples/history",
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert history_result.returncode == 0, history_result.stderr
    assert "History report" in history_result.stdout

    pair_result = subprocess.run(
        [
            "seattrellis",
            "pair-report",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--history-dir",
            "examples/history",
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert pair_result.returncode == 0, pair_result.stderr
    assert "Pair history report" in pair_result.stdout

    snapshot_path = tmp_path / "latest.snapshot.json"
    solve_result = subprocess.run(
        [
            "seattrellis",
            "solve",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--rules",
            "examples/rules_neighbor_avoidance.json",
            "--history-dir",
            "examples/history",
            "--output",
            str(snapshot_path),
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert solve_result.returncode == 0, solve_result.stderr

    html_path = tmp_path / "seating.html"
    export_result = subprocess.run(
        [
            "seattrellis",
            "export",
            "--snapshot",
            str(snapshot_path),
            "--format",
            "html",
            "--output",
            str(html_path),
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert export_result.returncode == 0, export_result.stderr
    assert html_path.exists()


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
