from __future__ import annotations

from pathlib import Path
from typing import Any

import pandas as pd

from seattrellis.models.student import Student

COLUMN_ALIASES = {
    "student_id": {"student_id", "id", "sid", "学号", "学生编号", "编号"},
    "name": {"name", "姓名", "学生姓名"},
    "gender": {"gender", "sex", "性别"},
    "height_cm": {"height_cm", "height", "身高", "身高cm", "身高_cm"},
    "score": {"score", "成绩", "总分", "分数"},
    "vision": {"vision", "视力"},
    "notes": {"notes", "note", "备注", "说明"},
    "tags": {"tags", "tag", "标签"},
    "needs": {"needs", "need", "特殊需求", "需求"},
}


def read_students(path: str | Path) -> list[Student]:
    source = Path(path)
    if not source.exists():
        raise FileNotFoundError(source)
    if source.suffix.lower() == ".csv":
        frame = pd.read_csv(source, dtype=object)
    elif source.suffix.lower() in {".xlsx", ".xls"}:
        frame = pd.read_excel(source, dtype=object)
    else:
        raise ValueError(f"Unsupported student file format: {source.suffix}")
    return students_from_dataframe(frame)


def students_from_dataframe(frame: pd.DataFrame) -> list[Student]:
    column_map = _build_column_map(frame.columns)
    students: list[Student] = []
    for _, row in frame.iterrows():
        data: dict[str, Any] = {}
        attributes: dict[str, Any] = {}
        for column in frame.columns:
            value = _clean_value(row[column])
            if value is None:
                continue
            canonical = column_map.get(str(column))
            if canonical:
                data[canonical] = value
            else:
                attributes[str(column)] = value
        if attributes:
            data["attributes"] = attributes
        if not data:
            continue
        students.append(Student(**data))
    if not students:
        raise ValueError("No valid students found.")
    _validate_unique_keys(students)
    return students


def _build_column_map(columns: list[str] | pd.Index) -> dict[str, str]:
    mapping: dict[str, str] = {}
    normalized_aliases = {
        alias.lower().replace(" ", "").replace("_", ""): canonical
        for canonical, aliases in COLUMN_ALIASES.items()
        for alias in aliases
    }
    for column in columns:
        text = str(column)
        normalized = text.lower().replace(" ", "").replace("_", "")
        if normalized in normalized_aliases:
            mapping[text] = normalized_aliases[normalized]
    return mapping


def _clean_value(value: Any) -> Any:
    if value is None:
        return None
    try:
        if pd.isna(value):
            return None
    except TypeError:
        pass
    if isinstance(value, str):
        text = value.strip()
        return text or None
    return value


def _validate_unique_keys(students: list[Student]) -> None:
    keys = [student.key for student in students]
    duplicates = sorted({key for key in keys if keys.count(key) > 1})
    if duplicates:
        raise ValueError(f"Duplicate student identifiers: {', '.join(duplicates)}")
