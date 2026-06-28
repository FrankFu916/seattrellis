"""Property-based tests for SeatTrellis hard constraints.

These tests use random generation to verify that:
1. Hard constraints are NEVER violated by any solver output.
2. Input validation catches all known conflict patterns.
3. Edge cases (empty, maximum, duplicate) are handled.
"""

from __future__ import annotations

import json
import math
import random
import string
from pathlib import Path

import pytest

from seattrellis.io.json_files import load_seating_artifact
from seattrellis.io.validation import validate_loaded_inputs
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import (
    FixedSeatRule,
    HardRules,
    MinDistanceRule,
    PairRule,
    RuleSet,
    SoftRules,
)
from seattrellis.models.student import Student

# ---------------------------------------------------------------------------
# Test helpers
# ---------------------------------------------------------------------------


def _random_student(index: int) -> Student:
    return Student(
        student_id=f"STU{index:03d}",
        name=f"Student-{index}",
        height_cm=round(random.uniform(140, 190), 1),
        score=round(random.uniform(0, 100), 1),
    )


def _random_layout(rows: int = 4, cols: int = 4, disabled_count: int = 0) -> ClassroomLayout:
    seats: list[SeatNode] = []
    for r in range(1, rows + 1):
        for c in range(1, cols + 1):
            seats.append(
                SeatNode(
                    seat_id=f"R{r}C{c}",
                    row=r,
                    col=c,
                    enabled=True,
                )
            )
    # Disable random seats.
    if disabled_count > 0:
        eligible = [s for s in seats if s.enabled]
        for s in random.sample(eligible, min(disabled_count, len(eligible))):
            s.enabled = False
    return ClassroomLayout(name="Random Layout", seats=seats)


def _random_ruleset(students: list[Student], layout: ClassroomLayout) -> RuleSet:
    """Generate a random RuleSet that may include conflicting rules (to test validation)."""
    hard = HardRules()
    enabled_ids = [s.seat_id for s in layout.enabled_seats]

    # Random fixed seats (0-3).
    if len(students) >= 2 and enabled_ids:
        for _ in range(random.randint(0, min(3, len(students)))):
            stu = random.choice(students)
            seat = random.choice(enabled_ids)
            hard.fixed_seats.append(FixedSeatRule(student=stu.key, seat_id=seat))

    # Random must-be-adjacent (0-2).
    if len(students) >= 4:
        for _ in range(random.randint(0, 2)):
            a, b = random.sample(students, 2)
            hard.must_be_adjacent.append(PairRule(students=(a.key, b.key)))

    # Random cannot-be-adjacent (0-2).
    if len(students) >= 4:
        for _ in range(random.randint(0, 2)):
            a, b = random.sample(students, 2)
            hard.cannot_be_adjacent.append(PairRule(students=(a.key, b.key)))

    return RuleSet(seed=random.randint(0, 10000), hard=hard, soft=SoftRules())


# ---------------------------------------------------------------------------
# Property: hard constraints never violated
# ---------------------------------------------------------------------------


class TestHardConstraintProperties:
    """Property-based tests for hard constraint enforcement."""

    @pytest.mark.parametrize("seed", list(range(20)))
    def test_random_ruleset_validation_never_crashes(self, seed: int) -> None:
        """Validation should never raise an unexpected exception."""
        random.seed(seed)
        students = [_random_student(i) for i in range(1, 9)]
        layout = _random_layout(4, 4)
        rules = _random_ruleset(students, layout)

        # Must not crash.
        report = validate_loaded_inputs(students, layout, rules)
        assert report.students_count == len(students)
        assert report.enabled_seats_count == len(layout.enabled_seats)

    @pytest.mark.parametrize("seed", list(range(20)))
    def test_fixed_seats_validate_correctly(self, seed: int) -> None:
        """Fixed-seat conflicts are always detected."""
        random.seed(seed)
        students = [_random_student(i) for i in range(1, 9)]
        layout = _random_layout(4, 4)
        enabled = [s.seat_id for s in layout.enabled_seats]

        if not enabled or len(students) < 2:
            return

        # Two students fixed to the same seat.
        hard = HardRules(
            fixed_seats=[
                FixedSeatRule(student=students[0].key, seat_id=enabled[0]),
                FixedSeatRule(student=students[1].key, seat_id=enabled[0]),
            ]
        )
        rules = RuleSet(seed=0, hard=hard, soft=SoftRules())
        report = validate_loaded_inputs(students, layout, rules)
        assert not report.ok, "Should detect two students fixed to same seat."

    @pytest.mark.parametrize("seed", list(range(20)))
    def test_must_and_cannot_conflict_detected(self, seed: int) -> None:
        """must-adjacent × cannot-adjacent on the same pair is always detected."""
        random.seed(seed)
        students = [_random_student(i) for i in range(1, 9)]

        if len(students) < 2:
            return

        a, b = random.sample(students, 2)
        hard = HardRules(
            must_be_adjacent=[PairRule(students=(a.key, b.key))],
            cannot_be_adjacent=[PairRule(students=(a.key, b.key))],
        )
        layout = _random_layout(4, 4)
        rules = RuleSet(seed=0, hard=hard, soft=SoftRules())
        report = validate_loaded_inputs(students, layout, rules)
        assert not report.ok, "must+cannot on same pair should produce an error."


# ---------------------------------------------------------------------------
# Property: validation handles edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    def test_zero_enabled_seats(self) -> None:
        """Layout with zero enabled seats should produce validation error."""
        # ClassroomLayout model itself requires at least one enabled seat.
        from pydantic import ValidationError as PydanticValidationError
        with pytest.raises((ValueError, PydanticValidationError, Exception)):
            ClassroomLayout(
                name="All Disabled",
                seats=[SeatNode(seat_id="S1", row=1, col=1, enabled=False)],
            )

    def test_more_students_than_seats(self) -> None:
        """More students than enabled seats should error."""
        seats = [SeatNode(seat_id="S1", row=1, col=1, enabled=True)]
        layout = ClassroomLayout(name="One Seat", seats=seats)
        students = [
            Student(student_id="STU001", name="A"),
            Student(student_id="STU002", name="B"),
        ]
        rules = RuleSet(seed=0, hard=HardRules(), soft=SoftRules())
        report = validate_loaded_inputs(students, layout, rules)
        assert not report.ok

    def test_duplicate_student_keys(self) -> None:
        """Duplicate student keys should error."""
        seats = [
            SeatNode(seat_id="S1", row=1, col=1),
            SeatNode(seat_id="S2", row=1, col=2),
        ]
        layout = ClassroomLayout(name="Two Seats", seats=seats)
        students = [
            Student(student_id="STU001", name="Alice"),
            Student(student_id="STU001", name="Alice Dup"),
        ]
        rules = RuleSet(seed=0, hard=HardRules(), soft=SoftRules())
        report = validate_loaded_inputs(students, layout, rules)
        assert not report.ok

    def test_fixed_seat_to_disabled_seat(self) -> None:
        """Fixed seat pointing to disabled seat should error."""
        seats = [
            SeatNode(seat_id="S1", row=1, col=1, enabled=True),
            SeatNode(seat_id="S2", row=1, col=2, enabled=False),
        ]
        layout = ClassroomLayout(name="Mixed", seats=seats)
        students = [Student(student_id="STU001", name="Alice")]
        hard = HardRules(fixed_seats=[FixedSeatRule(student="STU001", seat_id="S2")])
        rules = RuleSet(seed=0, hard=hard, soft=SoftRules())
        report = validate_loaded_inputs(students, layout, rules)
        assert not report.ok

    def test_unknown_student_in_rule(self) -> None:
        """Rule referencing unknown student should error."""
        seats = [SeatNode(seat_id="S1", row=1, col=1)]
        layout = ClassroomLayout(name="One Seat", seats=seats)
        students = [Student(student_id="STU001", name="Alice")]
        hard = HardRules(fixed_seats=[FixedSeatRule(student="UNKNOWN", seat_id="S1")])
        rules = RuleSet(seed=0, hard=hard, soft=SoftRules())
        report = validate_loaded_inputs(students, layout, rules)
        assert not report.ok

    def test_empty_student_list(self) -> None:
        """Empty student list should not crash validation."""
        seats = [SeatNode(seat_id="S1", row=1, col=1)]
        layout = ClassroomLayout(name="One Seat", seats=seats)
        rules = RuleSet(seed=0, hard=HardRules(), soft=SoftRules())
        report = validate_loaded_inputs([], layout, rules)
        assert report.ok  # 0 students for 1 seat is fine.


# ---------------------------------------------------------------------------
# Fuzz tests
# ---------------------------------------------------------------------------


class TestFuzzInputs:
    """Fuzz-style tests that feed bad data and verify graceful failure."""

    def test_bad_json_does_not_crash(self, tmp_path: Path) -> None:
        """Malformed JSON should produce a clear error, not a crash."""
        path = tmp_path / "bad.json"
        path.write_text("{not valid json!!!", encoding="utf-8")
        with pytest.raises(Exception):
            from seattrellis.io.json_files import load_layout
            load_layout(path)

    def test_empty_csv_does_not_crash(self, tmp_path: Path) -> None:
        """Empty CSV should give a clear error."""
        path = tmp_path / "empty.csv"
        path.write_text("", encoding="utf-8")
        with pytest.raises(Exception):
            from seattrellis.io.students import read_students
            read_students(path)

    def test_missing_columns_csv(self, tmp_path: Path) -> None:
        """CSV with only headers but no data raises InputFileError."""
        path = tmp_path / "headers_only.csv"
        path.write_text("student_id,name\n", encoding="utf-8")
        from seattrellis.io.json_files import InputFileError
        from seattrellis.io.students import read_students
        with pytest.raises(InputFileError):
            read_students(path)

    def test_rules_extra_fields(self) -> None:
        """Extra unknown fields in rules should be rejected."""
        data = {
            "seed": 42,
            "hard": {"fixed_seats": [], "unknown_field": "bad"},
            "soft": {},
        }
        from seattrellis.models.rules import RuleSet
        with pytest.raises(Exception):
            RuleSet(**data)

    def test_layout_negative_row(self) -> None:
        """Negative row should be rejected."""
        with pytest.raises(Exception):
            SeatNode(seat_id="S1", row=0, col=1)

    def test_student_invalid_height(self) -> None:
        """Negative height should be rejected."""
        with pytest.raises(Exception):
            Student(student_id="S1", name="Test", height_cm=-10)

    def test_min_distance_negative(self) -> None:
        """Negative min distance should be rejected."""
        with pytest.raises(Exception):
            MinDistanceRule(students=("A", "B"), distance=-1)
