"""Streamlit-renderable components for the SeatTrellis web UI.

All business-logic functions here accept plain data and return plain data
or HTML strings — they do not import streamlit, so they remain testable
without the ``web`` extra.
"""

from __future__ import annotations

from html import escape as html_escape
from typing import Any, Mapping, Sequence

from seattrellis.models.candidate import CandidatePlan, CandidateSet
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.web.i18n import normalize_locale, table_column_labels, translate


def build_data_table_html(
    rows: Sequence[Mapping[str, object]],
    *,
    caption: str,
    locale: str = "zh",
    columns: Sequence[str] | None = None,
) -> str:
    """Render a small, accessible table without pandas or Arrow conversion.

    Streamlit's dataframe element converts Python records through pandas and
    pyarrow. That native conversion is unnecessary for the compact tables in
    SeatTrellis and can terminate the whole Web process when the underlying
    libraries are ABI-incompatible. Keeping table rendering in plain HTML also
    makes escaping, column order, and accessibility explicit.
    """

    locale = normalize_locale(locale)
    resolved_columns = list(columns or (rows[0].keys() if rows else ()))
    if not resolved_columns:
        return '<p class="seattrellis-empty-table"><em>—</em></p>'

    labels = table_column_labels(locale)
    header_html = "".join(
        f'<th scope="col">{html_escape(labels.get(column, column))}</th>'
        for column in resolved_columns
    )
    body_html = "".join(
        "<tr>"
        + "".join(
            f"<td>{html_escape(_table_cell_text(row.get(column)))}</td>"
            for column in resolved_columns
        )
        + "</tr>"
        for row in rows
    )
    if not body_html:
        body_html = (
            f'<tr><td colspan="{len(resolved_columns)}" '
            'class="seattrellis-empty-cell">—</td></tr>'
        )

    escaped_caption = html_escape(caption, quote=True)
    return (
        "<style>"
        ".seattrellis-table-scroll{overflow-x:auto;margin:.35rem 0 1rem;"
        "border:1px solid rgba(128,128,128,.22);border-radius:.65rem;}"
        ".seattrellis-data-table{width:100%;border-collapse:collapse;"
        "font-size:.9rem;line-height:1.4;}"
        ".seattrellis-data-table th,.seattrellis-data-table td{"
        "padding:.58rem .7rem;text-align:left;white-space:nowrap;"
        "border-bottom:1px solid rgba(128,128,128,.18);}"
        ".seattrellis-data-table th{font-weight:600;"
        "background:rgba(128,128,128,.08);}"
        ".seattrellis-data-table tbody tr:last-child td{border-bottom:0;}"
        ".seattrellis-data-table tbody tr:hover{background:rgba(128,128,128,.05);}"
        ".seattrellis-table-caption{position:absolute;width:1px;height:1px;"
        "padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);"
        "white-space:nowrap;border:0;}"
        ".seattrellis-empty-cell{text-align:center!important;color:#777;}"
        "</style>"
        f'<div class="seattrellis-table-scroll" role="region" '
        f'aria-label="{escaped_caption}" tabindex="0">'
        '<table class="seattrellis-data-table">'
        f'<caption class="seattrellis-table-caption">{escaped_caption}</caption>'
        f"<thead><tr>{header_html}</tr></thead>"
        f"<tbody>{body_html}</tbody>"
        "</table></div>"
    )


def _table_cell_text(value: object) -> str:
    if value is None:
        return "—"
    if isinstance(value, bool):
        return "✓" if value else "—"
    if isinstance(value, (list, tuple, set, frozenset)):
        return ", ".join(str(item) for item in value) or "—"
    return str(value)


def build_seat_grid_html(
    layout: ClassroomLayout,
    snapshot: SeatingSnapshot | None = None,
    highlight_seat_id: str | None = None,
    locale: str = "zh",
) -> str:
    """Build an HTML/CSS Grid rendering of the classroom seat map.

    Parameters
    ----------
    layout:
        Classroom layout with seat nodes.
    snapshot:
        Optional seating assignments.  When ``None``, all enabled seats show as
        empty.
    highlight_seat_id:
        Optionally highlight one seat (e.g. on hover / click in the future).

    Returns
    -------
    str
        An inline HTML string suitable for ``st.markdown(..., unsafe_allow_html=True)``.
    """
    locale = normalize_locale(locale)
    if not layout.seats:
        return f"<p><em>{html_escape(translate('empty_layout', locale))}</em></p>"

    row_values, col_values = layout_grid_axes(layout.seats)

    assignment_map: dict[str, str] = {}
    if snapshot is not None:
        for a in snapshot.assignments:
            assignment_map[a.seat_id] = a.student_name or a.student_key
    seat_by_position = {(seat.row, seat.col): seat for seat in layout.seats}

    rows_html: list[str] = []
    for r in row_values:
        cells: list[str] = []
        for c in col_values:
            seat = seat_by_position.get((r, c))
            if seat is None:
                cells.append('<div class="seat-cell empty-cell"></div>')
                continue

            seat_class = "seat-cell"
            if not seat.enabled:
                seat_class += " disabled-seat"
            elif seat.seat_id == highlight_seat_id:
                seat_class += " highlighted-seat"

            # Tag-based color classes.
            tag_classes = _tag_color_classes(seat)
            if tag_classes:
                seat_class += " " + tag_classes

            student_name = assignment_map.get(seat.seat_id, "")
            label = html_escape(student_name or seat.seat_id)

            tooltip = _seat_tooltip(seat, student_name, locale)
            cells.append(
                f'<div class="{seat_class}" role="gridcell" '
                f'tabindex="{"0" if seat.enabled else "-1"}" '
                f'aria-disabled="{"false" if seat.enabled else "true"}" '
                f'aria-label="{tooltip}" title="{tooltip}">'
                f'<span class="seat-label">{label}</span>'
                f"</div>"
            )
        rows_html.append(
            '<div class="seat-row" role="row">' + "".join(cells) + "</div>"
        )

    css = _seat_grid_css()
    return (
        f"<style>{css}</style>"
        f'<div class="seat-grid" role="grid" '
        f'aria-label="{html_escape(translate("seat_grid_label", locale), quote=True)}">'
        + "".join(rows_html)
        + "</div>"
    )


def layout_grid_axes(seats: Sequence[SeatNode]) -> tuple[list[int], list[int]]:
    """Return compact row and column axes for a sparse classroom layout.

    Layout coordinates are identifiers rather than a license to allocate every
    intervening cell. Explicit disabled seats can still represent intentional
    gaps, while malformed coordinates cannot expand a small layout into a huge
    browser grid.
    """

    return (
        sorted({seat.row for seat in seats}),
        sorted({seat.col for seat in seats}),
    )


def build_candidate_selector(
    candidate_set: CandidateSet,
    current_id: str = "recommended",
    locale: str = "zh",
) -> list[dict[str, object]]:
    """Build a list of candidate options for a select-box / radio group.

    Returns a list of dicts with ``id``, ``label``, and ``is_recommended``
    keys so the Streamlit layer can render them.
    """
    options: list[dict[str, object]] = []
    # "recommended" pseudo-entry first.
    rec = candidate_set.get_candidate(candidate_set.recommended_candidate_id)
    options.append(
        {
            "id": "recommended",
            "label": f"{translate('recommended', locale)} — "
            f"{candidate_set.recommended_candidate_id}"
            f" ({rec.total_score:.1f})"
            if rec
            else translate("recommended", locale),
            "is_recommended": True,
        }
    )
    for candidate in sorted(
        candidate_set.candidates,
        key=lambda item: (-item.total_score, item.candidate_id),
    ):
        if candidate.candidate_id == candidate_set.recommended_candidate_id:
            continue  # already added as the "recommended" pseudo-entry above
        options.append(
            {
                "id": candidate.candidate_id,
                "label": f"{candidate.candidate_id} — {candidate.total_score:.1f}",
                "is_recommended": False,
            }
        )
    return options


def build_comparison_table(
    candidate_set: CandidateSet,
) -> dict[str, object]:
    """Build a comparison data structure for all candidates.

    Returns a dict with ``columns`` (list of column names) and ``rows``
    (list of dicts keyed by column name) suitable for ``st.dataframe`` or
    ``st.table``.
    """
    columns = [
        "candidate_id",
        "recommended",
        "total",
        "hard_constraints",
        "fair_rotation",
        "neighbors",
        "score_balance",
        "height",
        "vision",
        "diversity",
        "stability",
    ]
    rows: list[dict[str, object]] = []
    for candidate in sorted(
        candidate_set.candidates,
        key=lambda item: (-item.total_score, item.candidate_id),
    ):
        b = candidate.score.breakdown
        rows.append(
            {
                "candidate_id": candidate.candidate_id,
                "recommended": "⭐"
                if candidate.candidate_id == candidate_set.recommended_candidate_id
                else "",
                "total": round(candidate.total_score, 1),
                "hard_constraints": "✅"
                if b.hard_constraint_summary.satisfied
                else f"❌ {b.hard_constraint_summary.violation_count}",
                "fair_rotation": _score_cell(b.fair_rotation_score.score),
                "neighbors": _score_cell(b.avoid_recent_neighbors_score.score),
                "score_balance": _score_cell(b.score_balance_score.score),
                "height": _score_cell(b.height_preference_score.score),
                "vision": _score_cell(b.vision_preference_score.score),
                "diversity": _score_cell(b.diversity_score.score),
                "stability": _score_cell(b.stability_score.score),
            }
        )
    return {"columns": columns, "rows": rows}


def build_preset_cards(locale: str = "zh") -> list[dict[str, str]]:
    """Return metadata cards for each built-in preset.

    Each card has ``name``, ``description``, ``scenario``, ``requires``,
    and ``degradation`` keys suitable for rendering as expandable cards.
    """
    zh_cards = [
        {
            "name": "random",
            "description": "使用种子随机排列，不启用任何数据依赖偏好。",
            "scenario": "无历史数据、无特殊需求的快速排座。",
            "requires": "无额外字段要求。",
            "degradation": "不会降级，不依赖学生字段。",
        },
        {
            "name": "exam",
            "description": "可复现的随机打散，间距和固定座位由显式 hard rules 决定。",
            "scenario": "考场、测验、不需要考虑社交因素的场景。",
            "requires": "无需额外字段；建议使用 hard rules 控制间距。",
            "degradation": "不会降级。",
        },
        {
            "name": "daily",
            "description": "综合无障碍、身高、成绩混合、公平轮换、关系回避。",
            "scenario": "日常上课，有历史座位记录时效果最佳。",
            "requires": "vision、height、score 字段（可选）；历史 snapshot（推荐）。",
            "degradation": "缺失 vision/height/score 时对应 soft rule 降级为不启用；无历史时公平轮换和关系回避不生效。",
        },
        {
            "name": "fair-rotation",
            "description": "优先将学生从重复使用的位置类别中轮换出去。",
            "scenario": "定期轮换座位，关注公平性。",
            "requires": "历史 snapshot（必需）。",
            "degradation": "无历史时公平轮换不生效，降级为普通随机排座。",
        },
        {
            "name": "neighbor-aware",
            "description": "优先减少最近重复的同桌和邻座关系。",
            "scenario": "需要打破小团体、减少课堂讲话的场景。",
            "requires": "历史 snapshot（必需）。",
            "degradation": "无历史时关系回避不生效，降级为普通随机排座。",
        },
        {
            "name": "balanced",
            "description": "偏好不同成绩水平的学生相邻，促进互助。",
            "scenario": "有成绩数据，希望异质分组。",
            "requires": "学生 score 字段。",
            "degradation": "无 score 字段时降级为普通随机排座。",
        },
        {
            "name": "height-aware",
            "description": "偏好高个学生靠后、矮个学生靠前。",
            "scenario": "需要按身高排座，保留可复现随机性。",
            "requires": "学生 height 字段。",
            "degradation": "无 height 字段时降级为普通随机排座。",
        },
        {
            "name": "vision-friendly",
            "description": "优先将标记为视力需求的学生安排在前排。",
            "scenario": "有学生需要前排座位（视力、注意力等）。",
            "requires": "学生 vision 或 needs_front 字段。",
            "degradation": "无 vision/needs_front 字段时降级为普通随机排座。",
        },
    ]
    if normalize_locale(locale) == "zh":
        return zh_cards
    english = {
        "random": (
            "Seeded random placement with no data-dependent preferences.",
            "A quick plan without history or special requirements.",
            "No additional fields.",
            "No fallback is needed.",
        ),
        "exam": (
            "Reproducible shuffling; spacing and fixed seats come from explicit hard rules.",
            "Exams and quizzes where social preferences are not needed.",
            "No extra fields; use hard rules to control spacing.",
            "No fallback is needed.",
        ),
        "daily": (
            "Combines accessibility, height, score mixing, fair rotation, and relationship avoidance.",
            "Everyday classes, especially when seating history is available.",
            "Optional vision, height, and score fields; history is recommended.",
            "Preferences without matching data are skipped. History-based preferences do nothing without history.",
        ),
        "fair-rotation": (
            "Moves students away from seat categories they have used repeatedly.",
            "Regular seat rotations with an emphasis on fairness.",
            "History snapshots.",
            "Without history, it behaves like a regular random plan.",
        ),
        "neighbor-aware": (
            "Reduces recently repeated desk-mate and neighboring pairs.",
            "Breaking up recurring groups or reducing classroom chatter.",
            "History snapshots.",
            "Without history, it behaves like a regular random plan.",
        ),
        "balanced": (
            "Prefers neighbors with different score levels to support peer learning.",
            "Classes with score data that want mixed-ability seating.",
            "The student score field.",
            "Without scores, it behaves like a regular random plan.",
        ),
        "height-aware": (
            "Prefers taller students farther back and shorter students nearer the front.",
            "Height-aware seating with reproducible randomness.",
            "The student height field.",
            "Without heights, it behaves like a regular random plan.",
        ),
        "vision-friendly": (
            "Prioritizes front seats for students marked as needing them.",
            "Students who need the front for vision, attention, or another reason.",
            "The vision or needs_front field.",
            "Without either field, it behaves like a regular random plan.",
        ),
    }
    return [
        {
            "name": card["name"],
            "description": english[card["name"]][0],
            "scenario": english[card["name"]][1],
            "requires": english[card["name"]][2],
            "degradation": english[card["name"]][3],
        }
        for card in zh_cards
    ]


# ---------------------------------------------------------------------------
# Error diagnosis helpers
# ---------------------------------------------------------------------------

def diagnose_error(exc: Exception, locale: str = "zh") -> dict[str, str]:
    """Categorise a Python exception into a user-readable diagnosis.

    Returns a dict with ``category``, ``title``, and ``detail`` keys.
    """
    from seattrellis.io.json_files import InputFileError
    from seattrellis.optional import MissingOptionalDependencyError
    from seattrellis.solver import SeatTrellisSolveError

    name = type(exc).__name__
    msg = str(exc)

    # Pydantic validation errors.
    if "ValidationError" in name or "validation" in msg.lower():
        return {
            "category": "validation",
            "title": translate("format_error_title", locale),
            "detail": translate("format_error_detail", locale, error=msg),
        }

    if isinstance(exc, InputFileError):
        return {
            "category": "file_error",
            "title": translate("file_error_title", locale),
            "detail": translate("file_error_detail", locale, error=msg),
        }

    if isinstance(exc, SeatTrellisSolveError):
        return {
            "category": "solve_error",
            "title": translate("solve_error_title", locale),
            "detail": translate("solve_error_detail", locale, error=msg),
        }

    if isinstance(exc, MissingOptionalDependencyError):
        return {
            "category": "missing_dependency",
            "title": translate("dependency_error_title", locale),
            "detail": translate("dependency_error_detail", locale, error=msg),
        }

    if isinstance(exc, ValueError):
        return {
            "category": "value_error",
            "title": translate("value_error_title", locale),
            "detail": translate("value_error_detail", locale, error=msg),
        }

    # Generic fallback.
    return {
        "category": "unknown",
        "title": translate("unknown_error_title", locale),
        "detail": translate(
            "unknown_error_detail",
            locale,
            name=name,
            error=msg,
        ),
    }


# ---------------------------------------------------------------------------
# Privacy notice
# ---------------------------------------------------------------------------

def build_privacy_notice_html(locale: str = "zh") -> str:
    """Build the localized privacy notice shown above each workflow."""
    return f"""
<div style="background:#f0f8f4;border:1px solid #c3e6cb;border-radius:8px;
padding:12px 16px;margin:8px 0;font-size:0.9rem;color:#155724;"
role="note" aria-label="{html_escape(translate('privacy_title', locale), quote=True)}">
<strong>{html_escape(translate('privacy_title', locale))}</strong><br>
{html_escape(translate('privacy_body', locale))}
</div>
"""


PRIVACY_NOTICE_HTML = build_privacy_notice_html()


def accessibility_styles() -> str:
    """Return focus, touch-target, small-screen, and reduced-motion styles."""
    return """
<style>
.seattrellis-skip-link {
    position: fixed;
    left: 0.75rem;
    top: -4rem;
    z-index: 1000000;
    padding: 0.7rem 1rem;
    border-radius: 0.4rem;
    background: #ffffff;
    color: #0b57d0;
    box-shadow: 0 2px 8px rgba(0,0,0,0.25);
}
.seattrellis-skip-link:focus { top: 0.75rem; }
:where(button, a, input, textarea, select, [tabindex="0"]):focus-visible {
    outline: 3px solid #0b57d0 !important;
    outline-offset: 2px !important;
}
div[data-testid="stButton"] button,
div[data-testid="stDownloadButton"] button {
    min-height: 44px;
}
@media (max-width: 768px) {
    .main .block-container {
        padding-left: 1rem;
        padding-right: 1rem;
        padding-top: 2rem;
    }
    div[data-testid="stHorizontalBlock"] {
        flex-direction: column;
        gap: 0.75rem;
    }
    div[data-testid="column"] { width: 100% !important; }
    .seat-cell {
        width: clamp(54px, 18vw, 78px);
        min-width: 54px;
    }
}
@media (prefers-reduced-motion: reduce) {
    *, *::before, *::after {
        scroll-behavior: auto !important;
        transition-duration: 0.01ms !important;
        animation-duration: 0.01ms !important;
    }
}
</style>
"""


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _tag_color_classes(seat: SeatNode) -> str:
    classes: list[str] = []
    if seat.near_window:
        classes.append("tag-window")
    if seat.near_door:
        classes.append("tag-door")
    if seat.near_platform:
        classes.append("tag-platform")
    if seat.near_ac:
        classes.append("tag-ac")
    if "corner" in (t.lower() for t in seat.tags):
        classes.append("tag-corner")
    return " ".join(classes)


def _seat_tooltip(seat: SeatNode, student_name: str, locale: str) -> str:
    parts = [
        translate("seat", locale, seat_id=seat.seat_id)
    ]
    if student_name:
        parts.append(translate("student", locale, name=student_name))
    if not seat.enabled:
        parts.append(translate("disabled", locale))
    tags = []
    if seat.near_window:
        tags.append(translate("near_window", locale))
    if seat.near_door:
        tags.append(translate("near_door", locale))
    if seat.near_platform:
        tags.append(translate("near_platform", locale))
    if seat.near_ac:
        tags.append(translate("near_ac", locale))
    if tags:
        parts.append(translate("tags", locale, tags=", ".join(tags)))
    return html_escape(" | ".join(parts), quote=True)


def _seat_grid_css() -> str:
    return """
.seat-grid {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    background: #f8f9fa;
    border-radius: 8px;
    border: 1px solid #dee2e6;
    overflow-x: auto;
}
.seat-row {
    display: flex;
    gap: 4px;
    justify-content: center;
}
.seat-cell {
    width: 78px;
    min-height: 42px;
    padding: 4px 2px;
    border-radius: 6px;
    border: 1px solid #adb5bd;
    background: #ffffff;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    font-size: 0.75rem;
    transition: transform 0.1s;
}
.seat-cell:hover {
    transform: scale(1.05);
    z-index: 1;
}
.seat-cell:focus-visible {
    outline: 3px solid #0b57d0;
    outline-offset: 1px;
    transform: scale(1.03);
    z-index: 1;
}
.seat-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 70px;
}
.empty-cell {
    background: transparent;
    border: none;
}
.disabled-seat {
    background: #e9ecef;
    border-style: dashed;
    color: #868e96;
}
.highlighted-seat {
    border: 2px solid #228be6;
    background: #e7f5ff;
    box-shadow: 0 0 0 2px rgba(34,139,230,0.25);
}
/* Tag colour accents — left border only to avoid clutter */
.tag-window { border-left: 3px solid #4dabf7; }
.tag-door { border-left: 3px solid #ff922b; }
.tag-platform { border-left: 3px solid #7950f2; }
.tag-ac { border-left: 3px solid #20c997; }
.tag-corner { border-left: 3px solid #f06595; }
"""


def _score_cell(score: float | None) -> str:
    if score is None:
        return "n/a"
    return f"{score:.1f}"
