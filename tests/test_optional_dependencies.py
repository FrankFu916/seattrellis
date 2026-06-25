from __future__ import annotations

import builtins

from typer.testing import CliRunner

import seattrellis.solver.cp_sat as cp_sat
from seattrellis import cli
from seattrellis.exporters import export_snapshot
from seattrellis.io.json_files import write_json_model
from seattrellis.io.project import write_project
from seattrellis.io.students import read_students
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.solver import solve_seating


def test_missing_excel_extra_for_import_is_friendly(monkeypatch, tmp_path) -> None:
    _block_import(monkeypatch, "openpyxl")
    xlsx_path = tmp_path / "students.xlsx"
    xlsx_path.write_bytes(b"not read because openpyxl is missing")

    try:
        read_students(xlsx_path)
    except MissingOptionalDependencyError as exc:
        message = str(exc)
    else:
        raise AssertionError("Expected MissingOptionalDependencyError")

    assert "Excel import requires the excel extra." in message
    assert 'python -m pip install "seattrellis[excel]"' in message


def test_missing_excel_extra_for_export_is_friendly(monkeypatch, tmp_path) -> None:
    _block_import(monkeypatch, "openpyxl")

    try:
        export_snapshot(_snapshot(), "excel", tmp_path / "seating.xlsx")
    except MissingOptionalDependencyError as exc:
        message = str(exc)
    else:
        raise AssertionError("Expected MissingOptionalDependencyError")

    assert "Excel export requires the excel extra." in message
    assert 'python -m pip install -e ".[excel]"' in message


def test_missing_image_extra_for_png_export_is_friendly(monkeypatch, tmp_path) -> None:
    _block_import(monkeypatch, "PIL")

    try:
        export_snapshot(_snapshot(), "png", tmp_path / "seating.png")
    except MissingOptionalDependencyError as exc:
        message = str(exc)
    else:
        raise AssertionError("Expected MissingOptionalDependencyError")

    assert "PNG export requires the image extra." in message
    assert 'python -m pip install "seattrellis[image]"' in message


def test_cli_missing_image_extra_exits_without_traceback(monkeypatch, tmp_path) -> None:
    _block_import(monkeypatch, "PIL")
    snapshot_path = write_json_model(_snapshot(), tmp_path / "snapshot.json")

    result = CliRunner().invoke(
        cli.app,
        ["export", "--snapshot", str(snapshot_path), "--format", "png", "--output", str(tmp_path / "seating.png")],
    )

    assert result.exit_code == 1
    assert "PNG export requires the image extra." in result.output
    assert "Traceback" not in result.output


def test_project_export_preserves_optional_dependency_errors(monkeypatch, tmp_path) -> None:
    _block_import(monkeypatch, "PIL")
    project_path = write_project(
        SeatTrellisProject(
            students="students.csv",
            layout="classroom.json",
            rules="rules.json",
        ),
        tmp_path / "project.seattrellis.json",
    )
    snapshot_path = write_json_model(_snapshot(), tmp_path / "snapshot.json")

    result = CliRunner().invoke(
        cli.app,
        [
            "project-export",
            "--project",
            str(project_path),
            "--snapshot",
            str(snapshot_path),
            "--format",
            "png",
            "--output",
            str(tmp_path / "seating.png"),
        ],
    )

    assert result.exit_code == 1
    assert "PNG export requires the image extra." in result.output
    assert "Traceback" not in result.output


def test_missing_solver_extra_when_ortools_is_requested_is_friendly(monkeypatch) -> None:
    _block_import(monkeypatch, "ortools")
    monkeypatch.setenv("SEATTRELLIS_USE_ORTOOLS", "1")
    monkeypatch.setattr(cp_sat, "cp_model", None)
    monkeypatch.setattr(cp_sat, "_cp_model_unavailable", False)

    students = [Student(student_id="A", name="Student A")]
    layout = ClassroomLayout(
        layout_id="one-seat",
        name="One Seat",
        seats=[SeatNode(seat_id="A1", row=1, col=1)],
    )

    try:
        solve_seating(students, layout, RuleSet())
    except MissingOptionalDependencyError as exc:
        message = str(exc)
    else:
        raise AssertionError("Expected MissingOptionalDependencyError")

    assert "OR-Tools solver requires the solver extra." in message
    assert 'python -m pip install "seattrellis[solver]"' in message


def _snapshot() -> SeatingSnapshot:
    layout = ClassroomLayout(
        layout_id="room",
        name="Fictional Room",
        seats=[SeatNode(seat_id="A1", row=1, col=1)],
    )
    return SeatingSnapshot(
        layout=layout,
        rules=RuleSet(),
        students=[Student(student_id="STU001", name="Student001")],
        assignments=[SeatAssignment(student_key="STU001", student_name="Student001", seat_id="A1")],
        solver_status="FEASIBLE",
        seed=42,
    )


def _block_import(monkeypatch, blocked_root: str) -> None:
    original_import = builtins.__import__

    def guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == blocked_root or name.startswith(f"{blocked_root}."):
            raise ImportError(f"No module named {blocked_root}")
        return original_import(name, globals, locals, fromlist, level)

    monkeypatch.setattr(builtins, "__import__", guarded_import)
