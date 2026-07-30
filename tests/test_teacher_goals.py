from __future__ import annotations

import pytest

from seattrellis.application.teacher_goals import (
    TeacherGoalSelection,
    get_teacher_goal,
    list_teacher_goals,
    resolve_teacher_goal,
)
from seattrellis.models import ClassroomLayout, RuleSet, SeatNode, Student
from seattrellis.presets import get_preset
from seattrellis.service import compute_solve
from seattrellis.service_types import SolveInput


def test_teacher_goals_have_stable_ids_and_preset_mappings() -> None:
    goals = list_teacher_goals()

    assert [goal.goal_id for goal in goals] == [
        "daily-rotation",
        "fair-shuffle",
        "peer-support",
        "custom",
    ]
    assert [goal.preset_name for goal in goals] == [
        "daily",
        "fair-rotation",
        "balanced",
        None,
    ]
    assert all(goal.default_candidate_count == 3 for goal in goals)
    assert "neighboring seats" in get_teacher_goal("peer_support").description
    assert "group" not in get_teacher_goal("peer-support").description.lower()


def test_builtin_goal_returns_an_independent_ruleset_each_time() -> None:
    first = resolve_teacher_goal(
        TeacherGoalSelection(goal_id="fair-shuffle"),
        students=[],
    )
    second = resolve_teacher_goal(
        TeacherGoalSelection(goal_id="fair-shuffle"),
        students=[],
    )

    first.rules.soft.fair_rotation.weight = 1

    assert first.preset_name == "fair-rotation"
    assert second.rules.soft.fair_rotation.weight == 20
    assert get_preset("fair-rotation").rules.soft.fair_rotation.weight == 20


def test_daily_rotation_reports_only_unavailable_preferred_data() -> None:
    complete_students = [
        Student(
            student_id="S1",
            score=95,
            height_cm=160,
            needs=["vision_front"],
        ),
        Student(student_id="S2", score=70, height_cm=175),
    ]

    missing = resolve_teacher_goal(
        TeacherGoalSelection(),
        students=[Student(student_id="S1"), Student(student_id="S2")],
    )
    complete = resolve_teacher_goal(
        TeacherGoalSelection(),
        students=complete_students,
        history_count=1,
    )

    assert len(missing.warnings) == 4
    assert any("history data" in warning for warning in missing.warnings)
    assert any("score data" in warning for warning in missing.warnings)
    assert any("height data" in warning for warning in missing.warnings)
    assert any("vision data" in warning for warning in missing.warnings)
    assert complete.warnings == ()


def test_peer_support_describes_score_mixing_and_requires_score_variation() -> None:
    missing = resolve_teacher_goal(
        TeacherGoalSelection(goal_id="peer-support"),
        students=[Student(student_id="S1"), Student(student_id="S2")],
    )
    available = resolve_teacher_goal(
        TeacherGoalSelection(goal_id="peer-support"),
        students=[
            Student(student_id="S1", score=90),
            Student(student_id="S2", score=70),
        ],
    )

    assert len(missing.warnings) == 1
    assert "score data" in missing.warnings[0]
    assert available.warnings == ()


def test_custom_goal_requires_rules_and_copies_them() -> None:
    with pytest.raises(ValueError, match="requires custom_rules"):
        resolve_teacher_goal(
            TeacherGoalSelection(goal_id="custom"),
            students=[],
        )

    source = RuleSet(seed=17)
    resolved = resolve_teacher_goal(
        TeacherGoalSelection(goal_id="custom", custom_rules=source),
        students=[],
    )
    resolved.rules.seed = 99

    assert resolved.preset_name is None
    assert resolved.warnings == ()
    assert source.seed == 17


def test_non_custom_goal_rejects_custom_rules() -> None:
    with pytest.raises(ValueError, match="only be used with the custom"):
        resolve_teacher_goal(
            TeacherGoalSelection(custom_rules=RuleSet()),
            students=[],
        )


def test_resolve_teacher_goal_rejects_negative_history_count() -> None:
    with pytest.raises(ValueError, match="history_count must be non-negative"):
        resolve_teacher_goal(
            TeacherGoalSelection(),
            students=[],
            history_count=-1,
        )


def test_compute_solve_reports_preset_warnings_for_in_memory_requests() -> None:
    result = compute_solve(
        SolveInput(
            students=[Student(student_id="S1"), Student(student_id="S2")],
            layout=ClassroomLayout(
                seats=[
                    SeatNode(seat_id="A1", row=1, col=1),
                    SeatNode(seat_id="A2", row=1, col=2),
                ]
            ),
            rules=get_preset("balanced").rules.copy(deep=True),
            preset_name="balanced",
            backend="fallback",
        )
    )

    assert result.preset_warnings
    assert len(result.preset_warnings) == 1
    assert "score data" in result.preset_warnings[0]
    assert result.preset_warnings == result.warnings
    assert result.candidate_set.warnings == result.warnings
