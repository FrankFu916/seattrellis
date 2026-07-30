"""Application-facing roster import helpers.

This module gives teacher-oriented workflows a small, stable result shape while
leaving file parsing, column aliases, and validation to the existing I/O layer.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from seattrellis.io.students import read_students, students_from_records
from seattrellis.models.student import Student, student_needs_front


@dataclass(frozen=True)
class RosterSummary:
    """Counts used to explain which roster data can influence seating."""

    student_count: int
    name_only_count: int
    score_count: int
    height_count: int
    vision_or_front_need_count: int
    special_needs_count: int


@dataclass(frozen=True)
class ImportedRoster:
    """A validated roster together with a display name and concise summary."""

    students: tuple[Student, ...]
    summary: RosterSummary
    source_name: str | None = None


def import_roster(path: str | Path) -> ImportedRoster:
    """Read a CSV or Excel roster using the established student importer.

    Errors from :func:`seattrellis.io.students.read_students` deliberately pass
    through unchanged so CLI and web adapters retain the same diagnostics.
    """

    source = Path(path)
    students = read_students(source)
    return _imported_roster(students, source_name=source.name)


def import_roster_records(
    records: Iterable[Mapping[str, Any]],
    source_name: str | None = None,
) -> ImportedRoster:
    """Build a roster from tabular records using the existing alias handling."""

    students = students_from_records(records)
    return _imported_roster(students, source_name=source_name)


def summarize_roster(students: Iterable[Student]) -> RosterSummary:
    """Summarize data availability without exposing individual student data."""

    student_count = 0
    name_only_count = 0
    score_count = 0
    height_count = 0
    vision_or_front_need_count = 0
    special_needs_count = 0

    for student in students:
        student_count += 1
        name_only_count += student.student_id is None
        score_count += student.score is not None
        height_count += student.height_cm is not None
        # A front-seat need may be recorded in vision, tags, or needs. Reuse the
        # domain predicate so this summary stays aligned with solver behavior.
        vision_or_front_need_count += (
            student.vision is not None or student_needs_front(student)
        )
        special_needs_count += bool(student.needs)

    return RosterSummary(
        student_count=student_count,
        name_only_count=name_only_count,
        score_count=score_count,
        height_count=height_count,
        vision_or_front_need_count=vision_or_front_need_count,
        special_needs_count=special_needs_count,
    )


def _imported_roster(
    students: Iterable[Student],
    *,
    source_name: str | None,
) -> ImportedRoster:
    roster = tuple(students)
    return ImportedRoster(
        students=roster,
        summary=summarize_roster(roster),
        source_name=source_name,
    )
