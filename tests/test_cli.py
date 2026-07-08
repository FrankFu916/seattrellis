from __future__ import annotations

import subprocess

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
