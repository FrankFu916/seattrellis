from __future__ import annotations

import json
import subprocess

import pytest

from seattrellis import cli
from seattrellis.io.json_files import load_candidate_set, load_snapshot


def test_cli_helpers_init_solve_and_export(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    snapshot_path = cli.solve(
        students_path=paths["students_csv"],
        layout_path=paths["layout"],
        rules_path=paths["rules"],
        output_path=tmp_path / "outputs" / "latest.snapshot.json",
    )
    html_path = cli.export(snapshot_path=snapshot_path, output_format="html", output_path=tmp_path / "outputs" / "seating.html")

    snapshot = load_snapshot(snapshot_path)
    assert len(snapshot.assignments) == 8
    assert html_path.exists()


def test_cli_edit_operations_file_runs_and_precedes_inline_operations(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    snapshot_path = cli.solve(
        students_path=paths["students_csv"],
        layout_path=paths["layout"],
        rules_path=paths["rules"],
        output_path=tmp_path / "outputs" / "latest.snapshot.json",
    )
    operations_path = tmp_path / "outputs" / "manual-operations.json"
    operations_path.write_text(
        json.dumps(
            {
                "operations": [
                    {
                        "kind": "swap_students",
                        "payload": {
                            "first_student": "STU001",
                            "second_student": "STU002",
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )

    result = subprocess.run(
        [
            "seattrellis",
            "edit",
            "--snapshot",
            str(snapshot_path),
            "--operations-file",
            str(operations_path),
            "--operation",
            "lock-seat:R4C3",
            "--output",
            "outputs/edited-from-file.snapshot.json",
        ],
        cwd=tmp_path,
        check=False,
        text=True,
        capture_output=True,
    )

    assert result.returncode == 0, result.stderr or result.stdout
    edited = load_snapshot(tmp_path / "outputs" / "edited-from-file.snapshot.json")
    operations = edited.metadata["manual_edit"]["operations"]
    assert [operation["kind"] for operation in operations] == [
        "swap_students",
        "lock_seat",
    ]


def test_cli_edit_operations_file_accepts_a_list_and_rejects_bad_payloads(tmp_path) -> None:
    operations_path = tmp_path / "operations.json"
    operations_path.write_text(
        json.dumps(
            [
                {
                    "kind": "unseat",
                    "payload": {"student_key": "STU001"},
                }
            ]
        ),
        encoding="utf-8",
    )

    operations = cli._parse_edit_operations([], operations_file=operations_path)

    assert operations[0].kind == "unseat_student"
    assert operations[0].payload == {"student_key": "STU001"}

    operations_path.write_text(
        json.dumps(
            {
                "operations": [
                    {"kind": "unseat_student", "payload": ["STU001"]}
                ]
            }
        ),
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="payload object"):
        cli._parse_edit_operations([], operations_file=operations_path)


def test_cli_parses_inline_and_file_backed_batch_moves(tmp_path) -> None:
    inline = cli._parse_edit_operations(
        ["batch-move:STU001=R1C2,STU002=R1C1"]
    )

    assert inline == [
        cli.EditingOperation(
            kind="batch_move",
            payload={
                "moves": [
                    {"student_key": "STU001", "seat_id": "R1C2"},
                    {"student_key": "STU002", "seat_id": "R1C1"},
                ]
            },
        )
    ]

    operations_path = tmp_path / "batch-operations.json"
    operations_path.write_text(
        json.dumps(
            {
                "operations": [
                    {
                        "kind": "batch_move",
                        "payload": {
                            "moves": [
                                {
                                    "student_key": "STU001",
                                    "seat_id": "R1C2",
                                },
                                {
                                    "student_key": "STU002",
                                    "seat_id": "R1C1",
                                },
                            ]
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )

    from_file = cli._parse_edit_operations([], operations_file=operations_path)

    assert from_file == inline


@pytest.mark.parametrize(
    "value",
    [
        "batch-move:",
        "batch-move:STU001",
        "batch-move:STU001=R1C2,STU002",
    ],
)
def test_cli_rejects_malformed_inline_batch_move(value) -> None:
    with pytest.raises(ValueError, match="operation|batch move item"):
        cli._parse_edit_operations([value])


def test_readme_quick_start_commands_run(tmp_path) -> None:
    commands = [
        ["seattrellis", "--help"],
        ["seattrellis", "init-demo"],
        ["seattrellis", "presets", "list"],
        ["seattrellis", "presets", "show", "daily"],
        [
            "seattrellis",
            "presets",
            "export",
            "daily",
            "--output",
            "outputs/daily.rules.json",
        ],
        [
            "seattrellis",
            "validate",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--preset",
            "daily",
            "--history-dir",
            "examples/history",
        ],
        [
            "seattrellis",
            "solve",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--preset",
            "daily",
            "--history-dir",
            "examples/history",
            "--output",
            "outputs/daily.snapshot.json",
        ],
        ["seattrellis", "project-info", "--project", "examples/project.seattrellis.json"],
        ["seattrellis", "project-validate", "--project", "examples/project.seattrellis.json"],
        [
            "seattrellis",
            "project-solve",
            "--project",
            "examples/project.seattrellis.json",
            "--candidates",
            "3",
            "--output",
            "outputs/project.candidates.json",
            "--report",
            "outputs/project-plan-report.json",
        ],
        [
            "seattrellis",
            "project-export",
            "--project",
            "examples/project.seattrellis.json",
            "--snapshot",
            "outputs/project.candidates.json",
            "--candidate",
            "recommended",
            "--format",
            "html",
            "--output",
            "outputs/project-recommended.html",
        ],
        [
            "seattrellis",
            "project-edit",
            "--project",
            "examples/project.seattrellis.json",
            "--snapshot",
            "outputs/project.candidates.json",
            "--candidate",
            "recommended",
            "--operation",
            "swap:STU001:STU002",
            "--output",
            "outputs/project-edited.snapshot.json",
        ],
        [
            "seattrellis",
            "project-export",
            "--project",
            "examples/project.seattrellis.json",
            "--snapshot",
            "outputs/project-edited.snapshot.json",
            "--format",
            "html",
            "--output",
            "outputs/project-edited.html",
        ],
        [
            "seattrellis",
            "project-repair",
            "--project",
            "examples/project.seattrellis.json",
            "--snapshot",
            "outputs/project-edited.snapshot.json",
            "--affected-student",
            "STU001",
            "--affected-student",
            "STU002",
            "--backend",
            "fallback",
            "--time-limit",
            "1",
            "--output",
            "outputs/project-repaired.snapshot.json",
        ],
        [
            "seattrellis",
            "project-export",
            "--project",
            "examples/project.seattrellis.json",
            "--snapshot",
            "outputs/project-repaired.snapshot.json",
            "--format",
            "html",
            "--output",
            "outputs/project-repaired.html",
        ],
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
            "outputs/neighbor-aware.snapshot.json",
        ],
        [
            "seattrellis",
            "solve",
            "--students",
            "examples/students.csv",
            "--layout",
            "examples/classroom.json",
            "--rules",
            "examples/rules_multi_candidate.json",
            "--history-dir",
            "examples/history",
            "--candidates",
            "5",
            "--output",
            "outputs/candidates.json",
            "--report",
            "outputs/plan-report.json",
        ],
        [
            "seattrellis",
            "edit",
            "--snapshot",
            "outputs/candidates.json",
            "--candidate",
            "recommended",
            "--operation",
            "swap:STU001:STU002",
            "--output",
            "outputs/recommended-edited.snapshot.json",
        ],
        [
            "seattrellis",
            "export",
            "--snapshot",
            "outputs/candidates.json",
            "--candidate",
            "recommended",
            "--format",
            "html",
            "--output",
            "outputs/recommended.html",
        ],
        [
            "seattrellis",
            "export",
            "--snapshot",
            "outputs/candidates.json",
            "--candidate",
            "recommended",
            "--format",
            "print-html",
            "--template",
            "teacher",
            "--hide-score",
            "--hide-notes",
            "--hide-special-needs",
            "--hide-height",
            "--hide-vision",
            "--anonymize",
            "--orientation",
            "landscape",
            "--page-scale",
            "0.8",
            "--locale",
            "en",
            "--output",
            "outputs/private-print.html",
        ],
        [
            "seattrellis",
            "export",
            "--snapshot",
            "outputs/neighbor-aware.snapshot.json",
            "--format",
            "html",
            "--output",
            "outputs/neighbor-aware.html",
        ],
        [
            "seattrellis",
            "edit",
            "--snapshot",
            "outputs/neighbor-aware.snapshot.json",
            "--operation",
            "swap:STU001:STU002",
            "--operation",
            "lock-seat:R4C3",
            "--output",
            "outputs/neighbor-aware-edited.snapshot.json",
        ],
        [
            "seattrellis",
            "export",
            "--snapshot",
            "outputs/neighbor-aware-edited.snapshot.json",
            "--format",
            "html",
            "--output",
            "outputs/neighbor-aware-edited.html",
        ],
        [
            "seattrellis",
            "repair",
            "--snapshot",
            "outputs/neighbor-aware-edited.snapshot.json",
            "--affected-student",
            "STU001",
            "--affected-student",
            "STU002",
            "--history-dir",
            "examples/history",
            "--backend",
            "fallback",
            "--time-limit",
            "1",
            "--output",
            "outputs/neighbor-aware-repaired.snapshot.json",
        ],
        [
            "seattrellis",
            "export",
            "--snapshot",
            "outputs/neighbor-aware-repaired.snapshot.json",
            "--format",
            "html",
            "--output",
            "outputs/neighbor-aware-repaired.html",
        ],
        [
            "seattrellis",
            "solve",
            "--students",
            "examples/students.xlsx",
            "--layout",
            "examples/classroom.json",
            "--rules",
            "examples/rules.json",
            "--history-dir",
            "examples/history",
        ],
        ["seattrellis", "export", "--snapshot", "outputs/latest.snapshot.json", "--format", "excel"],
        ["seattrellis", "export", "--snapshot", "outputs/latest.snapshot.json", "--format", "png"],
        ["seattrellis", "export", "--snapshot", "outputs/latest.snapshot.json", "--format", "html"],
    ]

    for command in commands:
        result = subprocess.run(command, cwd=tmp_path, check=False, text=True, capture_output=True)
        assert result.returncode == 0, result.stderr or result.stdout

    assert (tmp_path / "outputs" / "latest.snapshot.json").exists()
    assert (tmp_path / "outputs" / "seating.xlsx").exists()
    assert (tmp_path / "outputs" / "seating.png").exists()
    assert (tmp_path / "outputs" / "seating.html").exists()
    assert (tmp_path / "outputs" / "neighbor-aware.snapshot.json").exists()
    assert (tmp_path / "outputs" / "neighbor-aware.html").exists()
    assert (tmp_path / "outputs" / "neighbor-aware-edited.snapshot.json").exists()
    assert (tmp_path / "outputs" / "neighbor-aware-edited.html").exists()
    assert (tmp_path / "outputs" / "neighbor-aware-repaired.snapshot.json").exists()
    assert (tmp_path / "outputs" / "neighbor-aware-repaired.html").exists()
    edited_snapshot = load_snapshot(
        tmp_path / "outputs" / "neighbor-aware-edited.snapshot.json"
    )
    edited_by_student = {
        assignment.student_key: assignment.seat_id
        for assignment in edited_snapshot.assignments
    }
    original_snapshot = load_snapshot(tmp_path / "outputs" / "neighbor-aware.snapshot.json")
    original_by_student = {
        assignment.student_key: assignment.seat_id
        for assignment in original_snapshot.assignments
    }
    assert edited_by_student["STU001"] == original_by_student["STU002"]
    assert edited_by_student["STU002"] == original_by_student["STU001"]
    repaired_snapshot = load_snapshot(
        tmp_path / "outputs" / "neighbor-aware-repaired.snapshot.json"
    )
    assert repaired_snapshot.metadata["repair"]["history_count"] == 3
    assert repaired_snapshot.metadata["lock_state"]["locked_seats"] == ["R4C3"]
    assert (tmp_path / "outputs" / "candidates.json").exists()
    assert (tmp_path / "outputs" / "plan-report.json").exists()
    assert (tmp_path / "outputs" / "recommended-edited.snapshot.json").exists()
    recommended_edited = load_snapshot(
        tmp_path / "outputs" / "recommended-edited.snapshot.json"
    )
    assert recommended_edited.metadata["candidate"]["candidate_id"]
    assert recommended_edited.metadata["manual_edit"]["operation_count"] == 1
    assert (tmp_path / "outputs" / "recommended.html").exists()
    private_print = (tmp_path / "outputs" / "private-print.html").read_text(
        encoding="utf-8"
    )
    assert '<html lang="en">' in private_print
    assert "A4 landscape" in private_print
    assert "Student 01" in private_print
    assert "Teacher information" in private_print
    assert "Student001" not in private_print
    assert (tmp_path / "outputs" / "daily.rules.json").exists()
    assert (tmp_path / "outputs" / "daily.snapshot.json").exists()
    assert (tmp_path / "outputs" / "project.candidates.json").exists()
    assert (tmp_path / "outputs" / "project-plan-report.json").exists()
    assert (tmp_path / "outputs" / "project-recommended.html").exists()
    assert (tmp_path / "outputs" / "project-edited.snapshot.json").exists()
    assert (tmp_path / "outputs" / "project-edited.html").exists()
    assert (tmp_path / "outputs" / "project-repaired.snapshot.json").exists()
    assert (tmp_path / "outputs" / "project-repaired.html").exists()
    project_edited = load_snapshot(tmp_path / "outputs" / "project-edited.snapshot.json")
    assert project_edited.metadata["candidate"]["candidate_id"]
    assert project_edited.metadata["manual_edit"]["operation_count"] == 1
    project_repaired = load_snapshot(
        tmp_path / "outputs" / "project-repaired.snapshot.json"
    )
    assert project_repaired.metadata["repair"]["history_count"] == 3
    assert len(load_candidate_set(tmp_path / "outputs" / "candidates.json").candidates) == 5


def test_cli_reports_friendly_missing_file_error(tmp_path) -> None:
    result = subprocess.run(
        [
            "seattrellis",
            "solve",
            "--students",
            "missing.csv",
            "--layout",
            "examples/classroom.json",
            "--rules",
            "examples/rules.json",
        ],
        cwd=tmp_path,
        check=False,
        text=True,
        capture_output=True,
    )

    assert result.returncode == 1
    assert "Student file not found" in result.stderr
    assert "Traceback" not in result.stderr
