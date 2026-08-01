from __future__ import annotations

import pytest

from seattrellis.api.models import (
    RosterMappingItem,
    RosterUpdatePreviewRequest,
)
from seattrellis.api.rosters import RosterDraftNotFoundError, RosterDraftStore
from seattrellis.io.roster_table import read_roster_table_bytes
from seattrellis.models.student import Student


def _table(content: str):
    return read_roster_table_bytes(content.encode(), filename="class.csv")


def test_roster_draft_exposes_bounded_preview_and_mapping_suggestions() -> None:
    table = _table(
        "姓名,学号,总分\n"
        "小林,001,91\n"
        "小周,002,88\n"
        "小陈,003,84\n"
        "小吴,004,82\n"
        "小方,005,79\n"
        "小许,006,77\n"
    )

    state = RosterDraftStore().create(table)

    assert state.row_count == 6
    assert len(state.preview_rows) == 5
    assert [column.header for column in state.columns] == ["姓名", "学号", "总分"]
    assert {
        item.field: item.column_index for item in state.suggested_mapping
    } == {"student_id": 1, "name": 0, "score": 2}
    assert state.mapping_issues == []


def test_roster_draft_keeps_headerless_uploads_and_suggests_identity_columns() -> None:
    state = RosterDraftStore().create(
        _table("小林,18513806422\n小周,18513806423\n")
    )

    assert state.headerless is True
    assert state.row_count == 2
    assert [row.cells for row in state.preview_rows] == [
        ["小林", "18513806422"],
        ["小周", "18513806423"],
    ]
    assert {
        item.field: item.column_index for item in state.suggested_mapping
    } == {"student_id": 1, "name": 0}


def test_roster_preview_supports_incremental_and_replace_updates() -> None:
    store = RosterDraftStore()
    state = store.create(_table("id,name,score\nS1,Alice,93\nS2,Bob,81\n"))
    mapping = [
        RosterMappingItem(field="student_id", column_index=0),
        RosterMappingItem(field="name", column_index=1),
        RosterMappingItem(field="score", column_index=2),
    ]
    current = [
        Student(student_id="S1", name="Alice", score=70, notes="keep"),
        Student(student_id="S3", name="Cara", score=80),
    ]

    incremental = store.preview_update(
        state.draft_id,
        RosterUpdatePreviewRequest(
            mapping=mapping,
            current_students=current,
            current_revision=4,
            mode="incremental",
            updated_fields=["student_id", "name", "score"],
        ),
    )
    replacement = store.preview_update(
        state.draft_id,
        RosterUpdatePreviewRequest(
            mapping=mapping,
            current_students=current,
            current_revision=4,
            mode="replace",
        ),
    )

    assert incremental.can_apply is True
    assert incremental.action_counts["update"] == 1
    assert incremental.action_counts["add"] == 1
    assert incremental.action_counts["remove"] == 0
    assert incremental.resulting_students is not None
    assert incremental.resulting_students[0].notes == "keep"
    assert replacement.action_counts["remove"] == 1
    assert [student.student_id for student in replacement.resulting_students or []] == [
        "S1",
        "S2",
    ]


def test_roster_preview_accepts_legacy_overwrite_alias() -> None:
    request = RosterUpdatePreviewRequest.model_validate(
        {
            "mapping": [],
            "mode": "overwrite",
        }
    )

    assert request.mode == "replace"


def test_roster_preview_keeps_identity_conflicts_visible() -> None:
    store = RosterDraftStore()
    state = store.create(_table("id,name\nS9,Alice\n"))

    preview = store.preview_update(
        state.draft_id,
        RosterUpdatePreviewRequest(
            mapping=[
                RosterMappingItem(field="student_id", column_index=0),
                RosterMappingItem(field="name", column_index=1),
            ],
            current_students=[Student(student_id="S1", name="Alice")],
        ),
    )

    assert preview.can_apply is False
    assert preview.resulting_students is None
    assert preview.conflicts[0].code == "student_id_name_mismatch"


def test_roster_store_is_bounded_and_deletion_is_immediate() -> None:
    store = RosterDraftStore(max_drafts=1)
    first = store.create(_table("name\nAlice\n"))
    second = store.create(_table("name\nBob\n"))

    with pytest.raises(RosterDraftNotFoundError):
        store.state(first.draft_id)
    assert store.delete(second.draft_id) is True
    assert store.delete(second.draft_id) is False
    with pytest.raises(RosterDraftNotFoundError):
        store.state(second.draft_id)


def test_roster_http_flow_keeps_parser_errors_private() -> None:
    pytest.importorskip("fastapi")
    pytest.importorskip("multipart")
    pytest.importorskip("httpx")
    from fastapi.testclient import TestClient

    from seattrellis.api.http import create_app

    with TestClient(create_app()) as client:
        uploaded = client.post(
            "/api/v1/rosters/drafts",
            files={"file": ("class.csv", b"id,name\nS1,Alice\n", "text/csv")},
            headers={"Host": "127.0.0.1"},
        )
        invalid = client.post(
            "/api/v1/rosters/drafts",
            files={"file": ("class.csv", b"\xff", "text/csv")},
            headers={"Host": "127.0.0.1"},
        )

    assert uploaded.status_code == 200
    assert uploaded.json()["suggested_mapping"] == [
        {"field": "student_id", "column_index": 0},
        {"field": "name", "column_index": 1},
    ]
    assert invalid.status_code == 422
    assert invalid.json()["error"]["code"] == "invalid_roster_file"
    assert "xff" not in invalid.text.lower()
