from __future__ import annotations

import pandas as pd

from seattrellis.io.students import read_students, students_from_dataframe


def test_read_students_csv() -> None:
    students = read_students("tests/fixtures/students.csv")
    assert [student.key for student in students] == ["STU001", "STU002", "STU003", "STU004"]
    assert students[0].needs == ["vision_front"]


def test_read_students_excel(tmp_path) -> None:
    path = tmp_path / "students.xlsx"
    pd.DataFrame(
        [
            {"学号": "A", "姓名": "甲", "身高": 150, "成绩": 90, "自定义列": "extra"},
            {"学号": "B", "姓名": "乙", "身高": 160, "成绩": 80, "自定义列": "extra2"},
        ]
    ).to_excel(path, index=False)

    students = read_students(path)

    assert students[0].student_id == "A"
    assert students[0].name == "甲"
    assert students[0].attributes["自定义列"] == "extra"


def test_students_from_dataframe_rejects_empty_data() -> None:
    try:
        students_from_dataframe(pd.DataFrame())
    except ValueError as exc:
        assert "No valid students" in str(exc)
    else:
        raise AssertionError("Expected ValueError")
