from __future__ import annotations

from pathlib import Path
from typing import Iterable, Sequence

from seattrellis.models.history import (
    FairnessReport,
    ROTATION_CATEGORIES,
    SeatHistory,
    SeatHistoryRecord,
    SeatPositionCategory,
    StudentSeatHistory,
)
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import FairRotationRule, RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.io.json_files import InputFileError, load_snapshot


DEFAULT_HISTORY_GLOB = "*.snapshot.json"
POSITION_REPORT_CATEGORIES: tuple[SeatPositionCategory, ...] = ROTATION_CATEGORIES


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
        warnings=warnings,
    )


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
) -> dict[str, object]:
    rule = rules.soft.fair_rotation
    history_count = history.history_count if history is not None else 0
    if not rule.enabled or rule.weight == 0:
        return {
            "history_count": history_count,
            "enabled_rules": [],
            "fair_rotation_enabled": rule.enabled,
        }
    if history_count == 0 or history is None:
        return {
            "history_count": 0,
            "enabled_rules": [],
            "fair_rotation_enabled": True,
            "message": "fair_rotation is enabled but no history snapshots were supplied; the rule was inactive.",
        }

    student_by_key = {student.key: student for student in students}
    seat_by_id = {seat.seat_id: seat for seat in layout.enabled_seats}
    assignment_cost = 0
    category_counts: dict[str, int] = {}
    for assignment in assignments:
        student = student_by_key.get(assignment.student_key)
        seat = seat_by_id.get(assignment.seat_id)
        if student is None or seat is None:
            continue
        assignment_cost += fair_rotation_cost(student, seat, layout, rule, history)
        for category in classify_seat_position(seat, layout):
            key = category.value
            category_counts[key] = category_counts.get(key, 0) + 1

    return {
        "history_count": history_count,
        "enabled_rules": ["fair_rotation"],
        "fair_rotation_enabled": True,
        "fair_rotation_cost": assignment_cost,
        "lookback": rule.lookback,
        "avoid_repeating_categories": [category.value for category in rule.avoid_repeating_categories],
        "current_assignment_categories": category_counts,
    }


def fairness_metadata(rules: RuleSet, history: SeatHistory | None) -> dict[str, object]:
    rule = rules.soft.fair_rotation
    history_count = history.history_count if history is not None else 0
    enabled = rule.enabled and rule.weight > 0 and history_count > 0
    return {
        "history_count": history_count,
        "enabled_rules": ["fair_rotation"] if enabled else [],
        "fair_rotation_enabled": rule.enabled,
        "warnings": history.warnings if history is not None and history.warnings else [],
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
