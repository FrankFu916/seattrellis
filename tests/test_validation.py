from __future__ import annotations

import pytest

from seattrellis.io.json_files import load_snapshot, write_json_model
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


def test_student_count_over_enabled_seats_fails_clearly() -> None:
    students = [Student(student_id=f"S{index}") for index in range(1, 4)]

    with pytest.raises(SeatTrellisSolveError, match="Not enough enabled seats"):
        solve_seating(students, _line_layout(2), _quiet_rules())


def test_duplicate_student_id_fails_clearly() -> None:
    with pytest.raises(ValueError, match="Duplicate student_id values: S1"):
        students_from_records([{"student_id": "S1", "name": "A"}, {"student_id": "S1", "name": "B"}])


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

    with pytest.raises(SeatTrellisSolveError, match="Unknown student reference"):
        solve_seating(students, _line_layout(1), rules)


def test_rule_reference_unknown_or_disabled_seat_fails() -> None:
    students = [Student(student_id="S1")]
    unknown_rules = _quiet_rules(HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="A9")]))
    disabled_rules = _quiet_rules(HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="A2")]))

    with pytest.raises(SeatTrellisSolveError, match="unknown or disabled"):
        solve_seating(students, _line_layout(1), unknown_rules)
    with pytest.raises(SeatTrellisSolveError, match="unknown or disabled"):
        solve_seating(students, _line_layout(2, disabled={2}), disabled_rules)


def test_conflicting_pair_rules_fail_before_solving() -> None:
    students = [Student(student_id="S1"), Student(student_id="S2")]
    rules = _quiet_rules(
        HardRules(
            must_be_adjacent=[PairRule(students=("S1", "S2"))],
            cannot_be_adjacent=[PairRule(students=("S2", "S1"))],
        )
    )

    with pytest.raises(SeatTrellisSolveError, match="both must_be_adjacent and cannot_be_adjacent"):
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
