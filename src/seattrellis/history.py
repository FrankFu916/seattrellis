from __future__ import annotations

from itertools import combinations
from pathlib import Path
from typing import Iterable, Sequence

from seattrellis.models.history import (
    FairnessReport,
    NeighborRelationType,
    PairHistory,
    PairHistoryRecord,
    PairHistoryReport,
    ROTATION_CATEGORIES,
    SeatHistory,
    SeatHistoryRecord,
    SeatPositionCategory,
    StudentPairHistory,
    StudentSeatHistory,
)
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import AvoidRecentNeighborsRule, FairRotationRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.io.json_files import InputFileError, load_snapshot
from seattrellis.solver.adjacency import SeatEdge, build_adjacency_edges, normalize_edge


DEFAULT_HISTORY_GLOB = "*.snapshot.json"
POSITION_REPORT_CATEGORIES: tuple[SeatPositionCategory, ...] = ROTATION_CATEGORIES
PAIR_REPORT_RELATIONS: tuple[NeighborRelationType, ...] = (
    NeighborRelationType.DESK_MATE,
    NeighborRelationType.HORIZONTAL,
    NeighborRelationType.VERTICAL,
    NeighborRelationType.DIAGONAL,
    NeighborRelationType.ADJACENT_ANY,
    NeighborRelationType.WITHIN_DISTANCE,
)


def load_history_snapshots(
    *,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
) -> list[SeatingSnapshot]:
    paths = list(history_paths or [])
    if history_dir is not None:
        paths.extend(_history_paths_in_dir(history_dir))
    return [load_snapshot(path) for path in paths]


def build_seat_history(
    students: Sequence[Student],
    layout: ClassroomLayout,
    snapshots: Sequence[SeatingSnapshot],
) -> SeatHistory:
    histories = {
        student.key: StudentSeatHistory(student_key=student.key, student_name=student.display_name)
        for student in students
    }
    current_student_keys = set(histories)
    seat_by_id = {seat.seat_id: seat for seat in layout.seats}
    category_totals: dict[str, int] = {}
    warnings: list[str] = []

    for snapshot_index, snapshot in enumerate(snapshots, start=1):
        snapshot_id = _snapshot_id(snapshot, snapshot_index)
        assignments_by_student = _assignments_for_current_students(snapshot.assignments, current_student_keys, snapshot_id, warnings)
        missing_students = sorted(current_student_keys - set(assignments_by_student))
        if missing_students:
            warnings.append(
                f"{snapshot_id} is missing current students: {_preview(missing_students)}. Missing students were skipped."
            )

        for student_key, assignment in assignments_by_student.items():
            student_history = histories[student_key]
            seat = seat_by_id.get(assignment.seat_id)
            categories: list[SeatPositionCategory]
            unknown_seat = False
            disabled_seat = False
            if seat is None:
                categories = [SeatPositionCategory.UNKNOWN]
                unknown_seat = True
                warnings.append(
                    f"{snapshot_id} references unknown seat_id {assignment.seat_id!r} for student {student_key!r}; marked as unknown."
                )
            elif not seat.enabled:
                categories = []
                disabled_seat = True
                warnings.append(
                    f"{snapshot_id} references disabled seat_id {assignment.seat_id!r} for student {student_key!r}; position categories were skipped."
                )
            else:
                categories = sorted(classify_seat_position(seat, layout), key=lambda category: category.value)

            record = SeatHistoryRecord(
                snapshot_index=snapshot_index,
                snapshot_id=snapshot_id,
                created_at=snapshot.created_at,
                seat_id=assignment.seat_id,
                categories=categories,
                unknown_seat=unknown_seat,
                disabled_seat=disabled_seat,
            )
            student_history.records.append(record)
            student_history.total_assignments += 1
            student_history.seat_counts[assignment.seat_id] = student_history.seat_counts.get(assignment.seat_id, 0) + 1
            for category in categories:
                key = category.value
                student_history.category_counts[key] = student_history.category_counts.get(key, 0) + 1
                category_totals[key] = category_totals.get(key, 0) + 1

    return SeatHistory(
        history_count=len(snapshots),
        students=histories,
        category_totals=category_totals,
        pair_history=build_pair_history(students, layout, snapshots),
        warnings=warnings,
    )


def build_pair_history(
    students: Sequence[Student],
    layout: ClassroomLayout,
    snapshots: Sequence[SeatingSnapshot],
    *,
    lookback: int | None = None,
    within_distance: int = 2,
) -> PairHistory:
    selected_snapshots = _select_lookback_snapshots(snapshots, lookback)
    start_index = len(snapshots) - len(selected_snapshots) + 1
    current_student_keys = {student.key for student in students}
    student_names = {student.key: student.display_name for student in students}
    seat_by_id = {seat.seat_id: seat for seat in layout.seats}
    edges = build_adjacency_edges(layout)
    pair_histories: dict[str, StudentPairHistory] = {}
    relation_totals: dict[str, int] = {}
    warnings: list[str] = []

    for snapshot_index, snapshot in enumerate(selected_snapshots, start=start_index):
        snapshot_id = _snapshot_id(snapshot, snapshot_index)
        assignments_by_student = _assignments_for_current_students(snapshot.assignments, current_student_keys, snapshot_id, warnings)
        missing_students = sorted(current_student_keys - set(assignments_by_student))
        if missing_students:
            warnings.append(
                f"{snapshot_id} is missing current students: {_preview(missing_students)}. Missing students were skipped for pair history."
            )

        known_assignments: dict[str, tuple[SeatAssignment, SeatNode]] = {}
        for student_key, assignment in assignments_by_student.items():
            seat = seat_by_id.get(assignment.seat_id)
            if seat is None:
                warnings.append(
                    f"{snapshot_id} references unknown seat_id {assignment.seat_id!r} for student {student_key!r}; pair relations for that assignment were skipped."
                )
                continue
            if not seat.enabled:
                warnings.append(
                    f"{snapshot_id} references disabled seat_id {assignment.seat_id!r} for student {student_key!r}; pair relations were still counted from row/col coordinates."
                )
            known_assignments[student_key] = (assignment, seat)

        for first_key, second_key in combinations(sorted(known_assignments), 2):
            first_assignment, first_seat = known_assignments[first_key]
            second_assignment, second_seat = known_assignments[second_key]
            relations = detect_neighbor_relation_types(
                first_seat,
                second_seat,
                layout,
                adjacency_edges=edges,
                within_distance=within_distance,
            )
            if not relations:
                continue

            key = student_pair_key(first_key, second_key)
            first_student_key, second_student_key = sorted((first_key, second_key))
            pair_history = pair_histories.get(key)
            if pair_history is None:
                pair_history = StudentPairHistory(
                    pair_key=key,
                    first_student_key=first_student_key,
                    second_student_key=second_student_key,
                    first_student_name=student_names.get(first_student_key),
                    second_student_name=student_names.get(second_student_key),
                )
                pair_histories[key] = pair_history

            row_delta = abs(first_seat.row - second_seat.row)
            col_delta = abs(first_seat.col - second_seat.col)
            record = PairHistoryRecord(
                snapshot_index=snapshot_index,
                snapshot_id=snapshot_id,
                created_at=snapshot.created_at,
                first_seat_id=first_assignment.seat_id,
                second_seat_id=second_assignment.seat_id,
                relations=_sorted_relations(relations),
                row_delta=row_delta,
                col_delta=col_delta,
                chebyshev_distance=max(row_delta, col_delta),
                manhattan_distance=row_delta + col_delta,
                first_seat_disabled=not first_seat.enabled,
                second_seat_disabled=not second_seat.enabled,
            )
            pair_history.records.append(record)
            pair_history.total_occurrences += 1
            for relation in record.relations:
                key_value = relation.value
                pair_history.relation_counts[key_value] = pair_history.relation_counts.get(key_value, 0) + 1
                relation_totals[key_value] = relation_totals.get(key_value, 0) + 1

    return PairHistory(
        history_count=len(selected_snapshots),
        student_count=len(students),
        pair_count=len(pair_histories),
        within_distance_metric="chebyshev",
        within_distance=within_distance,
        pairs=dict(sorted(pair_histories.items())),
        relation_totals={relation.value: relation_totals.get(relation.value, 0) for relation in PAIR_REPORT_RELATIONS},
        warnings=warnings,
    )


def detect_neighbor_relation_types(
    first_seat: SeatNode,
    second_seat: SeatNode,
    layout: ClassroomLayout,
    *,
    adjacency_edges: set[SeatEdge] | None = None,
    within_distance: int = 2,
) -> set[NeighborRelationType]:
    if first_seat.seat_id == second_seat.seat_id:
        return set()

    row_delta = abs(first_seat.row - second_seat.row)
    col_delta = abs(first_seat.col - second_seat.col)
    relations: set[NeighborRelationType] = set()

    if row_delta == 0 and col_delta == 1:
        relations.add(NeighborRelationType.HORIZONTAL)
        relations.add(NeighborRelationType.DESK_MATE)
    if col_delta == 0 and row_delta == 1:
        relations.add(NeighborRelationType.VERTICAL)
    if row_delta == 1 and col_delta == 1:
        relations.add(NeighborRelationType.DIAGONAL)

    if relations & {
        NeighborRelationType.HORIZONTAL,
        NeighborRelationType.VERTICAL,
        NeighborRelationType.DIAGONAL,
    }:
        relations.add(NeighborRelationType.ADJACENT_ANY)
    else:
        edges = adjacency_edges if adjacency_edges is not None else build_adjacency_edges(layout)
        if normalize_edge(first_seat.seat_id, second_seat.seat_id) in edges:
            relations.add(NeighborRelationType.ADJACENT_ANY)

    if max(row_delta, col_delta) <= within_distance:
        relations.add(NeighborRelationType.WITHIN_DISTANCE)
    return relations


def student_pair_key(first_student_key: str, second_student_key: str) -> str:
    first, second = sorted((first_student_key, second_student_key))
    return f"{first}|{second}"


def build_pair_history_report(pair_history: PairHistory, *, top: int = 10) -> PairHistoryReport:
    pairs = sorted(pair_history.pairs.values(), key=lambda item: item.pair_key)
    return PairHistoryReport(
        history_count=pair_history.history_count,
        student_count=pair_history.student_count,
        pair_count=pair_history.pair_count,
        within_distance_metric=pair_history.within_distance_metric,
        within_distance=pair_history.within_distance,
        relation_totals={
            relation.value: pair_history.relation_totals.get(relation.value, 0)
            for relation in PAIR_REPORT_RELATIONS
        },
        top_desk_mates=_top_pairs(pairs, NeighborRelationType.DESK_MATE, top),
        top_adjacent_pairs=_top_pairs(pairs, NeighborRelationType.ADJACENT_ANY, top),
        pairs=pairs,
        summary={
            "warning_count": len(pair_history.warnings),
            "within_distance_metric": pair_history.within_distance_metric,
            "within_distance": pair_history.within_distance,
        },
        warnings=pair_history.warnings,
    )


def format_pair_history_report(report: PairHistoryReport, *, top: int = 10) -> str:
    lines = [
        "Pair history report",
        "",
        "Summary:",
        f"- snapshots: {report.history_count}",
        f"- students: {report.student_count}",
        f"- pairs: {report.pair_count}",
        f"- warnings: {len(report.warnings)}",
        f"- within_distance: {report.within_distance_metric} <= {report.within_distance}",
        "",
        "Relation totals:",
    ]
    for relation in PAIR_REPORT_RELATIONS:
        lines.append(f"- {relation.value}: {report.relation_totals.get(relation.value, 0)}")

    lines.append("")
    lines.append(f"Top desk mates ({top}):")
    lines.extend(_format_pair_lines(report.top_desk_mates) or ["- none"])

    lines.append("")
    lines.append(f"Top adjacent pairs ({top}):")
    lines.extend(_format_pair_lines(report.top_adjacent_pairs) or ["- none"])

    lines.append("")
    lines.append("Pair relation counts:")
    shown_pairs = sorted(
        report.pairs,
        key=lambda item: (
            -item.relation_counts.get(NeighborRelationType.ADJACENT_ANY.value, 0),
            -item.relation_counts.get(NeighborRelationType.DESK_MATE.value, 0),
            item.pair_key,
        ),
    )[:top]
    lines.extend(_format_pair_lines(shown_pairs) or ["- none"])

    if report.warnings:
        lines.append("")
        lines.append("Warnings:")
        lines.extend(f"- {warning}" for warning in report.warnings)
    return "\n".join(lines)


def avoid_recent_neighbors_cost(
    first_student_key: str,
    second_student_key: str,
    first_seat: SeatNode,
    second_seat: SeatNode,
    layout: ClassroomLayout,
    rule: AvoidRecentNeighborsRule,
    pair_history: PairHistory | None,
    *,
    adjacency_edges: set[SeatEdge] | None = None,
) -> int:
    if not rule.enabled or rule.weight == 0 or pair_history is None or pair_history.history_count == 0:
        return 0

    selected_relations = set(rule.relation_types)
    current_relations = detect_neighbor_relation_types(
        first_seat,
        second_seat,
        layout,
        adjacency_edges=adjacency_edges,
        within_distance=rule.within_distance,
    )
    if not (current_relations & selected_relations):
        return 0

    pair = pair_history.pairs.get(student_pair_key(first_student_key, second_student_key))
    if pair is None:
        return 0
    recent_count = pair.recent_occurrence_count(selected_relations, rule.lookback)
    excess = max(0, recent_count - rule.max_recent_count)
    return rule.weight * excess * 100


def build_fairness_report(history: SeatHistory) -> FairnessReport:
    students = sorted(history.students.values(), key=lambda item: item.student_key)
    category_spread: dict[str, dict[str, int]] = {}
    for category in POSITION_REPORT_CATEGORIES:
        key = category.value
        counts = [student.category_counts.get(key, 0) for student in students]
        if not counts:
            category_spread[key] = {"min": 0, "max": 0, "spread": 0}
            continue
        category_spread[key] = {
            "min": min(counts),
            "max": max(counts),
            "spread": max(counts) - min(counts),
        }

    return FairnessReport(
        history_count=history.history_count,
        student_count=len(students),
        category_totals={category.value: history.category_totals.get(category.value, 0) for category in POSITION_REPORT_CATEGORIES},
        students=students,
        summary={
            "category_spread": category_spread,
            "warning_count": len(history.warnings),
        },
        warnings=history.warnings,
    )


def classify_seat_position(seat: SeatNode, layout: ClassroomLayout) -> set[SeatPositionCategory]:
    if not seat.enabled:
        return set()

    enabled_seats = layout.enabled_seats
    rows = sorted({item.row for item in enabled_seats})
    cols = sorted({item.col for item in enabled_seats})
    min_row, max_row = rows[0], rows[-1]
    min_col, max_col = cols[0], cols[-1]

    categories: set[SeatPositionCategory] = set()
    zone_category = _zone_category(seat.zone)
    if zone_category is not None:
        categories.add(zone_category)

    if zone_category not in {
        SeatPositionCategory.FRONT,
        SeatPositionCategory.BACK,
        SeatPositionCategory.MIDDLE,
    }:
        categories.add(_inferred_row_category(seat, rows, min_row, max_row))

    if seat.col in {min_col, max_col}:
        categories.add(SeatPositionCategory.SIDE)
    if seat.row in {min_row, max_row} and seat.col in {min_col, max_col}:
        categories.add(SeatPositionCategory.CORNER)

    if seat.near_window:
        categories.add(SeatPositionCategory.NEAR_WINDOW)
    if seat.near_door:
        categories.add(SeatPositionCategory.NEAR_DOOR)
    if seat.near_platform:
        categories.add(SeatPositionCategory.NEAR_PLATFORM)
    if seat.near_ac:
        categories.add(SeatPositionCategory.NEAR_AC)
    return categories


def fair_rotation_cost(
    student: Student,
    seat: SeatNode,
    layout: ClassroomLayout,
    rule: FairRotationRule,
    history: SeatHistory | None,
) -> int:
    if not rule.enabled or rule.weight == 0 or history is None or history.history_count == 0:
        return 0
    student_history = history.students.get(student.key)
    if student_history is None:
        return 0

    categories = classify_seat_position(seat, layout)
    avoid_categories = {category.value for category in rule.avoid_repeating_categories}
    candidate_categories = [category.value for category in categories if category.value in avoid_categories]
    if not candidate_categories:
        return 0

    recent_counts = student_history.recent_category_counts(rule.lookback)
    total_cost = 0
    for category in candidate_categories:
        total_count = student_history.category_counts.get(category, 0)
        min_count = min(
            item.category_counts.get(category, 0)
            for item in history.students.values()
        )
        repeated_recent_penalty = recent_counts.get(category, 0) * 100
        long_term_penalty = max(0, total_count - min_count) * 25
        compensation_bonus = 10 if total_count == min_count else 0
        total_cost += repeated_recent_penalty + long_term_penalty - compensation_bonus
    return rule.weight * total_cost


def assignment_fairness_summary(
    assignments: Sequence[SeatAssignment],
    students: Sequence[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    history: SeatHistory | None,
    pair_history: PairHistory | None = None,
) -> dict[str, object]:
    fair_rule = rules.soft.fair_rotation
    neighbor_rule = rules.soft.avoid_recent_neighbors
    pair_history = pair_history or (history.pair_history if history is not None else None)
    history_count = history.history_count if history is not None else 0
    pair_history_count = pair_history.history_count if pair_history is not None else 0
    enabled_rules: list[str] = []
    messages: list[str] = []
    summary: dict[str, object] = {
        "history_count": history_count,
        "pair_history_count": pair_history_count,
        "enabled_rules": enabled_rules,
        "fair_rotation_enabled": fair_rule.enabled,
        "avoid_recent_neighbors_enabled": neighbor_rule.enabled,
    }

    student_by_key = {student.key: student for student in students}
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}

    if fair_rule.enabled and fair_rule.weight > 0:
        if history_count == 0 or history is None:
            messages.append("fair_rotation is enabled but no history snapshots were supplied; the rule was inactive.")
        else:
            assignment_cost = 0
            category_counts: dict[str, int] = {}
            for assignment in assignments:
                student = student_by_key.get(assignment.student_key)
                seat = seat_by_id.get(assignment.seat_id)
                if student is None or seat is None:
                    continue
                assignment_cost += fair_rotation_cost(student, seat, layout, fair_rule, history)
                for category in classify_seat_position(seat, layout):
                    key = category.value
                    category_counts[key] = category_counts.get(key, 0) + 1
            enabled_rules.append("fair_rotation")
            summary.update(
                {
                    "fair_rotation_cost": assignment_cost,
                    "lookback": fair_rule.lookback,
                    "avoid_repeating_categories": [
                        category.value for category in fair_rule.avoid_repeating_categories
                    ],
                    "current_assignment_categories": category_counts,
                }
            )

    if neighbor_rule.enabled and neighbor_rule.weight > 0:
        if pair_history_count == 0 or pair_history is None:
            messages.append("avoid_recent_neighbors is enabled but no pair history was available; the rule was inactive.")
        else:
            neighbor_cost = 0
            all_seats_by_id = {seat.seat_id: seat for seat in layout.seats}
            edges = build_adjacency_edges(layout)
            for first_assignment, second_assignment in combinations(assignments, 2):
                first_seat = all_seats_by_id.get(first_assignment.seat_id)
                second_seat = all_seats_by_id.get(second_assignment.seat_id)
                if first_seat is None or second_seat is None:
                    continue
                neighbor_cost += avoid_recent_neighbors_cost(
                    first_assignment.student_key,
                    second_assignment.student_key,
                    first_seat,
                    second_seat,
                    layout,
                    neighbor_rule,
                    pair_history,
                    adjacency_edges=edges,
                )
            enabled_rules.append("avoid_recent_neighbors")
            summary.update(
                {
                    "avoid_recent_neighbors_cost": neighbor_cost,
                    "avoid_recent_neighbors_lookback": neighbor_rule.lookback,
                    "avoid_recent_neighbors_relation_types": [
                        relation.value for relation in neighbor_rule.relation_types
                    ],
                    "avoid_recent_neighbors_max_recent_count": neighbor_rule.max_recent_count,
                    "within_distance_metric": pair_history.within_distance_metric,
                    "within_distance": neighbor_rule.within_distance,
                }
            )

    if messages:
        summary["message"] = " ".join(messages)
    return summary


def fairness_metadata(
    rules: RuleSet,
    history: SeatHistory | None,
    pair_history: PairHistory | None = None,
) -> dict[str, object]:
    fair_rule = rules.soft.fair_rotation
    neighbor_rule = rules.soft.avoid_recent_neighbors
    pair_history = pair_history or (history.pair_history if history is not None else None)
    history_count = history.history_count if history is not None else 0
    pair_history_count = pair_history.history_count if pair_history is not None else 0
    enabled_rules: list[str] = []
    if fair_rule.enabled and fair_rule.weight > 0 and history_count > 0:
        enabled_rules.append("fair_rotation")
    if neighbor_rule.enabled and neighbor_rule.weight > 0 and pair_history_count > 0:
        enabled_rules.append("avoid_recent_neighbors")
    warnings: list[str] = []
    if history is not None and history.warnings:
        warnings.extend(history.warnings)
    if pair_history is not None and pair_history.warnings:
        warnings.extend(pair_history.warnings)
    return {
        "history_count": history_count,
        "pair_history_count": pair_history_count,
        "enabled_rules": enabled_rules,
        "fair_rotation_enabled": fair_rule.enabled,
        "avoid_recent_neighbors_enabled": neighbor_rule.enabled,
        "warnings": warnings,
    }


def format_history_report(report: FairnessReport) -> str:
    lines = [
        "History report",
        "",
        "Summary:",
        f"- snapshots: {report.history_count}",
        f"- students: {report.student_count}",
        f"- warnings: {len(report.warnings)}",
        "",
        "Category totals:",
    ]
    for category in POSITION_REPORT_CATEGORIES:
        lines.append(f"- {category.value}: {report.category_totals.get(category.value, 0)}")

    lines.append("")
    lines.append("Students:")
    for student in report.students:
        counts = [
            f"{category.value}={student.category_counts.get(category.value, 0)}"
            for category in POSITION_REPORT_CATEGORIES
        ]
        name = f" ({student.student_name})" if student.student_name and student.student_name != student.student_key else ""
        lines.append(f"- {student.student_key}{name}: total={student.total_assignments}, " + ", ".join(counts))

    if report.warnings:
        lines.append("")
        lines.append("Warnings:")
        lines.extend(f"- {warning}" for warning in report.warnings)
    return "\n".join(lines)


def _select_lookback_snapshots(
    snapshots: Sequence[SeatingSnapshot],
    lookback: int | None,
) -> list[SeatingSnapshot]:
    if lookback is not None and lookback <= 0:
        return []
    return list(snapshots[-lookback:] if lookback else snapshots)


def _sorted_relations(relations: set[NeighborRelationType]) -> list[NeighborRelationType]:
    return sorted(relations, key=lambda relation: relation.value)


def _top_pairs(
    pairs: Sequence[StudentPairHistory],
    relation_type: NeighborRelationType,
    top: int,
) -> list[StudentPairHistory]:
    if top <= 0:
        return []
    relation_key = relation_type.value
    ranked = [
        pair
        for pair in pairs
        if pair.relation_counts.get(relation_key, 0) > 0
    ]
    return sorted(
        ranked,
        key=lambda pair: (
            -pair.relation_counts.get(relation_key, 0),
            -pair.total_occurrences,
            pair.pair_key,
        ),
    )[:top]


def _format_pair_lines(pairs: Sequence[StudentPairHistory]) -> list[str]:
    lines: list[str] = []
    for pair in pairs:
        counts = [
            f"{relation.value}={pair.relation_counts.get(relation.value, 0)}"
            for relation in PAIR_REPORT_RELATIONS
        ]
        names = []
        if pair.first_student_name and pair.first_student_name != pair.first_student_key:
            names.append(pair.first_student_name)
        if pair.second_student_name and pair.second_student_name != pair.second_student_key:
            names.append(pair.second_student_name)
        name_suffix = f" ({' / '.join(names)})" if names else ""
        lines.append(f"- {pair.pair_key}{name_suffix}: total={pair.total_occurrences}, " + ", ".join(counts))
    return lines


def _history_paths_in_dir(history_dir: str | Path) -> list[Path]:
    directory = Path(history_dir)
    if not directory.exists():
        raise InputFileError(f"History directory not found: {directory}")
    if not directory.is_dir():
        raise InputFileError(f"History path is not a directory: {directory}")
    paths = sorted(directory.glob(DEFAULT_HISTORY_GLOB))
    if not paths:
        paths = sorted(directory.glob("*.json"))
    return paths


def _assignments_for_current_students(
    assignments: Iterable[SeatAssignment],
    current_student_keys: set[str],
    snapshot_id: str,
    warnings: list[str],
) -> dict[str, SeatAssignment]:
    assignments_by_student: dict[str, SeatAssignment] = {}
    for assignment in assignments:
        if assignment.student_key not in current_student_keys:
            continue
        if assignment.student_key in assignments_by_student:
            warnings.append(
                f"{snapshot_id} contains duplicate assignments for current student {assignment.student_key!r}; the last one was used."
            )
        assignments_by_student[assignment.student_key] = assignment
    return assignments_by_student


def _snapshot_id(snapshot: SeatingSnapshot, snapshot_index: int) -> str:
    created_at = snapshot.created_at.isoformat() if snapshot.created_at else None
    return created_at or f"history snapshot {snapshot_index}"


def _preview(values: Sequence[str], limit: int = 5) -> str:
    shown = list(values[:limit])
    suffix = "" if len(values) <= limit else f", and {len(values) - limit} more"
    return ", ".join(shown) + suffix


def _zone_category(zone: str | None) -> SeatPositionCategory | None:
    if zone is None:
        return None
    normalized = zone.strip().lower().replace("-", "_").replace(" ", "_")
    for category in ROTATION_CATEGORIES:
        if normalized == category.value:
            return category
    return None


def _inferred_row_category(
    seat: SeatNode,
    rows: Sequence[int],
    min_row: int,
    max_row: int,
) -> SeatPositionCategory:
    if len(rows) == 1:
        return SeatPositionCategory.MIDDLE
    if seat.row == min_row:
        return SeatPositionCategory.FRONT
    if seat.row == max_row:
        return SeatPositionCategory.BACK
    return SeatPositionCategory.MIDDLE
