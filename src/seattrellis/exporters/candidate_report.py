from __future__ import annotations

from html import escape
from pathlib import Path

from seattrellis.models.candidate import CandidateSet, PlanComparisonReport
from seattrellis.service_types import PageOptions, normalize_export_locale, score_text


_TEXT: dict[str, tuple[str, str]] = {
    "title": ("候选方案比较报告", "Candidate comparison report"),
    "generated": ("生成时间", "Generated"),
    "recommended": ("推荐方案", "Recommended"),
    "candidate_count": ("候选数量", "Candidates"),
    "method": ("推荐规则", "Recommendation method"),
    "warnings": ("警告", "Warnings"),
    "summary": ("摘要", "Summary"),
    "scores": ("评分对比", "Score comparison"),
    "candidate": ("候选方案", "Candidate"),
    "total": ("总分", "Total"),
    "hard": ("硬约束", "Hard constraints"),
    "passed": ("通过", "Passed"),
    "failed": ("未通过", "Failed"),
    "advantages": ("优势", "Advantages"),
    "costs": ("代价", "Trade-offs"),
    "history": ("历史对比", "History comparison"),
    "not_available": ("无", "None"),
    "seat_count": ("座位数", "Assignments"),
}


_DIMENSION_LABELS: dict[str, tuple[str, str]] = {
    "fair_rotation_score": ("公平轮换", "Fair rotation"),
    "avoid_recent_neighbors_score": ("关系回避", "Neighbor avoidance"),
    "score_balance_score": ("成绩搭配", "Score mixing"),
    "height_preference_score": ("身高偏好", "Height preference"),
    "vision_preference_score": ("视力偏好", "Vision preference"),
    "diversity_score": ("多样性", "Diversity"),
    "stability_score": ("稳定性", "Stability"),
}


def export_candidate_report_html(
    candidate_set: CandidateSet,
    report: PlanComparisonReport,
    output: str | Path,
    *,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> Path:
    """Write a full candidate-set comparison report as HTML."""

    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        render_candidate_report_html(
            candidate_set,
            report,
            page=page or PageOptions(),
            locale=locale,
        ),
        encoding="utf-8",
    )
    return path


def render_candidate_report_html(
    candidate_set: CandidateSet,
    report: PlanComparisonReport,
    *,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> str:
    locale = normalize_export_locale(locale)
    page = page or PageOptions()
    recommended = candidate_set.get_candidate(report.recommended_candidate_id)
    dimensions = _dimension_keys(report)
    created_at = candidate_set.created_at or report.created_at
    method = str(report.metadata.get("recommendation_method", ""))

    warning_html = ""
    warnings = [*candidate_set.warnings, *report.warnings]
    if warnings:
        warning_html = (
            f'<section class="card warning"><h2>{escape(_t("warnings", locale))}</h2>'
            "<ul>"
            + "".join(f"<li>{escape(str(warning))}</li>" for warning in warnings)
            + "</ul></section>"
        )

    return f"""<!doctype html>
<html lang="{"en" if locale == "en" else "zh-CN"}">
<head>
  <meta charset="utf-8">
  <title>{escape(_t("title", locale))} — SeatTrellis</title>
  <style>
    @page {{ size: {page.paper_size} {page.orientation}; margin: {page.margin_mm:g}mm; }}
    :root {{ color-scheme: light; }}
    body {{ font-family: "PingFang SC", "Microsoft YaHei", "Noto Sans SC", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 32px; color: #111827; background: #f8fafc; font-size: {13 * page.scale:g}px; }}
    h1 {{ margin: 0 0 8px; font-size: {26 * page.scale:g}px; }}
    h2 {{ margin: 0 0 12px; font-size: {17 * page.scale:g}px; }}
    .muted {{ color: #64748b; }}
    .hero {{ background: linear-gradient(135deg, #eef2ff, #ecfeff); border: 1px solid #dbeafe; border-radius: 18px; padding: 22px; margin-bottom: 18px; }}
    .summary {{ display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; margin-top: 16px; }}
    .metric {{ background: rgba(255,255,255,.82); border: 1px solid #e2e8f0; border-radius: 14px; padding: 14px; }}
    .metric .label {{ color: #64748b; font-size: {11 * page.scale:g}px; }}
    .metric .value {{ font-size: {20 * page.scale:g}px; font-weight: 750; margin-top: 4px; }}
    .card {{ background: #fff; border: 1px solid #e5e7eb; border-radius: 16px; padding: 18px; margin: 16px 0; box-shadow: 0 10px 30px rgba(15,23,42,.05); }}
    .warning {{ border-color: #facc15; background: #fffbeb; }}
    table {{ width: 100%; border-collapse: collapse; background: #fff; }}
    th, td {{ border-bottom: 1px solid #e5e7eb; padding: 9px 10px; text-align: left; vertical-align: top; }}
    th {{ color: #475569; font-size: {11 * page.scale:g}px; background: #f8fafc; }}
    .candidate {{ font-weight: 750; color: #1d4ed8; }}
    .recommended {{ display: inline-block; margin-left: 6px; border-radius: 999px; padding: 2px 8px; background: #dcfce7; color: #166534; font-size: {10 * page.scale:g}px; }}
    .pass {{ color: #15803d; font-weight: 700; }}
    .fail {{ color: #b91c1c; font-weight: 700; }}
    .details {{ color: #475569; font-size: {12 * page.scale:g}px; line-height: 1.55; }}
    .details ul {{ margin: 4px 0 0 18px; padding: 0; }}
    @media print {{ body {{ margin: 0; background: #fff; -webkit-print-color-adjust: exact; print-color-adjust: exact; }} .card, .hero {{ box-shadow: none; break-inside: avoid; }} }}
  </style>
</head>
<body>
  <header class="hero">
    <h1>{escape(_t("title", locale))}</h1>
    <div class="muted">{escape(_t("generated", locale))}: {escape(str(created_at))}</div>
    <div class="summary">
      {_metric(_t("candidate_count", locale), str(report.candidate_count))}
      {_metric(_t("recommended", locale), report.recommended_candidate_id)}
      {_metric(_t("total", locale), score_text(recommended.total_score))}
      {_metric(_t("seat_count", locale), str(len(recommended.snapshot.assignments)))}
    </div>
  </header>
  <section class="card">
    <h2>{escape(_t("summary", locale))}</h2>
    <p class="details"><strong>{escape(_t("method", locale))}:</strong> {escape(method or _t("not_available", locale))}</p>
  </section>
  {warning_html}
  <section class="card">
    <h2>{escape(_t("scores", locale))}</h2>
    {_score_table(report, dimensions, locale)}
  </section>
</body>
</html>"""


def _score_table(
    report: PlanComparisonReport,
    dimensions: list[str],
    locale: str,
) -> str:
    headers = [
        _t("candidate", locale),
        _t("total", locale),
        _t("hard", locale),
        *[_dimension_label(key, locale) for key in dimensions],
        _t("advantages", locale),
        _t("costs", locale),
        _t("history", locale),
    ]
    rows = []
    for entry in report.candidates:
        recommended_badge = (
            f'<span class="recommended">{escape(_t("recommended", locale))}</span>'
            if entry.candidate_id == report.recommended_candidate_id
            else ""
        )
        hard = (
            f'<span class="pass">{escape(_t("passed", locale))}</span>'
            if entry.hard_constraints_satisfied
            else f'<span class="fail">{escape(_t("failed", locale))}</span>'
        )
        cells = [
            f'<span class="candidate">{escape(entry.candidate_id)}</span>{recommended_badge}',
            score_text(entry.total_score),
            hard,
            *[
                escape(score_text(entry.dimension_scores.get(dimension)))
                for dimension in dimensions
            ],
            _list_html(entry.advantages, locale),
            _list_html(entry.costs, locale),
            _history_html(entry.history_comparison, locale),
        ]
        rows.append("<tr>" + "".join(f"<td>{cell}</td>" for cell in cells) + "</tr>")
    return (
        "<table><thead><tr>"
        + "".join(f"<th>{escape(header)}</th>" for header in headers)
        + "</tr></thead><tbody>"
        + "".join(rows)
        + "</tbody></table>"
    )


def _dimension_keys(report: PlanComparisonReport) -> list[str]:
    seen: list[str] = []
    for entry in report.candidates:
        for key in entry.dimension_scores:
            if key not in seen:
                seen.append(key)
    return [key for key in _DIMENSION_LABELS if key in seen] + [
        key for key in seen if key not in _DIMENSION_LABELS
    ]


def _list_html(items: list[str], locale: str) -> str:
    if not items:
        return escape(_t("not_available", locale))
    return '<div class="details"><ul>' + "".join(
        f"<li>{escape(str(item))}</li>" for item in items
    ) + "</ul></div>"


def _history_html(items: dict[str, str], locale: str) -> str:
    if not items:
        return escape(_t("not_available", locale))
    return '<div class="details"><ul>' + "".join(
        f"<li>{escape(str(key))}: {escape(str(value))}</li>"
        for key, value in items.items()
    ) + "</ul></div>"


def _metric(label: str, value: str) -> str:
    return (
        '<div class="metric">'
        f'<div class="label">{escape(label)}</div>'
        f'<div class="value">{escape(value)}</div>'
        "</div>"
    )


def _dimension_label(key: str, locale: str) -> str:
    if key in _DIMENSION_LABELS:
        pair = _DIMENSION_LABELS[key]
        return pair[1] if locale == "en" else pair[0]
    return key.replace("_", " ")


def _t(key: str, locale: str) -> str:
    pair = _TEXT[key]
    return pair[1] if locale == "en" else pair[0]
