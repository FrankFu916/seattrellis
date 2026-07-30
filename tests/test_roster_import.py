from __future__ import annotations

import pytest

from seattrellis.application.roster_import import (
    import_roster,
    import_roster_records,
    summarize_roster,
)
from seattrellis.io.json_files import InputFileError
from seattrellis.models.student import Student


def test_import_roster_uses_existing_file_importer() -> None:
    imported = import_roster("tests/fixtures/students.csv")

    assert imported.source_name == "students.csv"
    assert isinstance(imported.students, tuple)
    assert [student.key for student in imported.students] == [
        "STU001",
        "STU002",
        "STU003",
        "STU004",
    ]
    assert imported.summary.student_count == 4


def test_import_roster_records_accepts_english_aliases_and_summarizes() -> None:
    imported = import_roster_records(
        [
            {
                "id": "S1",
                "name": "Alice",
                "height": "158",
                "score": "91",
                "vision": 1.2,
            },
            {
                "name": "Bob",
                "needs": "vision_front; wheelchair access",
            },
        ],
        source_name="Class 3",
    )

    assert imported.source_name == "Class 3"
    assert imported.students[0].student_id == "S1"
    assert imported.students[0].height_cm == 158
    assert imported.summary.student_count == 2
    assert imported.summary.name_only_count == 1
    assert imported.summary.score_count == 1
    assert imported.summary.height_count == 1
    assert imported.summary.vision_or_front_need_count == 2
    assert imported.summary.special_needs_count == 1


def test_import_roster_records_accepts_chinese_aliases() -> None:
    imported = import_roster_records(
        [
            {
                "学号": "甲-01",
                "姓名": "小林",
                "成绩": 88,
                "身高": 152,
                "需求": "靠前、安静",
            }
        ]
    )

    student = imported.students[0]
    assert imported.source_name is None
    assert student.student_id == "甲-01"
    assert student.name == "小林"
    assert student.needs == ["靠前", "安静"]
    assert imported.summary.vision_or_front_need_count == 1
    assert imported.summary.special_needs_count == 1


def test_summarize_roster_accepts_a_single_pass_iterable() -> None:
    students = (
        student
        for student in [
            Student(student_id="S1", name="Alice"),
            Student(name="Bob", score=0, height_cm=150, needs=["quiet"]),
        ]
    )

    summary = summarize_roster(students)

    assert summary.student_count == 2
    assert summary.name_only_count == 1
    assert summary.score_count == 1
    assert summary.height_count == 1
    assert summary.vision_or_front_need_count == 0
    assert summary.special_needs_count == 1


def test_import_roster_records_preserves_duplicate_error() -> None:
    with pytest.raises(
        ValueError,
        match='Row 3: column "student_id" is duplicated: S1',
    ):
        import_roster_records(
            [
                {"student_id": "S1", "name": "Alice"},
                {"student_id": "S1", "name": "Bob"},
            ]
        )


def test_import_roster_preserves_file_error(tmp_path) -> None:
    missing = tmp_path / "missing.csv"

    with pytest.raises(InputFileError, match=f"Student file not found: {missing}"):
        import_roster(missing)
