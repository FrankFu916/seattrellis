"""Print-friendly HTML export templates for SeatTrellis.

Three scenario templates:
- **public** — class notice: names + seats, hide sensitive fields
- **teacher** — internal: rules, warnings, fairness summary
- **report** — explanation: score breakdown + recommendation rationale

Privacy options control what student fields appear.
"""

from __future__ import annotations

from html import escape
from pathlib import Path
from typing import TYPE_CHECKING

from seattrellis.service_types import (
    PageOptions,
    PrivacyOptions,
    normalize_export_locale,
    normalize_export_template,
)

if TYPE_CHECKING:
    from seattrellis.models.candidate import CandidatePlan
    from seattrellis.models.snapshot import SeatingSnapshot
    from seattrellis.models.student import Student


PrintPrivacyOptions = PrivacyOptions

_TEXT: dict[str, tuple[str, str]] = {
    "layout": ("布局", "Layout"),
    "generated_at": ("生成时间", "Generated"),
    "anonymous_student": ("学生 {index:02d}", "Student {index:02d}"),
    "public_badge": (
        "🔒 班级公示版 — 已隐藏敏感字段",
        "🔒 Public notice — sensitive fields hidden",
    ),
    "anonymous_badge": ("🔒 已匿名化处理", "🔒 Names anonymized"),
    "teacher_info": ("📋 教师信息", "📋 Teacher information"),
    "rules_summary": ("规则摘要", "Rules summary"),
    "warnings": ("⚠️ 警告", "⚠️ Warnings"),
    "student_details": ("学生明细", "Student details"),
    "seat": ("座位", "Seat"),
    "name": ("姓名", "Name"),
    "score": ("成绩", "Score"),
    "height": ("身高", "Height"),
    "vision": ("视力需求", "Vision"),
    "special_needs": ("特殊需求", "Special needs"),
    "notes": ("备注", "Notes"),
    "report_title": ("📊 方案解释报告", "📊 Plan explanation"),
    "candidate": ("候选方案", "Candidate"),
    "total_score": ("总分", "Total score"),
    "hard_constraints": ("硬约束", "Hard constraints"),
    "passed": ("✅ 通过", "✅ Passed"),
    "violations": ("❌ {count} 违规", "❌ {count} violations"),
    "violation_items": ("违规项", "Violations"),
    "fair_rotation": ("公平轮换", "Fair rotation"),
    "neighbor_avoidance": ("关系回避", "Neighbor avoidance"),
    "score_mixing": ("成绩搭配", "Score mixing"),
    "height_preference": ("身高偏好", "Height preference"),
    "vision_preference": ("视力偏好", "Vision preference"),
    "diversity": ("多样性", "Diversity"),
    "stability": ("稳定性", "Stability"),
    "recommendation": ("推荐理由", "Recommendation"),
    "reason_fair_rotation": ("公平轮换表现优秀", "Strong fairness rotation"),
    "reason_neighbors": (
        "有效减少重复同桌/邻座",
        "Reduces repeated desk-mates and neighbors",
    ),
    "reason_score_mixing": ("成绩搭配表现优秀", "Strong score mixing"),
    "reason_stability": ("座位变动较小", "Keeps seat changes limited"),
    "reason_overall": ("综合评分最优", "Best overall score"),
}


def _text(key: str, locale: str, **values: object) -> str:
    pair = _TEXT[key]
    template = pair[1] if locale == "en" else pair[0]
    return template.format(**values)


def export_print_html(
    snapshot: "SeatingSnapshot",
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: "CandidatePlan | None" = None,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> Path:
    """Write a print-friendly HTML file.

    Parameters
    ----------
    snapshot:
        The seating snapshot to render.
    output:
        Output file path.
    template:
        One of ``"public"``, ``"teacher"``, ``"report"``.
    privacy:
        Privacy options; defaults to a per-template sensible default.
    candidate:
        Candidate plan for score breakdown (used by ``"report"`` template).
    """
    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)
    template = _validate_template(template)
    locale = normalize_export_locale(locale)
    if privacy is None:
        privacy = _default_privacy(template)
    html = _render_print_html(
        snapshot,
        template=template,
        privacy=privacy,
        candidate=candidate,
        page=page or PageOptions(),
        locale=locale,
    )
    path.write_text(html, encoding="utf-8")
    return path


# ---------------------------------------------------------------------------
# Internal rendering
# ---------------------------------------------------------------------------


def _default_privacy(template: str) -> PrivacyOptions:
    return PrivacyOptions.for_template(template)


def _render_print_html(
    snapshot: "SeatingSnapshot",
    *,
    template: str,
    privacy: PrivacyOptions,
    candidate: "CandidatePlan | None" = None,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> str:
    template = _validate_template(template)
    locale = normalize_export_locale(locale)
    page = page or PageOptions()
    if template == "report" and candidate is None:
        raise ValueError("The report template requires a candidate plan.")

    min_row, max_row, min_col, max_col = _bounds(snapshot)
    seat_by_pos = {(s.row, s.col): s for s in snapshot.layout.seats}
    assign_by_seat = {a.seat_id: a for a in snapshot.assignments}
    display_names = {
        assignment.student_key: (
            _text("anonymous_student", locale, index=index)
            if privacy.anonymize
            else assignment.student_name or assignment.student_key
        )
        for index, assignment in enumerate(snapshot.assignments, start=1)
    }

    rows_html: list[str] = []
    for r in range(min_row, max_row + 1):
        cells: list[str] = []
        for c in range(min_col, max_col + 1):
            seat = seat_by_pos.get((r, c))
            if seat is None:
                cells.append('<td class="empty"></td>')
                continue
            cls = "seat disabled" if not seat.enabled else "seat"
            a = assign_by_seat.get(seat.seat_id)
            name = escape(display_names.get(a.student_key, "")) if a else ""
            label = name if (name and seat.enabled) else escape(seat.seat_id)
            cells.append(
                f'<td class="{cls}">'
                f'<div class="seat-label">{label}</div>'
                f"</td>"
            )
        rows_html.append("<tr>" + "".join(cells) + "</tr>")

    meta_parts = [f"{_text('layout', locale)}: {escape(snapshot.layout.name)}"]
    if snapshot.created_at:
        meta_parts.append(
            f"{_text('generated_at', locale)}: {escape(str(snapshot.created_at))}"
        )
    meta_html = " | ".join(meta_parts)

    extra_html = ""
    if template == "teacher":
        extra_html = _render_teacher_section(snapshot, privacy, locale)
    elif template == "report" and candidate is not None:
        extra_html = _render_report_section(candidate, locale)

    return f"""<!doctype html>
<html lang="{"en" if locale == "en" else "zh-CN"}">
<head>
  <meta charset="utf-8">
  <title>{escape(snapshot.layout.name)} — SeatTrellis</title>
  <style>
    @page {{ size: {page.paper_size} {page.orientation}; margin: {page.margin_mm:g}mm; }}
    body {{ font-family: "PingFang SC", "Microsoft YaHei", "Noto Sans SC", -apple-system, sans-serif; font-size: {13 * page.scale:g}px; color: #111; margin: 0; }}
    h1 {{ font-size: 20px; text-align: center; margin-bottom: 4px; }}
    .meta {{ text-align: center; color: #666; font-size: 11px; margin-bottom: 20px; }}
    .privacy-note {{ text-align: center; color: #999; font-size: 10px; margin-bottom: 16px; }}
    table {{ border-collapse: separate; border-spacing: 6px; margin: 0 auto; }}
    td {{ width: {100 * page.scale:g}px; height: {56 * page.scale:g}px; text-align: center; vertical-align: middle; border-radius: 6px; font-size: {11 * page.scale:g}px; }}
    .seat {{ background: #e8f0fe; border: 1px solid #5b8def; }}
    .disabled {{ background: #f0f0f0; border: 1px dashed #ccc; color: #999; }}
    .empty {{ background: transparent; border: none; }}
    .seat-label {{ font-weight: 600; }}
    .section {{ margin-top: 24px; }}
    .section h2 {{ font-size: 15px; border-bottom: 1px solid #ddd; padding-bottom: 4px; }}
    .score-grid {{ display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px; }}
    .score-card {{ background: #f8f9fa; border-radius: 6px; padding: 8px; text-align: center; }}
    .score-card .dim {{ font-size: 11px; color: #666; }}
    .score-card .val {{ font-size: 18px; font-weight: 700; color: #1d4ed8; }}
    .warning-box {{ background: #fff8e1; border: 1px solid #ffc107; border-radius: 6px; padding: 10px; margin: 8px 0; font-size: 12px; }}
    @media print {{ body {{ -webkit-print-color-adjust: exact; print-color-adjust: exact; }} }}
  </style>
</head>
<body>
  <h1>{escape(snapshot.layout.name)}</h1>
  <div class="meta">{meta_html}</div>
  {_privacy_badge(template, privacy, locale)}
  <table>
    {"".join(rows_html)}
  </table>
  {extra_html}
</body>
</html>"""


def _privacy_badge(
    template: str,
    privacy: PrivacyOptions,
    locale: str = "zh",
) -> str:
    if template == "public":
        return (
            f'<div class="privacy-note">{escape(_text("public_badge", locale))}</div>'
        )
    if privacy.anonymize:
        return (
            f'<div class="privacy-note">{escape(_text("anonymous_badge", locale))}</div>'
        )
    return ""


def _render_teacher_section(
    snapshot: "SeatingSnapshot",
    privacy: PrivacyOptions,
    locale: str = "zh",
) -> str:
    """Render teacher-internal section with rules, warnings, and fairness info."""
    rules_md = snapshot.metadata.get("rules_summary") or snapshot.metadata.get("rules")
    warnings = snapshot.metadata.get("warnings", [])

    # Build student lookup from snapshot.students.
    student_by_key = {s.key: s for s in snapshot.students}

    parts: list[str] = [
        f'<div class="section"><h2>{escape(_text("teacher_info", locale))}</h2>'
    ]

    if rules_md:
        parts.append(
            f"<p><strong>{escape(_text('rules_summary', locale))}:</strong></p>"
        )
        parts.append(f"<pre>{escape(str(rules_md))}</pre>")

    if warnings:
        parts.append(
            f'<div class="warning-box"><strong>'
            f'{escape(_text("warnings", locale))}:</strong><ul>'
        )
        for w in warnings:
            parts.append(f"<li>{escape(str(w))}</li>")
        parts.append("</ul></div>")

    # Student detail table
    detail_headers = [
        header
        for header, _value in _student_detail_fields(None, privacy, locale)
    ]
    parts.append(f"<h3>{escape(_text('student_details', locale))}</h3>")
    parts.append(
        f'<table class="student-table"><tr>'
        f'<th>{escape(_text("seat", locale))}</th>'
        f'<th>{escape(_text("name", locale))}</th>'
    )
    parts.extend(f"<th>{escape(header)}</th>" for header in detail_headers)
    parts.append("</tr>")

    for index, a in enumerate(snapshot.assignments, start=1):
        stu = student_by_key.get(a.student_key)
        display_name = (
            _text("anonymous_student", locale, index=index)
            if privacy.anonymize
            else a.student_name or a.student_key
        )
        parts.append(
            f"<tr><td>{escape(a.seat_id)}</td><td>{escape(display_name)}</td>"
        )
        for _header, value in _student_detail_fields(stu, privacy, locale):
            parts.append(f"<td>{escape(value)}</td>")
        parts.append("</tr>")

    parts.append("</table></div>")
    return "\n".join(parts)


def _render_report_section(candidate: "CandidatePlan", locale: str = "zh") -> str:
    """Render explanation report with score breakdown."""
    b = candidate.score.breakdown
    hard = b.hard_constraint_summary

    parts = [
        f'<div class="section"><h2>{escape(_text("report_title", locale))}</h2>'
    ]
    parts.append(
        f'<p><strong>{escape(_text("candidate", locale))}:</strong> '
        f'{escape(candidate.candidate_id)}</p>'
    )
    parts.append(
        f'<p><strong>{escape(_text("total_score", locale))}:</strong> '
        f'{candidate.total_score:.1f} / 100</p>'
    )
    parts.append(
        f'<p><strong>{escape(_text("hard_constraints", locale))}:</strong> '
        f'{_text("passed", locale) if hard.satisfied else _text("violations", locale, count=hard.violation_count)}'
        f"</p>"
    )

    if hard.violations:
        parts.append('<div class="warning-box">')
        parts.append(
            f"<strong>{escape(_text('violation_items', locale))}:</strong><ul>"
        )
        for v in hard.violations:
            parts.append(f"<li>{escape(str(v))}</li>")
        parts.append("</ul></div>")

    parts.append('<div class="score-grid">')
    for dim_name, dim_score in [
        (_text("fair_rotation", locale), b.fair_rotation_score),
        (_text("neighbor_avoidance", locale), b.avoid_recent_neighbors_score),
        (_text("score_mixing", locale), b.score_balance_score),
        (_text("height_preference", locale), b.height_preference_score),
        (_text("vision_preference", locale), b.vision_preference_score),
        (_text("diversity", locale), b.diversity_score),
        (_text("stability", locale), b.stability_score),
    ]:
        score_str = f"{dim_score.score:.1f}" if dim_score.score is not None else "n/a"
        rating = dim_score.rating or "-"
        parts.append(
            f'<div class="score-card">'
            f'<div class="dim">{dim_name}</div>'
            f'<div class="val">{score_str}</div>'
            f'<div style="font-size:10px;color:#888;">{escape(rating)} (w={dim_score.weight})</div>'
            f'</div>'
        )
    parts.append("</div>")

    parts.append(
        f'<p style="margin-top:12px;"><strong>'
        f'{escape(_text("recommendation", locale))}:</strong> '
        f'{escape(_recommendation_text(candidate, locale))}</p>'
    )
    parts.append("</div>")
    return "\n".join(parts)


def _recommendation_text(candidate: "CandidatePlan", locale: str = "zh") -> str:
    b = candidate.score.breakdown
    reasons = []
    if b.fair_rotation_score.score is not None and b.fair_rotation_score.score > 70:
        reasons.append(_text("reason_fair_rotation", locale))
    if b.avoid_recent_neighbors_score.score is not None and b.avoid_recent_neighbors_score.score > 70:
        reasons.append(_text("reason_neighbors", locale))
    if b.score_balance_score.score is not None and b.score_balance_score.score > 70:
        reasons.append(_text("reason_score_mixing", locale))
    if b.stability_score.score is not None and b.stability_score.score > 70:
        reasons.append(_text("reason_stability", locale))
    if not reasons:
        reasons.append(_text("reason_overall", locale))
    return ("; " if locale == "en" else "；").join(reasons)


def _bounds(snapshot: "SeatingSnapshot") -> tuple[int, int, int, int]:
    rows = [s.row for s in snapshot.layout.seats]
    cols = [s.col for s in snapshot.layout.seats]
    return min(rows), max(rows), min(cols), max(cols)


def _validate_template(template: str) -> str:
    try:
        return normalize_export_template(template)
    except ValueError as exc:
        raise ValueError(str(exc).replace("export template", "print template")) from exc


def _student_detail_fields(
    student: "Student | None",
    privacy: PrivacyOptions,
    locale: str = "zh",
) -> list[tuple[str, str]]:
    fields: list[tuple[str, str]] = []
    if not privacy.hide_scores:
        fields.append(
            (
                _text("score", locale),
                str(student.score) if student and student.score is not None else "-",
            )
        )
    if privacy.show_height:
        fields.append(
            (
                _text("height", locale),
                str(student.height_cm)
                if student and student.height_cm is not None
                else "-",
            )
        )
    if privacy.show_vision:
        fields.append(
            (
                _text("vision", locale),
                str(student.vision) if student and student.vision is not None else "-",
            )
        )
    if not privacy.hide_special_needs:
        needs = list(student.needs) + list(student.tags) if student else []
        separator = ", " if locale == "en" else "、"
        fields.append(
            (_text("special_needs", locale), separator.join(needs) if needs else "-")
        )
    if not privacy.hide_notes:
        fields.append(
            (_text("notes", locale), student.notes if student and student.notes else "-")
        )
    return fields
