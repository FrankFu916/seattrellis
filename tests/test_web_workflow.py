from __future__ import annotations

import builtins
import importlib
import py_compile
import subprocess
import sys
from pathlib import Path

import pytest

from seattrellis import cli
from seattrellis.editing import EditingOperation
import seattrellis.web.workflow as workflow
from seattrellis.io.json_files import InputFileError, load_layout, load_snapshot
from seattrellis.io.students import read_students
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.service_types import ExportRequest, PageOptions, PrivacyOptions
from seattrellis.web.keys import (
    QUICK_CANDIDATE_COUNT_INPUT,
    QUICK_EXPORT_ALL_CANDIDATES_CHECKBOX,
    QUICK_EXPORT_FORMAT_SELECT,
    QUICK_GENERATE_BUTTON,
    QUICK_LOAD_DEMO_BUTTON,
    QUICK_REPAIR_BUTTON,
    QUICK_REDO_BUTTON,
    QUICK_SWAP_BUTTON,
    QUICK_UNDO_BUTTON,
)


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


def test_web_workflow_repairs_selected_candidate_with_locks(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="daily",
        history_dir="examples/history",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    selected = workflow.selected_snapshot(result)
    locked_student = selected.assignments[0].student_key
    locked_seat = selected.assignments[0].seat_id

    repaired = workflow.repair_for_web(
        result,
        output_dir=tmp_path / "repair",
        locked_students=[locked_student],
        history_dir="examples/history",
        backend="fallback",
    )

    assert isinstance(repaired.artifact, SeatingSnapshot)
    assert repaired.artifact_path.name == "seattrellis.repaired.snapshot.json"
    assert repaired.artifact.metadata["repair"]["history_count"] == 3
    assert repaired.artifact.metadata["lock_state"]["locked_students"] == [
        locked_student
    ]
    assignments = {
        item.student_key: item.seat_id for item in repaired.artifact.assignments
    }
    assert assignments[locked_student] == locked_seat
    assert repaired.artifact.metadata["source_candidate"]["candidate_id"]


def test_web_editing_draft_replays_undo_and_redo(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    original = workflow.selected_snapshot(result)
    first, second = [item.student_key for item in original.assignments[:2]]
    original_seats = {
        item.student_key: item.seat_id for item in original.assignments
    }
    draft = workflow.begin_web_editing(result)

    edited = workflow.apply_web_edit(
        draft,
        EditingOperation(
            kind="swap_students",
            payload={"first_student": first, "second_student": second},
        ),
        output_dir=tmp_path / "edit",
    )

    assert edited.can_undo is True
    assert edited.can_redo is False
    edited_snapshot = workflow.selected_snapshot(edited.current_result)
    edited_seats = {
        item.student_key: item.seat_id for item in edited_snapshot.assignments
    }
    assert edited_seats[first] == original_seats[second]
    assert edited_seats[second] == original_seats[first]
    assert edited_snapshot.metadata["manual_edit"]["operation_count"] == 1

    undone = workflow.undo_web_edit(edited, output_dir=tmp_path / "edit")
    assert undone.current_result is result
    assert undone.can_undo is False
    assert undone.can_redo is True

    redone = workflow.redo_web_edit(undone, output_dir=tmp_path / "edit")
    assert redone.can_undo is True
    assert redone.can_redo is False
    redone_snapshot = workflow.selected_snapshot(redone.current_result)
    assert {
        item.student_key: item.seat_id for item in redone_snapshot.assignments
    } == edited_seats


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


def test_web_export_applies_shared_privacy_and_page_options(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    request = ExportRequest(
        output_format="print-html",
        template="teacher",
        privacy=PrivacyOptions(
            hide_scores=True,
            hide_notes=True,
            hide_special_needs=True,
            anonymize=True,
            show_height=False,
            show_vision=False,
        ),
        page=PageOptions(orientation="landscape", scale=0.8),
        locale="en",
    )

    output = workflow.export_for_web(
        result,
        output_format="print-html",
        output_dir=tmp_path / "exports",
        request=request,
    )
    html = output.read_text(encoding="utf-8")

    assert output.name == "seating.print.html"
    assert '<html lang="en">' in html
    assert "A4 landscape" in html
    assert "Student 01" in html
    assert "Teacher information" in html
    assert result.artifact.candidates[0].snapshot.students[0].name not in html


def test_web_pdf_export_runs_in_isolated_subprocess(monkeypatch, tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    request = ExportRequest(
        output_format="pdf",
        template="teacher",
        privacy=PrivacyOptions(
            hide_scores=True,
            hide_notes=True,
            hide_special_needs=True,
            anonymize=True,
            show_height=False,
            show_vision=False,
        ),
        page=PageOptions(orientation="landscape", scale=0.9),
        locale="en",
    )
    captured: dict[str, object] = {}

    def fake_run(cmd, **kwargs):
        captured["cmd"] = cmd
        captured["kwargs"] = kwargs
        output = Path(cmd[cmd.index("--output") + 1])
        output.write_bytes(b"%PDF-1.7\n")
        return subprocess.CompletedProcess(cmd, 0, stdout="ok", stderr="")

    monkeypatch.setattr(workflow.subprocess, "run", fake_run)

    output = workflow.export_for_web(
        result,
        output_format="pdf",
        output_dir=tmp_path / "exports",
        candidate_id="recommended",
        request=request,
    )

    cmd = captured["cmd"]
    assert output.read_bytes().startswith(b"%PDF")
    assert cmd[:4] == [sys.executable, "-m", "seattrellis.cli", "export"]
    assert ["--candidate", "recommended"] == cmd[
        cmd.index("--candidate") : cmd.index("--candidate") + 2
    ]
    assert "--hide-score" in cmd
    assert "--hide-notes" in cmd
    assert "--hide-special-needs" in cmd
    assert "--hide-height" in cmd
    assert "--hide-vision" in cmd
    assert "--anonymize" in cmd
    assert captured["kwargs"]["timeout"] == 60


def test_web_pdf_export_reports_worker_crash(monkeypatch, tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=1,
    )
    request = ExportRequest(output_format="pdf")

    def fake_run(cmd, **kwargs):
        return subprocess.CompletedProcess(cmd, -5, stdout="", stderr="glib failed")

    monkeypatch.setattr(workflow.subprocess, "run", fake_run)

    with pytest.raises(MissingOptionalDependencyError, match="signal 5"):
        workflow.export_for_web(
            result,
            output_format="pdf",
            output_dir=tmp_path / "exports",
            request=request,
        )


def test_web_export_rejects_request_format_mismatch(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=1,
    )

    with pytest.raises(ValueError, match="does not match"):
        workflow.export_for_web(
            result,
            output_format="pdf",
            output_dir=tmp_path / "exports",
            request=ExportRequest(output_format="print-html"),
        )


def test_cli_and_web_export_options_produce_equivalent_output(tmp_path) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    request = ExportRequest(
        output_format="print-html",
        template="teacher",
        privacy=PrivacyOptions(
            hide_scores=True,
            hide_notes=True,
            hide_special_needs=True,
            anonymize=True,
            show_height=False,
            show_vision=False,
        ),
        page=PageOptions(orientation="landscape", scale=0.8),
        locale="en",
    )
    web_path = workflow.export_for_web(
        result,
        output_format="print-html",
        output_dir=tmp_path / "web",
        request=request,
    )
    cli_path = tmp_path / "cli" / "seating.html"
    cli_result = subprocess.run(
        [
            "seattrellis",
            "export",
            "--snapshot",
            str(result.artifact_path),
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
            str(cli_path),
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert cli_result.returncode == 0, cli_result.stderr
    assert cli_path.read_bytes() == web_path.read_bytes()


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


def test_project_web_workflow_repairs_with_project_history(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    result = workflow.project_solve_for_web(
        project_path=paths["project"],
        candidate_count=2,
    )

    repaired = workflow.project_repair_for_web(
        result,
        project_path=paths["project"],
        backend="fallback",
    )

    assert isinstance(repaired.artifact, SeatingSnapshot)
    assert repaired.artifact.metadata["repair"]["history_count"] == 3
    assert repaired.artifact_path.parent == paths["project"].parent / "outputs"


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
    assert [tab.label for tab in app.tabs] == ["快速排座", "Project 工作区"]
    assert [uploader.label for uploader in app.file_uploader][0] == "Web 配置 JSON"


def test_streamlit_demo_rules_and_history_preview() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    app.radio[0].set_value("solve")
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


def test_streamlit_results_expose_export_privacy_controls() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    app.radio[0].set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(2)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    app.radio[0].set_value("results")
    app.run(timeout=30)

    assert not app.exception
    labels = {control.label for control in app.selectbox}
    assert {"模板", "A4 方向", "导出语言", "导出格式"} <= labels
    checkbox_labels = {control.label for control in app.checkbox}
    assert {
        "隐藏成绩",
        "隐藏备注",
        "隐藏特殊需求",
        "隐藏身高",
        "隐藏视力信息",
        "匿名化姓名",
        "导出完整候选集比较报告",
    } <= checkbox_labels
    assert any(button.label == "生成 Print HTML 导出文件" for button in app.button)
    assert any(expander.label == "🛠️ 锁定与局部重排" for expander in app.expander)
    assert _control_by_key(app.button, QUICK_REPAIR_BUTTON).label == "执行局部重排"
    multiselect_labels = {control.label for control in app.multiselect}
    assert {"受影响学生", "锁定学生当前位置", "锁定座位"} <= multiselect_labels
    assert not any("PDF export requires" in message.value for message in app.info)

    export_format = _control_by_key(app.selectbox, QUICK_EXPORT_FORMAT_SELECT)
    export_format.set_value("html")
    app.run(timeout=30)

    assert not app.exception
    assert any(
        "不会应用匿名化或隐藏字段选项" in message.value
        for message in app.info
    )

    _control_by_key(app.checkbox, QUICK_EXPORT_ALL_CANDIDATES_CHECKBOX).set_value(True)
    app.run(timeout=30)
    _control_by_key(app.button, "quick_export_prepare_html").click()
    app.run(timeout=30)

    assert not app.exception
    assert any("HTML 已生成" in message.value for message in app.success)


def test_streamlit_results_can_run_repair() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    app.radio[0].set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    app.radio[0].set_value("results")
    app.run(timeout=30)
    _control_by_key(app.selectbox, "quick_repair_backend").set_value("fallback")
    _control_by_key(app.button, QUICK_REPAIR_BUTTON).click()
    app.run(timeout=30)

    assert not app.exception
    repaired = app.session_state["result"]
    assert isinstance(repaired.artifact, SeatingSnapshot)
    assert repaired.artifact.metadata["repair"]["history_count"] == 3
    assert repaired.artifact.metadata["repair"]["solver_backend"] == "fallback"


def test_streamlit_results_can_swap_undo_and_redo() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    app.radio[0].set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    app.radio[0].set_value("results")
    app.run(timeout=30)

    _control_by_key(app.button, QUICK_SWAP_BUTTON).click()
    app.run(timeout=30)
    edited = app.session_state["result"].artifact
    assert edited.metadata["manual_edit"]["operation_count"] == 1

    _control_by_key(app.button, QUICK_UNDO_BUTTON).click()
    app.run(timeout=30)
    assert "manual_edit" not in app.session_state["result"].artifact.metadata

    _control_by_key(app.button, QUICK_REDO_BUTTON).click()
    app.run(timeout=30)
    redone = app.session_state["result"].artifact
    assert redone.metadata["manual_edit"]["operation_count"] == 1
    assert not app.exception


def test_streamlit_app_switches_to_english_without_losing_step_state() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    language = next(
        selectbox
        for selectbox in app.selectbox
        if selectbox.label == "语言 / Language"
    )
    language.set_value("English")
    app.run(timeout=10)

    assert not app.exception
    assert [title.value for title in app.title] == ["🏫 SeatTrellis"]
    assert [tab.label for tab in app.tabs] == ["Quick solve", "Project workspace"]
    assert app.radio[0].label == "Steps"
    assert app.radio[0].options == [
        "1. Load data",
        "2. Configure & solve",
        "3. Review & export",
    ]
    assert [uploader.label for uploader in app.file_uploader][0] == (
        "Web settings JSON"
    )

    app.radio[0].set_value("solve")
    app.run(timeout=10)
    assert not app.exception
    assert any(message.value.startswith("Upload both") for message in app.warning)


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


def _control_by_key(controls, key: str):
    return next(control for control in controls if control.key == key)
