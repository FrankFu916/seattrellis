from __future__ import annotations

from itertools import combinations
from statistics import mean
from typing import Sequence

from seattrellis.history import (
    avoid_recent_neighbors_cost,
    build_pair_history,
    build_seat_history,
    detect_neighbor_relation_types,
    fair_rotation_cost,
)
from seattrellis.models.candidate import (
    CandidatePlan,
    CandidateSet,
    HardConstraintSummary,
    PlanComparisonEntry,
    PlanComparisonReport,
    PlanScore,
    ScoreBreakdown,
    ScoreDimension,
)
from seattrellis.models.history import PairHistory, SeatHistory
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student, student_needs_front
from seattrellis.solver.adjacency import build_adjacency_edges, graph_distance, normalize_edge, seat_distance


DIMENSION_LABELS = {
    "fair_rotation_score": "fair rotation",
    "avoid_recent_neighbors_score": "avoid repeated neighbors",
    "score_balance_score": "score balance",
    "height_preference_score": "height preference",
    "vision_preference_score": "vision preference",
    "diversity_score": "plan diversity",
    "stability_score": "stability",
}


def score_snapshot(
    snapshot: SeatingSnapshot,
    *,
    history: SeatHistory | None = None,
    pair_history: PairHistory | None = None,
    latest_snapshot: SeatingSnapshot | None = None,
    diversity_score: float | None = None,
) -> PlanScore:
    students = snapshot.students
    layout = snapshot.layout
    rules = snapshot.rules
    assignments = snapshot.assignments
    pair_history = pair_history or (history.pair_history if history is not None else None)

    breakdown = ScoreBreakdown(
        fair_rotation_score=_score_fair_rotation(assignments, students, layout, rules, history),
        avoid_recent_neighbors_score=_score_recent_neighbors(
            assignments, layout, rules, pair_history
        ),
        score_balance_score=_score_balance(assignments, students, layout, rules),
        height_preference_score=_score_height(assignments, students, layout, rules),
        vision_preference_score=_score_vision(assignments, students, layout, rules),
        diversity_score=(
            _available_dimension(
                diversity_score,
                raw_value=diversity_score,
                weight=max(1, rules.soft.randomize.weight),
                details={"meaning": "Mean percentage of students seated differently from the other candidates."},
            )
            if diversity_score is not None
            else _not_available("Diversity requires at least two generated candidates.")
        ),
        stability_score=_score_stability(assignments, latest_snapshot),
        hard_constraint_summary=evaluate_hard_constraints(assignments, students, layout, rules),
    )
    return PlanScore(total=_weighted_total(breakdown), breakdown=breakdown)


def evaluate_hard_constraints(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
) -> HardConstraintSummary:
    violations: list[str] = []
    assignment_by_student = {assignment.student_key: assignment.seat_id for assignment in assignments}
    assigned_students = [assignment.student_key for assignment in assignments]
    assigned_seats = [assignment.seat_id for assignment in assignments]
    enabled_seats = {seat.seat_id for seat in layout.enabled_seats}
    reference_map = _student_reference_map(students)
    edges = build_adjacency_edges(layout)
    checked = 3

    if len(assigned_students) != len(set(assigned_students)):
        violations.append("A student is assigned more than once.")
    if len(assigned_seats) != len(set(assigned_seats)):
        violations.append("A seat is assigned more than once.")
    expected_students = {student.key for student in students}
    if set(assigned_students) != expected_students:
        violations.append("Assignments do not contain every current student exactly once.")
    unknown_seats = sorted(set(assigned_seats) - enabled_seats)
    if unknown_seats:
        violations.append(f"Assignments use unknown or disabled seats: {', '.join(unknown_seats)}.")

    for rule in rules.hard.fixed_seats:
        checked += 1
        student_key = reference_map.get(rule.student)
        if student_key is None or assignment_by_student.get(student_key) != rule.seat_id:
            violations.append(f"fixed_seats is not satisfied for {rule.student!r}.")

    for label, pair_rules, expected_adjacent in (
        ("must_be_adjacent", rules.hard.must_be_adjacent, True),
        ("cannot_be_adjacent", rules.hard.cannot_be_adjacent, False),
    ):
        for rule in pair_rules:
            checked += 1
            first_key = reference_map.get(rule.students[0])
            second_key = reference_map.get(rule.students[1])
            first_seat = assignment_by_student.get(first_key or "")
            second_seat = assignment_by_student.get(second_key or "")
            adjacent = (
                first_seat is not None
                and second_seat is not None
                and normalize_edge(first_seat, second_seat) in edges
            )
            if adjacent != expected_adjacent:
                violations.append(f"{label} is not satisfied for {rule.students!r}.")

    seat_by_id = {seat.seat_id: seat for seat in layout.seats}
    for rule in rules.hard.min_distance:
        checked += 1
        first_key = reference_map.get(rule.students[0])
        second_key = reference_map.get(rule.students[1])
        first_seat = seat_by_id.get(assignment_by_student.get(first_key or "", ""))
        second_seat = seat_by_id.get(assignment_by_student.get(second_key or "", ""))
        if first_seat is None or second_seat is None:
            violations.append(f"min_distance cannot be evaluated for {rule.students!r}.")
            continue
        distance = (
            graph_distance(layout, first_seat.seat_id, second_seat.seat_id)
            if rule.metric == "graph"
            else seat_distance(first_seat, second_seat)
        )
        if distance < rule.distance:
            violations.append(f"min_distance is not satisfied for {rule.students!r}.")

    return HardConstraintSummary(
        satisfied=not violations,
        checked_rule_count=checked,
        violation_count=len(violations),
        violations=violations,
        details={
            "student_count": len(students),
            "assignment_count": len(assignments),
            "fixed_seat_count": len(rules.hard.fixed_seats),
            "must_be_adjacent_count": len(rules.hard.must_be_adjacent),
            "cannot_be_adjacent_count": len(rules.hard.cannot_be_adjacent),
            "min_distance_count": len(rules.hard.min_distance),
        },
    )


def apply_diversity_scores(candidates: list[CandidatePlan]) -> None:
    if len(candidates) < 2:
        return
    assignment_maps = [
        {
            assignment.student_key: assignment.seat_id
            for assignment in candidate.snapshot.assignments
        }
        for candidate in candidates
    ]
    for index, candidate in enumerate(candidates):
        distances = [
            _assignment_distance(assignment_maps[index], other_map)
            for other_index, other_map in enumerate(assignment_maps)
            if other_index != index
        ]
        diversity = mean(distances) if distances else 0.0
        candidate.score.breakdown.diversity_score = _available_dimension(
            diversity,
            raw_value=diversity,
            weight=max(1, candidate.snapshot.rules.soft.randomize.weight),
            details={
                "meaning": "Mean percentage of students seated differently from the other candidates.",
                "compared_candidate_count": len(distances),
            },
        )
        candidate.score.total = _weighted_total(candidate.score.breakdown)


def build_plan_comparison_report(
    candidate_set: CandidateSet,
    *,
    history_snapshots: Sequence[SeatingSnapshot] | None = None,
) -> PlanComparisonReport:
    baseline = _history_baseline_scores(
        candidate_set.candidates[0].snapshot.students,
        candidate_set.candidates[0].snapshot.layout,
        candidate_set.candidates[0].snapshot.rules,
        list(history_snapshots or []),
    )
    entries: list[PlanComparisonEntry] = []
    for candidate in candidate_set.candidates:
        dimensions = _dimension_map(candidate.score.breakdown)
        available = [
            (name, dimension.score)
            for name, dimension in dimensions.items()
            if dimension.status == "available" and dimension.score is not None
        ]
        ranked = sorted(available, key=lambda item: (-item[1], item[0]))
        low_ranked = sorted(available, key=lambda item: (item[1], item[0]))
        advantages = [
            f"{DIMENSION_LABELS[name]}: {_rating_text(score)}"
            for name, score in ranked[:2]
            if score >= 65
        ]
        if not advantages and ranked:
            name, score = ranked[0]
            advantages = [f"best relative dimension is {DIMENSION_LABELS[name]} ({score:.1f})"]
        costs = [
            f"{DIMENSION_LABELS[name]}: {_rating_text(score)}"
            for name, score in low_ranked[:2]
            if score < 65
        ]
        history_comparison = {
            "fair_rotation": _compare_with_baseline(
                candidate.score.breakdown.fair_rotation_score, baseline.get("fair_rotation_score")
            ),
            "avoid_recent_neighbors": _compare_with_baseline(
                candidate.score.breakdown.avoid_recent_neighbors_score,
                baseline.get("avoid_recent_neighbors_score"),
            ),
        }
        entries.append(
            PlanComparisonEntry(
                candidate_id=candidate.candidate_id,
                total_score=candidate.total_score,
                hard_constraints_satisfied=candidate.hard_constraints_satisfied,
                dimension_scores={
                    name: dimension.score for name, dimension in dimensions.items()
                },
                advantages=advantages,
                costs=costs,
                history_comparison=history_comparison,
            )
        )
    return PlanComparisonReport(
        candidate_count=len(entries),
        recommended_candidate_id=candidate_set.recommended_candidate_id,
        candidates=entries,
        warnings=candidate_set.warnings,
        metadata={
            "recommendation_method": (
                "Highest weighted total among hard-constraint-satisfying candidates; "
                "ties are resolved by candidate_id."
            ),
            "history_baseline": (
                "Latest historical snapshot scored against the snapshots before it."
                if baseline
                else "not_available"
            ),
        },
    )


def refresh_recommendation(candidate_set: CandidateSet) -> None:
    eligible = [candidate for candidate in candidate_set.candidates if candidate.hard_constraints_satisfied]
    if not eligible:
        raise ValueError("No candidate satisfies all hard constraints.")
    recommended = sorted(eligible, key=lambda candidate: (-candidate.total_score, candidate.candidate_id))[0]
    candidate_set.recommended_candidate_id = recommended.candidate_id


def _score_fair_rotation(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
) -> ScoreDimension:
    rule = rules.soft.fair_rotation
    if not rule.enabled or rule.weight == 0:
        return _not_available("fair_rotation is disabled.")
    if history is None or history.history_count == 0:
        return _not_available("No history snapshots were supplied.")
    student_by_key = {student.key: student for student in students}
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}
    penalty = 0
    for assignment in assignments:
        student = student_by_key.get(assignment.student_key)
        seat = seat_by_id.get(assignment.seat_id)
        if student is not None and seat is not None:
            penalty += fair_rotation_cost(student, seat, layout, rule, history)
    penalty_units = penalty / max(1, rule.weight * 100)
    score = 100 / (1 + penalty_units / max(1, len(assignments)))
    return _available_dimension(
        score,
        raw_value=float(penalty),
        weight=rule.weight,
        details={
            "penalty_cost": penalty,
            "history_count": history.history_count,
            "lookback": rule.lookback,
            "lower_penalty_is_better": True,
        },
    )


def _score_recent_neighbors(
    assignments: Sequence[SeatAssignment],
    layout: ClassroomLayout,
    rules: RuleSet,
    pair_history: PairHistory | None,
) -> ScoreDimension:
    rule = rules.soft.avoid_recent_neighbors
    if not rule.enabled or rule.weight == 0:
        return _not_available("avoid_recent_neighbors is disabled.")
    if pair_history is None or pair_history.history_count == 0:
        return _not_available("No pair history was supplied.")
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}
    edges = build_adjacency_edges(layout)
    penalty = 0
    relevant_pairs = 0
    selected_relations = set(rule.relation_types)
    for first, second in combinations(assignments, 2):
        first_seat = seat_by_id.get(first.seat_id)
        second_seat = seat_by_id.get(second.seat_id)
        if first_seat is None or second_seat is None:
            continue
        current_relations = detect_neighbor_relation_types(
            first_seat,
            second_seat,
            layout,
            adjacency_edges=edges,
            within_distance=rule.within_distance,
        )
        if current_relations & selected_relations:
            relevant_pairs += 1
        penalty += avoid_recent_neighbors_cost(
            first.student_key,
            second.student_key,
            first_seat,
            second_seat,
            layout,
            rule,
            pair_history,
            adjacency_edges=edges,
        )
    excess_units = penalty / max(1, rule.weight * 100)
    score = 100 / (1 + excess_units / max(1, relevant_pairs))
    return _available_dimension(
        score,
        raw_value=float(penalty),
        weight=rule.weight,
        details={
            "penalty_cost": penalty,
            "relevant_current_pairs": relevant_pairs,
            "history_count": pair_history.history_count,
            "lookback": rule.lookback,
            "lower_penalty_is_better": True,
        },
    )


def _score_balance(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
) -> ScoreDimension:
    rule = rules.soft.score_balance
    if not rule.enabled or rule.weight == 0:
        return _not_available("score_balance is disabled.")
    scores = {student.key: float(student.score) for student in students if student.score is not None}
    if len(scores) < 2 or max(scores.values()) == min(scores.values()):
        return _not_available("At least two different student scores are required.")
    assignment_by_seat = {assignment.seat_id: assignment.student_key for assignment in assignments}
    gaps: list[float] = []
    for first_seat, second_seat in build_adjacency_edges(layout):
        first_key = assignment_by_seat.get(first_seat)
        second_key = assignment_by_seat.get(second_seat)
        if first_key in scores and second_key in scores:
            gaps.append(abs(scores[first_key] - scores[second_key]))
    if not gaps:
        return _not_available("No adjacent assigned pairs have score data.")
    score_range = max(scores.values()) - min(scores.values())
    normalized = min(100.0, mean(gaps) / score_range * 100)
    return _available_dimension(
        normalized,
        raw_value=mean(gaps),
        weight=rule.weight,
        details={
            "mean_adjacent_score_gap": mean(gaps),
            "score_range": score_range,
            "adjacent_pair_count": len(gaps),
            "meaning": "Higher values indicate stronger mixing of different score levels across adjacent seats.",
        },
    )


def _score_height(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
) -> ScoreDimension:
    rule = rules.soft.height_back
    if not rule.enabled or rule.weight == 0:
        return _not_available("height_back is disabled.")
    heights = {student.key: float(student.height_cm) for student in students if student.height_cm is not None}
    rows = [seat.row for seat in layout.enabled_seats]
    if len(heights) < 2 or max(heights.values()) == min(heights.values()) or max(rows) == min(rows):
        return _not_available("Different heights and more than one seat row are required.")
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}
    errors: list[float] = []
    min_height, max_height = min(heights.values()), max(heights.values())
    min_row, max_row = min(rows), max(rows)
    for assignment in assignments:
        if assignment.student_key not in heights or assignment.seat_id not in seat_by_id:
            continue
        height_position = (heights[assignment.student_key] - min_height) / (max_height - min_height)
        row_position = (seat_by_id[assignment.seat_id].row - min_row) / (max_row - min_row)
        errors.append(abs(height_position - row_position))
    if not errors:
        return _not_available("No assignments have height data.")
    score = max(0.0, 100 * (1 - mean(errors)))
    return _available_dimension(
        score,
        raw_value=mean(errors),
        weight=rule.weight,
        details={"mean_normalized_position_error": mean(errors), "lower_error_is_better": True},
    )


def _score_vision(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
) -> ScoreDimension:
    rule = rules.soft.vision_front
    if not rule.enabled or rule.weight == 0:
        return _not_available("vision_front is disabled.")
    students_needing_front = {student.key for student in students if student_needs_front(student)}
    if not students_needing_front:
        return _not_available("No students are marked as needing a front seat.")
    rows = [seat.row for seat in layout.enabled_seats]
    min_row, max_row = min(rows), max(rows)
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}
    positions: list[float] = []
    for assignment in assignments:
        if assignment.student_key not in students_needing_front:
            continue
        seat = seat_by_id.get(assignment.seat_id)
        if seat is None:
            continue
        normalized = 0.0 if min_row == max_row else (seat.row - min_row) / (max_row - min_row)
        positions.append(normalized)
    if not positions:
        return _not_available("No front-seat preference assignments could be evaluated.")
    score = max(0.0, 100 * (1 - mean(positions)))
    return _available_dimension(
        score,
        raw_value=mean(positions),
        weight=rule.weight,
        details={
            "students_needing_front": len(students_needing_front),
            "mean_normalized_row": mean(positions),
            "lower_row_value_is_better": True,
        },
    )


def _score_stability(
    assignments: Sequence[SeatAssignment],
    latest_snapshot: SeatingSnapshot | None,
) -> ScoreDimension:
    if latest_snapshot is None:
        return _not_available("No previous snapshot was supplied.")
    previous = {assignment.student_key: assignment.seat_id for assignment in latest_snapshot.assignments}
    comparable = [assignment for assignment in assignments if assignment.student_key in previous]
    if not comparable:
        return _not_available("The previous snapshot has no comparable students.")
    unchanged = sum(previous[assignment.student_key] == assignment.seat_id for assignment in comparable)
    score = unchanged / len(comparable) * 100
    return _available_dimension(
        score,
        raw_value=float(unchanged),
        weight=1,
        details={
            "unchanged_students": unchanged,
            "changed_students": len(comparable) - unchanged,
            "comparable_students": len(comparable),
            "meaning": "Higher values preserve more seats from the latest historical snapshot.",
        },
    )


def _weighted_total(breakdown: ScoreBreakdown) -> float:
    if not breakdown.hard_constraint_summary.satisfied:
        return 0.0
    dimensions = _dimension_map(breakdown).values()
    available = [
        dimension
        for dimension in dimensions
        if dimension.status == "available" and dimension.score is not None and dimension.weight > 0
    ]
    if not available:
        return 100.0
    total_weight = sum(dimension.weight for dimension in available)
    return round(
        sum(float(dimension.score) * dimension.weight for dimension in available) / total_weight,
        2,
    )


def _available_dimension(
    score: float,
    *,
    raw_value: float | None,
    weight: float,
    details: dict[str, object],
) -> ScoreDimension:
    score = round(max(0.0, min(100.0, float(score))), 2)
    return ScoreDimension(
        status="available",
        score=score,
        raw_value=raw_value,
        weight=float(weight),
        rating=_rating(score),
        details=details,
    )


def _not_available(reason: str) -> ScoreDimension:
    return ScoreDimension(
        status="not_available",
        score=None,
        raw_value=None,
        weight=0,
        rating="not_available",
        details={"reason": reason},
    )


def _dimension_map(breakdown: ScoreBreakdown) -> dict[str, ScoreDimension]:
    return {
        "fair_rotation_score": breakdown.fair_rotation_score,
        "avoid_recent_neighbors_score": breakdown.avoid_recent_neighbors_score,
        "score_balance_score": breakdown.score_balance_score,
        "height_preference_score": breakdown.height_preference_score,
        "vision_preference_score": breakdown.vision_preference_score,
        "diversity_score": breakdown.diversity_score,
        "stability_score": breakdown.stability_score,
    }


def _rating(score: float) -> str:
    if score >= 75:
        return "high"
    if score >= 50:
        return "medium"
    return "low"


def _rating_text(score: float) -> str:
    return f"{_rating(score)} ({score:.1f})"


def _student_reference_map(students: Sequence[Student]) -> dict[str, str]:
    refs: dict[str, str] = {}
    for student in students:
        if student.student_id:
            refs[student.student_id] = student.key
        if student.name:
            refs[student.name] = student.key
    return refs


def _assignment_distance(first: dict[str, str], second: dict[str, str]) -> float:
    comparable = sorted(set(first) & set(second))
    if not comparable:
        return 0.0
    changed = sum(first[key] != second[key] for key in comparable)
    return changed / len(comparable) * 100


def _history_baseline_scores(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    snapshots: list[SeatingSnapshot],
) -> dict[str, ScoreDimension]:
    if len(snapshots) < 2:
        return {}
    previous_snapshots = snapshots[:-1]
    latest = snapshots[-1]
    seat_history = build_seat_history(students, layout, previous_snapshots)
    pair_rule = rules.soft.avoid_recent_neighbors
    pair_history = build_pair_history(
        students,
        layout,
        previous_snapshots,
        lookback=pair_rule.lookback,
        within_distance=pair_rule.within_distance,
    )
    comparable_snapshot = _copy_snapshot_with_current_inputs(latest, students, layout, rules)
    baseline_score = score_snapshot(
        comparable_snapshot,
        history=seat_history,
        pair_history=pair_history,
        latest_snapshot=previous_snapshots[-1] if previous_snapshots else None,
    )
    return {
        "fair_rotation_score": baseline_score.breakdown.fair_rotation_score,
        "avoid_recent_neighbors_score": baseline_score.breakdown.avoid_recent_neighbors_score,
    }


def _copy_snapshot_with_current_inputs(
    snapshot: SeatingSnapshot,
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
) -> SeatingSnapshot:
    update = {"students": students, "layout": layout, "rules": rules}
    if hasattr(snapshot, "model_copy"):
        return snapshot.model_copy(update=update)  # type: ignore[attr-defined,return-value]
    return snapshot.copy(update=update)


def _compare_with_baseline(
    current: ScoreDimension,
    baseline: ScoreDimension | None,
) -> str:
    if (
        baseline is None
        or baseline.status != "available"
        or baseline.score is None
        or current.status != "available"
        or current.score is None
    ):
        return "not_available"
    difference = current.score - baseline.score
    if difference > 2:
        return "improved"
    if difference < -2:
        return "worse"
    return "similar"
