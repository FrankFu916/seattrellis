from __future__ import annotations

import csv
from pathlib import Path

from openpyxl import Workbook

from seattrellis.io.json_files import write_json_model
from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode
from seattrellis.models.rules import (
    FixedSeatRule,
    HardRules,
    PairRule,
    RuleSet,
    SoftRules,
    WeightedRule,
)


DEMO_STUDENTS = [
    {
        "student_id": "STU001",
        "name": "Student001",
        "gender": "F",
        "height_cm": 154,
        "score": 92,
        "vision": "poor",
        "tags": "leader",
        "needs": "vision_front",
        "notes": "",
    },
    {"student_id": "STU002", "name": "Student002", "gender": "M", "height_cm": 172, "score": 81, "vision": "", "tags": "", "needs": "", "notes": ""},
    {"student_id": "STU003", "name": "Student003", "gender": "F", "height_cm": 160, "score": 76, "vision": "", "tags": "", "needs": "", "notes": ""},
    {"student_id": "STU004", "name": "Student004", "gender": "M", "height_cm": 178, "score": 88, "vision": "", "tags": "", "needs": "", "notes": ""},
    {"student_id": "STU005", "name": "Student005", "gender": "F", "height_cm": 158, "score": 95, "vision": "0.8", "tags": "", "needs": "", "notes": ""},
    {"student_id": "STU006", "name": "Student006", "gender": "M", "height_cm": 169, "score": 70, "vision": "", "tags": "", "needs": "", "notes": ""},
    {"student_id": "STU007", "name": "Student007", "gender": "F", "height_cm": 165, "score": 84, "vision": "", "tags": "", "needs": "", "notes": ""},
    {"student_id": "STU008", "name": "Student008", "gender": "M", "height_cm": 181, "score": 67, "vision": "", "tags": "", "needs": "", "notes": ""},
]


def create_demo_files(base_dir: str | Path = ".", *, overwrite: bool = False) -> dict[str, Path]:
    base = Path(base_dir)
    examples = base / "examples"
    outputs = base / "outputs"
    examples.mkdir(parents=True, exist_ok=True)
    outputs.mkdir(parents=True, exist_ok=True)

    students_csv = examples / "students.csv"
    students_xlsx = examples / "students.xlsx"
    layout_json = examples / "classroom.json"
    rules_json = examples / "rules.json"

    if overwrite or not students_csv.exists():
        _write_students_csv(students_csv)
    if overwrite or not students_xlsx.exists():
        _write_students_xlsx(students_xlsx)
    if overwrite or not layout_json.exists():
        write_json_model(_demo_layout(), layout_json)
    if overwrite or not rules_json.exists():
        write_json_model(_demo_rules(), rules_json)

    return {
        "students_csv": students_csv,
        "students_xlsx": students_xlsx,
        "layout": layout_json,
        "rules": rules_json,
        "outputs": outputs,
    }


def _write_students_csv(path: Path) -> None:
    with path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(DEMO_STUDENTS[0].keys()))
        writer.writeheader()
        writer.writerows(DEMO_STUDENTS)


def _write_students_xlsx(path: Path) -> None:
    workbook = Workbook()
    sheet = workbook.active
    sheet.title = "Students"
    headers = list(DEMO_STUDENTS[0].keys())
    sheet.append(headers)
    for student in DEMO_STUDENTS:
        sheet.append([student.get(header, "") for header in headers])
    workbook.save(path)


def _demo_layout() -> ClassroomLayout:
    seats: list[SeatNode] = []
    for row in range(1, 5):
        for col in range(1, 5):
            enabled = not (row == 2 and col == 3) and not (row == 4 and col == 2)
            zone = "aisle" if not enabled else ("front" if row == 1 else "middle" if row < 4 else "back")
            seats.append(
                SeatNode(
                    seat_id=f"R{row}C{col}",
                    row=row,
                    col=col,
                    enabled=enabled,
                    zone=zone,
                    near_platform=row == 1,
                    near_window=col == 1,
                    near_door=row == 4 and col == 4,
                )
            )
    return ClassroomLayout(
        layout_id="fictional-room",
        name="Fictional Classroom",
        seats=seats,
        adjacency=AdjacencyConfig(
            include_horizontal=True,
            include_vertical=False,
            include_diagonal=False,
            max_row_delta=1,
            max_col_delta=1,
            custom_edges=[("R1C2", "R2C2")],
        ),
        metadata={"platform": "front", "description": "Fictional 4x4 irregular classroom with two disabled seats."},
    )


def _demo_rules() -> RuleSet:
    return RuleSet(
        seed=42,
        hard=HardRules(
            fixed_seats=[FixedSeatRule(student="STU001", seat_id="R1C1")],
            must_be_adjacent=[PairRule(students=("STU002", "STU003"))],
            cannot_be_adjacent=[PairRule(students=("STU004", "STU005"))],
        ),
        soft=SoftRules(
            vision_front=WeightedRule(enabled=True, weight=20),
            height_back=WeightedRule(enabled=True, weight=1),
            randomize=WeightedRule(enabled=True, weight=1),
            score_balance=WeightedRule(enabled=True, weight=1),
        ),
    )
