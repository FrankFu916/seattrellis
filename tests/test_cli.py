from __future__ import annotations

import subprocess

from seattrellis import cli
from seattrellis.io.json_files import load_snapshot


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
