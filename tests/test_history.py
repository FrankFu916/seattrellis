from __future__ import annotations

import json

import pytest

import seattrellis.solver.cp_sat as cp_sat
from seattrellis import cli
from seattrellis.history import (
    build_fairness_report,
    build_seat_history,
    classify_seat_position,
    fair_rotation_cost,
    load_history_snapshots,
)
from seattrellis.io.json_files import load_snapshot, write_json_model
from seattrellis.models.history import SeatPositionCategory
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import FairRotationRule, FixedSeatRule, HardRules, PairRule, RuleSet, SoftRules, WeightedRule
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.solver import solve_seating
from seattrellis.solver.adjacency import build_adjacency_edges, normalize_edge


def _students(count: int = 2) -> list[Student]:
    return [Student(student_id=f"S{index}", name=f"Student{index:03d}") for index in range(1, count + 1)]


def _layout() -> ClassroomLayout:
    return ClassroomLayout(
        layout_id="fictional-history-room",
        name="Fictional History Room",
        seats=[
            SeatNode(seat_id="F1", row=1, col=1, near_window=True, near_platform=True),
            SeatNode(seat_id="F2", row=1, col=2, near_platform=True),
            SeatNode(seat_id="M1", row=2, col=1, near_window=True, near_ac=True),
            SeatNode(seat_id="M2", row=2, col=2),
            SeatNode(seat_id="B1", row=3, col=1, near_window=True),
            SeatNode(seat_id="B2", row=3, col=2, near_door=True),
            SeatNode(seat_id="X1", row=4, col=1, enabled=False),
        ],
    )


def _two_seat_layout() -> ClassroomLayout:
    return ClassroomLayout(
        layout_id="two-seat-room",
        name="Two Seat Room",
        seats=[
            SeatNode(seat_id="FRONT", row=1, col=1),
            SeatNode(seat_id="BACK", row=2, col=1),
        ],
    )


def _quiet_rules(
    *,
    fair_rotation: FairRotationRule | None = None,
    hard: HardRules | None = None,
) -> RuleSet:
    disabled = WeightedRule(enabled=False, weight=0)
    return RuleSet(
        seed=11,
        hard=hard or HardRules(),
        soft=SoftRules(
            vision_front=disabled,
            height_back=disabled,
            randomize=disabled,
            score_balance=disabled,
            fair_rotation=fair_rotation or FairRotationRule(enabled=False, weight=0),
        ),
    )


def _fair_rules(*, weight: int = 10, hard: HardRules | None = None) -> RuleSet:
    return _quiet_rules(fair_rotation=FairRotationRule(enabled=True, weight=weight), hard=hard)


def _snapshot(assignments: dict[str, str], *, students: list[Student] | None = None) -> SeatingSnapshot:
    students = students or _students(max(2, len(assignments)))
    return SeatingSnapshot(
        seed=11,
        students=students,
        layout=_layout(),
        rules=_quiet_rules(),
        assignments=[
            SeatAssignment(student_key=student_key, student_name=student_key, seat_id=seat_id)
            for student_key, seat_id in assignments.items()
        ],
        solver_status="FEASIBLE",
    )


def test_load_single_history_snapshot(tmp_path) -> None:
    path = write_json_model(_snapshot({"S1": "F1", "S2": "B2"}), tmp_path / "week1.snapshot.json")

    snapshots = load_history_snapshots(history_paths=[path])

    assert len(snapshots) == 1
    assert snapshots[0].assignments[0].seat_id == "F1"


def test_load_multiple_history_snapshots_from_directory(tmp_path) -> None:
    history_dir = tmp_path / "history"
    write_json_model(_snapshot({"S1": "F1", "S2": "B2"}), history_dir / "week1.snapshot.json")
    write_json_model(_snapshot({"S1": "B1", "S2": "F2"}), history_dir / "week2.snapshot.json")

    snapshots = load_history_snapshots(history_dir=history_dir)

    assert [snapshot.assignments[0].seat_id for snapshot in snapshots] == ["F1", "B1"]


def test_history_missing_current_student_warns_without_crashing() -> None:
    history = build_seat_history(_students(3), _layout(), [_snapshot({"S1": "F1", "S2": "B2"})])

    assert history.students["S3"].total_assignments == 0
    assert any("missing current students" in warning for warning in history.warnings)


def test_history_unknown_seat_is_marked_unknown_and_warns() -> None:
    history = build_seat_history(_students(2), _layout(), [_snapshot({"S1": "UNKNOWN", "S2": "B2"})])

    assert history.students["S1"].category_counts["unknown"] == 1
    assert history.students["S1"].records[0].unknown_seat is True
    assert any("unknown seat_id" in warning for warning in history.warnings)


def test_disabled_history_seat_is_skipped_for_position_counts() -> None:
    history = build_seat_history(_students(2), _layout(), [_snapshot({"S1": "X1", "S2": "B2"})])

    assert history.students["S1"].total_assignments == 1
    assert history.students["S1"].category_counts == {}
    assert history.students["S1"].records[0].disabled_seat is True


def test_position_category_inference_for_front_back_side_and_corner() -> None:
    layout = _layout()
    front_corner = layout.seat_by_id("F1")
    middle = layout.seat_by_id("M2")
    back_corner = layout.seat_by_id("B2")

    assert {
        SeatPositionCategory.FRONT,
        SeatPositionCategory.SIDE,
        SeatPositionCategory.CORNER,
    }.issubset(classify_seat_position(front_corner, layout))
    assert SeatPositionCategory.MIDDLE in classify_seat_position(middle, layout)
    assert {
        SeatPositionCategory.BACK,
        SeatPositionCategory.SIDE,
        SeatPositionCategory.CORNER,
    }.issubset(classify_seat_position(back_corner, layout))


def test_position_category_uses_explicit_near_fields() -> None:
    layout = _layout()

    assert SeatPositionCategory.NEAR_WINDOW in classify_seat_position(layout.seat_by_id("M1"), layout)
    assert SeatPositionCategory.NEAR_AC in classify_seat_position(layout.seat_by_id("M1"), layout)
    assert SeatPositionCategory.NEAR_DOOR in classify_seat_position(layout.seat_by_id("B2"), layout)
    assert SeatPositionCategory.NEAR_PLATFORM in classify_seat_position(layout.seat_by_id("F2"), layout)


def test_fair_rotation_has_no_cost_without_history() -> None:
    student = _students(1)[0]
    layout = _two_seat_layout()
    rules = _fair_rules()

    assert fair_rotation_cost(student, layout.seat_by_id("FRONT"), layout, rules.soft.fair_rotation, None) == 0


def test_fair_rotation_prefers_rotating_recent_front_and_back_history() -> None:
    students = _students(2)
    layout = _two_seat_layout()
    history = build_seat_history(students, layout, [_snapshot({"S1": "FRONT", "S2": "BACK"}, students=students)])

    solution = solve_seating(students, layout, _fair_rules(), history=history)

    assert solution.metrics["solver"] == "fallback-heuristic"
    assert solution.assignment_map == {"S1": "BACK", "S2": "FRONT"}
    assert solution.metrics["fairness"]["enabled_rules"] == ["fair_rotation"]


def test_weight_zero_fair_rotation_does_not_affect_solver() -> None:
    students = _students(2)
    layout = _two_seat_layout()
    history = build_seat_history(students, layout, [_snapshot({"S1": "FRONT", "S2": "BACK"}, students=students)])

    solution = solve_seating(students, layout, _fair_rules(weight=0), history=history)

    assert solution.assignment_map == {"S1": "FRONT", "S2": "BACK"}
    assert solution.metrics["fairness"]["enabled_rules"] == []


def test_fixed_seat_is_not_overridden_by_fair_rotation() -> None:
    students = _students(2)
    layout = _two_seat_layout()
    history = build_seat_history(students, layout, [_snapshot({"S1": "FRONT", "S2": "BACK"}, students=students)])
    hard = HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="FRONT")])

    solution = solve_seating(students, layout, _fair_rules(hard=hard), history=history)

    assert solution.assignment_map["S1"] == "FRONT"


def test_cannot_be_adjacent_is_not_broken_by_fair_rotation() -> None:
    students = _students(2)
    layout = ClassroomLayout(
        layout_id="line-room",
        name="Line Room",
        seats=[
            SeatNode(seat_id="A1", row=1, col=1),
            SeatNode(seat_id="A2", row=1, col=2),
            SeatNode(seat_id="A3", row=1, col=3),
        ],
    )
    history = build_seat_history(students, layout, [_snapshot({"S1": "A1", "S2": "A3"}, students=students)])
    hard = HardRules(cannot_be_adjacent=[PairRule(students=("S1", "S2"))])

    solution = solve_seating(students, layout, _fair_rules(hard=hard), history=history)

    assert normalize_edge(solution.assignment_map["S1"], solution.assignment_map["S2"]) not in build_adjacency_edges(layout)


def test_ortools_solver_uses_fair_rotation_when_available(monkeypatch) -> None:
    pytest.importorskip("ortools.sat.python.cp_model")
    monkeypatch.setenv("SEATTRELLIS_USE_ORTOOLS", "1")
    monkeypatch.setattr(cp_sat, "cp_model", None)
    monkeypatch.setattr(cp_sat, "_cp_model_unavailable", False)
    students = _students(2)
    layout = _two_seat_layout()
    history = build_seat_history(students, layout, [_snapshot({"S1": "FRONT", "S2": "BACK"}, students=students)])

    solution = solve_seating(students, layout, _fair_rules(), history=history)

    assert solution.metrics["solver"] == "ortools-cp-sat"
    assert solution.assignment_map == {"S1": "BACK", "S2": "FRONT"}


def test_history_report_formats_summary_and_writes_json(tmp_path) -> None:
    students_csv = tmp_path / "students.csv"
    students_csv.write_text("student_id,name\nS1,Student001\nS2,Student002\n", encoding="utf-8")
    layout_path = write_json_model(_layout(), tmp_path / "classroom.json")
    history_path = write_json_model(_snapshot({"S1": "F1", "S2": "B2"}), tmp_path / "week1.snapshot.json")
    output_path = tmp_path / "history-report.json"

    output = cli.run_history_report(
        students_path=students_csv,
        layout_path=layout_path,
        history_paths=[history_path],
        output_path=output_path,
    )

    assert "History report" in output
    assert "- snapshots: 1" in output
    report = json.loads(output_path.read_text(encoding="utf-8"))
    assert report["history_count"] == 1
    assert report["students"][0]["student_key"] == "S1"


def test_fairness_report_contains_category_spread() -> None:
    history = build_seat_history(_students(2), _layout(), [_snapshot({"S1": "F1", "S2": "B2"})])

    report = build_fairness_report(history)

    assert report.summary["category_spread"]["front"]["spread"] == 1


def test_old_snapshot_without_metadata_still_loads(tmp_path) -> None:
    snapshot = _snapshot({"S1": "F1", "S2": "B2"})
    data = json.loads(snapshot.json())
    data.pop("metadata", None)
    path = tmp_path / "old.snapshot.json"
    path.write_text(json.dumps(data), encoding="utf-8")

    loaded = load_snapshot(path)

    assert loaded.metadata == {}
