from __future__ import annotations

import json

import pytest

from pydantic import ValidationError

from seattrellis.editing import EditingError
from seattrellis.editing_protocol import (
    EDITOR_PROTOCOL_VERSION,
    EditorCommandEnvelope,
    EditorProtocolConflictError,
    operation_to_domain,
)
from seattrellis.web.editor_protocol import (
    build_editor_state_for_web,
    dispatch_editor_command_for_web,
)
from seattrellis.web.workflow import (
    begin_web_editing,
    selected_snapshot,
    solve_for_web,
)


def _command(
    *,
    draft_id: str = "draft-1",
    command_id: str = "command-1",
    base_revision: int = 0,
    action: str = "apply",
    operations: list[dict[str, object]] | None = None,
) -> EditorCommandEnvelope:
    return EditorCommandEnvelope.model_validate(
        {
            "kind": "seattrellis_editor_command",
            "protocol_version": EDITOR_PROTOCOL_VERSION,
            "command_id": command_id,
            "draft_id": draft_id,
            "base_revision": base_revision,
            "action": action,
            "operations": operations or [],
        }
    )


@pytest.mark.parametrize(
    ("kind", "payload"),
    [
        (
            "swap_students",
            {"first_student": "STU001", "second_student": "STU002"},
        ),
        ("move_student", {"student_key": "STU001", "seat_id": "R1C1"}),
        (
            "batch_move",
            {
                "moves": [
                    {"student_key": "STU001", "seat_id": "R1C2"},
                    {"student_key": "STU002", "seat_id": "R1C1"},
                ]
            },
        ),
        ("seat_student", {"student_key": "STU001", "seat_id": "R1C1"}),
        ("unseat_student", {"student_key": "STU001"}),
        ("lock_student", {"student_key": "STU001"}),
        ("unlock_student", {"student_key": "STU001"}),
        ("lock_seat", {"seat_id": "R1C1"}),
        ("unlock_seat", {"seat_id": "R1C1"}),
    ],
)
def test_editor_command_parses_each_operation(
    kind: str,
    payload: dict[str, object],
) -> None:
    command = _command(operations=[{"kind": kind, "payload": payload}])

    domain = operation_to_domain(command.operations[0])

    assert domain.kind == kind
    assert domain.payload == payload


@pytest.mark.parametrize("action", ["undo", "redo"])
def test_history_commands_reject_operations(action: str) -> None:
    with pytest.raises(ValidationError, match="must not contain operations"):
        _command(
            action=action,
            operations=[
                {
                    "kind": "lock_seat",
                    "payload": {"seat_id": "R1C1"},
                }
            ],
        )

    command = _command(action=action)
    assert command.action == action
    assert command.operations == []


def test_apply_command_requires_an_operation() -> None:
    with pytest.raises(ValidationError, match="require at least one operation"):
        _command()


@pytest.mark.parametrize(
    "change",
    [
        {"kind": None},
        {"protocol_version": None},
        {"protocol_version": "2.0"},
        {"action": None},
        {"command_id": "   "},
        {"command_id": 123},
        {"draft_id": 123},
        {"base_revision": "1"},
        {"base_revision": 1.5},
        {"base_revision": True},
        {"base_revision": -1},
        {"unexpected": True},
    ],
)
def test_editor_command_rejects_invalid_envelopes(
    change: dict[str, object],
) -> None:
    payload: dict[str, object] = {
        "kind": "seattrellis_editor_command",
        "protocol_version": EDITOR_PROTOCOL_VERSION,
        "command_id": "command-1",
        "draft_id": "draft-1",
        "base_revision": 0,
        "action": "apply",
        "operations": [
            {
                "kind": "lock_student",
                "payload": {"student_key": "STU001"},
            }
        ],
    }
    if change.get("kind") is None and "kind" in change:
        payload.pop("kind")
    elif change.get("protocol_version") is None and "protocol_version" in change:
        payload.pop("protocol_version")
    else:
        payload.update(change)

    with pytest.raises(ValidationError):
        EditorCommandEnvelope.model_validate(payload)


def test_editor_command_requires_action_and_operation_kind() -> None:
    command_payload = {
        "kind": "seattrellis_editor_command",
        "protocol_version": EDITOR_PROTOCOL_VERSION,
        "command_id": "command-1",
        "draft_id": "draft-1",
        "base_revision": 0,
        "operations": [
            {
                "kind": "lock_student",
                "payload": {"student_key": "STU001"},
            }
        ],
    }
    with pytest.raises(ValidationError):
        EditorCommandEnvelope.model_validate(command_payload)

    command_payload["action"] = "apply"
    del command_payload["operations"][0]["kind"]
    with pytest.raises(ValidationError):
        EditorCommandEnvelope.model_validate(command_payload)


def test_editor_command_rejects_non_string_operation_identifiers() -> None:
    with pytest.raises(ValidationError, match="valid string|str type expected"):
        _command(
            operations=[
                {
                    "kind": "lock_student",
                    "payload": {"student_key": 123},
                }
            ]
        )


@pytest.mark.parametrize(
    "moves",
    [
        [
            {"student_key": "STU001", "seat_id": "R1C1"},
            {"student_key": "STU001", "seat_id": "R1C2"},
        ],
        [
            {"student_key": "STU001", "seat_id": "R1C1"},
            {"student_key": " STU001 ", "seat_id": "R1C2"},
        ],
        [
            {"student_key": "STU001", "seat_id": "R1C1"},
            {"student_key": "STU002", "seat_id": "R1C1"},
        ],
    ],
)
def test_batch_move_rejects_duplicate_sources_or_targets(
    moves: list[dict[str, str]],
) -> None:
    with pytest.raises(ValidationError, match="must be unique"):
        _command(
            operations=[
                {
                    "kind": "batch_move",
                    "payload": {"moves": moves},
                }
            ]
        )


def test_editor_command_limits_expanded_operation_count() -> None:
    moves = [
        {"student_key": f"STU{index:03d}", "seat_id": f"R1C{index:03d}"}
        for index in range(100)
    ]

    with pytest.raises(ValidationError, match="at most 100 expanded operations"):
        _command(
            operations=[
                {
                    "kind": "batch_move",
                    "payload": {"moves": moves},
                },
                {
                    "kind": "lock_student",
                    "payload": {"student_key": "EXTRA-STUDENT"},
                },
            ]
        )


def test_editor_state_is_minimized_for_frontend_clients(tmp_path) -> None:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    draft = begin_web_editing(result)

    state = build_editor_state_for_web(draft)
    payload = json.loads(state.model_dump_json())

    assert state.protocol_version == EDITOR_PROTOCOL_VERSION
    assert state.draft_id == draft.draft_id
    assert state.revision == 0
    assert state.candidate_id is not None
    assert len(state.students) == len(selected_snapshot(result).students)
    assert len(state.seats) == len(selected_snapshot(result).layout.seats)
    assert state.hard_constraints.satisfied is True
    assert not _all_keys(payload) & {
        "attributes",
        "gender",
        "height_cm",
        "needs",
        "notes",
        "score",
        "tags",
        "vision",
    }
    students_by_key = {student.student_key: student for student in state.students}
    for seat in state.seats:
        if seat.student_key is None:
            continue
        assert students_by_key[seat.student_key].seat_id == seat.seat_id


def test_editor_dispatch_tracks_revision_and_command_batches(tmp_path) -> None:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=2,
    )
    draft = begin_web_editing(result)
    snapshot = selected_snapshot(result)
    student_key = snapshot.assignments[0].student_key
    seat_id = snapshot.assignments[0].seat_id
    apply_command = _command(
        draft_id=draft.draft_id,
        operations=[
            {
                "kind": "lock_student",
                "payload": {"student_key": student_key},
            },
            {
                "kind": "lock_seat",
                "payload": {"seat_id": seat_id},
            },
        ],
    )

    applied = dispatch_editor_command_for_web(
        draft,
        apply_command,
        output_dir=tmp_path / "edit",
    )
    applied_state = build_editor_state_for_web(applied)

    assert applied.revision == 1
    assert applied.applied_command_ids == ("command-1",)
    assert len(applied.operation_batches) == 1
    assert len(applied.operation_batches[0]) == 2
    assert applied_state.undo_depth == 1
    assert next(
        item for item in applied_state.students if item.student_key == student_key
    ).locked
    assert next(item for item in applied_state.seats if item.seat_id == seat_id).locked

    undone = dispatch_editor_command_for_web(
        applied,
        _command(
            draft_id=draft.draft_id,
            command_id="command-2",
            base_revision=1,
            action="undo",
        ),
        output_dir=tmp_path / "edit",
    )
    undone_state = build_editor_state_for_web(undone)

    assert undone.revision == 2
    assert undone_state.undo_depth == 0
    assert undone_state.redo_depth == 1
    assert not next(
        item for item in undone_state.students if item.student_key == student_key
    ).locked
    assert not next(
        item for item in undone_state.seats if item.seat_id == seat_id
    ).locked

    redone = dispatch_editor_command_for_web(
        undone,
        _command(
            draft_id=draft.draft_id,
            command_id="command-3",
            base_revision=2,
            action="redo",
        ),
        output_dir=tmp_path / "edit",
    )

    assert redone.revision == 3
    assert build_editor_state_for_web(redone).undo_depth == 1


def test_editor_dispatch_rejects_conflicting_commands_without_writing(tmp_path) -> None:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=1,
    )
    draft = begin_web_editing(result)
    student_key = selected_snapshot(result).assignments[0].student_key
    valid = _command(
        draft_id=draft.draft_id,
        operations=[
            {
                "kind": "lock_student",
                "payload": {"student_key": student_key},
            }
        ],
    )
    applied = dispatch_editor_command_for_web(
        draft,
        valid,
        output_dir=tmp_path / "edit",
    )
    output_path = applied.current_result.artifact_path
    written = output_path.read_bytes()

    conflicts = [
        valid,
        _command(
            draft_id="different-draft",
            command_id="command-2",
            base_revision=1,
            operations=[
                {
                    "kind": "unlock_student",
                    "payload": {"student_key": student_key},
                }
            ],
        ),
        _command(
            draft_id=draft.draft_id,
            command_id="command-3",
            base_revision=0,
            operations=[
                {
                    "kind": "unlock_student",
                    "payload": {"student_key": student_key},
                }
            ],
        ),
    ]
    for command in conflicts:
        with pytest.raises(EditorProtocolConflictError):
            dispatch_editor_command_for_web(
                applied,
                command,
                output_dir=tmp_path / "edit",
            )
        assert output_path.read_bytes() == written


def test_editor_dispatch_applies_operation_batches_atomically(tmp_path) -> None:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="random",
        output_dir=tmp_path / "solve",
        candidate_count=1,
    )
    draft = begin_web_editing(result)
    student_key = selected_snapshot(result).assignments[0].student_key
    command = _command(
        draft_id=draft.draft_id,
        operations=[
            {
                "kind": "lock_student",
                "payload": {"student_key": student_key},
            },
            {
                "kind": "move_student",
                "payload": {
                    "student_key": "UNKNOWN-STUDENT",
                    "seat_id": "R1C1",
                },
            },
        ],
    )

    with pytest.raises(EditingError, match="Unknown student"):
        dispatch_editor_command_for_web(
            draft,
            command,
            output_dir=tmp_path / "edit",
        )

    assert draft.revision == 0
    assert draft.operations == ()
    assert not (tmp_path / "edit" / "seattrellis.edited.snapshot.json").exists()


def _all_keys(value: object) -> set[str]:
    if isinstance(value, dict):
        keys = set(value)
        for item in value.values():
            keys.update(_all_keys(item))
        return keys
    if isinstance(value, list):
        keys: set[str] = set()
        for item in value:
            keys.update(_all_keys(item))
        return keys
    return set()
