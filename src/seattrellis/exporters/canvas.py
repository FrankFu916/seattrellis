"""Format-neutral seating canvas used by vector and slide exporters."""

from __future__ import annotations

from dataclasses import dataclass
from math import isclose, isfinite
from re import fullmatch
from typing import TYPE_CHECKING

from seattrellis.exporters.presentation import (
    student_detail_fields,
    student_display_names,
)
from seattrellis.service_types import (
    PrivacyOptions,
    normalize_export_locale,
    normalize_export_template,
)

if TYPE_CHECKING:
    from seattrellis.models.candidate import CandidatePlan
    from seattrellis.models.snapshot import SeatingSnapshot


CANVAS_WIDTH = 1600.0
CANVAS_HEIGHT = 900.0


@dataclass(frozen=True)
class CanvasTheme:
    """Renderer-independent colours for a seating canvas."""

    background: str = "F7F8FA"
    heading: str = "172033"
    secondary_text: str = "5D6678"
    seat_fill: str = "EAF2FF"
    seat_border: str = "4E7EDB"
    disabled_fill: str = "ECEFF3"
    disabled_border: str = "A6ADBA"
    empty_text: str = "7B8495"
    footer_fill: str = "EDF4FF"

    def __post_init__(self) -> None:
        for field_name in (
            "background",
            "heading",
            "secondary_text",
            "seat_fill",
            "seat_border",
            "disabled_fill",
            "disabled_border",
            "empty_text",
            "footer_fill",
        ):
            value = getattr(self, field_name)
            if not isinstance(value, str) or fullmatch(r"[0-9A-Fa-f]{6}", value) is None:
                raise ValueError(
                    f"CanvasTheme.{field_name} must be a six-digit hexadecimal colour."
                )
            object.__setattr__(self, field_name, value.upper())


DEFAULT_CANVAS_THEME = CanvasTheme()


@dataclass(frozen=True)
class CanvasSeat:
    """One editable seat card with resolved geometry and display text."""

    seat_id: str
    x: float
    y: float
    width: float
    height: float
    enabled: bool
    student_key: str | None
    primary_text: str
    detail_lines: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.seat_id, str) or not self.seat_id.strip():
            raise ValueError("CanvasSeat.seat_id cannot be empty.")
        for field_name in ("x", "y", "width", "height"):
            value = getattr(self, field_name)
            if not isinstance(value, (int, float)) or not isfinite(float(value)):
                raise ValueError(f"CanvasSeat.{field_name} must be finite.")
            if field_name in {"x", "y"} and value < 0:
                raise ValueError(f"CanvasSeat.{field_name} cannot be negative.")
            if field_name in {"width", "height"} and value <= 0:
                raise ValueError(f"CanvasSeat.{field_name} must be positive.")
            object.__setattr__(self, field_name, float(value))
        object.__setattr__(self, "primary_text", str(self.primary_text))
        object.__setattr__(
            self,
            "detail_lines",
            tuple(str(line) for line in self.detail_lines),
        )

    @property
    def occupied(self) -> bool:
        return self.student_key is not None

    @property
    def text_lines(self) -> tuple[str, ...]:
        return (self.primary_text, *self.detail_lines)

    @property
    def render_lines(self) -> tuple[str, ...]:
        """Return every visible line, including the physical seat label."""

        if self.primary_text == self.seat_id:
            return (self.seat_id, *self.detail_lines)
        return (self.seat_id, self.primary_text, *self.detail_lines)


@dataclass(frozen=True)
class SeatingCanvasDocument:
    """A stable 16:9 seating document shared by SVG and PowerPoint."""

    width: float
    height: float
    title: str
    subtitle: str
    footer: str
    locale: str
    template: str
    seats: tuple[CanvasSeat, ...]
    theme: CanvasTheme = DEFAULT_CANVAS_THEME

    def __post_init__(self) -> None:
        for field_name in ("width", "height"):
            value = getattr(self, field_name)
            if not isinstance(value, (int, float)) or not isfinite(float(value)):
                raise ValueError(
                    f"SeatingCanvasDocument.{field_name} must be finite."
                )
            if value <= 0:
                raise ValueError(
                    f"SeatingCanvasDocument.{field_name} must be positive."
                )
            object.__setattr__(self, field_name, float(value))
        if not isclose(self.width / self.height, 16 / 9, rel_tol=1e-9):
            raise ValueError("SeatingCanvasDocument must use a 16:9 aspect ratio.")
        object.__setattr__(self, "title", str(self.title))
        object.__setattr__(self, "subtitle", str(self.subtitle))
        object.__setattr__(self, "footer", str(self.footer))
        object.__setattr__(self, "locale", normalize_export_locale(self.locale))
        object.__setattr__(
            self,
            "template",
            normalize_export_template(self.template),
        )
        seats = tuple(self.seats)
        for seat in seats:
            if not isinstance(seat, CanvasSeat):
                raise TypeError("SeatingCanvasDocument.seats must contain CanvasSeat values.")
            if seat.x + seat.width > self.width + 1e-6:
                raise ValueError(f"Canvas seat {seat.seat_id!r} exceeds canvas width.")
            if seat.y + seat.height > self.height + 1e-6:
                raise ValueError(f"Canvas seat {seat.seat_id!r} exceeds canvas height.")
        object.__setattr__(self, "seats", seats)
        if not isinstance(self.theme, CanvasTheme):
            raise TypeError("SeatingCanvasDocument.theme must be a CanvasTheme.")

    @property
    def aspect_ratio(self) -> float:
        return self.width / self.height


_TEXT: dict[str, dict[str, str]] = {
    "public_subtitle": {
        "zh": "班级座位表",
        "en": "Class seating chart",
    },
    "teacher_subtitle": {
        "zh": "教师工作版",
        "en": "Teacher working copy",
    },
    "report_subtitle": {
        "zh": "候选方案报告",
        "en": "Candidate plan report",
    },
    "generated": {"zh": "生成时间", "en": "Generated"},
    "candidate": {"zh": "候选方案", "en": "Candidate"},
    "score": {"zh": "总分", "en": "Total score"},
    "public_footer": {
        "zh": "公示版：敏感字段已隐藏",
        "en": "Public copy: sensitive fields are hidden",
    },
    "teacher_footer": {
        "zh": "教师内部资料，请妥善保管",
        "en": "Teacher copy: keep this information private",
    },
    "report_footer": {
        "zh": "方案说明资料",
        "en": "Plan explanation",
    },
    "anonymous_footer": {
        "zh": "姓名已匿名化",
        "en": "Names are anonymized",
    },
}


def build_seating_canvas(
    snapshot: "SeatingSnapshot",
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: "CandidatePlan | None" = None,
    locale: str = "zh",
) -> SeatingCanvasDocument:
    """Compile a snapshot into a deterministic 16:9 canvas document.

    Privacy defaults and student display decisions come from the same helpers
    used by the existing configurable exporters.
    """

    template = normalize_export_template(template)
    locale = normalize_export_locale(locale)
    privacy = privacy or PrivacyOptions.for_template(template)
    if template == "report" and candidate is None:
        raise ValueError("The report template requires a candidate plan.")

    display_names = student_display_names(snapshot, privacy, locale)
    student_by_key = {student.key: student for student in snapshot.students}
    assignment_by_seat = {
        assignment.seat_id: assignment for assignment in snapshot.assignments
    }

    min_row, max_row, min_col, max_col = _bounds(snapshot)
    row_count = max_row - min_row + 1
    column_count = max_col - min_col + 1
    grid_left = 80.0
    grid_top = 150.0
    grid_width = CANVAS_WIDTH - 160.0
    grid_height = CANVAS_HEIGHT - 225.0
    gap_x = min(18.0, grid_width / max(column_count * 10.0, 1.0))
    gap_y = min(16.0, grid_height / max(row_count * 10.0, 1.0))
    cell_width = (grid_width - gap_x * (column_count - 1)) / column_count
    cell_height = (grid_height - gap_y * (row_count - 1)) / row_count

    # Avoid oversized cards for sparse layouts and keep the grid centred.
    card_width = min(cell_width, 260.0)
    card_height = min(cell_height, 160.0)
    rendered_width = card_width * column_count + gap_x * (column_count - 1)
    rendered_height = card_height * row_count + gap_y * (row_count - 1)
    grid_left += (grid_width - rendered_width) / 2.0
    grid_top += (grid_height - rendered_height) / 2.0

    canvas_seats: list[CanvasSeat] = []
    for seat in sorted(snapshot.layout.seats, key=lambda item: (item.row, item.col, item.seat_id)):
        assignment = assignment_by_seat.get(seat.seat_id)
        student_key = assignment.student_key if assignment and seat.enabled else None
        primary_text = (
            display_names.get(student_key, student_key or seat.seat_id)
            if student_key
            else seat.seat_id
        )
        detail_lines: tuple[str, ...] = ()
        if template == "teacher" and student_key is not None:
            student = student_by_key.get(student_key)
            detail_lines = tuple(
                f"{label}: {value}"
                for label, value in student_detail_fields(student, privacy, locale)
                if value != "-"
            )
        canvas_seats.append(
            CanvasSeat(
                seat_id=seat.seat_id,
                x=_stable_number(
                    grid_left + (seat.col - min_col) * (card_width + gap_x)
                ),
                y=_stable_number(
                    grid_top + (seat.row - min_row) * (card_height + gap_y)
                ),
                width=_stable_number(card_width),
                height=_stable_number(card_height),
                enabled=seat.enabled,
                student_key=student_key,
                primary_text=primary_text,
                detail_lines=detail_lines,
            )
        )

    subtitle_parts = [_TEXT[f"{template}_subtitle"][locale]]
    if snapshot.created_at:
        subtitle_parts.append(
            f"{_TEXT['generated'][locale]}: {snapshot.created_at.isoformat()}"
        )
    if candidate is not None:
        subtitle_parts.append(
            f"{_TEXT['candidate'][locale]}: {candidate.candidate_id}"
        )
        subtitle_parts.append(
            f"{_TEXT['score'][locale]}: {candidate.total_score:.1f}"
        )

    footer = _TEXT[f"{template}_footer"][locale]
    if privacy.anonymize:
        footer = f"{footer} · {_TEXT['anonymous_footer'][locale]}"

    return SeatingCanvasDocument(
        width=CANVAS_WIDTH,
        height=CANVAS_HEIGHT,
        title=snapshot.layout.name,
        subtitle=" · ".join(subtitle_parts),
        footer=footer,
        locale=locale,
        template=template,
        seats=tuple(canvas_seats),
    )


def _bounds(snapshot: "SeatingSnapshot") -> tuple[int, int, int, int]:
    rows = [seat.row for seat in snapshot.layout.seats]
    columns = [seat.col for seat in snapshot.layout.seats]
    return min(rows), max(rows), min(columns), max(columns)


def _stable_number(value: float) -> float:
    return round(value, 3)


def seat_font_size(seat: CanvasSeat) -> float:
    """Choose a stable canvas font size that keeps all seat lines visible."""

    lines = seat.render_lines
    line_count = max(len(lines), 1)
    height_limit = (seat.height - 18.0) / (line_count * 1.22)
    longest = max((len(line) for line in lines), default=1)
    width_limit = (seat.width - 16.0) / max(longest * 0.58, 1.0)
    return max(6.0, min(22.0, height_limit, width_limit))
