"""Shared, backend-neutral score-based soft objectives.

The functions in this module deliberately operate on complete or partial
assignments and never declare an assignment invalid. Solver backends may use
the returned cost to rank already-feasible choices, while scoring uses the
same normalized losses to explain a finished plan.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from math import sqrt
from statistics import mean
from typing import Mapping, Sequence

from seattrellis.history import student_pair_key
from seattrellis.models.history import NeighborRelationType, PairHistory
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.solver.adjacency import SeatEdge, build_adjacency_edges, normalize_edge


@dataclass(frozen=True)
class MentorPair:
    """A deterministic mentor/learner pair selected before seat optimization."""

    mentor_key: str
    learner_key: str
    recent_occurrences: int = 0


@dataclass(frozen=True)
class SoftObjectiveContext:
    """Precomputed data shared by fallback solving and result scoring."""

    score_percentiles: dict[str, float]
    seat_row_percentiles: dict[str, float]
    distribution_buckets: dict[str, str]
    mentor_pairs: tuple[MentorPair, ...]
    seat_by_id: dict[str, SeatNode]
    adjacency_edges: set[SeatEdge]
    warnings: tuple[str, ...] = ()


@dataclass(frozen=True)
class SoftObjectiveEvaluation:
    """Normalized losses plus comparable weighted costs for enabled goals."""

    losses: dict[str, float | None] = field(default_factory=dict)
    weighted_costs: dict[str, float] = field(default_factory=dict)
    details: dict[str, dict[str, object]] = field(default_factory=dict)
    warnings: tuple[str, ...] = ()

    @property
    def total_cost(self) -> float:
        return sum(self.weighted_costs.values())


def score_rank_percentiles(students: Sequence[Student]) -> dict[str, float]:
    """Return average-rank percentiles, preserving ties and grading scales.

    The lowest distinct score approaches ``0`` and the highest approaches
    ``1``. Tied students receive the average of their occupied rank positions.
    A single score value is not enough to express a preference, so an empty
    mapping is returned in that case.
    """

    scored = sorted(
        ((student.key, float(student.score)) for student in students if student.score is not None),
        key=lambda item: (item[1], item[0]),
    )
    if len(scored) < 2 or scored[0][1] == scored[-1][1]:
        return {}
    denominator = len(scored) - 1
    result: dict[str, float] = {}
    start = 0
    while start < len(scored):
        end = start + 1
        while end < len(scored) and scored[end][1] == scored[start][1]:
            end += 1
        average_rank = (start + end - 1) / 2
        percentile = average_rank / denominator
        for key, _score in scored[start:end]:
            result[key] = percentile
        start = end
    return result


def compile_soft_objectives(
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    pair_history: PairHistory | None = None,
) -> SoftObjectiveContext:
    """Compile score goals once for a solve or scoring operation."""

    percentiles = score_rank_percentiles(students)
    enabled_seats = layout.enabled_seats
    rows = sorted({seat.row for seat in enabled_seats})
    row_percentile = {
        row: (index / (len(rows) - 1) if len(rows) > 1 else 0.5)
        for index, row in enumerate(rows)
    }
    seat_rows = {seat.seat_id: row_percentile[seat.row] for seat in enabled_seats}

    distribution_buckets: dict[str, str] = {}
    warnings: list[str] = []
    distribution_rule = rules.soft.score_distribution
    if distribution_rule.scope == "row":
        distribution_buckets = {
            seat.seat_id: f"row:{seat.row}" for seat in enabled_seats
        }
    else:
        grouped = [seat for seat in enabled_seats if seat.group_id]
        distribution_buckets = {
            seat.seat_id: f"group:{seat.group_id}" for seat in grouped
        }
        if distribution_rule.enabled and len(grouped) != len(enabled_seats):
            missing_count = len(enabled_seats) - len(grouped)
            warnings.append(
                "score_distribution with scope='group' requires group_id on every "
                f"enabled seat; {missing_count} seat(s) are missing it, so the group "
                "distribution objective is unavailable."
            )
            distribution_buckets = {}

    mentor_pairs = _select_mentor_pairs(
        percentiles,
        rules,
        pair_history,
    )
    return SoftObjectiveContext(
        score_percentiles=percentiles,
        seat_row_percentiles=seat_rows,
        distribution_buckets=distribution_buckets,
        mentor_pairs=mentor_pairs,
        seat_by_id={seat.seat_id: seat for seat in enabled_seats},
        adjacency_edges=build_adjacency_edges(layout),
        warnings=tuple(warnings),
    )


def evaluate_soft_objectives(
    assignment: Mapping[str, str],
    context: SoftObjectiveContext,
    rules: RuleSet,
) -> SoftObjectiveEvaluation:
    """Evaluate enabled score goals for a partial or complete assignment."""

    losses: dict[str, float | None] = {}
    weighted_costs: dict[str, float] = {}
    details: dict[str, dict[str, object]] = {}

    position_rule = rules.soft.score_position
    if position_rule.enabled and position_rule.weight:
        errors: list[float] = []
        for student_key, seat_id in assignment.items():
            score_position = context.score_percentiles.get(student_key)
            row_position = context.seat_row_percentiles.get(seat_id)
            if score_position is None or row_position is None:
                continue
            target = score_position if position_rule.direction == "high_back" else 1 - score_position
            errors.append(abs(target - row_position))
        loss = mean(errors) if errors and context.score_percentiles else None
        losses["score_position"] = loss
        details["score_position"] = {
            "direction": position_rule.direction,
            "evaluated_students": len(errors),
            "mean_percentile_error": loss,
            "lower_error_is_better": True,
        }
        _add_weighted_cost(weighted_costs, "score_position", loss, position_rule.weight)

    distribution_rule = rules.soft.score_distribution
    if distribution_rule.enabled and distribution_rule.weight:
        bucket_values: dict[str, list[float]] = {}
        for student_key, seat_id in assignment.items():
            percentile = context.score_percentiles.get(student_key)
            bucket = context.distribution_buckets.get(seat_id)
            if percentile is None or bucket is None:
                continue
            bucket_values.setdefault(bucket, []).append(percentile)
        usable = [values for values in bucket_values.values() if values]
        if len(usable) >= 2:
            overall = mean(value for values in usable for value in values)
            rms = sqrt(mean((mean(values) - overall) ** 2 for values in usable))
            # With percentile data the largest practical between-bucket RMS is
            # 0.5. Scaling by two maps that range to a readable 0..1 loss.
            loss = min(1.0, rms * 2)
        else:
            overall = None
            rms = None
            loss = None
        losses["score_distribution"] = loss
        details["score_distribution"] = {
            "scope": distribution_rule.scope,
            "bucket_count": len(usable),
            "bucket_sizes": {key: len(values) for key, values in sorted(bucket_values.items())},
            "overall_mean_percentile": overall,
            "between_bucket_rms": rms,
            "lower_error_is_better": True,
        }
        _add_weighted_cost(
            weighted_costs,
            "score_distribution",
            loss,
            distribution_rule.weight,
        )

    mentor_rule = rules.soft.mentor_pairing
    if mentor_rule.enabled and mentor_rule.weight:
        evaluated = 0
        satisfied = 0
        pair_details: list[dict[str, object]] = []
        for pair in context.mentor_pairs:
            mentor_seat_id = assignment.get(pair.mentor_key)
            learner_seat_id = assignment.get(pair.learner_key)
            if mentor_seat_id is None or learner_seat_id is None:
                continue
            evaluated += 1
            is_satisfied = _relation_satisfied(
                mentor_seat_id,
                learner_seat_id,
                mentor_rule.relation,
                context,
            )
            satisfied += int(is_satisfied)
            pair_details.append(
                {
                    "mentor": pair.mentor_key,
                    "learner": pair.learner_key,
                    "satisfied": is_satisfied,
                    "recent_occurrences": pair.recent_occurrences,
                }
            )
        loss = 1 - satisfied / evaluated if evaluated else None
        losses["mentor_pairing"] = loss
        details["mentor_pairing"] = {
            "relation": mentor_rule.relation,
            "selected_pair_count": len(context.mentor_pairs),
            "evaluated_pair_count": evaluated,
            "satisfied_pair_count": satisfied,
            "pairs": pair_details,
        }
        _add_weighted_cost(weighted_costs, "mentor_pairing", loss, mentor_rule.weight)

    return SoftObjectiveEvaluation(
        losses=losses,
        weighted_costs=weighted_costs,
        details=details,
        warnings=context.warnings,
    )


def _add_weighted_cost(
    costs: dict[str, float],
    name: str,
    loss: float | None,
    weight: int,
) -> None:
    if loss is not None:
        costs[name] = loss * weight * 100


def _select_mentor_pairs(
    percentiles: Mapping[str, float],
    rules: RuleSet,
    pair_history: PairHistory | None,
) -> tuple[MentorPair, ...]:
    rule = rules.soft.mentor_pairing
    if not rule.enabled or not rule.weight or not percentiles:
        return ()
    mentors = sorted(
        (key for key, value in percentiles.items() if value >= rule.mentor_percentile),
        key=lambda key: (-percentiles[key], key),
    )
    learners = sorted(
        (key for key, value in percentiles.items() if value <= rule.learner_percentile),
        key=lambda key: (percentiles[key], key),
    )
    pair_count = min(len(mentors), len(learners))
    if pair_count == 0:
        return ()

    occurrence_by_pair: dict[tuple[str, str], int] = {}
    costs: dict[tuple[str, str], int] = {}
    for mentor_index, mentor_key in enumerate(mentors):
        for learner_index, learner_key in enumerate(learners):
            occurrences = _recent_pair_occurrences(
                mentor_key,
                learner_key,
                rule.relation,
                rule.history_lookback,
                pair_history,
            ) if rule.avoid_recent_repeats else 0
            occurrence_by_pair[(mentor_key, learner_key)] = occurrences
            complement_error = abs(
                percentiles[mentor_key] + percentiles[learner_key] - 1
            )
            # Occurrence count dominates rank complement, which dominates the
            # stable key order. The global assignment avoids greedy dead ends.
            costs[(mentor_key, learner_key)] = (
                occurrences * 1_000_000
                + int(round(complement_error * 10_000)) * 100
                + mentor_index * len(learners)
                + learner_index
            )

    selected = [
        MentorPair(
            mentor_key,
            learner_key,
            occurrence_by_pair[(mentor_key, learner_key)],
        )
        for mentor_key, learner_key in _minimum_cost_bipartite_pairs(
            mentors,
            learners,
            costs,
        )
    ]
    return tuple(sorted(selected, key=lambda pair: (pair.mentor_key, pair.learner_key)))


def _minimum_cost_bipartite_pairs(
    mentors: Sequence[str],
    learners: Sequence[str],
    costs: Mapping[tuple[str, str], int],
) -> list[tuple[str, str]]:
    """Return a deterministic minimum-cost matching using the Hungarian method."""

    if not mentors or not learners:
        return []
    rows_are_mentors = len(mentors) <= len(learners)
    rows = list(mentors if rows_are_mentors else learners)
    columns = list(learners if rows_are_mentors else mentors)
    matrix = [
        [
            costs[(row, column)] if rows_are_mentors else costs[(column, row)]
            for column in columns
        ]
        for row in rows
    ]

    # This O(n^2 m) rectangular implementation is small enough for classroom
    # cohorts and avoids introducing a numerical dependency for one objective.
    row_count = len(rows)
    column_count = len(columns)
    u = [0] * (row_count + 1)
    v = [0] * (column_count + 1)
    matched_row = [0] * (column_count + 1)
    previous_column = [0] * (column_count + 1)
    infinity = 10**30
    for row_index in range(1, row_count + 1):
        matched_row[0] = row_index
        column0 = 0
        minimum = [infinity] * (column_count + 1)
        used = [False] * (column_count + 1)
        while True:
            used[column0] = True
            current_row = matched_row[column0]
            delta = infinity
            column1 = 0
            for column_index in range(1, column_count + 1):
                if used[column_index]:
                    continue
                current = (
                    matrix[current_row - 1][column_index - 1]
                    - u[current_row]
                    - v[column_index]
                )
                if current < minimum[column_index]:
                    minimum[column_index] = current
                    previous_column[column_index] = column0
                if minimum[column_index] < delta:
                    delta = minimum[column_index]
                    column1 = column_index
            for column_index in range(column_count + 1):
                if used[column_index]:
                    u[matched_row[column_index]] += delta
                    v[column_index] -= delta
                else:
                    minimum[column_index] -= delta
            column0 = column1
            if matched_row[column0] == 0:
                break
        while True:
            column1 = previous_column[column0]
            matched_row[column0] = matched_row[column1]
            column0 = column1
            if column0 == 0:
                break

    pairs: list[tuple[str, str]] = []
    for column_index in range(1, column_count + 1):
        row_index = matched_row[column_index]
        if row_index == 0:
            continue
        row = rows[row_index - 1]
        column = columns[column_index - 1]
        pairs.append((row, column) if rows_are_mentors else (column, row))
    return pairs


def _recent_pair_occurrences(
    first_key: str,
    second_key: str,
    relation: str,
    lookback: int,
    pair_history: PairHistory | None,
) -> int:
    if pair_history is None or pair_history.history_count == 0 or lookback == 0:
        return 0
    history = pair_history.pairs.get(student_pair_key(first_key, second_key))
    if history is None:
        return 0
    relation_type = (
        NeighborRelationType.DESK_MATE
        if relation == "desk_mate"
        else NeighborRelationType.ADJACENT_ANY
    )
    return history.recent_occurrence_count({relation_type}, lookback)


def _relation_satisfied(
    first_seat_id: str,
    second_seat_id: str,
    relation: str,
    context: SoftObjectiveContext,
) -> bool:
    first = context.seat_by_id.get(first_seat_id)
    second = context.seat_by_id.get(second_seat_id)
    if first is None or second is None or first.seat_id == second.seat_id:
        return False
    if relation == "desk_mate":
        return first.row == second.row and abs(first.col - second.col) == 1
    return normalize_edge(first.seat_id, second.seat_id) in context.adjacency_edges
