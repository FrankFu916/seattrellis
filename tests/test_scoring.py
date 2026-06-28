from __future__ import annotations

from datetime import datetime, timezone

import pytest

from seattrellis.models.candidate import (
    CandidatePlan,
    CandidateSet,
    HardConstraintSummary,
    PlanScore,
    ScoreBreakdown,
    ScoreDimension,
)
from seattrellis.models.history import (
    PairHistory,
    SeatHistory,
)
from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode
from seattrellis.models.rules import (
    AvoidRecentNeighborsRule,
    FairRotationRule,
    FixedSeatRule,
    HardRules,
    MinDistanceRule,
    PairRule,
    RuleSet,
    SoftRules,
    WeightedRule,
)
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student, student_needs_front
from seattrellis.scoring import (
    apply_diversity_scores,
    build_plan_comparison_report,
    evaluate_hard_constraints,
    refresh_recommendation,
    score_snapshot,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _disabled_rules() -> RuleSet:
    return RuleSet(
        seed=42,
        soft=SoftRules(
            vision_front=WeightedRule(enabled=False, weight=0),
            height_back=WeightedRule(enabled=False, weight=0),
            randomize=WeightedRule(enabled=False, weight=0),
            score_balance=WeightedRule(enabled=False, weight=0),
            fair_rotation=FairRotationRule(enabled=False, weight=0),
            avoid_recent_neighbors=AvoidRecentNeighborsRule(enabled=False, weight=0),
        ),
    )


def _not_available(reason: str = "none") -> ScoreDimension:
    return ScoreDimension(
        status="not_available",
        score=None,
        raw_value=None,
        weight=0,
        rating="not_available",
        details={"reason": reason},
    )


def _empty_breakdown() -> ScoreBreakdown:
    na = _not_available()
    return ScoreBreakdown(
        fair_rotation_score=na,
        avoid_recent_neighbors_score=na,
        score_balance_score=na,
        height_preference_score=na,
        vision_preference_score=na,
        diversity_score=na,
        stability_score=na,
        hard_constraint_summary=HardConstraintSummary(
            satisfied=True, checked_rule_count=0, violation_count=0, violations=[],
        ),
    )


def _layout(*, rows: int = 3, cols: int = 4) -> ClassroomLayout:
    seats = [
        SeatNode(
            seat_id=f"R{r}C{c}", row=r, col=c,
            near_window=(c == 1),
            near_door=(r == rows and c == cols),
            near_platform=(r == 1),
        )
        for r in range(1, rows + 1)
        for c in range(1, cols + 1)
    ]
    return ClassroomLayout(
        layout_id="test-room",
        name="Test Room",
        seats=seats,
        adjacency=AdjacencyConfig(
            include_horizontal=True,
            include_vertical=True,
            include_diagonal=False,
            max_row_delta=1,
            max_col_delta=1,
        ),
    )


def _students(count: int = 4) -> list[Student]:
    return [
        Student(
            student_id=f"S{i:02d}",
            name=f"Student{i:02d}",
            height_cm=150.0 + i * 10,
            score=90.0 - i * 5,
            vision="1.0",
        )
        for i in range(1, count + 1)
    ]


def _assignments(
    students: list[Student],
    layout: ClassroomLayout,
    offset: int = 0,
) -> list[SeatAssignment]:
    seats = layout.enabled_seats
    return [
        SeatAssignment(
            student_key=student.key,
            student_name=student.display_name,
            seat_id=seats[(idx + offset) % len(seats)].seat_id,
        )
        for idx, student in enumerate(students)
    ]


def _snapshot(
    *,
    students: list[Student] | None = None,
    layout: ClassroomLayout | None = None,
    rules: RuleSet | None = None,
    assignments: list[SeatAssignment] | None = None,
) -> SeatingSnapshot:
    st = students or _students()
    lay = layout or _layout()
    return SeatingSnapshot(
        created_at=datetime.now(timezone.utc),
        seed=42,
        students=st,
        layout=lay,
        rules=rules or _disabled_rules(),
        assignments=assignments or _assignments(st, lay),
        solver_status="FEASIBLE",
    )


# ---------------------------------------------------------------------------
# evaluate_hard_constraints
# ---------------------------------------------------------------------------

class TestEvaluateHardConstraints:
    def test_happy_path(self) -> None:
        students = _students(4)
        layout = _layout()
        assignments = _assignments(students, layout)
        rules = _disabled_rules()
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is True
        assert result.violation_count == 0

    def test_fixed_seat_violation(self) -> None:
        students = _students(4)
        layout = _layout()
        assignments = _assignments(students, layout)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(),
            hard=HardRules(
                fixed_seats=[FixedSeatRule(student="S01", seat_id="R99C99")],
            ),
        )
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is False
        assert any("fixed_seats" in v for v in result.violations)

    def test_duplicate_student_assignment(self) -> None:
        students = _students(2)
        layout = _layout()
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C2"),
        ]
        rules = _disabled_rules()
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is False
        assert any("more than once" in v for v in result.violations)

    def test_unknown_seat(self) -> None:
        students = _students(1)
        layout = _layout()
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="UNKNOWN"),
        ]
        rules = _disabled_rules()
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is False
        assert any("unknown" in v.lower() for v in result.violations)

    def test_must_be_adjacent_satisfied(self) -> None:
        students = _students(2)
        layout = _layout()
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S02", student_name="S02", seat_id="R1C2"),
        ]
        rules = RuleSet(
            seed=42,
            soft=SoftRules(),
            hard=HardRules(
                must_be_adjacent=[PairRule(students=("S01", "S02"))],
            ),
        )
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is True

    def test_cannot_be_adjacent_violated(self) -> None:
        students = _students(2)
        layout = _layout()
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S02", student_name="S02", seat_id="R1C2"),
        ]
        rules = RuleSet(
            seed=42,
            soft=SoftRules(),
            hard=HardRules(
                cannot_be_adjacent=[PairRule(students=("S01", "S02"))],
            ),
        )
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is False
        assert any("cannot_be_adjacent" in v for v in result.violations)

    def test_missing_student_in_assignments(self) -> None:
        students = _students(3)
        layout = _layout()
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S02", student_name="S02", seat_id="R1C2"),
        ]
        rules = _disabled_rules()
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is False
        assert any("every current student" in v for v in result.violations)

    def test_min_distance_graph_metric(self) -> None:
        students = _students(2)
        layout = _layout(rows=2, cols=1)
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S02", student_name="S02", seat_id="R2C1"),
        ]
        # R1C1 and R2C1 are vertically adjacent (distance 1 in graph), so min_distance 2 fails
        rules = RuleSet(
            seed=42,
            soft=SoftRules(),
            hard=HardRules(
                min_distance=[MinDistanceRule(students=("S01", "S02"), distance=2, metric="graph")],
            ),
        )
        result = evaluate_hard_constraints(assignments, students, layout, rules)
        assert result.satisfied is False


# ---------------------------------------------------------------------------
# score_snapshot — dimension-level tests
# ---------------------------------------------------------------------------

class TestScoreFairRotation:
    def test_not_available_when_disabled(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.fair_rotation_score.status == "not_available"

    def test_not_available_when_no_history(self) -> None:
        rules = RuleSet(
            seed=42,
            soft=SoftRules(fair_rotation=FairRotationRule(enabled=True, weight=10)),
        )
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.fair_rotation_score.status == "not_available"

    def test_available_with_history(self) -> None:
        students = _students(2)
        layout = _layout(rows=2, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                fair_rotation=FairRotationRule(enabled=True, weight=10),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        history = SeatHistory(
            history_count=1,
            students={},
            category_totals={"front": 1},
        )
        score = score_snapshot(snap, history=history)
        assert score.breakdown.fair_rotation_score.status == "available"


class TestScoreRecentNeighbors:
    def test_not_available_when_disabled(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.avoid_recent_neighbors_score.status == "not_available"

    def test_not_available_when_no_pair_history(self) -> None:
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                avoid_recent_neighbors=AvoidRecentNeighborsRule(enabled=True, weight=10),
            ),
        )
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.avoid_recent_neighbors_score.status == "not_available"

    def test_available_with_pair_history(self) -> None:
        students = _students(2)
        layout = _layout(rows=2, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                avoid_recent_neighbors=AvoidRecentNeighborsRule(enabled=True, weight=10),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        pair_history = PairHistory(
            history_count=1,
            student_count=2,
            pair_count=0,
            pairs={},
            relation_totals={},
        )
        score = score_snapshot(snap, pair_history=pair_history)
        assert score.breakdown.avoid_recent_neighbors_score.status == "available"


class TestScoreBalance:
    def test_not_available_when_disabled(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.score_balance_score.status == "not_available"

    def test_not_available_when_no_scores(self) -> None:
        students = [Student(student_id="S01", name="S01"), Student(student_id="S02", name="S02")]
        layout = _layout(rows=1, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                score_balance=WeightedRule(enabled=True, weight=5),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.score_balance_score.status == "not_available"

    def test_available_with_scores(self) -> None:
        students = [
            Student(student_id="S01", name="S01", score=90),
            Student(student_id="S02", name="S02", score=60),
        ]
        layout = _layout(rows=1, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                score_balance=WeightedRule(enabled=True, weight=5),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.score_balance_score.status == "available"

    def test_not_available_when_only_one_unique_score(self) -> None:
        students = [
            Student(student_id="S01", name="S01", score=80),
            Student(student_id="S02", name="S02", score=80),
        ]
        layout = _layout(rows=1, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                score_balance=WeightedRule(enabled=True, weight=5),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.score_balance_score.status == "not_available"


class TestScoreHeight:
    def test_not_available_when_disabled(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.height_preference_score.status == "not_available"

    def test_not_available_when_single_row(self) -> None:
        students = [
            Student(student_id="S01", name="S01", height_cm=150),
            Student(student_id="S02", name="S02", height_cm=180),
        ]
        layout = _layout(rows=1, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                height_back=WeightedRule(enabled=True, weight=5),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.height_preference_score.status == "not_available"

    def test_available_with_heights_and_rows(self) -> None:
        students = [
            Student(student_id="S01", name="S01", height_cm=150),
            Student(student_id="S02", name="S02", height_cm=190),
        ]
        layout = _layout(rows=2, cols=1)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                height_back=WeightedRule(enabled=True, weight=5),
            ),
        )
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S02", student_name="S02", seat_id="R2C1"),
        ]
        snap = _snapshot(students=students, layout=layout, rules=rules, assignments=assignments)
        score = score_snapshot(snap)
        assert score.breakdown.height_preference_score.status == "available"

    def test_not_available_when_same_height(self) -> None:
        students = [
            Student(student_id="S01", name="S01", height_cm=160),
            Student(student_id="S02", name="S02", height_cm=160),
        ]
        layout = _layout(rows=2, cols=1)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                height_back=WeightedRule(enabled=True, weight=5),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.height_preference_score.status == "not_available"


class TestScoreVision:
    def test_not_available_when_disabled(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.vision_preference_score.status == "not_available"

    def test_not_available_when_no_student_needs_front(self) -> None:
        students = [Student(student_id="S01", name="S01"), Student(student_id="S02", name="S02")]
        layout = _layout(rows=2, cols=1)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=True, weight=10),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.vision_preference_score.status == "not_available"

    def test_available_with_vision_students(self) -> None:
        students = [
            Student(student_id="S01", name="S01", tags="vision"),
            Student(student_id="S02", name="S02", vision="1.0"),
        ]
        layout = _layout(rows=2, cols=1)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=True, weight=10),
            ),
        )
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S02", student_name="S02", seat_id="R2C1"),
        ]
        snap = _snapshot(students=students, layout=layout, rules=rules, assignments=assignments)
        score = score_snapshot(snap)
        assert score.breakdown.vision_preference_score.status == "available"


class TestScoreStability:
    def test_not_available_when_no_latest(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.stability_score.status == "not_available"

    def test_full_stability(self) -> None:
        students = _students(2)
        layout = _layout(rows=1, cols=2)
        assignments = _assignments(students, layout)
        rules = _disabled_rules()
        latest_snap = _snapshot(students=students, layout=layout, rules=rules, assignments=assignments)
        snap = _snapshot(students=students, layout=layout, rules=rules, assignments=assignments)
        score = score_snapshot(snap, latest_snapshot=latest_snap)
        assert score.breakdown.stability_score.status == "available"
        assert score.breakdown.stability_score.score == 100.0

    def test_no_comparable_students(self) -> None:
        students_current = [Student(student_id="S01", name="S01")]
        students_prev = [Student(student_id="S99", name="S99")]
        layout = _layout(rows=1, cols=2)
        rules = _disabled_rules()
        prev_snap = _snapshot(students=students_prev, layout=layout, rules=rules)
        snap = _snapshot(students=students_current, layout=layout, rules=rules)
        score = score_snapshot(snap, latest_snapshot=prev_snap)
        assert score.breakdown.stability_score.status == "not_available"


class TestScoreDiversity:
    def test_not_available_when_none(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.breakdown.diversity_score.status == "not_available"

    def test_available_when_provided(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules), diversity_score=45.0)
        assert score.breakdown.diversity_score.status == "available"
        assert score.breakdown.diversity_score.score == 45.0

    def test_near_zero_diversity(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules), diversity_score=0.5)
        assert score.breakdown.diversity_score.status == "available"
        assert score.breakdown.diversity_score.score == 0.5

    @pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
    def test_rejects_non_finite_diversity(self, value: float) -> None:
        rules = _disabled_rules()
        with pytest.raises(ValueError, match="finite number"):
            score_snapshot(_snapshot(rules=rules), diversity_score=value)


# ---------------------------------------------------------------------------
# Total score
# ---------------------------------------------------------------------------

class TestTotalScore:
    def test_defaults_to_100_when_nothing_available(self) -> None:
        rules = _disabled_rules()
        score = score_snapshot(_snapshot(rules=rules))
        assert score.total == 100.0

    def test_weighted_total_with_available_dimensions(self) -> None:
        students = [
            Student(student_id="S01", name="S01", score=90),
            Student(student_id="S02", name="S02", score=60),
        ]
        layout = _layout(rows=1, cols=2)
        rules = RuleSet(
            seed=42,
            soft=SoftRules(
                height_back=WeightedRule(enabled=False, weight=0),
                vision_front=WeightedRule(enabled=False, weight=0),
                randomize=WeightedRule(enabled=False, weight=0),
                score_balance=WeightedRule(enabled=True, weight=5),
            ),
        )
        snap = _snapshot(students=students, layout=layout, rules=rules)
        score = score_snapshot(snap)
        assert score.breakdown.score_balance_score.status == "available"
        assert score.total > 0

    def test_zero_when_hard_constraints_violated(self) -> None:
        students = _students(2)
        layout = _layout()
        rules = _disabled_rules()
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C1"),
            SeatAssignment(student_key="S01", student_name="S01", seat_id="R1C2"),
        ]
        snap = _snapshot(students=students, layout=layout, rules=rules, assignments=assignments)
        score = score_snapshot(snap)
        assert score.total == 0.0


# ---------------------------------------------------------------------------
# apply_diversity_scores
# ---------------------------------------------------------------------------

class TestApplyDiversityScores:
    def test_noop_when_single_candidate(self) -> None:
        snap = _snapshot(rules=_disabled_rules())
        candidates = [
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap,
                score=PlanScore(total=100.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
        ]
        apply_diversity_scores(candidates)
        assert candidates[0].score.breakdown.diversity_score.status == "not_available"

    def test_diversity_with_two_candidates(self) -> None:
        students = _students(2)
        layout = _layout(rows=1, cols=2)
        seats = layout.enabled_seats
        snap1 = _snapshot(
            students=students,
            layout=layout,
            rules=_disabled_rules(),
            assignments=[
                SeatAssignment(student_key="S01", student_name="S01", seat_id=seats[0].seat_id),
                SeatAssignment(student_key="S02", student_name="S02", seat_id=seats[1].seat_id),
            ],
        )
        snap2 = _snapshot(
            students=students,
            layout=layout,
            rules=_disabled_rules(),
            assignments=[
                SeatAssignment(student_key="S01", student_name="S01", seat_id=seats[1].seat_id),
                SeatAssignment(student_key="S02", student_name="S02", seat_id=seats[0].seat_id),
            ],
        )
        candidates = [
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap1,
                score=PlanScore(total=100.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
            CandidatePlan(
                candidate_id="candidate_02",
                snapshot=snap2,
                score=PlanScore(total=100.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
        ]
        apply_diversity_scores(candidates)
        assert candidates[0].score.breakdown.diversity_score.status == "available"
        assert candidates[0].score.breakdown.diversity_score.score > 0

    def test_diversity_identical_assignments(self) -> None:
        students = _students(2)
        layout = _layout(rows=1, cols=2)
        seats = layout.enabled_seats
        assignments = [
            SeatAssignment(student_key="S01", student_name="S01", seat_id=seats[0].seat_id),
            SeatAssignment(student_key="S02", student_name="S02", seat_id=seats[1].seat_id),
        ]
        snap = _snapshot(students=students, layout=layout, rules=_disabled_rules(), assignments=assignments)
        candidates = [
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap,
                score=PlanScore(total=100.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
            CandidatePlan(
                candidate_id="candidate_02",
                snapshot=snap,
                score=PlanScore(total=100.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
        ]
        apply_diversity_scores(candidates)
        # Both have identical assignments → diversity = 0
        assert candidates[0].score.breakdown.diversity_score.score == 0


# ---------------------------------------------------------------------------
# refresh_recommendation
# ---------------------------------------------------------------------------

class TestRefreshRecommendation:
    def test_picks_highest_scoring(self) -> None:
        snap = _snapshot(rules=_disabled_rules())
        candidates = [
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap,
                score=PlanScore(total=80.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
            CandidatePlan(
                candidate_id="candidate_02",
                snapshot=snap,
                score=PlanScore(total=90.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
        ]
        cs = CandidateSet(
            metadata={"version": "test"},
            candidates=candidates,
            recommended_candidate_id="candidate_01",
        )
        refresh_recommendation(cs)
        assert cs.recommended_candidate_id == "candidate_02"

    def test_breaks_tie_by_candidate_id(self) -> None:
        snap = _snapshot(rules=_disabled_rules())
        candidates = [
            CandidatePlan(
                candidate_id="candidate_02",
                snapshot=snap,
                score=PlanScore(total=90.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap,
                score=PlanScore(total=90.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
        ]
        cs = CandidateSet(
            metadata={"version": "test"},
            candidates=candidates,
            recommended_candidate_id="candidate_02",
        )
        refresh_recommendation(cs)
        assert cs.recommended_candidate_id == "candidate_01"

    def test_errors_when_no_valid_candidates(self) -> None:
        snap = _snapshot(rules=_disabled_rules())
        candidates = [
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap,
                score=PlanScore(total=80.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=False,
            ),
        ]
        cs = CandidateSet(
            metadata={"version": "test"},
            candidates=candidates,
            recommended_candidate_id="candidate_01",
        )
        with pytest.raises(ValueError, match="No candidate satisfies"):
            refresh_recommendation(cs)


# ---------------------------------------------------------------------------
# build_plan_comparison_report
# ---------------------------------------------------------------------------

class TestBuildPlanComparisonReport:
    def test_returns_report_with_candidates(self) -> None:
        snap = _snapshot(rules=_disabled_rules())
        candidates = [
            CandidatePlan(
                candidate_id="candidate_01",
                snapshot=snap,
                score=PlanScore(total=80.0, breakdown=_empty_breakdown()),
                hard_constraints_satisfied=True,
            ),
        ]
        cs = CandidateSet(
            metadata={"version": "test"},
            candidates=candidates,
            recommended_candidate_id="candidate_01",
        )
        report = build_plan_comparison_report(cs)
        assert report.candidate_count == 1
        assert report.recommended_candidate_id == "candidate_01"
        assert len(report.candidates) == 1


# ---------------------------------------------------------------------------
# student_needs_front (extracted utility)
# ---------------------------------------------------------------------------

class TestStudentNeedsFront:
    def test_numeric_vision_below_one(self) -> None:
        s = Student(student_id="S01", name="S01", vision=0.6)
        assert student_needs_front(s) is True

    def test_numeric_vision_one_or_above(self) -> None:
        s = Student(student_id="S01", name="S01", vision=1.0)
        assert student_needs_front(s) is False
        s2 = Student(student_id="S02", name="S02", vision=1.5)
        assert student_needs_front(s2) is False

    def test_keyword_tags(self) -> None:
        s = Student(student_id="S01", name="S01", tags="vision")
        assert student_needs_front(s) is True
        s2 = Student(student_id="S02", name="S02", needs="front")
        assert student_needs_front(s2) is True

    def test_vision_string_tags(self) -> None:
        s = Student(student_id="S01", name="S01", vision="poor")
        assert student_needs_front(s) is True

    def test_no_markers(self) -> None:
        s = Student(student_id="S01", name="S01")
        assert student_needs_front(s) is False

    def test_non_numeric_vision_above_one(self) -> None:
        s = Student(student_id="S01", name="S01", vision="1.2")
        assert student_needs_front(s) is False

    def test_vision_as_string_poor(self) -> None:
        s = Student(student_id="S01", name="S01", vision="poor")
        assert student_needs_front(s) is True
