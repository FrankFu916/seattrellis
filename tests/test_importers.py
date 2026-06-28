from __future__ import annotations

import pytest
from openpyxl import Workbook

from seattrellis.io.json_files import InputFileError, read_json
from seattrellis.io.students import read_students, students_from_records


def test_read_students_csv() -> None:
    students = read_students("tests/fixtures/students.csv")
    assert [student.key for student in students] == ["STU001", "STU002", "STU003", "STU004"]
    assert students[0].needs == ["vision_front"]


def test_read_students_excel(tmp_path) -> None:
    path = tmp_path / "students.xlsx"
    workbook = Workbook()
    sheet = workbook.active
    sheet.append(["学号", "姓名", "身高", "成绩", "自定义列"])
    sheet.append(["A", "甲", 150, 90, "extra"])
    sheet.append(["B", "乙", 160, 80, "extra2"])
    workbook.save(path)

    students = read_students(path)

    assert students[0].student_id == "A"
    assert students[0].name == "甲"
    assert students[0].attributes["自定义列"] == "extra"


def test_students_from_records_rejects_empty_data() -> None:
    try:
        students_from_records([])
    except ValueError as exc:
        assert "No valid students" in str(exc)
    else:
        raise AssertionError("Expected ValueError")


def test_read_json_reports_invalid_utf8_as_input_error(tmp_path) -> None:
    path = tmp_path / "invalid.json"
    path.write_bytes(b"\xff\xfe")

    with pytest.raises(InputFileError, match="Invalid UTF-8"):
        read_json(path)


def test_read_json_reports_directory_as_input_error(tmp_path) -> None:
    with pytest.raises(InputFileError, match="Could not read input file"):
        read_json(tmp_path)
