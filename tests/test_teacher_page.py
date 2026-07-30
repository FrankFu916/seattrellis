from __future__ import annotations

import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from seattrellis.application.roster_import import import_roster_records
from seattrellis.application.room_templates import RoomTemplate, build_standard_room
from seattrellis.models.candidate import CandidateSet
from seattrellis.web.teacher_page import (
    CachedRosterUpload,
    TeacherSetupSignature,
    _central_aisle,
    _resolve_uploaded_roster,
    _room_identity,
    _store_edited_result,
    build_teacher_setup_signature,
    clear_teacher_workspace_state,
    invalidate_teacher_results,
    load_cached_roster_upload,
    prepare_teacher_print_export,
    prepared_export_state_key,
    remember_teacher_input,
    restore_teacher_input,
    roster_upload_fingerprint,
    teacher_export_filename,
    teacher_input_state_key,
)
from seattrellis.web.keys import (
    TEACHER_CLASS_NAME_INPUT,
    TEACHER_GOAL_SELECT,
    TEACHER_ROSTER_UPLOAD,
    TEACHER_ROOM_AISLES_INPUT,
    TEACHER_START_OVER_BUTTON,
)
from seattrellis.web.workflow import WebSolveResult


@pytest.fixture(scope="module", autouse=True)
def _release_teacher_modules_after_unit_tests():
    """Let Streamlit AppTest import the page after workflow reload tests."""

    yield
    sys.modules.pop("seattrellis.web.teacher_page", None)
    sys.modules.pop("seattrellis.web.class_adapter", None)


def test_roster_upload_cache_parses_each_distinct_payload_once() -> None:
    calls: list[tuple[str, bytes]] = []

    def importer(filename: str, content: bytes):
        calls.append((filename, content))
        return import_roster_records(
            [{"name": "Alice"}, {"name": "Bob"}],
            source_name=filename,
        )

    first, changed = load_cached_roster_upload(
        "class.csv",
        b"name\nAlice\nBob\n",
        importer=importer,
    )
    repeated, changed_again = load_cached_roster_upload(
        "class.csv",
        b"name\nAlice\nBob\n",
        first,
        importer=importer,
    )
    replacement, replacement_changed = load_cached_roster_upload(
        "class.csv",
        b"name\nAlice\nBob\nCara\n",
        repeated,
        importer=importer,
    )

    assert changed is True
    assert changed_again is False
    assert replacement_changed is True
    assert repeated is first
    assert replacement.fingerprint != first.fingerprint
    assert len(calls) == 2
    assert first.roster is not None
    assert first.roster.summary.student_count == 2
    assert not hasattr(first, "content")


def test_roster_upload_cache_retains_a_safe_failure_without_retrying() -> None:
    calls = 0

    def importer(filename: str, content: bytes):
        nonlocal calls
        calls += 1
        raise ValueError("The roster needs a name column.")

    cached, changed = load_cached_roster_upload(
        "class.csv",
        b"unknown\nvalue\n",
        importer=importer,
    )
    repeated, changed_again = load_cached_roster_upload(
        "class.csv",
        b"unknown\nvalue\n",
        cached,
        importer=importer,
    )

    assert changed is True
    assert changed_again is False
    assert repeated is cached
    assert cached.ready is False
    assert cached.error_message == "The roster needs a name column."
    assert calls == 1


def test_missing_uploader_value_keeps_the_parsed_roster_cache() -> None:
    state: dict[str, object] = {}
    st = SimpleNamespace(session_state=state)
    upload = SimpleNamespace(
        name="class.csv",
        getvalue=lambda: "name\nAlice\nBob\n".encode(),
    )

    cached, changed = _resolve_uploaded_roster(st, upload)
    restored, changed_after_return = _resolve_uploaded_roster(st, None)

    assert changed is True
    assert cached is not None and cached.ready
    assert restored is cached
    assert changed_after_return is False


def test_roster_fingerprint_includes_the_display_name_and_validates_types() -> None:
    content = b"name\nAlice\n"

    assert roster_upload_fingerprint("a.csv", content) != roster_upload_fingerprint(
        "b.csv", content
    )
    with pytest.raises(TypeError, match="filename"):
        roster_upload_fingerprint(1, content)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="content"):
        roster_upload_fingerprint("a.csv", "text")  # type: ignore[arg-type]


def test_teacher_inputs_restore_from_durable_non_file_values() -> None:
    session: dict[str, object] = {}

    remember_teacher_input(session, TEACHER_CLASS_NAME_INPUT, "Class 7 A")
    remember_teacher_input(session, TEACHER_GOAL_SELECT, "peer-support")
    aisles = [2, 5]
    remember_teacher_input(session, TEACHER_ROOM_AISLES_INPUT, aisles)
    aisles.append(7)

    restore_teacher_input(session, TEACHER_CLASS_NAME_INPUT)
    restore_teacher_input(session, TEACHER_GOAL_SELECT)
    restore_teacher_input(session, TEACHER_ROOM_AISLES_INPUT)

    assert session[TEACHER_CLASS_NAME_INPUT] == "Class 7 A"
    assert session[TEACHER_GOAL_SELECT] == "peer-support"
    assert session[TEACHER_ROOM_AISLES_INPUT] == [2, 5]
    assert session[TEACHER_ROOM_AISLES_INPUT] is not session[
        teacher_input_state_key(TEACHER_ROOM_AISLES_INPUT)
    ]
    with pytest.raises(ValueError, match="not durable"):
        remember_teacher_input(session, TEACHER_ROSTER_UPLOAD, b"private")


def test_start_over_clears_only_the_teacher_workspace() -> None:
    session: dict[str, object] = {
        "_teacher_roster_cache": CachedRosterUpload("digest", None),
        "_teacher_result": object(),
        "_teacher_editing_draft": object(),
        TEACHER_CLASS_NAME_INPUT: "Class 7 A",
        teacher_input_state_key(TEACHER_CLASS_NAME_INPUT): "Class 7 A",
        TEACHER_ROSTER_UPLOAD: b"browser-owned bytes",
        TEACHER_START_OVER_BUTTON: True,
        "result": object(),
        "_quick_editing_draft": object(),
    }

    clear_teacher_workspace_state(session)

    assert TEACHER_START_OVER_BUTTON in session
    assert not any(
        key.startswith("teacher_") or key.startswith("_teacher_")
        for key in session
        if key != TEACHER_START_OVER_BUTTON
    )
    assert "result" in session
    assert "_quick_editing_draft" in session


def test_changed_setup_invalidates_only_teacher_derived_state() -> None:
    previous = TeacherSetupSignature("Class 1", "old", "standard-30", "daily-rotation")
    current = build_teacher_setup_signature(
        class_name="  Class 1  ",
        roster_fingerprint="new",
        room_template_id="standard-30",
        goal_id="daily-rotation",
    )
    session: dict[str, object] = {
        "_teacher_setup_signature": previous,
        "_teacher_result": object(),
        "_teacher_output_dir": "/tmp/teacher",
        "teacher_candidate_selector": "candidate-2",
        prepared_export_state_key("public"): object(),
        prepared_export_state_key("teacher"): object(),
        "_teacher_editing_draft": object(),
        "_teacher_canvas_source_seat": "R1C1",
        "teacher_edit_first_student": "STU001",
        teacher_input_state_key(TEACHER_CLASS_NAME_INPUT): "Class 1",
        "_teacher_roster_cache": CachedRosterUpload("new", None),
        "quick_result": object(),
    }

    assert invalidate_teacher_results(session, current) is True
    assert session["_teacher_setup_signature"] == current
    assert "_teacher_result" not in session
    assert "_teacher_output_dir" not in session
    assert "teacher_candidate_selector" not in session
    assert prepared_export_state_key("public") not in session
    assert prepared_export_state_key("teacher") not in session
    assert "_teacher_editing_draft" not in session
    assert "_teacher_canvas_source_seat" not in session
    assert "teacher_edit_first_student" not in session
    assert session[teacher_input_state_key(TEACHER_CLASS_NAME_INPUT)] == "Class 1"
    assert "_teacher_roster_cache" in session
    assert "quick_result" in session
    assert invalidate_teacher_results(session, current) is False


def test_room_identity_and_central_aisle_cover_built_in_and_custom_rooms() -> None:
    template = RoomTemplate("standard-12", 3, 4, (2,), "Small room")
    custom = build_standard_room(
        4,
        7,
        aisles_after=(3,),
        layout_id="custom-4x7-aisles-3",
    )

    assert _room_identity(template) == "standard-12"
    assert _room_identity(custom) == "custom-4x7-aisles-3"
    assert _central_aisle(3) == []
    assert _central_aisle(8) == [4]


def test_prepare_print_export_uses_template_privacy_and_reads_bytes(
    tmp_path: Path,
) -> None:
    artifact = CandidateSet.parse_obj(
        {
            "candidates": [
                {
                    "candidate_id": "candidate-1",
                    "snapshot": _snapshot_payload(),
                    "score": _score_payload(),
                    "hard_constraints_satisfied": True,
                }
            ],
            "recommended_candidate_id": "candidate-1",
        }
    )
    artifact_path = tmp_path / "candidates.json"
    artifact_path.write_text("{}", encoding="utf-8")
    result = WebSolveResult(artifact_path=artifact_path, artifact=artifact)
    calls: list[dict[str, object]] = []

    def exporter(result_arg, **kwargs):
        calls.append({"result": result_arg, **kwargs})
        path = Path(kwargs["output_dir"]) / "seating.print.html"
        path.parent.mkdir(parents=True)
        path.write_bytes(b"<html>prepared</html>")
        return path

    prepared = prepare_teacher_print_export(
        result,
        output_dir=tmp_path / "exports",
        candidate_id="recommended",
        template="public",
        class_name='Class / 7: "A"',
        locale="en",
        exporter=exporter,
    )

    assert prepared.data == b"<html>prepared</html>"
    assert prepared.file_name == "Class-7-A-public.html"
    assert prepared.signature[1:] == ("recommended", "public", "en")
    assert len(calls) == 1
    request = calls[0]["request"]
    assert request.output_format == "print-html"
    assert request.template == "public"
    assert request.page.orientation == "landscape"
    assert request.resolved_privacy.hide_scores is True
    assert request.resolved_privacy.hide_special_needs is True
    assert calls[0]["candidate_id"] == "recommended"


def test_edited_teacher_result_replaces_only_teacher_plan_state(tmp_path: Path) -> None:
    artifact = CandidateSet.parse_obj(
        {
            "candidates": [
                {
                    "candidate_id": "candidate-1",
                    "snapshot": _snapshot_payload(),
                    "score": _score_payload(),
                    "hard_constraints_satisfied": True,
                }
            ],
            "recommended_candidate_id": "candidate-1",
        }
    )
    artifact_path = tmp_path / "edited.json"
    artifact_path.write_text("{}", encoding="utf-8")
    result = WebSolveResult(artifact_path=artifact_path, artifact=artifact)
    session: dict[str, object] = {
        "teacher_candidate_selector": "candidate-2",
        prepared_export_state_key("public"): object(),
        prepared_export_state_key("teacher"): object(),
        "result": "advanced result",
    }

    _store_edited_result(session, result)

    assert session["_teacher_result"] is result
    assert "teacher_candidate_selector" not in session
    assert prepared_export_state_key("public") not in session
    assert prepared_export_state_key("teacher") not in session
    assert session["result"] == "advanced result"


@pytest.mark.parametrize(
    ("class_name", "template", "expected"),
    [
        ("高一 3 班", "teacher", "高一-3-班-teacher.html"),
        ("  ", "public", "classroom-public.html"),
    ],
)
def test_teacher_export_filename_is_portable(
    class_name: str,
    template: str,
    expected: str,
) -> None:
    assert teacher_export_filename(class_name, template) == expected


def _snapshot_payload() -> dict[str, object]:
    return {
        "students": [{"name": "Alice"}],
        "layout": {
            "layout_id": "room",
            "name": "Room",
            "seats": [{"seat_id": "R1C1", "row": 1, "col": 1}],
        },
        "rules": {},
        "assignments": [
            {
                "student_key": "Alice",
                "student_name": "Alice",
                "seat_id": "R1C1",
            }
        ],
        "solver_status": "FEASIBLE",
    }


def _score_payload() -> dict[str, object]:
    unavailable = {"status": "not_available"}
    return {
        "total": 100.0,
        "breakdown": {
            "hard_constraint_summary": {
                "satisfied": True,
                "checked_rule_count": 0,
                "violation_count": 0,
            },
            "fair_rotation_score": unavailable,
            "avoid_recent_neighbors_score": unavailable,
            "score_balance_score": unavailable,
            "height_preference_score": unavailable,
            "vision_preference_score": unavailable,
            "diversity_score": unavailable,
            "stability_score": unavailable,
        },
    }
