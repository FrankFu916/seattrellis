from __future__ import annotations

import builtins
import importlib
import py_compile
import sys

import pytest

from seattrellis import cli
import seattrellis.web.workflow as workflow
from seattrellis.io.json_files import InputFileError
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.optional import MissingOptionalDependencyError


def test_web_workflow_generates_candidates_with_preset_overlay_and_history(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        rules_path="examples/rules_multi_candidate.json",
        preset_name="daily",
        history_dir="examples/history",
        output_dir=tmp_path,
        candidate_count=3,
    )

    assert isinstance(result.artifact, CandidateSet)
    assert result.report is not None
    assert result.report_path is not None
    assert result.report_path.exists()
    assert result.report.recommended_candidate_id == result.artifact.recommended_candidate_id
    assert len(result.artifact.candidates) == 3

    recommended = workflow.selected_candidate(result)
    assert recommended is not None
    assert recommended.hard_constraints_satisfied is True
    assert recommended.score.breakdown.hard_constraint_summary.satisfied is True
    assert workflow.candidate_summary_rows(result.artifact)[0]["total_score"] is not None

    assignments = {
        assignment.student_key: assignment.seat_id
        for assignment in recommended.snapshot.assignments
    }
    assert assignments["STU001"] == "R1C1"


def test_web_workflow_keeps_single_snapshot_path(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path,
        candidate_count=1,
    )

    assert isinstance(result.artifact, SeatingSnapshot)
    assert result.artifact_path.name == "seattrellis.snapshot.json"
    assert result.report is None
    assert len(workflow.assignment_rows(result.artifact)) == 8


def test_web_workflow_requires_rules_or_preset(tmp_path) -> None:
    with pytest.raises(InputFileError, match="Provide --rules, --preset, or both"):
        workflow.solve_for_web(
            students_path="examples/students.csv",
            layout_path="examples/classroom.json",
            output_dir=tmp_path,
        )


def test_web_export_uses_recommended_candidate(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        rules_path="examples/rules_multi_candidate.json",
        history_dir="examples/history",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )

    html_path = workflow.export_for_web(
        result,
        output_format="html",
        output_dir=tmp_path / "exports",
    )

    assert html_path.exists()
    assert result.artifact.recommended_candidate_id in html_path.read_text(encoding="utf-8")


def test_web_export_missing_image_extra_is_friendly(monkeypatch, tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=1,
    )
    _block_import(monkeypatch, "PIL")

    with pytest.raises(MissingOptionalDependencyError, match="PNG export requires the image extra"):
        workflow.export_for_web(
            result,
            output_format="png",
            output_dir=tmp_path / "exports",
        )


def test_project_web_workflow_info_validate_solve_and_export(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    project_path = paths["project"]

    info = workflow.project_info_for_web(project_path=project_path)
    validation = workflow.project_validate_for_web(project_path=project_path)
    result = workflow.project_solve_for_web(
        project_path=project_path,
        candidate_count=3,
    )
    html_path = workflow.project_export_for_web(
        result,
        project_path=project_path,
        output_format="html",
        output_dir=tmp_path / "exports",
    )

    assert "Project: Demo Class" in info
    assert "Validation passed." in validation
    assert isinstance(result.artifact, CandidateSet)
    assert result.artifact_path == project_path.parent / "outputs" / "latest.candidates.json"
    assert result.report_path == project_path.parent / "outputs" / "latest.plan-report.json"
    assert result.report is not None
    assert len(result.artifact.candidates) == 3
    assert html_path.exists()
    assert result.artifact.recommended_candidate_id in html_path.read_text(encoding="utf-8")


def test_project_web_workflow_uses_project_default_candidates(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)

    result = workflow.project_solve_for_web(project_path=paths["project"])

    assert isinstance(result.artifact, CandidateSet)
    assert len(result.artifact.candidates) == 5


def test_project_web_workflow_can_override_to_single_snapshot(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)

    result = workflow.project_solve_for_web(
        project_path=paths["project"],
        candidate_count=1,
    )

    assert isinstance(result.artifact, SeatingSnapshot)
    assert result.artifact_path == paths["project"].parent / "outputs" / "latest.snapshot.json"
    assert result.report is None


def test_web_workflow_module_does_not_import_streamlit(monkeypatch) -> None:
    _block_import(monkeypatch, "streamlit")

    importlib.reload(workflow)


def test_streamlit_app_compiles() -> None:
    py_compile.compile("src/seattrellis/web/app.py", doraise=True)


def test_streamlit_app_smoke() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)

    assert not app.exception
    assert [title.value for title in app.title] == ["SeatTrellis"]
    assert [tab.label for tab in app.tabs] == ["快速排座", "Project workspace"]


def _block_import(monkeypatch, package_name: str) -> None:
    for module_name in list(sys.modules):
        if module_name == package_name or module_name.startswith(f"{package_name}."):
            monkeypatch.delitem(sys.modules, module_name, raising=False)
    original_import = builtins.__import__

    def blocked_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == package_name or name.startswith(f"{package_name}."):
            raise ImportError(f"blocked {package_name}")
        return original_import(name, globals, locals, fromlist, level)

    monkeypatch.setattr(builtins, "__import__", blocked_import)
