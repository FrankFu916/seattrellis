from __future__ import annotations

import json
import subprocess

import pytest

from seattrellis import cli
from seattrellis.io.json_files import InputFileError, load_snapshot, write_json_model
from seattrellis.io.students import students_from_records
from seattrellis.models import ClassroomLayout, SeatNode, Student
from seattrellis.models.rules import FixedSeatRule, HardRules, PairRule, RuleSet, SoftRules, WeightedRule
from seattrellis.solver import SeatTrellisSolveError, solve_seating
from seattrellis.solver.adjacency import build_adjacency_edges, normalize_edge


def _quiet_rules(hard: HardRules | None = None) -> RuleSet:
    disabled = WeightedRule(enabled=False, weight=0)
    return RuleSet(
        seed=7,
        hard=hard or HardRules(),
        soft=SoftRules(
            vision_front=disabled,
            height_back=disabled,
            randomize=disabled,
            score_balance=disabled,
        ),
    )


def _line_layout(count: int, *, disabled: set[int] | None = None) -> ClassroomLayout:
    disabled = disabled or set()
    return ClassroomLayout(
        seats=[
            SeatNode(seat_id=f"A{index}", row=1, col=index, enabled=index not in disabled)
            for index in range(1, count + 1)
        ]
    )


def _write_json(path, data: dict) -> None:
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def _write_valid_students(path, count: int = 2) -> None:
    lines = ["student_id,name,height_cm,score"]
    for index in range(1, count + 1):
        lines.append(f"S{index},Student{index:03d},{150 + index},{80 + index}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def _write_valid_layout(path, *, enabled_count: int = 2, disabled: bool = False) -> None:
    seats = [
        {"seat_id": f"A{index}", "row": 1, "col": index, "enabled": True}
        for index in range(1, enabled_count + 1)
    ]
    if disabled:
        seats.append({"seat_id": "A_DISABLED", "row": 2, "col": 1, "enabled": False})
    _write_json(path, {"seats": seats, "adjacency": {"include_horizontal": True}})


def _write_rules(path, hard: dict | None = None, soft: dict | None = None) -> None:
    _write_json(path, {"hard": hard or {}, "soft": soft or {}})


def _validation_error(tmp_path, *, students_text: str | None = None, layout: dict | None = None, hard: dict | None = None) -> str:
    students_path = tmp_path / "students.csv"
    layout_path = tmp_path / "classroom.json"
    rules_path = tmp_path / "rules.json"
    if students_text is None:
        _write_valid_students(students_path)
    else:
        students_path.write_text(students_text, encoding="utf-8")
    if layout is None:
        _write_valid_layout(layout_path, enabled_count=2, disabled=True)
    else:
        _write_json(layout_path, layout)
    _write_rules(rules_path, hard=hard)

    with pytest.raises(InputFileError) as exc_info:
        cli.run_validate(students_path=students_path, layout_path=layout_path, rules_path=rules_path)
    return str(exc_info.value)


def test_validate_command_passes_for_demo_files() -> None:
    output = cli.run_validate(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        rules_path="examples/rules.json",
    )

    assert "Validation passed." in output
    assert "- students: 8" in output


def test_student_count_over_enabled_seats_fails_clearly() -> None:
    students = [Student(student_id=f"S{index}") for index in range(1, 4)]

    with pytest.raises(SeatTrellisSolveError, match="Not enough enabled seats"):
        solve_seating(students, _line_layout(2), _quiet_rules())


def test_duplicate_student_id_fails_clearly() -> None:
    with pytest.raises(ValueError, match='Row 3: column "student_id" is duplicated: S1'):
        students_from_records([{"student_id": "S1", "name": "A"}, {"student_id": "S1", "name": "B"}])


def test_validate_reports_duplicate_student_id_with_row(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        students_text="student_id,name\nS1,Student001\nS1,Student002\n",
    )

    assert 'Row 3: column "student_id" is duplicated: S1' in message


def test_validate_reports_empty_name(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        students_text="student_id,name\nS1,\n",
    )

    assert 'Row 2: column "name" cannot be empty.' in message


def test_validate_reports_invalid_numeric_student_field(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        students_text="student_id,name,height_cm,score\nS1,Student001,one seventy,90\n",
    )

    assert 'Row 2: column "height_cm" must be a number, got "one seventy".' in message


def test_validate_reports_too_few_enabled_seats(tmp_path) -> None:
    students_path = tmp_path / "students.csv"
    layout_path = tmp_path / "classroom.json"
    rules_path = tmp_path / "rules.json"
    _write_valid_students(students_path, count=3)
    _write_valid_layout(layout_path, enabled_count=2)
    _write_rules(rules_path)

    with pytest.raises(InputFileError) as exc_info:
        cli.run_validate(students_path=students_path, layout_path=layout_path, rules_path=rules_path)

    assert "Not enough enabled seats: 3 students but only 2 enabled seats." in str(exc_info.value)


def test_validate_reports_duplicate_seat_id(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        layout={
            "seats": [
                {"seat_id": "A1", "row": 1, "col": 1},
                {"seat_id": "A1", "row": 1, "col": 2},
            ]
        },
    )

    assert "Duplicate seat_id: A1" in message


def test_validate_reports_rule_reference_unknown_student(tmp_path) -> None:
    message = _validation_error(tmp_path, hard={"fixed_seats": [{"student": "S9", "seat_id": "A1"}]})

    assert 'hard.fixed_seats[1] references unknown student: "S9".' in message


def test_validate_reports_rule_reference_unknown_seat(tmp_path) -> None:
    message = _validation_error(tmp_path, hard={"fixed_seats": [{"student": "S1", "seat_id": "A9"}]})

    assert 'hard.fixed_seats[1] references unknown seat_id: "A9".' in message


def test_validate_reports_fixed_to_disabled_seat(tmp_path) -> None:
    message = _validation_error(tmp_path, hard={"fixed_seats": [{"student": "S1", "seat_id": "A_DISABLED"}]})

    assert 'hard.fixed_seats[1] fixes "S1" to disabled seat: "A_DISABLED".' in message


def test_validate_reports_student_fixed_to_multiple_seats(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        hard={
            "fixed_seats": [
                {"student": "S1", "seat_id": "A1"},
                {"student": "S1", "seat_id": "A2"},
            ]
        },
    )

    assert "Conflicting fixed seats:" in message
    assert "S1 is also fixed to A2" in message


def test_validate_reports_seat_fixed_to_multiple_students(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        hard={
            "fixed_seats": [
                {"student": "S1", "seat_id": "A1"},
                {"student": "S2", "seat_id": "A1"},
            ]
        },
    )

    assert "A seat can only be fixed to one student." in message


def test_validate_reports_must_and_cannot_adjacent_conflict(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        hard={
            "must_be_adjacent": [{"students": ["S1", "S2"]}],
            "cannot_be_adjacent": [{"students": ["S2", "S1"]}],
        },
    )

    assert "must be adjacent" in message
    assert "cannot be adjacent" in message
    assert "The same student pair cannot have both rules." in message


def test_validate_reports_min_distance_and_must_adjacent_conflict(tmp_path) -> None:
    message = _validation_error(
        tmp_path,
        hard={
            "must_be_adjacent": [{"students": ["S1", "S2"]}],
            "min_distance": [{"students": ["S1", "S2"], "distance": 2}],
        },
    )

    assert "No adjacent enabled seat pair can satisfy both rules." in message


def test_validate_reports_unknown_rule_field(tmp_path) -> None:
    message = _validation_error(tmp_path, hard={"fixed_seatz": []})

    assert "extra fields not permitted" in message


def test_fixed_seat_is_enforced() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    rules = _quiet_rules(HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="A2")]))

    solution = solve_seating(students, _line_layout(2), rules)

    assert solution.assignment_map["S1"] == "A2"


def test_cannot_be_adjacent_is_enforced() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    rules = _quiet_rules(HardRules(cannot_be_adjacent=[PairRule(students=("S1", "S2"))]))

    solution = solve_seating(students, _line_layout(3), rules)

    assert normalize_edge(solution.assignment_map["S1"], solution.assignment_map["S2"]) not in build_adjacency_edges(
        _line_layout(3)
    )


def test_must_be_adjacent_is_enforced() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    layout = _line_layout(3)
    rules = _quiet_rules(HardRules(must_be_adjacent=[PairRule(students=("S1", "S2"))]))

    solution = solve_seating(students, layout, rules)

    assert normalize_edge(solution.assignment_map["S1"], solution.assignment_map["S2"]) in build_adjacency_edges(layout)


def test_rule_reference_unknown_student_fails() -> None:
    students = [Student(student_id="S1")]
    rules = _quiet_rules(HardRules(fixed_seats=[FixedSeatRule(student="S9", seat_id="A1")]))

    with pytest.raises(SeatTrellisSolveError, match="references unknown student"):
        solve_seating(students, _line_layout(1), rules)


def test_rule_reference_unknown_or_disabled_seat_fails() -> None:
    students = [Student(student_id="S1")]
    unknown_rules = _quiet_rules(HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="A9")]))
    disabled_rules = _quiet_rules(HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="A2")]))

    with pytest.raises(SeatTrellisSolveError, match="references unknown seat_id"):
        solve_seating(students, _line_layout(1), unknown_rules)
    with pytest.raises(SeatTrellisSolveError, match="disabled seat"):
        solve_seating(students, _line_layout(2, disabled={2}), disabled_rules)


def test_fixed_cannot_adjacent_conflict_has_context() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    rules = _quiet_rules(
        HardRules(
            fixed_seats=[FixedSeatRule(student="S1", seat_id="A1"), FixedSeatRule(student="S2", seat_id="A2")],
            cannot_be_adjacent=[PairRule(students=("S1", "S2"))],
        )
    )

    with pytest.raises(SeatTrellisSolveError) as exc_info:
        solve_seating(students, _line_layout(2), rules)

    message = str(exc_info.value)
    assert "Conflicting hard constraints:" in message
    assert "S1 is fixed to A1" in message
    assert "A1 and A2 are adjacent seats" in message


def test_cli_infeasible_solution_outputs_diagnostic_summary(tmp_path) -> None:
    students_path = tmp_path / "students.csv"
    layout_path = tmp_path / "classroom.json"
    rules_path = tmp_path / "rules.json"
    _write_valid_students(students_path, count=3)
    _write_valid_layout(layout_path, enabled_count=3)
    _write_rules(
        rules_path,
        hard={
            "cannot_be_adjacent": [
                {"students": ["S1", "S2"]},
                {"students": ["S1", "S3"]},
                {"students": ["S2", "S3"]},
            ]
        },
    )

    result = subprocess.run(
        [
            "seattrellis",
            "solve",
            "--students",
            str(students_path),
            "--layout",
            str(layout_path),
            "--rules",
            str(rules_path),
            "--output",
            str(tmp_path / "snapshot.json"),
        ],
        check=False,
        text=True,
        capture_output=True,
    )

    assert result.returncode == 1
    assert "No feasible seating plan was found." in result.stderr
    assert "Summary:" in result.stderr
    assert "- students: 3" in result.stderr
    assert "No direct contradiction was detected before solving." in result.stderr
    assert "Traceback" not in result.stderr


def test_conflicting_pair_rules_fail_before_solving() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    rules = _quiet_rules(
        HardRules(
            must_be_adjacent=[PairRule(students=("S1", "S2"))],
            cannot_be_adjacent=[PairRule(students=("S2", "S1"))],
        )
    )

    with pytest.raises(SeatTrellisSolveError, match="The same student pair cannot have both rules"):
        solve_seating(students, _line_layout(2), rules)


def test_snapshot_can_be_saved_and_loaded_from_path_object(tmp_path) -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    layout = _line_layout(2)
    rules = _quiet_rules()
    solution = solve_seating(students, layout, rules)
    snapshot = solution.to_snapshot(students=students, layout=layout, rules=rules, seed=rules.seed)
    output = tmp_path / "nested folder" / "snapshot.json"

    write_json_model(snapshot, output)
    loaded = load_snapshot(output)

    assert loaded.assignments == snapshot.assignments
