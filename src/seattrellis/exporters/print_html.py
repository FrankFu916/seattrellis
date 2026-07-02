"""Print-friendly HTML export templates for SeatTrellis.

Three scenario templates:
- **public** — class notice: names + seats, hide sensitive fields
- **teacher** — internal: rules, warnings, fairness summary
- **report** — explanation: score breakdown + recommendation rationale

Privacy options control what student fields appear.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from html import escape
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from seattrellis.models.candidate import CandidatePlan
    from seattrellis.models.snapshot import SeatingSnapshot
    from seattrellis.models.student import Student


PRINT_TEMPLATES = ("public", "teacher", "report")


@dataclass
class PrintPrivacyOptions:
    """Control which student fields appear in exported output."""

    hide_scores: bool = False
    hide_notes: bool = True
    hide_special_needs: bool = True
    anonymize: bool = False
    show_height: bool = True
    show_vision: bool = True


def export_print_html(
    snapshot: "SeatingSnapshot",
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrintPrivacyOptions | None = None,
    candidate: "CandidatePlan | None" = None,
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
    if privacy is None:
        privacy = _default_privacy(template)
    html = _render_print_html(snapshot, template=template, privacy=privacy, candidate=candidate)
    path.write_text(html, encoding="utf-8")
    return path


# ---------------------------------------------------------------------------
# Internal rendering
# ---------------------------------------------------------------------------


def _default_privacy(template: str) -> PrintPrivacyOptions:
    template = _validate_template(template)
    if template == "public":
        return PrintPrivacyOptions(
            hide_scores=True,
            hide_notes=True,
            hide_special_needs=True,
            anonymize=False,
            show_height=False,
            show_vision=False,
        )
    if template == "teacher":
        return PrintPrivacyOptions(
            hide_scores=False, hide_notes=False, hide_special_needs=False, anonymize=False
        )
    # report
    return PrintPrivacyOptions(
        hide_scores=False,
        hide_notes=True,
        hide_special_needs=True,
        anonymize=False,
        show_height=False,
        show_vision=False,
    )


def _render_print_html(
    snapshot: "SeatingSnapshot",
    *,
    template: str,
    privacy: PrintPrivacyOptions,
    candidate: "CandidatePlan | None" = None,
) -> str:
    template = _validate_template(template)
    if template == "report" and candidate is None:
        raise ValueError("The report template requires a candidate plan.")

    min_row, max_row, min_col, max_col = _bounds(snapshot)
    seat_by_pos = {(s.row, s.col): s for s in snapshot.layout.seats}
    assign_by_seat = {a.seat_id: a for a in snapshot.assignments}
    display_names = {
        assignment.student_key: (
            f"学生 {index:02d}"
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

    meta_parts = [f"布局: {escape(snapshot.layout.name)}"]
    if snapshot.created_at:
        meta_parts.append(f"生成时间: {escape(str(snapshot.created_at))}")
    meta_html = " | ".join(meta_parts)

    extra_html = ""
    if template == "teacher":
        extra_html = _render_teacher_section(snapshot, privacy)
    elif template == "report" and candidate is not None:
        extra_html = _render_report_section(candidate)

    return f"""<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <title>{escape(snapshot.layout.name)} — SeatTrellis</title>
  <style>
    @page {{ size: A4 portrait; margin: 15mm; }}
    body {{ font-family: "PingFang SC", "Microsoft YaHei", "Noto Sans SC", -apple-system, sans-serif; font-size: 13px; color: #111; margin: 0; }}
    h1 {{ font-size: 20px; text-align: center; margin-bottom: 4px; }}
    .meta {{ text-align: center; color: #666; font-size: 11px; margin-bottom: 20px; }}
    .privacy-note {{ text-align: center; color: #999; font-size: 10px; margin-bottom: 16px; }}
    table {{ border-collapse: separate; border-spacing: 6px; margin: 0 auto; }}
    td {{ width: 100px; height: 56px; text-align: center; vertical-align: middle; border-radius: 6px; font-size: 11px; }}
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
  {_privacy_badge(template, privacy)}
  <table>
    {"".join(rows_html)}
  </table>
  {extra_html}
</body>
</html>"""


def _privacy_badge(template: str, privacy: PrintPrivacyOptions) -> str:
    if template == "public":
        return '<div class="privacy-note">🔒 班级公示版 — 已隐藏敏感字段</div>'
    if privacy.anonymize:
        return '<div class="privacy-note">🔒 已匿名化处理</div>'
    return ""


def _render_teacher_section(snapshot: "SeatingSnapshot", privacy: PrintPrivacyOptions) -> str:
    """Render teacher-internal section with rules, warnings, and fairness info."""
    rules_md = snapshot.metadata.get("rules_summary") or snapshot.metadata.get("rules")
    warnings = snapshot.metadata.get("warnings", [])

    # Build student lookup from snapshot.students.
    student_by_key = {s.key: s for s in snapshot.students}

    parts: list[str] = ['<div class="section"><h2>📋 教师信息</h2>']

    if rules_md:
        parts.append("<p><strong>规则摘要:</strong></p>")
        parts.append(f"<pre>{escape(str(rules_md))}</pre>")

    if warnings:
        parts.append('<div class="warning-box"><strong>⚠️ 警告:</strong><ul>')
        for w in warnings:
            parts.append(f"<li>{escape(str(w))}</li>")
        parts.append("</ul></div>")

    # Student detail table
    detail_headers = [
        header for header, _value in _student_detail_fields(None, privacy)
    ]
    parts.append("<h3>学生明细</h3>")
    parts.append('<table class="student-table"><tr><th>座位</th><th>姓名</th>')
    parts.extend(f"<th>{escape(header)}</th>" for header in detail_headers)
    parts.append("</tr>")

    for index, a in enumerate(snapshot.assignments, start=1):
        stu = student_by_key.get(a.student_key)
        display_name = (
            f"学生 {index:02d}"
            if privacy.anonymize
            else a.student_name or a.student_key
        )
        parts.append(
            f"<tr><td>{escape(a.seat_id)}</td><td>{escape(display_name)}</td>"
        )
        for _header, value in _student_detail_fields(stu, privacy):
            parts.append(f"<td>{escape(value)}</td>")
        parts.append("</tr>")

    parts.append("</table></div>")
    return "\n".join(parts)


def _render_report_section(candidate: "CandidatePlan") -> str:
    """Render explanation report with score breakdown."""
    b = candidate.score.breakdown
    hard = b.hard_constraint_summary

    parts = [f'<div class="section"><h2>📊 方案解释报告</h2>']
    parts.append(f'<p><strong>候选方案:</strong> {escape(candidate.candidate_id)}</p>')
    parts.append(f'<p><strong>总分:</strong> {candidate.total_score:.1f} / 100</p>')
    parts.append(
        f'<p><strong>Hard Constraints:</strong> '
        f'{"✅ 通过" if hard.satisfied else f"❌ {hard.violation_count} 违规"}'
        f"</p>"
    )

    if hard.violations:
        parts.append('<div class="warning-box">')
        parts.append("<strong>违规项:</strong><ul>")
        for v in hard.violations:
            parts.append(f"<li>{escape(str(v))}</li>")
        parts.append("</ul></div>")

    parts.append('<div class="score-grid">')
    for dim_name, dim_score in [
        ("公平轮换", b.fair_rotation_score),
        ("关系回避", b.avoid_recent_neighbors_score),
        ("成绩均衡", b.score_balance_score),
        ("身高偏好", b.height_preference_score),
        ("视力偏好", b.vision_preference_score),
        ("多样性", b.diversity_score),
        ("稳定性", b.stability_score),
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

    parts.append(f'<p style="margin-top:12px;"><strong>推荐理由:</strong> {escape(_recommendation_text(candidate))}</p>')
    parts.append("</div>")
    return "\n".join(parts)


def _recommendation_text(candidate: "CandidatePlan") -> str:
    b = candidate.score.breakdown
    reasons = []
    if b.fair_rotation_score.score is not None and b.fair_rotation_score.score > 70:
        reasons.append("公平轮换表现优秀")
    if b.avoid_recent_neighbors_score.score is not None and b.avoid_recent_neighbors_score.score > 70:
        reasons.append("有效减少重复同桌/邻座")
    if b.score_balance_score.score is not None and b.score_balance_score.score > 70:
        reasons.append("成绩分布均衡")
    if b.stability_score.score is not None and b.stability_score.score > 70:
        reasons.append("座位变动较小")
    if not reasons:
        reasons.append("综合评分最优")
    return "；".join(reasons)


def _bounds(snapshot: "SeatingSnapshot") -> tuple[int, int, int, int]:
    rows = [s.row for s in snapshot.layout.seats]
    cols = [s.col for s in snapshot.layout.seats]
    return min(rows), max(rows), min(cols), max(cols)


def _validate_template(template: str) -> str:
    normalized = str(template).strip().lower()
    if normalized not in PRINT_TEMPLATES:
        supported = ", ".join(PRINT_TEMPLATES)
        raise ValueError(
            f"Unsupported print template {template!r}. Supported templates: {supported}."
        )
    return normalized


def _student_detail_fields(
    student: "Student | None",
    privacy: PrintPrivacyOptions,
) -> list[tuple[str, str]]:
    fields: list[tuple[str, str]] = []
    if not privacy.hide_scores:
        fields.append(
            ("成绩", str(student.score) if student and student.score is not None else "-")
        )
    if privacy.show_height:
        fields.append(
            (
                "身高",
                str(student.height_cm)
                if student and student.height_cm is not None
                else "-",
            )
        )
    if privacy.show_vision:
        fields.append(
            (
                "视力需求",
                str(student.vision) if student and student.vision is not None else "-",
            )
        )
    if not privacy.hide_special_needs:
        needs = list(student.needs) + list(student.tags) if student else []
        fields.append(("特殊需求", "、".join(needs) if needs else "-"))
    if not privacy.hide_notes:
        fields.append(("备注", student.notes if student and student.notes else "-"))
    return fields
