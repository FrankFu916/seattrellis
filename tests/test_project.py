from __future__ import annotations

import json
import subprocess
import sys

import pytest

from seattrellis import cli
from seattrellis.io.json_files import InputFileError, load_candidate_set, load_snapshot
from seattrellis.io.project import load_project, resolve_project_paths
from seattrellis.io.project import write_project
from seattrellis.models.project import SeatTrellisProject


def test_project_model_minimum_and_defaults() -> None:
    project = SeatTrellisProject(
        students="students.csv",
        layout="classroom.json",
        rules="rules.json",
    )

    assert project.kind == "seattrellis_project"
    assert project.schema_version == 1
    assert project.name == "SeatTrellis Project"
    assert project.history_dir is None
    assert project.outputs_dir == "outputs"
    assert project.default_candidates == 5
    assert project.default_candidate == "recommended"
    assert project.default_export_format == "html"


def test_project_model_rejects_missing_fields_wrong_kind_and_absolute_paths() -> None:
    with pytest.raises(ValueError, match="students"):
        SeatTrellisProject(layout="classroom.json", rules="rules.json")
    with pytest.raises(ValueError, match="unexpected value"):
        SeatTrellisProject(
            kind="other",
            students="students.csv",
            layout="classroom.json",
            rules="rules.json",
        )
    with pytest.raises(ValueError, match="relative path"):
        SeatTrellisProject(
            students="/private/students.csv",
            layout="classroom.json",
            rules="rules.json",
        )
    with pytest.raises(ValueError, match="relative path"):
        SeatTrellisProject(
            students=r"C:\private\students.csv",
            layout="classroom.json",
            rules="rules.json",
        )


def test_project_load_resolves_paths_and_creates_outputs(tmp_path) -> None:
    project_dir = tmp_path / "class-project"
    project_dir.mkdir()
    for filename in ("students.csv", "classroom.json", "rules.json"):
        (project_dir / filename).write_text("placeholder", encoding="utf-8")
    project_path = project_dir / "project.seattrellis.json"
    project_path.write_text(
        json.dumps(
            {
                "kind": "seattrellis_project",
                "schema_version": 1,
                "students": "students.csv",
                "layout": "classroom.json",
                "rules": "rules.json",
            }
        ),
        encoding="utf-8",
    )

    project = load_project(project_path)
    paths = resolve_project_paths(
        project,
        project_path,
        require_inputs=True,
        create_outputs=True,
    )

    assert paths.students == (project_dir / "students.csv").resolve()
    assert paths.layout == (project_dir / "classroom.json").resolve()
    assert paths.rules == (project_dir / "rules.json").resolve()
    assert paths.outputs_dir == (project_dir / "outputs").resolve()
    assert paths.outputs_dir.is_dir()


def test_project_path_errors_name_the_referenced_field(tmp_path) -> None:
    project_path = tmp_path / "project.seattrellis.json"
    project = SeatTrellisProject(
        students="missing.csv",
        layout="missing-layout.json",
        rules="missing-rules.json",
        history_dir="missing-history",
    )
    write_project(project, project_path)

    with pytest.raises(InputFileError, match='Project reference "students" not found'):
        resolve_project_paths(project, project_path, require_inputs=True)

    (tmp_path / "missing.csv").write_text("student_id,name\nS1,Student001\n", encoding="utf-8")
    (tmp_path / "missing-layout.json").write_text("{}", encoding="utf-8")
    (tmp_path / "missing-rules.json").write_text("{}", encoding="utf-8")
    with pytest.raises(InputFileError, match='Project reference "history_dir" directory not found'):
        resolve_project_paths(project, project_path, require_inputs=True, require_history=True)


def test_project_cli_init_info_validate_solve_and_export(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    project_path = paths["project"]

    init_path = tmp_path / "custom" / "project.seattrellis.json"
    init_result = subprocess.run(
        [
            "seattrellis",
            "project-init",
            "--project",
            str(init_path),
            "--name",
            "Fictional Project",
            "--students",
            "../examples/students.csv",
            "--layout",
            "../examples/classroom.json",
            "--rules",
            "../examples/rules.json",
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert init_result.returncode == 0, init_result.stderr
    assert init_path.exists()

    info_result = subprocess.run(
        ["seattrellis", "project-info", "--project", str(project_path)],
        check=False,
        text=True,
        capture_output=True,
    )
    assert info_result.returncode == 0, info_result.stderr
    assert "Project: Demo Class" in info_result.stdout
    assert "students.csv" in info_result.stdout
    assert "[exists]" in info_result.stdout

    validate_result = subprocess.run(
        ["seattrellis", "project-validate", "--project", str(project_path)],
        check=False,
        text=True,
        capture_output=True,
    )
    assert validate_result.returncode == 0, validate_result.stderr
    assert "Validation passed." in validate_result.stdout

    single_path = tmp_path / "outputs" / "project.snapshot.json"
    single_result = subprocess.run(
        [
            "seattrellis",
            "project-solve",
            "--project",
            str(project_path),
            "--candidates",
            "1",
            "--output",
            str(single_path),
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert single_result.returncode == 0, single_result.stderr
    assert len(load_snapshot(single_path).assignments) == 8

    candidates_path = tmp_path / "outputs" / "project.candidates.json"
    report_path = tmp_path / "outputs" / "project-plan-report.json"
    multi_result = subprocess.run(
        [
            "seattrellis",
            "project-solve",
            "--project",
            str(project_path),
            "--candidates",
            "3",
            "--output",
            str(candidates_path),
            "--report",
            str(report_path),
        ],
        check=False,
        text=True,
        capture_output=True,
    )
    assert multi_result.returncode == 0, multi_result.stderr
    assert len(load_candidate_set(candidates_path).candidates) == 3
    assert report_path.exists()

    html_path = tmp_path / "outputs" / "project-recommended.html"
    export_result = subprocess.run(
        [
            "seattrellis",
            "project-export",
            "--project",
            str(project_path),
            "--snapshot",
            str(candidates_path),
            "--candidate",
            "recommended",
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


def test_project_export_uses_latest_default_output_and_handles_snapshot(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    project_path = paths["project"]
    snapshot_path, _summary = cli.project_solve(
        project_path=project_path,
        candidate_count=1,
    )

    html_path = cli.project_export(project_path=project_path)

    assert snapshot_path.name == "latest.snapshot.json"
    assert html_path.name == "seating.html"
    assert html_path.exists()


def test_project_info_argparse_fallback(monkeypatch, capsys, tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    monkeypatch.setattr(
        sys,
        "argv",
        ["seattrellis", "project-info", "--project", str(paths["project"])],
    )

    cli._run_argparse()

    output = capsys.readouterr().out
    assert "Project: Demo Class" in output
    assert "students.csv" in output
