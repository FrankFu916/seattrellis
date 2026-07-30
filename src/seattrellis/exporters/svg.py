"""Safe, dependency-free SVG export for seating canvas documents."""

from __future__ import annotations

from html import escape
from pathlib import Path
from typing import TYPE_CHECKING

from seattrellis.exporters.canvas import (
    CanvasSeat,
    SeatingCanvasDocument,
    build_seating_canvas,
    seat_font_size,
)
from seattrellis.exporters.presentation import xml_safe_text
from seattrellis.service_types import PrivacyOptions

if TYPE_CHECKING:
    from seattrellis.models.candidate import CandidatePlan
    from seattrellis.models.snapshot import SeatingSnapshot


def export_svg(
    snapshot: "SeatingSnapshot",
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: "CandidatePlan | None" = None,
    locale: str = "zh",
) -> Path:
    """Write a self-contained SVG made only from native vector elements."""

    document = build_seating_canvas(
        snapshot,
        template=template,
        privacy=privacy,
        candidate=candidate,
        locale=locale,
    )
    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render_svg(document), encoding="utf-8")
    return path


def render_svg(document: SeatingCanvasDocument) -> str:
    """Render ``document`` without scripts, links, or embedded resources."""

    theme = document.theme
    parts = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        (
            f'<svg xmlns="http://www.w3.org/2000/svg" '
            f'width="{_number(document.width)}" '
            f'height="{_number(document.height)}" '
            f'viewBox="0 0 {_number(document.width)} {_number(document.height)}" '
            f'lang="{_xml(document.locale)}" '
            f'aria-label="{_xml(document.title)}">'
        ),
        (
            f'<rect x="0" y="0" width="{_number(document.width)}" '
            f'height="{_number(document.height)}" fill="#{theme.background}"/>'
        ),
        _single_line_text(
            document.title,
            x=document.width / 2.0,
            y=60.0,
            size=34.0,
            colour=theme.heading,
            weight=700,
        ),
        _single_line_text(
            document.subtitle,
            x=document.width / 2.0,
            y=100.0,
            size=17.0,
            colour=theme.secondary_text,
            weight=400,
        ),
    ]

    for seat in document.seats:
        fill = theme.seat_fill if seat.enabled else theme.disabled_fill
        border = theme.seat_border if seat.enabled else theme.disabled_border
        parts.append(
            f'<rect x="{_number(seat.x)}" y="{_number(seat.y)}" '
            f'width="{_number(seat.width)}" height="{_number(seat.height)}" '
            f'rx="10" fill="#{fill}" stroke="#{border}" stroke-width="2"/>'
        )
        parts.append(_seat_text(seat, document))

    footer_y = document.height - 32.0
    parts.extend(
        [
            (
                f'<rect x="80" y="{_number(footer_y - 24.0)}" '
                f'width="{_number(document.width - 160.0)}" height="38" '
                f'rx="10" fill="#{theme.footer_fill}"/>'
            ),
            _single_line_text(
                document.footer,
                x=document.width / 2.0,
                y=footer_y,
                size=14.0,
                colour=theme.secondary_text,
                weight=500,
            ),
            "</svg>",
        ]
    )
    return "\n".join(parts) + "\n"


def _seat_text(seat: CanvasSeat, document: SeatingCanvasDocument) -> str:
    lines = seat.render_lines
    size = seat_font_size(seat)
    line_height = size * 1.22
    first_y = seat.y + seat.height / 2.0 - line_height * (len(lines) - 1) / 2.0
    text_colour = (
        document.theme.heading if seat.enabled else document.theme.empty_text
    )
    spans: list[str] = []
    for index, line in enumerate(lines):
        weight = 600 if index == 1 or (len(lines) == 1 and index == 0) else 400
        dy = "0" if index == 0 else _number(line_height)
        spans.append(
            f'<tspan x="{_number(seat.x + seat.width / 2.0)}" '
            f'dy="{dy}" font-weight="{weight}">{_xml(line)}</tspan>'
        )
    return (
        f'<text x="{_number(seat.x + seat.width / 2.0)}" '
        f'y="{_number(first_y)}" text-anchor="middle" '
        f'font-family="Arial, sans-serif" font-size="{_number(size)}" '
        f'fill="#{text_colour}">' + "".join(spans) + "</text>"
    )


def _single_line_text(
    value: str,
    *,
    x: float,
    y: float,
    size: float,
    colour: str,
    weight: int,
) -> str:
    return (
        f'<text x="{_number(x)}" y="{_number(y)}" text-anchor="middle" '
        f'font-family="Arial, sans-serif" font-size="{_number(size)}" '
        f'font-weight="{weight}" fill="#{colour}">'
        f'<tspan x="{_number(x)}">{_xml(value)}</tspan></text>'
    )


def _number(value: float) -> str:
    return f"{value:.3f}".rstrip("0").rstrip(".")


def _xml(value: object) -> str:
    return escape(xml_safe_text(value), quote=True)
