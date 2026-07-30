from __future__ import annotations

from datetime import datetime, timezone

import pytest

from seattrellis.history import student_pair_key
from seattrellis.io.validation import validate_loaded_inputs
from seattrellis.models.history import (
    NeighborRelationType,
    PairHistory,
    PairHistoryRecord,
    StudentPairHistory,
)
from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode
from seattrellis.models.rules import (
    FixedSeatRule,
    HardRules,
    MentorPairingRule,
    RuleSet,
    ScoreDistributionRule,
    ScorePositionRule,
    SoftRules,
    WeightedRule,
)
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.presets import get_preset, preset_context_warnings
from seattrellis.scoring import score_snapshot
from seattrellis.solver import solve_seating
from seattrellis.solver.errors import SeatTrellisSolveError
from seattrellis.solver.registry import get_solver_backend
from seattrellis.solver.soft_objectives import (
    compile_soft_objectives,
    evaluate_soft_objectives,
    score_rank_percentiles,
)


def _students(scores: list[float]) -> list[Student]:
    return [
        Student(student_id=f"S{index}", name=f"Student {index}", score=score)
        for index, score in enumerate(scores, start=1)
    ]


def _layout(*, grouped: bool = True) -> ClassroomLayout:
    return ClassroomLayout(
        seats=[
            SeatNode(
                seat_id=f"R{row}C{col}",
                row=row,
                col=col,
                group_id=(f"G{col}" if grouped else None),
            )
            for row in (1, 2)
            for col in (1, 2)
        ],
        adjacency=AdjacencyConfig(include_horizontal=True, include_vertical=True),
    )


def _score_only_rules(**updates: object) -> RuleSet:
    disabled = WeightedRule(enabled=False, weight=0)
    soft = SoftRules(
        vision_front=disabled,
        height_back=disabled,
        randomize=disabled,
        score_balance=disabled,
        **updates,
    )
    return RuleSet(seed=7, soft=soft)


def test_rule_models_are_backward_compatible_and_validate_percentiles() -> None:
    rules = RuleSet.parse_obj({"seed": 12, "hard": {}, "soft": {}})

    assert rules.soft.score_position.enabled is False
    assert rules.soft.score_distribution.enabled is False
    assert rules.soft.mentor_pairing.enabled is False

    with pytest.raises(ValueError, match="lower than mentor"):
        MentorPairingRule(mentor_percentile=0.4, learner_percentile=0.5)


def test_score_percentiles_use_average_ranks_for_ties() -> None:
    percentiles = score_rank_percentiles(_students([40, 60, 60, 100]))

    assert percentiles["S1"] == 0
    assert percentiles["S2"] == pytest.approx(0.5)
    assert percentiles["S3"] == pytest.approx(0.5)
    assert percentiles["S4"] == 1


def test_position_and_row_distribution_share_normalized_objectives() -> None:
    students = _students([100, 80, 60, 40])
    layout = _layout()
    rules = _score_only_rules(
        score_position=ScorePositionRule(enabled=True, weight=10, direction="high_front"),
        score_distribution=ScoreDistributionRule(enabled=True, weight=10, scope="row"),
    )
    context = compile_soft_objectives(students, layout, rules)
    mixed_rows = {
        "S1": "R1C1",
        "S2": "R2C1",
        "S3": "R2C2",
        "S4": "R1C2",
    }
    separated_rows = {
        "S1": "R2C1",
        "S2": "R2C2",
        "S3": "R1C1",
        "S4": "R1C2",
    }

    mixed = evaluate_soft_objectives(mixed_rows, context, rules)
    separated = evaluate_soft_objectives(separated_rows, context, rules)

    assert mixed.losses["score_distribution"] == 0
    assert separated.losses["score_distribution"] > 0
    assert mixed.losses["score_position"] < separated.losses["score_position"]


def test_group_distribution_warns_and_stays_unavailable_without_group_ids() -> None:
    students = _students([100, 80, 60, 40])
    layout = _layout(grouped=False)
    rules = _score_only_rules(
        score_distribution=ScoreDistributionRule(enabled=True, weight=10, scope="group")
    )

    report = validate_loaded_inputs(students, layout, rules)
    context = compile_soft_objectives(students, layout, rules)
    evaluation = evaluate_soft_objectives(
        {student.key: seat.seat_id for student, seat in zip(students, layout.enabled_seats)},
        context,
        rules,
    )

    assert any("requires group_id" in warning for warning in report.warnings)
    assert evaluation.losses["score_distribution"] is None
    assert any("objective is unavailable" in warning for warning in evaluation.warnings)


def test_mentor_matching_is_deterministic_and_avoids_a_recent_repeat() -> None:
    students = _students([10, 20, 30, 40, 50, 60, 70, 80])
    layout = ClassroomLayout(
        seats=[SeatNode(seat_id=f"R1C{index}", row=1, col=index) for index in range(1, 9)]
    )
    repeated_key = student_pair_key("S1", "S8")
    history = PairHistory(
        history_count=1,
        pairs={
            repeated_key: StudentPairHistory(
                pair_key=repeated_key,
                first_student_key="S1",
                second_student_key="S8",
                records=[
                    PairHistoryRecord(
                        snapshot_index=1,
                        first_seat_id="R1C1",
                        second_seat_id="R1C2",
                        relations=[NeighborRelationType.DESK_MATE],
                    )
                ],
            )
        },
    )
    rules = _score_only_rules(
        mentor_pairing=MentorPairingRule(enabled=True, weight=20)
    )

    first = compile_soft_objectives(students, layout, rules, history)
    second = compile_soft_objectives(students, layout, rules, history)
    pairs = {(pair.mentor_key, pair.learner_key) for pair in first.mentor_pairs}

    assert first.mentor_pairs == second.mentor_pairs
    assert len(first.mentor_pairs) == 2
    assert ("S8", "S1") not in pairs


def test_fallback_score_position_keeps_hard_rules_and_orders_available_seats() -> None:
    students = _students([100, 80, 60, 40])
    layout = _layout()
    soft = _score_only_rules(
        score_position=ScorePositionRule(enabled=True, weight=20, direction="high_front")
    ).soft

    solution = solve_seating(
        students,
        layout,
        RuleSet(
            seed=7,
            hard=HardRules(fixed_seats=[FixedSeatRule(student="S1", seat_id="R2C1")]),
            soft=soft,
        ),
        backend="fallback",
    )

    assert solution.assignment_map["S1"] == "R2C1"
    assert solution.metrics["solver_backend_effective"] == "fallback"


def test_fallback_balances_rows_and_places_selected_mentor_pair_together() -> None:
    students = _students([100, 80, 60, 40])
    layout = _layout()
    rules = _score_only_rules(
        score_distribution=ScoreDistributionRule(enabled=True, weight=20, scope="row"),
        mentor_pairing=MentorPairingRule(enabled=True, weight=20),
    )

    solution = solve_seating(students, layout, rules, backend="fallback")
    assignment = solution.assignment_map
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}
    row_scores: dict[int, list[float]] = {}
    score_by_key = {student.key: float(student.score) for student in students}
    for student_key, seat_id in assignment.items():
        row_scores.setdefault(seat_by_id[seat_id].row, []).append(score_by_key[student_key])

    assert sum(row_scores[1]) / len(row_scores[1]) == pytest.approx(
        sum(row_scores[2]) / len(row_scores[2])
    )
    mentor_seat = seat_by_id[assignment["S1"]]
    learner_seat = seat_by_id[assignment["S4"]]
    assert mentor_seat.row == learner_seat.row
    assert abs(mentor_seat.col - learner_seat.col) == 1


def test_scoring_exposes_new_rule_dimensions_without_breaking_legacy_fields() -> None:
    students = _students([100, 80, 60, 40])
    layout = _layout()
    rules = _score_only_rules(
        score_position=ScorePositionRule(enabled=True, weight=10, direction="high_front"),
        score_distribution=ScoreDistributionRule(enabled=True, weight=10, scope="row"),
        mentor_pairing=MentorPairingRule(enabled=True, weight=10),
    )
    assignments = [
        SeatAssignment(student_key="S1", student_name="Student 1", seat_id="R1C1"),
        SeatAssignment(student_key="S2", student_name="Student 2", seat_id="R2C1"),
        SeatAssignment(student_key="S3", student_name="Student 3", seat_id="R2C2"),
        SeatAssignment(student_key="S4", student_name="Student 4", seat_id="R1C2"),
    ]
    snapshot = SeatingSnapshot(
        created_at=datetime.now(timezone.utc),
        seed=rules.seed,
        students=students,
        layout=layout,
        rules=rules,
        assignments=assignments,
        solver_status="FEASIBLE",
    )

    score = score_snapshot(snapshot)

    assert score.breakdown.score_balance_score.status == "not_available"
    assert set(score.breakdown.rule_scores) == {
        "score_position_score",
        "score_distribution_score",
        "mentor_pairing_score",
    }
    assert all(
        dimension.status == "available"
        for dimension in score.breakdown.rule_scores.values()
    )


def test_new_presets_are_composable_rulesets_and_balanced_remains_compatible() -> None:
    assert get_preset("peer_mixing").rules.soft.score_balance.enabled is True
    assert get_preset("balanced").rules.soft.score_balance.enabled is True
    assert get_preset("score-high-front").rules.soft.score_position.direction == "high_front"
    assert get_preset("row-score-balanced").rules.soft.score_distribution.scope == "row"
    assert get_preset("group-score-balanced").rules.soft.score_distribution.scope == "group"
    assert get_preset("mentor-pairing").rules.soft.mentor_pairing.enabled is True
    preset = get_preset("score-high-front")
    assert any(
        "missing preferred score data" in warning
        for warning in preset_context_warnings(
            preset,
            [Student(student_id="S1")],
            rules=preset.rules,
        )
    )


def test_ortools_rejects_new_goals_instead_of_silently_ignoring_them() -> None:
    rules = _score_only_rules(
        score_position=ScorePositionRule(enabled=True, weight=10)
    )

    with pytest.raises(SeatTrellisSolveError, match="does not yet support"):
        solve_seating(_students([100, 50]), _layout(), rules, backend="ortools")


def test_backend_capabilities_publish_the_staged_support_level() -> None:
    fallback_rules = get_solver_backend("fallback").capabilities.supported_soft_rules
    ortools_rules = get_solver_backend("ortools").capabilities.supported_soft_rules

    assert {"score_position", "score_distribution", "mentor_pairing"} <= fallback_rules
    assert "score_position" not in ortools_rules
