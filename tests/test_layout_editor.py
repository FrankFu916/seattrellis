from __future__ import annotations

import pytest

from seattrellis.application.layout_editor import (
    LayoutCommand,
    LayoutDraft,
    LayoutEditingError,
    LayoutRevisionConflictError,
)


def test_layout_draft_represents_all_editor_cell_kinds() -> None:
    draft = LayoutDraft.rectangular(2, 3)

    draft.apply(
        LayoutCommand("set_cell", {"row": 1, "column": 1, "kind": "platform"})
    )
    draft.apply(
        LayoutCommand("set_cell", {"row": 1, "column": 2, "kind": "aisle"})
    )
    draft.apply(
        LayoutCommand("set_cell", {"row": 2, "column": 3, "kind": "empty"})
    )

    assert [cell.kind for cell in draft.ordered_cells()] == [
        "platform",
        "aisle",
        "seat",
        "seat",
        "seat",
        "empty",
    ]
    layout = draft.to_layout()
    assert len(layout.enabled_seats) == 3
    assert {seat.zone for seat in layout.seats if not seat.enabled} == {
        "aisle",
        "platform",
    }


def test_row_and_column_commands_preserve_existing_seat_ids() -> None:
    draft = LayoutDraft.rectangular(2, 2)
    original_ids = {cell.seat_id for cell in draft.cells.values()}

    draft.apply(LayoutCommand("insert_row", {"index": 1}))
    draft.apply(LayoutCommand("insert_column", {"index": 2}))

    assert draft.rows == 3
    assert draft.columns == 3
    assert {cell.seat_id for cell in draft.cells.values()} == original_ids

    draft.apply(LayoutCommand("delete_row", {"index": 1}))
    draft.apply(LayoutCommand("delete_column", {"index": 2}))
    assert draft.rows == 2
    assert draft.columns == 2


def test_mirror_flip_and_translate_are_atomic() -> None:
    draft = LayoutDraft.rectangular(3, 3)
    draft.apply(
        LayoutCommand("set_cell", {"row": 3, "column": 3, "kind": "empty"})
    )

    draft.apply(LayoutCommand("mirror_horizontal"))
    assert draft.cells[(3, 1)].kind == "empty"
    draft.apply(LayoutCommand("flip_vertical"))
    assert draft.cells[(1, 1)].kind == "empty"

    before = draft.ordered_cells()
    with pytest.raises(LayoutEditingError, match="outside the layout"):
        draft.apply(
            LayoutCommand("translate", {"row_delta": -1, "column_delta": 0})
        )
    assert draft.ordered_cells() == before


def test_layout_changes_support_undo_redo_and_stale_revision_checks() -> None:
    draft = LayoutDraft.rectangular(2, 2)
    draft.apply(
        LayoutCommand("set_cell", {"row": 1, "column": 1, "kind": "aisle"}),
        base_revision=0,
    )
    assert draft.revision == 1

    with pytest.raises(LayoutRevisionConflictError, match="stale"):
        draft.apply(LayoutCommand("mirror_horizontal"), base_revision=0)

    draft.undo(base_revision=1)
    assert draft.cells[(1, 1)].kind == "seat"
    draft.redo(base_revision=2)
    assert draft.cells[(1, 1)].kind == "aisle"
    assert draft.revision == 3


def test_layout_conversion_requires_at_least_one_seat() -> None:
    draft = LayoutDraft(
        rows=1,
        columns=2,
        cells={},
    )

    with pytest.raises(LayoutEditingError, match="at least one seat"):
        draft.to_layout()


def test_layout_round_trip_preserves_editor_cell_semantics() -> None:
    draft = LayoutDraft.rectangular(2, 3, name="Science room")
    draft.apply(
        LayoutCommand("set_cell", {"row": 1, "column": 2, "kind": "platform"})
    )
    draft.apply(
        LayoutCommand("set_cell", {"row": 2, "column": 2, "kind": "aisle"})
    )

    restored = LayoutDraft.from_layout(draft.to_layout(layout_id="science"))

    assert restored.name == "Science room"
    assert restored.draft_id == "science"
    assert restored.cells[(1, 2)].kind == "platform"
    assert restored.cells[(2, 2)].kind == "aisle"
