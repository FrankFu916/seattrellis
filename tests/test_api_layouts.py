from __future__ import annotations

import pytest

from seattrellis.api.layouts import (
    LayoutCommandConflictError,
    LayoutDraftStore,
)
from seattrellis.api.models import LayoutCommandRequest
from seattrellis.application.layout_editor import LayoutDraft, LayoutEditingError


def _command(state, command_id: str, action: str, operation=None):
    return LayoutCommandRequest.parse_obj(
        {
            "command_id": command_id,
            "draft_id": state.draft_id,
            "base_revision": state.revision,
            "action": action,
            "operation": operation,
        }
    )


def test_layout_store_exposes_all_cells_and_compiles_solver_layout() -> None:
    store = LayoutDraftStore()
    state = store.create(LayoutDraft.rectangular(2, 3, name="Art room"))
    aisle = store.dispatch(
        state.draft_id,
        _command(
            state,
            "aisle-1",
            "apply",
            {
                "kind": "set_cell",
                "payload": {"row": 1, "column": 2, "kind": "aisle"},
            },
        ),
    )

    assert aisle.usable_seat_count == 5
    assert len(aisle.cells) == 6
    assert next(cell for cell in aisle.cells if cell.row == 1 and cell.column == 2).kind == "aisle"
    compiled = store.compile(state.draft_id)
    assert compiled.layout.name == "Art room"
    assert len(compiled.layout.enabled_seats) == 5


def test_layout_store_undo_redo_and_revision_conflicts() -> None:
    store = LayoutDraftStore()
    state = store.create(LayoutDraft.rectangular(2, 2))
    changed = store.dispatch(
        state.draft_id,
        _command(
            state,
            "mirror",
            "apply",
            {"kind": "mirror_horizontal", "payload": {}},
        ),
    )
    undone = store.dispatch(
        state.draft_id,
        _command(changed, "undo", "undo"),
    )
    redone = store.dispatch(
        state.draft_id,
        _command(undone, "redo", "redo"),
    )

    assert (changed.revision, undone.revision, redone.revision) == (1, 2, 3)
    assert redone.undo_depth == 1
    with pytest.raises(LayoutCommandConflictError, match="stale"):
        store.dispatch(
            state.draft_id,
            _command(state, "stale", "undo"),
        )


def test_layout_store_rejects_duplicate_and_cross_draft_commands() -> None:
    store = LayoutDraftStore()
    state = store.create(LayoutDraft.rectangular(2, 2))
    command = _command(
        state,
        "flip",
        "apply",
        {"kind": "flip_vertical", "payload": {}},
    )
    store.dispatch(state.draft_id, command)

    with pytest.raises(LayoutCommandConflictError, match="already"):
        store.dispatch(state.draft_id, command)
    wrong = command.copy(
        update={
            "command_id": "wrong",
            "draft_id": "another",
            "base_revision": 1,
        }
    )
    with pytest.raises(LayoutCommandConflictError, match="different"):
        store.dispatch(state.draft_id, wrong)


def test_empty_layout_draft_remains_editable_but_cannot_compile() -> None:
    store = LayoutDraftStore()
    state = store.create(LayoutDraft(rows=1, columns=2))

    assert state.usable_seat_count == 0
    with pytest.raises(LayoutEditingError, match="at least one seat"):
        store.compile(state.draft_id)


def test_layout_command_payload_does_not_coerce_strings_or_booleans() -> None:
    with pytest.raises(ValueError):
        LayoutCommandRequest.parse_obj(
            {
                "command_id": "bad",
                "draft_id": "draft",
                "base_revision": 0,
                "action": "apply",
                "operation": {
                    "kind": "insert_row",
                    "payload": {"index": True},
                },
            }
        )

    command = LayoutCommandRequest.parse_obj(
        {
            "command_id": "seat",
            "draft_id": "draft",
            "base_revision": 0,
            "action": "apply",
            "operation": {
                "kind": "set_cell",
                "payload": {"row": 1, "column": 2, "kind": "seat", "seat_id": "001"},
            },
        }
    )
    assert command.operation is not None
    assert command.operation.payload["seat_id"] == "001"
