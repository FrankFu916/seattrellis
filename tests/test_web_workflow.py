from __future__ import annotations

import builtins
import importlib
import py_compile
import sys

import pytest

from seattrellis import cli
import seattrellis.web.workflow as workflow
from seattrellis.io.json_files import InputFileError, load_layout, load_snapshot
from seattrellis.io.students import read_students
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
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


def test_rules_preview_shows_preset_with_overlay_applied() -> None:
    overlay = workflow.parse_rules_overlay(
        b'{"soft":{"randomize":{"enabled":false}}}'
    )

    preview = workflow.build_rules_preview(
        preset_name="daily",
        rules_data=overlay,
    )

    assert preview.preset_name == "daily"
    assert preview.overlay_applied is True
    assert preview.rules.soft.randomize.enabled is False
    assert preview.rules.soft.fair_rotation.enabled is True
    assert b'"randomize"' in preview.json_bytes


def test_rules_overlay_parser_reports_invalid_json() -> None:
    with pytest.raises(InputFileError, match="Invalid rules JSON"):
        workflow.parse_rules_overlay(b'{"soft":')


def test_history_quality_accepts_consistent_demo_history() -> None:
    students = read_students("examples/students.csv")
    layout = load_layout("examples/classroom.json")
    snapshots = [
        load_snapshot(f"examples/history/week{week}.snapshot.json")
        for week in range(1, 4)
    ]

    report = workflow.analyze_history_quality(students, layout, snapshots)

    assert report.snapshot_count == 3
    assert report.student_count == 8
    assert report.average_coverage_percent == 100.0
    assert report.complete_snapshot_count == 3
    assert report.warnings == ()
    assert all(row["layout_matches"] for row in report.rows())


def test_history_quality_reports_missing_students_and_unknown_references() -> None:
    students = read_students("examples/students.csv")
    layout = load_layout("examples/classroom.json")
    snapshot = load_snapshot("examples/history/week1.snapshot.json")
    changed_assignments = list(snapshot.assignments[:-1])
    changed_assignments.append(
        SeatAssignment(
            student_key="OLD-STUDENT",
            student_name="Former Student",
            seat_id="OLD-SEAT",
        )
    )
    old_layout_seats = list(snapshot.layout.seats)
    old_layout_seats[0] = old_layout_seats[0].copy(
        update={"enabled": not old_layout_seats[0].enabled}
    )
    changed_snapshot = snapshot.copy(
        update={
            "assignments": changed_assignments,
            "layout": snapshot.layout.copy(
                update={"layout_id": "old-layout", "seats": old_layout_seats}
            ),
        }
    )

    report = workflow.analyze_history_quality(
        students,
        layout,
        [changed_snapshot],
    )
    quality = report.snapshots[0]

    assert quality.covered_students == 7
    assert len(quality.missing_students) == 1
    assert quality.unknown_students == ("OLD-STUDENT",)
    assert quality.unknown_seats == ("OLD-SEAT",)
    assert quality.layout_matches is False
    assert report.complete_snapshot_count == 0
    assert len(report.warnings) == 4


@pytest.mark.parametrize("time_limit", [float("nan"), float("inf"), float("-inf")])
def test_web_workflow_rejects_non_finite_time_limit(tmp_path, time_limit) -> None:
    with pytest.raises(ValueError, match="finite number"):
        workflow.solve_for_web(
            students_path="examples/students.csv",
            layout_path="examples/classroom.json",
            preset_name="random",
            output_dir=tmp_path,
            time_limit_seconds=time_limit,
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
    assert [title.value for title in app.title] == ["🏫 SeatTrellis · 席序"]
    assert [tab.label for tab in app.tabs] == ["快速排座", "Project workspace"]
    assert [uploader.label for uploader in app.file_uploader][0] == "Web 配置 JSON"


def test_streamlit_demo_rules_and_history_preview() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    next(button for button in app.button if button.label == "🚀 一键加载 Demo").click()
    app.run(timeout=10)
    app.radio[0].set_value("2. 设置 & 求解")
    app.run(timeout=10)

    assert not app.exception
    assert [expander.label for expander in app.expander] == [
        "最终生效的 rules",
        "History 质量检查",
    ]

    next(button for button in app.button if button.key == "inspect_history").click()
    app.run(timeout=10)

    assert not app.exception
    metrics = {metric.label: metric.value for metric in app.metric}
    assert metrics == {
        "Snapshot 数量": "3",
        "平均学生覆盖率": "100.0%",
        "完全匹配": "3/3",
    }
    assert any(
        message.value == "历史记录与当前学生名单和 layout 一致。"
        for message in app.success
    )


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
