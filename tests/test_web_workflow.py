from __future__ import annotations

import builtins
import importlib
import json
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
    APP_WORKSPACE_SELECT,
    PROJECT_CANDIDATE_COUNT_INPUT,
    PROJECT_EXPORT_PREFIX,
    PROJECT_MODE_RADIO,
    PROJECT_PATH_INPUT,
    PROJECT_SOLVE_BUTTON,
    PROJECT_UPLOAD_INPUT,
    PROJECT_USE_DEFAULT_CANDIDATES,
    PROJECT_VALIDATE_BUTTON,
    QUICK_CANDIDATE_COUNT_INPUT,
    QUICK_BATCH_MOVE_BUTTON,
    QUICK_BATCH_SEATS_SELECT,
    QUICK_BATCH_STUDENTS_SELECT,
    QUICK_CANVAS_MODE_SELECT,
    QUICK_CLEAR_UPLOADS_BUTTON,
    QUICK_CONFIG_UPLOAD,
    QUICK_EDIT_ACTION_SELECT,
    QUICK_EDIT_APPLY_BUTTON,
    QUICK_EXPORT_ALL_CANDIDATES_CHECKBOX,
    QUICK_EXPORT_FORMAT_SELECT,
    QUICK_EXPORT_PREFIX,
    QUICK_GENERATE_BUTTON,
    QUICK_INSPECT_HISTORY_BUTTON,
    QUICK_LAYOUT_UPLOAD,
    QUICK_LOAD_DEMO_BUTTON,
    QUICK_LOCK_SEAT_BUTTON,
    QUICK_LOCK_STUDENT_BUTTON,
    QUICK_REPAIR_BUTTON,
    QUICK_REDO_BUTTON,
    QUICK_RULES_UPLOAD,
    QUICK_STUDENTS_UPLOAD,
    QUICK_SWAP_BUTTON,
    QUICK_STEP_RADIO,
    QUICK_UNDO_BUTTON,
    TEACHER_CLASS_NAME_INPUT,
    TEACHER_GENERATE_BUTTON,
    TEACHER_GOAL_SELECT,
    TEACHER_HOME_STATUS,
    TEACHER_ROOM_AISLES_INPUT,
    TEACHER_ROOM_ROWS_INPUT,
    TEACHER_ROOM_SEATS_PER_ROW_INPUT,
    TEACHER_ROOM_TEMPLATE_SELECT,
    TEACHER_ROSTER_UPLOAD,
    TEACHER_START_OVER_BUTTON,
    export_prepare_key,
    export_prepared_state_key,
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
    assert draft.revision == 0
    assert draft.draft_id

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
    assert edited.revision == 1
    assert edited.draft_id == draft.draft_id
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
    assert undone.revision == 2

    redone = workflow.redo_web_edit(undone, output_dir=tmp_path / "edit")
    assert redone.can_undo is True
    assert redone.can_redo is False
    assert redone.revision == 3
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


def test_web_pdf_export_rejects_all_candidate_scope_before_worker(
    monkeypatch,
    tmp_path,
) -> None:
    result = workflow.solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    request = ExportRequest(output_format="pdf", candidate_scope="all")

    def unexpected_run(*_args, **_kwargs):
        raise AssertionError("PDF worker must not run for an unsupported scope")

    monkeypatch.setattr(workflow.subprocess, "run", unexpected_run)

    with pytest.raises(ValueError, match="only html and print-html"):
        workflow.export_for_web(
            result,
            output_format="pdf",
            output_dir=tmp_path / "quick-exports",
            request=request,
        )
    with pytest.raises(ValueError, match="only html and print-html"):
        workflow.project_export_for_web(
            result,
            project_path=tmp_path / "project.json",
            output_format="pdf",
            output_dir=tmp_path / "project-exports",
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
    session_dir = tmp_path / "web-session"

    info = workflow.project_info_for_web(project_path=project_path)
    validation = workflow.project_validate_for_web(project_path=project_path)
    result = workflow.project_solve_for_web(
        project_path=project_path,
        output_dir=session_dir,
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
    assert result.artifact_path == session_dir / "seattrellis.candidates.json"
    assert result.report_path == session_dir / "seattrellis.plan-report.json"
    assert result.report is not None
    assert len(result.artifact.candidates) == 3
    assert html_path.exists()
    assert result.artifact.recommended_candidate_id in html_path.read_text(encoding="utf-8")


def test_project_web_workflow_uses_project_default_candidates(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)

    result = workflow.project_solve_for_web(
        project_path=paths["project"],
        output_dir=tmp_path / "web-session",
    )

    assert isinstance(result.artifact, CandidateSet)
    assert len(result.artifact.candidates) == 5


def test_project_web_workflow_can_override_to_single_snapshot(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)

    result = workflow.project_solve_for_web(
        project_path=paths["project"],
        output_dir=tmp_path / "web-session",
        candidate_count=1,
    )

    assert isinstance(result.artifact, SeatingSnapshot)
    assert result.artifact_path == (
        tmp_path / "web-session" / "seattrellis.snapshot.json"
    )
    assert result.report is None


def test_project_web_results_are_isolated_between_sessions(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    first_session = tmp_path / "session-a"
    second_session = tmp_path / "session-b"

    first = workflow.project_solve_for_web(
        project_path=paths["project"],
        output_dir=first_session,
        candidate_count=2,
        seed=17,
    )
    first_artifact = first.artifact_path.read_bytes()
    assert first.report_path is not None
    first_report = first.report_path.read_bytes()

    second = workflow.project_solve_for_web(
        project_path=paths["project"],
        output_dir=second_session,
        candidate_count=2,
        seed=29,
    )

    assert first.artifact_path.parent == first_session
    assert second.artifact_path.parent == second_session
    assert first.report_path.parent == first_session
    assert second.report_path is not None
    assert second.report_path.parent == second_session
    assert first.artifact_path.read_bytes() == first_artifact
    assert first.report_path.read_bytes() == first_report
    assert not (
        paths["project"].parent / "outputs" / "latest.candidates.json"
    ).exists()
    assert not (
        paths["project"].parent / "outputs" / "latest.plan-report.json"
    ).exists()


def test_project_web_workflow_repairs_with_project_history(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    session_dir = tmp_path / "web-session"
    result = workflow.project_solve_for_web(
        project_path=paths["project"],
        output_dir=session_dir,
        candidate_count=2,
    )

    repaired = workflow.project_repair_for_web(
        result,
        project_path=paths["project"],
        output_dir=session_dir,
        backend="fallback",
    )

    assert isinstance(repaired.artifact, SeatingSnapshot)
    assert repaired.artifact.metadata["repair"]["history_count"] == 3
    assert repaired.artifact_path == (
        session_dir / "seattrellis.repaired.snapshot.json"
    )


def test_web_workflow_module_does_not_import_streamlit(monkeypatch) -> None:
    _block_import(monkeypatch, "streamlit")

    importlib.reload(workflow)


def test_streamlit_app_compiles() -> None:
    py_compile.compile("src/seattrellis/web/app.py", doraise=True)
    py_compile.compile("src/seattrellis/web/interactive_panels.py", doraise=True)


def test_streamlit_app_smoke() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)

    assert not app.exception
    assert [title.value for title in app.title] == ["🏫 SeatTrellis · 席序"]
    assert [tab.label for tab in app.tabs] == ["快速排座", "Project 工作区"]
    assert [uploader.label for uploader in app.file_uploader][0] == "Web 配置 JSON"


def test_teacher_workspace_survives_advanced_tools_and_can_start_over() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    _control_by_key(app.text_input, TEACHER_CLASS_NAME_INPUT).set_value("Class 7 A")
    roster_path = Path("tests/fixtures/students.csv")
    _control_by_key(app.file_uploader, TEACHER_ROSTER_UPLOAD).upload(
        roster_path.name,
        roster_path.read_bytes(),
        "text/csv",
    )
    app.run(timeout=10)

    _control_by_key(app.selectbox, TEACHER_ROOM_TEMPLATE_SELECT).set_value("custom")
    app.run(timeout=10)
    _control_by_key(app.number_input, TEACHER_ROOM_ROWS_INPUT).set_value(3)
    _control_by_key(
        app.number_input,
        TEACHER_ROOM_SEATS_PER_ROW_INPUT,
    ).set_value(4)
    app.run(timeout=10)
    _control_by_key(app.multiselect, TEACHER_ROOM_AISLES_INPUT).set_value([2])
    _control_by_key(app.radio, TEACHER_GOAL_SELECT).set_value("peer-support")
    app.run(timeout=10)
    _control_by_key(app.button, TEACHER_GENERATE_BUTTON).click()
    app.run(timeout=30)

    first_student = _control_by_key(
        app.selectbox,
        "teacher_edit_first_student",
    )
    second_student = _control_by_key(
        app.selectbox,
        "teacher_edit_second_student",
    )
    assert first_student.value != second_student.value
    _control_by_key(app.button, "teacher_swap_students").click()
    app.run(timeout=30)

    edited_snapshot = app.session_state["_teacher_result"].artifact
    assert edited_snapshot.metadata["manual_edit"]["operation_count"] == 1
    assert "result" not in app.session_state
    result_path = app.session_state["_teacher_result"].artifact_path
    signature = app.session_state["_teacher_setup_signature"]
    assert not app.exception

    _control_by_key(app.radio, APP_WORKSPACE_SELECT).set_value("advanced")
    app.run(timeout=10)
    _control_by_key(app.radio, APP_WORKSPACE_SELECT).set_value("teacher")
    app.run(timeout=10)

    assert not app.exception
    assert _control_by_key(app.text_input, TEACHER_CLASS_NAME_INPUT).value == "Class 7 A"
    assert _control_by_key(app.selectbox, TEACHER_ROOM_TEMPLATE_SELECT).value == "custom"
    assert _control_by_key(app.number_input, TEACHER_ROOM_ROWS_INPUT).value == 3
    assert (
        _control_by_key(app.number_input, TEACHER_ROOM_SEATS_PER_ROW_INPUT).value
        == 4
    )
    assert _control_by_key(app.multiselect, TEACHER_ROOM_AISLES_INPUT).value == [2]
    assert _control_by_key(app.radio, TEACHER_GOAL_SELECT).value == "peer-support"
    assert app.session_state["_teacher_setup_signature"] == signature
    assert app.session_state["_teacher_result"].artifact_path == result_path
    assert app.session_state["_teacher_roster_cache"].ready is True
    assert any(
        "原始上传文件不会保留" in message.value
        for message in app.info
        if getattr(message, "key", None) is None
    )

    _control_by_key(app.button, TEACHER_START_OVER_BUTTON).click()
    app.run(timeout=10)

    assert not app.exception
    assert _control_by_key(app.text_input, TEACHER_CLASS_NAME_INPUT).value == ""
    assert "_teacher_roster_cache" not in app.session_state
    assert "_teacher_result" not in app.session_state
    assert "_teacher_output_dir" not in app.session_state
    assert not any(
        control.key == TEACHER_ROOM_TEMPLATE_SELECT for control in app.selectbox
    )
    assert not any(control.key == TEACHER_GOAL_SELECT for control in app.radio)
    assert not any(
        control.key == TEACHER_START_OVER_BUTTON for control in app.button
    )


def test_streamlit_uploads_survive_wizard_navigation_and_invalidate_results(
    tmp_path,
) -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    for key, path, upload_name, mime_type in (
        (
            QUICK_STUDENTS_UPLOAD,
            Path("tests/fixtures/students.csv"),
            "students.csv",
            "text/csv",
        ),
        (
            QUICK_LAYOUT_UPLOAD,
            Path("tests/fixtures/classroom.json"),
            "config.json",
            "application/json",
        ),
        (
            QUICK_RULES_UPLOAD,
            Path("tests/fixtures/rules.json"),
            "config.json",
            "application/json",
        ),
    ):
        _control_by_key(app.file_uploader, key).upload(
            upload_name,
            path.read_bytes(),
            mime_type,
        )
        app.run(timeout=10)

    assert app.session_state["_qf_students"].name == "students.csv"
    assert app.session_state["_qf_layout"].name == "config.json"
    assert app.session_state["_qf_rules"].name == "config.json"

    step = _control_by_key(app.radio, QUICK_STEP_RADIO)
    step.set_value("solve")
    app.run(timeout=10)
    step = _control_by_key(app.radio, QUICK_STEP_RADIO)
    step.set_value("load")
    app.run(timeout=10)

    assert app.session_state["_qf_students"].name == "students.csv"
    assert any(
        "跨步骤保留的输入文件" in caption.value
        for caption in app.caption
    )

    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)

    assert app.session_state["result"] is not None
    assert app.session_state["result_origin"] == "quick"
    assert app.session_state["solved"] is True

    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(2)
    app.run(timeout=10)
    assert app.session_state["result"] is None
    assert app.session_state["result_origin"] is None

    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    assert app.session_state["result"] is not None

    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("load")
    app.run(timeout=10)
    original_students = Path("tests/fixtures/students.csv").read_text(
        encoding="utf-8"
    )
    updated_students = original_students.replace(
        "STU004,Student004",
        "STU004,Updated Student",
    )
    student_path = tmp_path / "students-updated.csv"
    student_path.write_text(updated_students, encoding="utf-8")
    _control_by_key(app.file_uploader, QUICK_STUDENTS_UPLOAD).upload(
        student_path.name,
        student_path.read_bytes(),
        "text/csv",
    )
    app.run(timeout=10)
    assert app.session_state["_qf_students"].name == student_path.name
    assert app.session_state["_qf_students"].getvalue() == student_path.read_bytes()
    assert app.session_state["solved"] is False
    assert app.session_state["result"] is None
    assert app.session_state["artifact_json"] is None
    assert app.session_state["report_json"] is None
    assert app.session_state["output_dir"] is None
    assert app.session_state["layout_loaded"] is None

    _control_by_key(app.button, QUICK_CLEAR_UPLOADS_BUTTON).click()
    app.run(timeout=10)
    assert app.session_state["_qf_students"] is None
    assert app.session_state["_qf_layout"] is None
    assert app.session_state["_qf_rules"] is None
    assert app.session_state["_qf_history"] is None
    app.run(timeout=10)
    assert not any(
        button.key == QUICK_CLEAR_UPLOADS_BUTTON for button in app.button
    )
    assert not app.exception


def test_streamlit_clear_uploads_preserves_restored_rules_metadata() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    config = {
        "kind": "seattrellis_web_config",
        "schema_version": 1,
        "preset_name": None,
        "rules_overlay": {"seed": 99},
        "candidate_count": 3,
        "seed": None,
        "time_limit_seconds": 3.0,
    }
    _control_by_key(app.file_uploader, QUICK_CONFIG_UPLOAD).upload(
        "settings.json",
        json.dumps(config).encode("utf-8"),
        "application/json",
    )
    app.run(timeout=10)
    students_path = Path("tests/fixtures/students.csv")
    _control_by_key(app.file_uploader, QUICK_STUDENTS_UPLOAD).upload(
        students_path.name,
        students_path.read_bytes(),
        "text/csv",
    )
    app.run(timeout=10)

    assert app.session_state["_qf_rules_data"] == {"seed": 99}
    assert app.session_state["_qf_rules_name"] == "restored.rules.json"
    _control_by_key(app.button, QUICK_CLEAR_UPLOADS_BUTTON).click()
    app.run(timeout=10)

    assert app.session_state["_qf_students"] is None
    assert app.session_state["_qf_rules_data"] == {"seed": 99}
    assert app.session_state["_qf_rules_name"] == "restored.rules.json"
    assert any(
        "restored.rules.json" in caption.value for caption in app.caption
    )
    assert not app.exception


def test_streamlit_loading_demo_invalidates_an_existing_quick_result() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    assert app.session_state["result"] is not None

    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("load")
    app.run(timeout=10)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)

    assert app.session_state["result"] is None
    assert app.session_state["result_origin"] is None
    assert app.session_state["solved"] is False
    assert not app.exception


def test_streamlit_project_upload_only_inspects_the_manifest() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.radio, PROJECT_MODE_RADIO).set_value("upload")
    app.run(timeout=10)

    manifest = {
        "kind": "seattrellis_project",
        "schema_version": 1,
        "name": "Uploaded Manifest",
        "students": "../../private/students.csv",
        "layout": "../../private/layout.json",
        "rules": "../../private/rules.json",
        "outputs_dir": "../../private/outputs",
    }
    _control_by_key(app.file_uploader, PROJECT_UPLOAD_INPUT).upload(
        "uploaded.seattrellis.json",
        json.dumps(manifest).encode("utf-8"),
        "application/json",
    )
    app.run(timeout=10)

    assert not app.exception
    assert any(
        "单独上传的清单无法取得" in message.value
        for message in app.info
    )
    assert any("Uploaded Manifest" in block.value for block in app.code)
    assert not any(
        button.key == PROJECT_SOLVE_BUTTON
        for button in app.button
    )


@pytest.mark.parametrize(
    "name,payload",
    [
        ("invalid-json.seattrellis.json", b"{"),
        (
            "missing-fields.seattrellis.json",
            json.dumps(
                {
                    "kind": "seattrellis_project",
                    "schema_version": 1,
                }
            ).encode("utf-8"),
        ),
        (
            "future-schema.seattrellis.json",
            json.dumps(
                {
                    "kind": "seattrellis_project",
                    "schema_version": 999,
                    "students": "students.csv",
                    "layout": "layout.json",
                    "rules": "rules.json",
                }
            ).encode("utf-8"),
        ),
    ],
)
def test_streamlit_project_upload_reports_invalid_manifests(
    name,
    payload,
) -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.radio, PROJECT_MODE_RADIO).set_value("upload")
    app.run(timeout=10)
    _control_by_key(app.file_uploader, PROJECT_UPLOAD_INPUT).upload(
        name,
        payload,
        "application/json",
    )
    app.run(timeout=10)

    assert app.error
    assert not app.exception
    assert not any(
        button.key == PROJECT_SOLVE_BUTTON for button in app.button
    )


def test_streamlit_project_path_reports_unexpandable_home() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.text_input, PROJECT_PATH_INPUT).set_value(
        "~seattrellis-user-that-does-not-exist/project.json"
    )
    app.run(timeout=10)

    assert app.error
    assert not app.exception
    assert not any(
        button.key == PROJECT_SOLVE_BUTTON for button in app.button
    )


def test_expand_user_path_rejects_unknown_home_on_every_platform() -> None:
    with pytest.raises(ValueError, match="Named home-directory shortcuts"):
        workflow.expand_user_path(
            "~seattrellis-user-that-does-not-exist/project.json"
        )


def test_expand_user_path_preserves_relative_paths() -> None:
    assert workflow.expand_user_path("projects/classroom.json") == Path(
        "projects/classroom.json"
    )


def test_streamlit_project_path_validates_and_solves_in_isolation(tmp_path) -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    app = _advanced_app(streamlit_testing)
    _control_by_key(app.text_input, PROJECT_PATH_INPUT).set_value(
        str(paths["project"])
    )
    app.run(timeout=10)
    _control_by_key(app.button, PROJECT_VALIDATE_BUTTON).click()
    app.run(timeout=10)

    assert any(
        message.value.startswith("Validation passed.")
        for message in app.success
    )

    _control_by_key(
        app.checkbox,
        PROJECT_USE_DEFAULT_CANDIDATES,
    ).set_value(False)
    app.run(timeout=10)
    _control_by_key(
        app.number_input,
        PROJECT_CANDIDATE_COUNT_INPUT,
    ).set_value(1)
    _control_by_key(app.button, PROJECT_SOLVE_BUTTON).click()
    app.run(timeout=30)

    result = app.session_state["result"]
    assert isinstance(result.artifact, SeatingSnapshot)
    assert result.artifact_path.parent != paths["project"].parent / "outputs"
    assert result.artifact_path.name == "seattrellis.snapshot.json"
    assert Path(app.session_state["output_dir"]) == result.artifact_path.parent
    assert app.session_state["artifact_json"] == result.artifact_path.read_bytes()
    assert app.session_state["report_json"] is None
    assert app.session_state["result_origin"] == "project"
    first_output_dir = result.artifact_path.parent

    prepared_state_key = export_prepared_state_key(PROJECT_EXPORT_PREFIX)
    _control_by_key(
        app.button,
        export_prepare_key(PROJECT_EXPORT_PREFIX, "print-html"),
    ).click()
    app.run(timeout=30)
    assert app.session_state[prepared_state_key]["data"]

    _control_by_key(
        app.number_input,
        PROJECT_CANDIDATE_COUNT_INPUT,
    ).set_value(2)
    app.run(timeout=10)
    _control_by_key(app.button, PROJECT_SOLVE_BUTTON).click()
    app.run(timeout=30)
    assert prepared_state_key not in app.session_state
    assert app.session_state["result_origin"] == "project"
    second_result = app.session_state["result"]
    assert isinstance(second_result.artifact, CandidateSet)
    assert second_result.artifact_path.parent != first_output_dir
    assert second_result.report_path is not None
    assert app.session_state["artifact_json"] == second_result.artifact_path.read_bytes()
    assert app.session_state["report_json"] == second_result.report_path.read_bytes()

    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("load")
    app.run(timeout=10)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    assert app.session_state["result_origin"] == "project"
    assert app.session_state["result"] is not None

    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=10)
    assert not any(
        selectbox.key == QUICK_EXPORT_FORMAT_SELECT
        for selectbox in app.selectbox
    )
    assert any(
        "请先在“设置与求解”中生成座位表" in message.value
        for message in app.info
    )
    assert not app.exception


def test_streamlit_demo_rules_and_history_preview() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)

    assert not app.exception
    assert [expander.label for expander in app.expander] == [
        "最终生效的 rules",
        "History 质量检查",
    ]

    _control_by_key(app.button, QUICK_INSPECT_HISTORY_BUTTON).click()
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

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(2)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
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
    _control_by_key(
        app.button,
        export_prepare_key(QUICK_EXPORT_PREFIX, "html"),
    ).click()
    app.run(timeout=30)

    assert not app.exception
    assert any("HTML 已生成" in message.value for message in app.success)


def test_streamlit_results_can_run_repair() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=30)
    prepared_state_key = export_prepared_state_key(QUICK_EXPORT_PREFIX)
    _control_by_key(
        app.button,
        export_prepare_key(QUICK_EXPORT_PREFIX, "print-html"),
    ).click()
    app.run(timeout=30)
    assert app.session_state[prepared_state_key]["data"]

    _control_by_key(app.selectbox, "quick_repair_backend").set_value("fallback")
    _control_by_key(app.button, QUICK_REPAIR_BUTTON).click()
    app.run(timeout=30)

    assert not app.exception
    repaired = app.session_state["result"]
    assert isinstance(repaired.artifact, SeatingSnapshot)
    assert repaired.artifact.metadata["repair"]["history_count"] == 3
    assert repaired.artifact.metadata["repair"]["solver_backend"] == "fallback"
    assert prepared_state_key not in app.session_state


def test_streamlit_results_can_swap_undo_and_redo() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=30)
    prepared_state_key = export_prepared_state_key(QUICK_EXPORT_PREFIX)
    _control_by_key(
        app.button,
        export_prepare_key(QUICK_EXPORT_PREFIX, "print-html"),
    ).click()
    app.run(timeout=30)
    assert app.session_state[prepared_state_key]["data"]

    _control_by_key(app.button, QUICK_SWAP_BUTTON).click()
    app.run(timeout=30)
    edited = app.session_state["result"].artifact
    assert edited.metadata["manual_edit"]["operation_count"] == 1
    assert prepared_state_key not in app.session_state

    _control_by_key(app.button, QUICK_UNDO_BUTTON).click()
    app.run(timeout=30)
    assert "manual_edit" not in app.session_state["result"].artifact.metadata

    _control_by_key(app.button, QUICK_REDO_BUTTON).click()
    app.run(timeout=30)
    redone = app.session_state["result"].artifact
    assert redone.metadata["manual_edit"]["operation_count"] == 1
    assert not app.exception


def test_streamlit_results_can_move_unseat_and_reseat_student() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=30)

    original = app.session_state["result"].artifact
    original_seats = {
        item.student_key: item.seat_id for item in original.assignments
    }
    action = _control_by_key(app.selectbox, QUICK_EDIT_ACTION_SELECT)
    action.set_value("安排未入座学生")
    app.run(timeout=30)
    assert _control_by_key(app.button, QUICK_EDIT_APPLY_BUTTON).disabled is True
    assert any(message.value == "当前没有未入座学生。" for message in app.info)
    _control_by_key(app.selectbox, QUICK_EDIT_ACTION_SELECT).set_value("移动到空座")
    app.run(timeout=30)
    _control_by_key(app.button, QUICK_EDIT_APPLY_BUTTON).click()
    app.run(timeout=30)
    moved = app.session_state["result"].artifact
    moved_seats = {item.student_key: item.seat_id for item in moved.assignments}
    assert len(moved.assignments) == 8
    assert moved_seats != original_seats
    assert moved.metadata["manual_edit"]["operation_count"] == 1

    _control_by_key(app.selectbox, QUICK_EDIT_ACTION_SELECT).set_value("移出座位")
    app.run(timeout=30)
    _control_by_key(app.button, QUICK_EDIT_APPLY_BUTTON).click()
    app.run(timeout=30)

    unseated = app.session_state["result"].artifact
    assert len(unseated.assignments) == 7
    assert len(unseated.metadata["manual_edit"]["unseated_students"]) == 1
    assert unseated.metadata["manual_edit"]["operation_count"] == 2

    _control_by_key(app.selectbox, QUICK_EDIT_ACTION_SELECT).set_value(
        "安排未入座学生"
    )
    app.run(timeout=30)
    _control_by_key(app.button, QUICK_EDIT_APPLY_BUTTON).click()
    app.run(timeout=30)

    reseated = app.session_state["result"].artifact
    assert len(reseated.assignments) == 8
    assert reseated.metadata["manual_edit"]["unseated_students"] == []
    assert reseated.metadata["manual_edit"]["operation_count"] == 3
    assert not app.exception


def test_streamlit_locks_survive_undo_redo_and_repair() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=30)

    original = app.session_state["result"].artifact
    student_key = sorted(item.student_key for item in original.assignments)[0]
    original_seat = next(
        item.seat_id
        for item in original.assignments
        if item.student_key == student_key
    )

    _control_by_key(app.button, QUICK_LOCK_STUDENT_BUTTON).click()
    app.run(timeout=30)
    locked = app.session_state["result"].artifact
    assert locked.metadata["lock_state"]["locked_students"] == [student_key]
    assert _control_by_key(app.button, QUICK_LOCK_STUDENT_BUTTON).label == "解锁学生"
    assert _control_by_key(
        app.button,
        f"quick_canvas_seat_{original_seat}",
    ).disabled is True

    _control_by_key(app.button, QUICK_UNDO_BUTTON).click()
    app.run(timeout=30)
    undone = app.session_state["result"].artifact
    assert undone.metadata.get("lock_state", {}).get("locked_students", []) == []

    _control_by_key(app.button, QUICK_REDO_BUTTON).click()
    app.run(timeout=30)
    redone = app.session_state["result"].artifact
    assert redone.metadata["lock_state"]["locked_students"] == [student_key]

    _control_by_key(app.selectbox, "quick_repair_backend").set_value("fallback")
    _control_by_key(app.button, QUICK_REPAIR_BUTTON).click()
    app.run(timeout=30)
    # AppTest exposes Streamlit's nested rerun as a separate test cycle. A
    # browser completes it automatically before accepting the next click.
    app.run(timeout=30)
    repaired = app.session_state["result"].artifact
    repaired_seat = next(
        item.seat_id
        for item in repaired.assignments
        if item.student_key == student_key
    )
    assert repaired_seat == original_seat
    assert repaired.metadata["repair"]["locked_students"] == [student_key]
    assert repaired.metadata["lock_state"]["locked_students"] == [student_key]

    _control_by_key(app.button, QUICK_LOCK_STUDENT_BUTTON).click()
    app.run(timeout=30)
    unlocked = app.session_state["result"].artifact
    assert unlocked.metadata["lock_state"]["locked_students"] == []

    _control_by_key(app.button, QUICK_LOCK_SEAT_BUTTON).click()
    app.run(timeout=30)
    seat_locked = app.session_state["result"].artifact
    assert len(seat_locked.metadata["lock_state"]["locked_seats"]) == 1
    assert _control_by_key(app.button, QUICK_LOCK_SEAT_BUTTON).label == "解锁座位"

    _control_by_key(app.button, QUICK_LOCK_SEAT_BUTTON).click()
    app.run(timeout=30)
    seat_unlocked = app.session_state["result"].artifact
    assert seat_unlocked.metadata["lock_state"]["locked_seats"] == []
    assert not app.exception


def test_streamlit_batch_move_is_one_undoable_operation() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=30)

    original = app.session_state["result"].artifact
    original_seats = {
        item.student_key: item.seat_id for item in original.assignments
    }
    students = sorted(original_seats)[:2]
    assigned_seats = set(original_seats.values())
    empty_seats = sorted(
        seat.seat_id
        for seat in original.layout.seats
        if seat.enabled and seat.seat_id not in assigned_seats
    )[:2]

    _control_by_key(app.multiselect, QUICK_BATCH_STUDENTS_SELECT).set_value(
        students
    )
    app.run(timeout=30)
    _control_by_key(app.multiselect, QUICK_BATCH_SEATS_SELECT).set_value(
        empty_seats
    )
    app.run(timeout=30)
    _control_by_key(app.button, QUICK_BATCH_MOVE_BUTTON).click()
    app.run(timeout=30)

    moved = app.session_state["result"].artifact
    moved_seats = {item.student_key: item.seat_id for item in moved.assignments}
    assert [moved_seats[student] for student in students] == empty_seats
    assert moved.metadata["manual_edit"]["operation_count"] == 1
    assert moved.metadata["manual_edit"]["operations"][0]["kind"] == "batch_move"

    _control_by_key(app.button, QUICK_UNDO_BUTTON).click()
    app.run(timeout=30)
    undone = app.session_state["result"].artifact
    assert {
        item.student_key: item.seat_id for item in undone.assignments
    } == original_seats

    _control_by_key(app.button, QUICK_REDO_BUTTON).click()
    app.run(timeout=30)
    redone = app.session_state["result"].artifact
    assert {
        item.student_key: item.seat_id for item in redone.assignments
    } == moved_seats
    assert not app.exception


def test_streamlit_seat_canvas_moves_swaps_and_toggles_lock() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.button, QUICK_LOAD_DEMO_BUTTON).click()
    app.run(timeout=10)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
    app.run(timeout=10)
    _control_by_key(app.number_input, QUICK_CANDIDATE_COUNT_INPUT).set_value(1)
    _control_by_key(app.button, QUICK_GENERATE_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("results")
    app.run(timeout=30)

    original = app.session_state["result"].artifact
    assignments = sorted(
        original.assignments,
        key=lambda item: item.student_key,
    )
    first, second = assignments[:2]
    assigned_seats = {item.seat_id for item in original.assignments}
    empty_seat = next(
        seat.seat_id
        for seat in sorted(original.layout.seats, key=lambda item: item.seat_id)
        if seat.enabled and seat.seat_id not in assigned_seats
    )

    _control_by_key(
        app.button,
        f"quick_canvas_seat_{first.seat_id}",
    ).click()
    app.run(timeout=30)
    _control_by_key(
        app.button,
        f"quick_canvas_seat_{empty_seat}",
    ).click()
    app.run(timeout=30)
    moved = app.session_state["result"].artifact
    assert next(
        item.seat_id for item in moved.assignments
        if item.student_key == first.student_key
    ) == empty_seat
    assert moved.metadata["manual_edit"]["operations"][0]["kind"] == "move_student"

    _control_by_key(app.button, QUICK_UNDO_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(
        app.button,
        f"quick_canvas_seat_{first.seat_id}",
    ).click()
    app.run(timeout=30)
    _control_by_key(
        app.button,
        f"quick_canvas_seat_{second.seat_id}",
    ).click()
    app.run(timeout=30)
    swapped = app.session_state["result"].artifact
    swapped_seats = {
        item.student_key: item.seat_id for item in swapped.assignments
    }
    assert swapped_seats[first.student_key] == second.seat_id
    assert swapped_seats[second.student_key] == first.seat_id
    assert swapped.metadata["manual_edit"]["operations"][0]["kind"] == (
        "swap_students"
    )

    _control_by_key(app.button, QUICK_UNDO_BUTTON).click()
    app.run(timeout=30)
    _control_by_key(app.selectbox, QUICK_CANVAS_MODE_SELECT).set_value(
        "锁定 / 解锁座位"
    )
    app.run(timeout=30)
    _control_by_key(
        app.button,
        f"quick_canvas_seat_{empty_seat}",
    ).click()
    app.run(timeout=30)
    seat_locked = app.session_state["result"].artifact
    assert seat_locked.metadata["lock_state"]["locked_seats"] == [empty_seat]

    _control_by_key(
        app.button,
        f"quick_canvas_seat_{empty_seat}",
    ).click()
    app.run(timeout=30)
    seat_unlocked = app.session_state["result"].artifact
    assert seat_unlocked.metadata["lock_state"]["locked_seats"] == []
    assert not app.exception


def test_streamlit_app_switches_to_english_without_losing_step_state() -> None:
    streamlit_testing = pytest.importorskip("streamlit.testing.v1")

    app = _advanced_app(streamlit_testing)
    _control_by_key(app.radio, QUICK_STEP_RADIO).set_value("solve")
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
    step_radio = _control_by_key(app.radio, QUICK_STEP_RADIO)
    assert step_radio.label == "Steps"
    assert step_radio.options == [
        "1. Load data",
        "2. Configure & solve",
        "3. Review & export",
    ]
    assert step_radio.value == "solve"
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


def _advanced_app(streamlit_testing):
    """Open the legacy Quick and Project workspaces for their AppTest coverage."""

    app = streamlit_testing.AppTest.from_file("src/seattrellis/web/app.py")
    app.run(timeout=10)
    _control_by_key(app.radio, APP_WORKSPACE_SELECT).set_value("advanced")
    app.run(timeout=10)
    return app
