from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

from seattrellis.io.json_files import InputFileError, load_layout
from seattrellis.io.students import read_students
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import MinDistanceRule, RuleSet
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import load_rules_with_preset, preset_context_warnings

SeatEdge = tuple[str, str]

IssueLevel = Literal["error", "warning"]


@dataclass(frozen=True)
class ValidationIssue:
    level: IssueLevel
    message: str


@dataclass
class ValidationReport:
    students_count: int = 0
    enabled_seats_count: int = 0
    hard_constraints_count: int = 0
    issues: list[ValidationIssue] = field(default_factory=list)

    @property
    def errors(self) -> list[str]:
        return [issue.message for issue in self.issues if issue.level == "error"]

    @property
    def warnings(self) -> list[str]:
        return [issue.message for issue in self.issues if issue.level == "warning"]

    @property
    def ok(self) -> bool:
        return not self.errors

    def add_error(self, message: str) -> None:
        self.issues.append(ValidationIssue("error", message))

    def add_warning(self, message: str) -> None:
        self.issues.append(ValidationIssue("warning", message))

    def extend(self, other: "ValidationReport") -> None:
        self.students_count = other.students_count
        self.enabled_seats_count = other.enabled_seats_count
        self.hard_constraints_count = other.hard_constraints_count
        self.issues.extend(other.issues)

    def raise_for_errors(self, *, strict: bool = False, title: str = "Input validation failed.") -> None:
        if self.errors or (strict and self.warnings):
            raise InputFileError(self.format_failure(strict=strict, title=title))

    def format_success(self) -> str:
        lines = ["Validation passed.", "", "Summary:"]
        lines.extend(_summary_lines(self))
        if self.warnings:
            lines.append("")
            lines.append("Warnings:")
            lines.extend(f"- {warning}" for warning in self.warnings)
        return "\n".join(lines)

    def format_failure(self, *, strict: bool = False, title: str = "Validation failed.") -> str:
        lines = [title]
        if self.students_count or self.enabled_seats_count or self.hard_constraints_count:
            lines.append("")
            lines.append("Summary:")
            lines.extend(_summary_lines(self))
        if self.errors:
            lines.append("")
            lines.append("Errors:")
            lines.extend(f"- {error}" for error in self.errors)
        if self.warnings:
            lines.append("")
            warning_title = "Warnings treated as errors by --strict:" if strict else "Warnings:"
            lines.append(warning_title)
            lines.extend(f"- {warning}" for warning in self.warnings)
        return "\n".join(lines)


def validate_files(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    history_count: int = 0,
) -> ValidationReport:
    report = ValidationReport()
    students: list[Student] | None = None
    layout: ClassroomLayout | None = None
    rules: RuleSet | None = None
    preset = None

    try:
        students = read_students(students_path)
    except (InputFileError, MissingOptionalDependencyError, OSError) as exc:
        report.add_error(_friendly_exception(exc))

    try:
        layout = load_layout(layout_path)
    except (InputFileError, MissingOptionalDependencyError, OSError) as exc:
        report.add_error(_friendly_exception(exc))

    try:
        rules, preset = load_rules_with_preset(
            rules_path=rules_path,
            preset_name=preset_name,
        )
    except (InputFileError, MissingOptionalDependencyError, OSError) as exc:
        report.add_error(_friendly_exception(exc))

    if students is not None and layout is not None and rules is not None:
        report.extend(validate_loaded_inputs(students, layout, rules))
        for warning in preset_context_warnings(
            preset,
            students,
            history_count=history_count,
            rules=rules,
        ):
            report.add_warning(warning)
    return report


def validate_loaded_inputs(students: list[Student], layout: ClassroomLayout, rules: RuleSet) -> ValidationReport:
    report = ValidationReport(
        students_count=len(students),
        enabled_seats_count=len(layout.enabled_seats),
        hard_constraints_count=count_hard_constraints(rules),
    )
    enabled_seats = {seat.seat_id: seat for seat in layout.enabled_seats}
    all_seats = {seat.seat_id: seat for seat in layout.seats}
    edges = _build_adjacency_edges(layout)

    if len(students) > len(enabled_seats):
        report.add_error(
            f"Not enough enabled seats: {len(students)} students but only {len(enabled_seats)} enabled seats."
        )
    if not enabled_seats:
        report.add_error("Classroom layout has no enabled seats.")

    missing_ids = [student.display_name for student in students if not student.student_id]
    if missing_ids:
        shown = ", ".join(missing_ids[:5])
        suffix = "" if len(missing_ids) <= 5 else f", and {len(missing_ids) - 5} more"
        report.add_warning(
            f"Students without student_id will use name as the stable internal identifier: {shown}{suffix}."
        )

    refs, ambiguous_refs = _student_reference_map(students)
    fixed_student_seats: dict[int, tuple[str, str]] = {}
    fixed_seat_students: dict[str, tuple[int, str]] = {}

    for index, rule in enumerate(rules.hard.fixed_seats, start=1):
        student_index = _resolve_student(rule.student, refs, ambiguous_refs, report, f"hard.fixed_seats[{index}]")
        seat = all_seats.get(rule.seat_id)
        if seat is None:
            report.add_error(f'hard.fixed_seats[{index}] references unknown seat_id: "{rule.seat_id}".')
            continue
        if rule.seat_id not in enabled_seats:
            report.add_error(f'hard.fixed_seats[{index}] fixes "{rule.student}" to disabled seat: "{rule.seat_id}".')
            continue
        if student_index is None:
            continue
        previous = fixed_student_seats.get(student_index)
        if previous and previous[0] != rule.seat_id:
            report.add_error(
                "Conflicting fixed seats:\n"
                f'- {rule.student} is fixed to {previous[0]}\n'
                f"- {rule.student} is also fixed to {rule.seat_id}\n"
                "A student can only be fixed to one seat."
            )
        else:
            fixed_student_seats[student_index] = (rule.seat_id, rule.student)
        previous_student = fixed_seat_students.get(rule.seat_id)
        if previous_student and previous_student[0] != student_index:
            report.add_error(
                "Conflicting fixed seats:\n"
                f'- {previous_student[1]} is fixed to {rule.seat_id}\n'
                f"- {rule.student} is also fixed to {rule.seat_id}\n"
                "A seat can only be fixed to one student."
            )
        else:
            fixed_seat_students[rule.seat_id] = (student_index, rule.student)

    must_pairs = _collect_pair_rules(rules.hard.must_be_adjacent, refs, ambiguous_refs, report, "hard.must_be_adjacent")
    cannot_pairs = _collect_pair_rules(
        rules.hard.cannot_be_adjacent, refs, ambiguous_refs, report, "hard.cannot_be_adjacent"
    )
    min_distance_pairs = _collect_min_distance_rules(rules.hard.min_distance, refs, ambiguous_refs, report)

    _check_pair_rule_conflicts(report, must_pairs, cannot_pairs)
    _check_fixed_pair_conflicts(report, fixed_student_seats, must_pairs, cannot_pairs, min_distance_pairs, enabled_seats, layout, edges)
    _check_min_distance_must_adjacent_conflicts(report, must_pairs, min_distance_pairs, layout, enabled_seats, edges)
    return report


def validate_capacity(students: list[Student], layout: ClassroomLayout) -> None:
    enabled_count = len(layout.enabled_seats)
    if len(students) > enabled_count:
        raise ValueError(f"Not enough enabled seats: {len(students)} students but only {enabled_count} enabled seats.")


def count_hard_constraints(rules: RuleSet) -> int:
    return (
        len(rules.hard.fixed_seats)
        + len(rules.hard.must_be_adjacent)
        + len(rules.hard.cannot_be_adjacent)
        + len(rules.hard.min_distance)
    )


def format_infeasible_diagnostic(
    students: list[Student],
    layout: ClassroomLayout,
    rules: RuleSet,
    report: ValidationReport | None = None,
) -> str:
    report = report or validate_loaded_inputs(students, layout, rules)
    lines = ["No feasible seating plan was found.", "", "Summary:"]
    lines.extend(_summary_lines(report))
    lines.append("")
    if report.errors:
        lines.append("Conflicts detected before solving:")
        lines.extend(f"- {error}" for error in report.errors)
    else:
        lines.append("No direct contradiction was detected before solving.")
        lines.append("Possible causes:")
        lines.append("- too many fixed seats near each other;")
        lines.append("- cannot-adjacent rules are too dense;")
        lines.append("- min-distance rules leave too few candidate seats;")
        lines.append("- disabled seats reduce available positions.")
    lines.append("")
    lines.append("Try relaxing one or more hard constraints, or run with fewer restrictions first.")
    return "\n".join(lines)


def _summary_lines(report: ValidationReport) -> list[str]:
    return [
        f"- students: {report.students_count}",
        f"- enabled seats: {report.enabled_seats_count}",
        f"- hard constraints: {report.hard_constraints_count}",
    ]


def _friendly_exception(exc: Exception) -> str:
    return str(exc) or exc.__class__.__name__


def _student_reference_map(students: list[Student]) -> tuple[dict[str, int], set[str]]:
    refs: dict[str, int] = {}
    ambiguous: set[str] = set()
    for index, student in enumerate(students):
        for value in (student.student_id, student.name):
            if not value:
                continue
            if value in refs and refs[value] != index:
                ambiguous.add(value)
            else:
                refs[value] = index
    return refs, ambiguous


def _resolve_student(
    ref: str,
    refs: dict[str, int],
    ambiguous_refs: set[str],
    report: ValidationReport,
    location: str,
) -> int | None:
    if ref in ambiguous_refs:
        report.add_error(f'{location} references ambiguous student: "{ref}". Use a unique student_id.')
        return None
    if ref not in refs:
        report.add_error(f'{location} references unknown student: "{ref}".')
        return None
    return refs[ref]


PairEntry = tuple[int, int, str, str, str]
MinDistanceEntry = tuple[int, int, str, str, str, MinDistanceRule]


def _pair_key(first: int, second: int) -> tuple[int, int]:
    return (first, second) if first < second else (second, first)


def _collect_pair_rules(
    rules: list,
    refs: dict[str, int],
    ambiguous_refs: set[str],
    report: ValidationReport,
    label: str,
) -> dict[tuple[int, int], list[PairEntry]]:
    pairs: dict[tuple[int, int], list[PairEntry]] = {}
    for index, rule in enumerate(rules, start=1):
        first_ref, second_ref = rule.students
        first = _resolve_student(first_ref, refs, ambiguous_refs, report, f"{label}[{index}]")
        second = _resolve_student(second_ref, refs, ambiguous_refs, report, f"{label}[{index}]")
        if first is None or second is None:
            continue
        if first == second:
            report.add_error(f"{label}[{index}] must reference two different students.")
            continue
        pairs.setdefault(_pair_key(first, second), []).append((first, second, first_ref, second_ref, f"{label}[{index}]"))
    return pairs


def _collect_min_distance_rules(
    rules: list[MinDistanceRule],
    refs: dict[str, int],
    ambiguous_refs: set[str],
    report: ValidationReport,
) -> dict[tuple[int, int], list[MinDistanceEntry]]:
    pairs: dict[tuple[int, int], list[MinDistanceEntry]] = {}
    for index, rule in enumerate(rules, start=1):
        first_ref, second_ref = rule.students
        first = _resolve_student(first_ref, refs, ambiguous_refs, report, f"hard.min_distance[{index}]")
        second = _resolve_student(second_ref, refs, ambiguous_refs, report, f"hard.min_distance[{index}]")
        if first is None or second is None:
            continue
        if first == second:
            report.add_error(f"hard.min_distance[{index}] must reference two different students.")
            continue
        pairs.setdefault(_pair_key(first, second), []).append(
            (first, second, first_ref, second_ref, f"hard.min_distance[{index}]", rule)
        )
    return pairs


def _check_pair_rule_conflicts(
    report: ValidationReport,
    must_pairs: dict[tuple[int, int], list[PairEntry]],
    cannot_pairs: dict[tuple[int, int], list[PairEntry]],
) -> None:
    for key in sorted(set(must_pairs) & set(cannot_pairs)):
        must = must_pairs[key][0]
        cannot = cannot_pairs[key][0]
        report.add_error(
            "Conflicting hard constraints:\n"
            f"- {must[2]} and {must[3]} must be adjacent ({must[4]})\n"
            f"- {cannot[2]} and {cannot[3]} cannot be adjacent ({cannot[4]})\n"
            "The same student pair cannot have both rules."
        )


def _check_fixed_pair_conflicts(
    report: ValidationReport,
    fixed_student_seats: dict[int, tuple[str, str]],
    must_pairs: dict[tuple[int, int], list[PairEntry]],
    cannot_pairs: dict[tuple[int, int], list[PairEntry]],
    min_distance_pairs: dict[tuple[int, int], list[MinDistanceEntry]],
    enabled_seats: dict[str, SeatNode],
    layout: ClassroomLayout,
    edges: set[SeatEdge],
) -> None:
    for pair_list in must_pairs.values():
        first, second, first_ref, second_ref, location = pair_list[0]
        if first in fixed_student_seats and second in fixed_student_seats:
            first_seat_id = fixed_student_seats[first][0]
            second_seat_id = fixed_student_seats[second][0]
            if first_seat_id == second_seat_id or _normalize_edge(first_seat_id, second_seat_id) not in edges:
                report.add_error(
                    "Conflicting hard constraints:\n"
                    f"- {first_ref} is fixed to {first_seat_id}\n"
                    f"- {second_ref} is fixed to {second_seat_id}\n"
                    f"- {first_ref} and {second_ref} must be adjacent ({location})\n"
                    "But those seats are not adjacent."
                )

    for pair_list in cannot_pairs.values():
        first, second, first_ref, second_ref, location = pair_list[0]
        if first in fixed_student_seats and second in fixed_student_seats:
            first_seat_id = fixed_student_seats[first][0]
            second_seat_id = fixed_student_seats[second][0]
            if first_seat_id != second_seat_id and _normalize_edge(first_seat_id, second_seat_id) in edges:
                report.add_error(
                    "Conflicting hard constraints:\n"
                    f"- {first_ref} is fixed to {first_seat_id}\n"
                    f"- {second_ref} is fixed to {second_seat_id}\n"
                    f"- {first_ref} and {second_ref} cannot be adjacent ({location})\n"
                    f"But {first_seat_id} and {second_seat_id} are adjacent seats."
                )

    for pair_list in min_distance_pairs.values():
        first, second, first_ref, second_ref, location, rule = pair_list[0]
        if first in fixed_student_seats and second in fixed_student_seats:
            first_seat = enabled_seats[fixed_student_seats[first][0]]
            second_seat = enabled_seats[fixed_student_seats[second][0]]
            distance = _distance_for_rule(layout, first_seat, second_seat, rule)
            if distance < rule.distance:
                report.add_error(
                    "Conflicting hard constraints:\n"
                    f"- {first_ref} is fixed to {first_seat.seat_id}\n"
                    f"- {second_ref} is fixed to {second_seat.seat_id}\n"
                    f"- {first_ref} and {second_ref} require min_distance {rule.distance:g} ({location})\n"
                    f"But their current distance is {distance:g}."
                )


def _check_min_distance_must_adjacent_conflicts(
    report: ValidationReport,
    must_pairs: dict[tuple[int, int], list[PairEntry]],
    min_distance_pairs: dict[tuple[int, int], list[MinDistanceEntry]],
    layout: ClassroomLayout,
    enabled_seats: dict[str, SeatNode],
    edges: set[SeatEdge],
) -> None:
    if not edges and must_pairs:
        for pair_list in must_pairs.values():
            entry = pair_list[0]
            report.add_error(
                f"Conflicting hard constraints: {entry[4]} cannot be satisfied because the layout has no adjacent enabled seats."
            )

    for key in sorted(set(must_pairs) & set(min_distance_pairs)):
        must = must_pairs[key][0]
        for _, _, first_ref, second_ref, location, rule in min_distance_pairs[key]:
            if not _any_adjacent_pair_satisfies_min_distance(layout, enabled_seats, edges, rule):
                report.add_error(
                    "Conflicting hard constraints:\n"
                    f"- {must[2]} and {must[3]} must be adjacent ({must[4]})\n"
                    f"- {first_ref} and {second_ref} require min_distance {rule.distance:g} ({location})\n"
                    "No adjacent enabled seat pair can satisfy both rules."
                )


def _any_adjacent_pair_satisfies_min_distance(
    layout: ClassroomLayout,
    enabled_seats: dict[str, SeatNode],
    edges: set[SeatEdge],
    rule: MinDistanceRule,
) -> bool:
    if rule.metric == "graph":
        return rule.distance <= 1
    for first_id, second_id in edges:
        first = enabled_seats[first_id]
        second = enabled_seats[second_id]
        if _distance_for_rule(layout, first, second, rule) >= rule.distance:
            return True
    return False


def _distance_for_rule(layout: ClassroomLayout, first: SeatNode, second: SeatNode, rule: MinDistanceRule) -> float:
    if rule.metric == "graph":
        return _graph_distance(layout, first.seat_id, second.seat_id)
    return _seat_distance(first, second)


def _build_adjacency_edges(layout: ClassroomLayout) -> set[SeatEdge]:
    from seattrellis.solver.adjacency import build_adjacency_edges

    return build_adjacency_edges(layout)


def _normalize_edge(first: str, second: str) -> SeatEdge:
    from seattrellis.solver.adjacency import normalize_edge

    return normalize_edge(first, second)


def _seat_distance(first: SeatNode, second: SeatNode) -> float:
    from seattrellis.solver.adjacency import seat_distance

    return seat_distance(first, second)


def _graph_distance(layout: ClassroomLayout, first_id: str, second_id: str) -> float:
    from seattrellis.solver.adjacency import graph_distance

    return graph_distance(layout, first_id, second_id)
