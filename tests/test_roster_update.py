from __future__ import annotations

import pytest

from seattrellis.application.roster_update import (
    RosterState,
    RosterUpdateConflictError,
    StaleRosterRevisionError,
    apply_roster_update,
    normalize_student_name,
    preview_roster_update,
)
from seattrellis.models.student import Student


def test_incremental_preview_prefers_student_id_and_updates_selected_fields() -> None:
    current = RosterState(
        revision=4,
        students=(
            Student(
                student_id="S1",
                name="Shared",
                score=70,
                notes="keep this note",
            ),
            Student(student_id="S2", name="Shared", score=80),
        ),
    )
    incoming = [
        Student(student_id="S2", name="Shared", score=88),
        Student(student_id="S3", name="New student", score=75),
    ]

    preview = preview_roster_update(
        current,
        incoming,
        updated_fields=("student_id", "name", "score"),
    )

    assert preview.can_apply is True
    assert preview.count("update") == 1
    assert preview.count("add") == 1
    assert preview.changes[0].match_method == "student_id"
    assert preview.resulting_students is not None
    assert [student.student_id for student in preview.resulting_students] == [
        "S1",
        "S2",
        "S3",
    ]
    assert preview.resulting_students[0].notes == "keep this note"
    assert preview.resulting_students[1].score == 88


def test_unique_normalized_exact_name_is_the_only_fallback() -> None:
    state = RosterState(students=(Student(name="  ALICE   SMITH ", score=70),))

    exact = preview_roster_update(
        state,
        [Student(student_id="S1", name="Alice Smith", score=90)],
    )
    fuzzy = preview_roster_update(
        state,
        [Student(student_id="S2", name="Alice Smyth", score=90)],
    )

    assert normalize_student_name(" ＡＬＩＣＥ   Smith ") == "alice smith"
    assert exact.changes[0].match_method == "name"
    assert exact.count("update") == 1
    assert exact.resulting_students is not None
    assert exact.resulting_students[0].student_id == "S1"
    assert fuzzy.changes[0].match_method == "new"
    assert fuzzy.count("add") == 1


def test_different_id_with_an_existing_exact_name_is_a_conflict() -> None:
    state = RosterState(students=(Student(student_id="S1", name="Alice"),))

    preview = preview_roster_update(
        state,
        [Student(student_id="S9", name="Alice")],
    )

    assert preview.can_apply is False
    assert preview.resulting_students is None
    assert preview.conflicts[0].code == "student_id_name_mismatch"
    with pytest.raises(RosterUpdateConflictError, match="1 unresolved conflict"):
        apply_roster_update(state, preview)


def test_ambiguous_name_fallback_and_duplicate_import_ids_block_apply() -> None:
    ambiguous = RosterState(
        students=(
            Student(student_id="S1", name="Shared"),
            Student(student_id="S2", name="Shared"),
        )
    )
    name_preview = preview_roster_update(ambiguous, [Student(name="shared")])
    duplicate_preview = preview_roster_update(
        RosterState(students=()),
        [
            Student(student_id="S1", name="Alice"),
            Student(student_id="S1", name="Bob"),
        ],
    )

    assert name_preview.conflicts[0].code == "ambiguous_name"
    assert duplicate_preview.conflicts[0].code == "duplicate_incoming_student_id"
    assert duplicate_preview.count("conflict") == 2


def test_full_replace_preview_reports_removals_and_uses_import_order() -> None:
    state = RosterState(
        revision=2,
        students=(
            Student(student_id="S1", name="Alice", notes="old"),
            Student(student_id="S2", name="Bob"),
        ),
    )
    incoming = (
        Student(student_id="S3", name="Cara"),
        Student(student_id="S1", name="Alice", notes="new"),
    )

    preview = preview_roster_update(state, incoming, mode="replace")

    assert preview.can_apply is True
    assert preview.count("add") == 1
    assert preview.count("update") == 1
    assert preview.count("remove") == 1
    assert preview.resulting_students == incoming

    applied = apply_roster_update(state, preview)
    assert applied.revision == 3
    assert applied.students == incoming


def test_stale_revision_and_same_revision_data_changes_both_block_apply() -> None:
    original = RosterState(
        revision=3,
        students=(Student(student_id="S1", name="Alice", score=70),),
    )
    preview = preview_roster_update(
        original,
        [Student(student_id="S1", name="Alice", score=80)],
    )

    with pytest.raises(StaleRosterRevisionError, match="base revision 3, current revision 4"):
        apply_roster_update(
            RosterState(students=original.students, revision=4),
            preview,
        )
    with pytest.raises(StaleRosterRevisionError, match="data changed"):
        apply_roster_update(
            RosterState(
                revision=3,
                students=(Student(student_id="S1", name="Alice", score=71),),
            ),
            preview,
        )


def test_sequence_apply_requires_an_explicit_current_revision() -> None:
    current = (Student(student_id="S1", name="Alice"),)
    preview = preview_roster_update(
        current,
        [Student(student_id="S1", name="Alice", score=80)],
        base_revision=7,
    )

    with pytest.raises(TypeError, match="current_revision is required"):
        apply_roster_update(current, preview)
    applied = apply_roster_update(current, preview, current_revision=7)
    assert applied.revision == 8


def test_resulting_duplicate_keys_are_reported_instead_of_applied() -> None:
    state = RosterState(
        students=(Student(student_id="Alice", name="Different person"),)
    )

    preview = preview_roster_update(state, [Student(name="Alice")])

    assert preview.can_apply is False
    assert preview.conflicts[-1].code == "duplicate_resulting_identifier"


def test_two_rows_cannot_update_the_same_current_student() -> None:
    state = RosterState(
        students=(Student(student_id="S1", name="Alice"),)
    )

    preview = preview_roster_update(
        state,
        [Student(student_id="S1", name="Alice"), Student(name="Alice")],
    )

    assert preview.can_apply is False
    assert any(
        conflict.code == "existing_student_matched_twice"
        for conflict in preview.conflicts
    )

