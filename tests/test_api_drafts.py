from __future__ import annotations

import pytest

from seattrellis.api.drafts import EditorDraftNotFoundError, EditorDraftStore
from seattrellis.editing import EditingError
from seattrellis.editing_protocol import (
    EDITOR_PROTOCOL_VERSION,
    EditorCommandEnvelope,
    EditorProtocolConflictError,
)
from seattrellis.models.candidate import (
    CandidatePlan,
    CandidateSet,
    HardConstraintSummary,
    PlanScore,
    ScoreBreakdown,
    ScoreDimension,
)
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student


def _score_dimension() -> ScoreDimension:
    return ScoreDimension(status="not_available")


def _candidate_set() -> CandidateSet:
    students = [
        Student(student_id="PRIVATE-1", name="Alice", score=99, notes="secret"),
        Student(student_id="PRIVATE-2", name="Bob", needs=["private need"]),
    ]
    layout = ClassroomLayout(
        layout_id="room",
        name="Room",
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
        ],
    )
    snapshot = SeatingSnapshot(
        students=students,
        layout=layout,
        rules=RuleSet(),
        assignments=[
            SeatAssignment(student_key="PRIVATE-1", student_name="Alice", seat_id="A1"),
            SeatAssignment(student_key="PRIVATE-2", student_name="Bob", seat_id="A2"),
        ],
        solver_status="feasible",
    )
    dimension = _score_dimension()
    score = PlanScore(
        total=80,
        breakdown=ScoreBreakdown(
            fair_rotation_score=dimension,
            avoid_recent_neighbors_score=dimension,
            score_balance_score=dimension,
            height_preference_score=dimension,
            vision_preference_score=dimension,
            diversity_score=dimension,
            stability_score=dimension,
            hard_constraint_summary=HardConstraintSummary(satisfied=True),
        ),
    )
    return CandidateSet(
        candidates=[
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snapshot,
                score=score,
                hard_constraints_satisfied=True,
            )
        ],
        recommended_candidate_id="candidate_01",
    )


def _command(state, command_id: str, action: str, operations=()):
    return EditorCommandEnvelope.model_validate(
        {
            "kind": "seattrellis_editor_command",
            "protocol_version": EDITOR_PROTOCOL_VERSION,
            "command_id": command_id,
            "draft_id": state.draft_id,
            "base_revision": state.revision,
            "action": action,
            "operations": list(operations),
        }
    )


def test_draft_state_is_minimized_and_does_not_serialize_private_fields() -> None:
    state = EditorDraftStore().create(_candidate_set())

    serialized = state.model_dump_json()
    assert state.candidate_id == "candidate_01"
    assert [student.display_name for student in state.students] == ["Alice", "Bob"]
    # The editor contract carries only opaque keys and display names.  No
    # sensitive student field name or value may serialize.  Field names are
    # checked instead of the numeric score so a random hex draft id can never
    # make the privacy assertion flaky.  (Field names must not be substrings
    # of protocol field names such as "revision".)
    for private_key in ("score", "notes", "needs"):
        assert private_key not in serialized
    assert "secret" not in serialized
    assert "private need" not in serialized


def test_editor_commands_are_atomic_and_undo_redo_at_command_level() -> None:
    store = EditorDraftStore()
    state = store.create(_candidate_set())
    swapped = store.dispatch(
        state.draft_id,
        _command(
            state,
            "swap-and-lock",
            "apply",
            [
                {
                    "kind": "swap_students",
                    "payload": {
                        "first_student": "PRIVATE-1",
                        "second_student": "PRIVATE-2",
                    },
                },
                {"kind": "lock_seat", "payload": {"seat_id": "A2"}},
            ],
        ),
    )

    assert swapped.revision == 1
    assert swapped.undo_depth == 1
    assert next(item for item in swapped.students if item.student_key == "PRIVATE-1").seat_id == "A2"
    assert next(item for item in swapped.seats if item.seat_id == "A2").locked

    undone = store.dispatch(
        state.draft_id,
        _command(swapped, "undo-1", "undo"),
    )
    assert undone.revision == 2
    assert next(item for item in undone.students if item.student_key == "PRIVATE-1").seat_id == "A1"
    assert not next(item for item in undone.seats if item.seat_id == "A2").locked

    redone = store.dispatch(
        state.draft_id,
        _command(undone, "redo-1", "redo"),
    )
    assert redone.revision == 3
    assert next(item for item in redone.students if item.student_key == "PRIVATE-1").seat_id == "A2"


def test_failed_multi_operation_command_rolls_back_all_changes() -> None:
    store = EditorDraftStore()
    state = store.create(_candidate_set())
    command = _command(
        state,
        "invalid-batch",
        "apply",
        [
            {"kind": "lock_seat", "payload": {"seat_id": "A1"}},
            {
                "kind": "move_student",
                "payload": {"student_key": "PRIVATE-2", "seat_id": "MISSING"},
            },
        ],
    )

    with pytest.raises(EditingError):
        store.dispatch(state.draft_id, command)

    unchanged = store.state(state.draft_id)
    assert unchanged.revision == 0
    assert unchanged.undo_depth == 0
    assert not next(item for item in unchanged.seats if item.seat_id == "A1").locked


def test_stale_duplicate_and_wrong_draft_commands_are_rejected() -> None:
    store = EditorDraftStore()
    state = store.create(_candidate_set())
    command = _command(
        state,
        "lock-1",
        "apply",
        [{"kind": "lock_seat", "payload": {"seat_id": "A1"}}],
    )
    store.dispatch(state.draft_id, command)

    with pytest.raises(EditorProtocolConflictError, match="already"):
        store.dispatch(state.draft_id, command)
    stale = command.copy(update={"command_id": "lock-2"})
    with pytest.raises(EditorProtocolConflictError, match="stale"):
        store.dispatch(state.draft_id, stale)
    wrong = command.copy(
        update={
            "command_id": "lock-3",
            "draft_id": "another-draft",
            "base_revision": 1,
        }
    )
    with pytest.raises(EditorProtocolConflictError, match="different"):
        store.dispatch(state.draft_id, wrong)


def test_store_is_bounded_and_delete_is_idempotent() -> None:
    store = EditorDraftStore(max_drafts=1)
    first = store.create(_candidate_set())
    second = store.create(_candidate_set())

    with pytest.raises(EditorDraftNotFoundError):
        store.state(first.draft_id)
    assert store.delete(second.draft_id)
    assert not store.delete(second.draft_id)
    with pytest.raises(EditorDraftNotFoundError):
        store.state(second.draft_id)
