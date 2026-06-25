from __future__ import annotations

import csv
from datetime import datetime, timezone
from pathlib import Path

from seattrellis.io.json_files import write_json_model
from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.rules import (
    AvoidRecentNeighborsRule,
    FairRotationRule,
    FixedSeatRule,
    HardRules,
    PairRule,
    RuleSet,
    SoftRules,
    WeightedRule,
)
from seattrellis.models.history import NeighborRelationType
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError


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
    neighbor_rules_json = examples / "rules_neighbor_avoidance.json"
    multi_candidate_rules_json = examples / "rules_multi_candidate.json"
    project_json = examples / "project.seattrellis.json"
    history = examples / "history"

    if overwrite or not students_csv.exists():
        _write_students_csv(students_csv)
    if overwrite or not students_xlsx.exists():
        try:
            _write_students_xlsx(students_xlsx)
        except MissingOptionalDependencyError:
            pass
    if overwrite or not layout_json.exists():
        write_json_model(_demo_layout(), layout_json)
    if overwrite or not rules_json.exists():
        write_json_model(_demo_rules(), rules_json)
    if overwrite or not neighbor_rules_json.exists():
        write_json_model(_demo_neighbor_rules(), neighbor_rules_json)
    if overwrite or not multi_candidate_rules_json.exists():
        write_json_model(_demo_neighbor_rules(), multi_candidate_rules_json)
    if overwrite or not project_json.exists():
        write_json_model(
            SeatTrellisProject(
                name="Demo Class",
                students="students.csv",
                layout="classroom.json",
                rules="rules_multi_candidate.json",
                history_dir="history",
                outputs_dir="outputs",
                default_candidates=5,
            ),
            project_json,
        )
    _write_history_examples(history, overwrite=overwrite)

    return {
        "students_csv": students_csv,
        "students_xlsx": students_xlsx,
        "layout": layout_json,
        "rules": rules_json,
        "neighbor_rules": neighbor_rules_json,
        "multi_candidate_rules": multi_candidate_rules_json,
        "project": project_json,
        "history": history,
        "outputs": outputs,
    }


def _write_students_csv(path: Path) -> None:
    with path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(DEMO_STUDENTS[0].keys()))
        writer.writeheader()
        writer.writerows(DEMO_STUDENTS)


def _write_students_xlsx(path: Path) -> None:
    try:
        from openpyxl import Workbook
    except ImportError as exc:
        raise MissingOptionalDependencyError("Excel demo generation", "excel") from exc

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
            fair_rotation=FairRotationRule(enabled=True, weight=10),
        ),
    )


def _demo_neighbor_rules() -> RuleSet:
    rules = _demo_rules()
    rules.soft.avoid_recent_neighbors = AvoidRecentNeighborsRule(
        enabled=True,
        weight=10,
        lookback=4,
        relation_types=[NeighborRelationType.DESK_MATE, NeighborRelationType.ADJACENT_ANY],
        max_recent_count=1,
    )
    return rules


def _write_history_examples(history_dir: Path, *, overwrite: bool) -> None:
    history_dir.mkdir(parents=True, exist_ok=True)
    readme = history_dir / "README.md"
    if overwrite or not readme.exists():
        readme.write_text(
            "# Fictional history snapshots\n\n"
            "These files are fictional SeatTrellis seating snapshots for fair-rotation demos. "
            "Do not store real historical seating records in this repository.\n",
            encoding="utf-8",
        )

    snapshots = [
        (
            "week1.snapshot.json",
            "2026-06-01T00:00:00+00:00",
            {
                "STU001": "R1C1",
                "STU002": "R1C2",
                "STU003": "R2C1",
                "STU004": "R2C2",
                "STU005": "R3C1",
                "STU006": "R3C2",
                "STU007": "R4C3",
                "STU008": "R4C4",
            },
        ),
        (
            "week2.snapshot.json",
            "2026-06-08T00:00:00+00:00",
            {
                "STU001": "R1C3",
                "STU002": "R2C4",
                "STU003": "R1C4",
                "STU004": "R3C3",
                "STU005": "R3C1",
                "STU006": "R3C2",
                "STU007": "R2C1",
                "STU008": "R3C4",
            },
        ),
        (
            "week3.snapshot.json",
            "2026-06-15T00:00:00+00:00",
            {
                "STU001": "R2C1",
                "STU002": "R4C4",
                "STU003": "R3C2",
                "STU004": "R1C2",
                "STU005": "R1C1",
                "STU006": "R2C4",
                "STU007": "R3C1",
                "STU008": "R4C3",
            },
        ),
    ]
    for filename, created_at, assignments in snapshots:
        path = history_dir / filename
        if overwrite or not path.exists():
            write_json_model(_demo_history_snapshot(assignments, created_at), path)


def _demo_history_snapshot(assignments: dict[str, str], created_at: str) -> SeatingSnapshot:
    student_names = {record["student_id"]: record["name"] for record in DEMO_STUDENTS}
    return SeatingSnapshot(
        created_at=datetime.fromisoformat(created_at).astimezone(timezone.utc),
        seed=42,
        metadata={"version": "0.2.1", "example": "fictional history sample"},
        students=[
            Student(student_id=record["student_id"], name=record["name"])
            for record in DEMO_STUDENTS
        ],
        layout=_demo_layout(),
        rules=RuleSet(seed=42),
        assignments=[
            SeatAssignment(student_key=student_id, student_name=student_names[student_id], seat_id=seat_id)
            for student_id, seat_id in assignments.items()
        ],
        solver_status="FEASIBLE",
        metrics={"example": "fictional"},
    )
