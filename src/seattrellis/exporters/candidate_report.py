from __future__ import annotations

from html import escape
from pathlib import Path

from seattrellis.models.candidate import (
    CandidateSet,
    PlanComparisonEntry,
    PlanComparisonExplanation,
    PlanComparisonReport,
)
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
    "score_delta": ("与推荐差值", "Difference from recommended"),
    "hard": ("硬约束", "Hard constraints"),
    "passed": ("通过", "Passed"),
    "failed": ("未通过", "Failed"),
    "advantages": ("优势", "Advantages"),
    "costs": ("代价", "Trade-offs"),
    "history": ("历史对比", "History comparison"),
    "not_available": ("无", "None"),
    "seat_count": ("座位数", "Assignments"),
    "recommendation_highest_valid_weighted_total": (
        "在满足全部硬约束的方案中选择加权总分最高者；同分时按方案 ID 排序。",
        "Select the highest weighted total among plans that satisfy every hard "
        "constraint; ties are resolved by candidate ID.",
    ),
    "rating_high": ("较高", "high"),
    "rating_medium": ("中等", "medium"),
    "rating_low": ("较低", "low"),
    "rating_not_available": ("不可用", "not available"),
    "best_available": ("相对较好的维度", "Best relative dimension"),
    "checked_violations": (
        "已检查 {checked} 项，违反 {violations} 项",
        "{checked} checked, {violations} violations",
    ),
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


_HISTORY_LABELS: dict[str, tuple[str, str]] = {
    "fair_rotation": ("公平轮换", "Fair rotation"),
    "avoid_recent_neighbors": ("避免重复邻座", "Neighbor repetition"),
}


_HISTORY_VALUES: dict[str, tuple[str, str]] = {
    "improved": ("优于最近历史方案", "Improved from recent history"),
    "similar": ("与最近历史方案接近", "Similar to recent history"),
    "worse": ("低于最近历史方案", "Worse than recent history"),
    "not_available": ("无可比历史", "No comparable history"),
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
    method = _recommendation_method(report, locale)

    warning_html = ""
    warnings = list(
        dict.fromkeys(str(item) for item in [*candidate_set.warnings, *report.warnings])
    )
    if warnings:
        warning_html = (
            f'<section class="card warning"><h2>{escape(_t("warnings", locale))}</h2>'
            f"<p>{escape(_warning_notice(len(warnings), locale))}</p></section>"
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
        _t("score_delta", locale),
        _t("hard", locale),
        *[_dimension_label(key, locale) for key in dimensions],
        _t("advantages", locale),
        _t("costs", locale),
        _t("history", locale),
    ]
    recommended_score = next(
        entry.total_score
        for entry in report.candidates
        if entry.candidate_id == report.recommended_candidate_id
    )
    rows = []
    for entry in report.candidates:
        recommended_badge = (
            f'<span class="recommended">{escape(_t("recommended", locale))}</span>'
            if entry.candidate_id == report.recommended_candidate_id
            else ""
        )
        hard_status = (
            f'<span class="pass">{escape(_t("passed", locale))}</span>'
            if entry.hard_constraints_satisfied
            else f'<span class="fail">{escape(_t("failed", locale))}</span>'
        )
        constraint_summary = _hard_constraint_summary(entry, locale)
        hard = hard_status
        if constraint_summary:
            hard += (
                '<div class="details">'
                + escape(constraint_summary)
                + "</div>"
            )
        score_delta = entry.score_delta_from_recommended
        if score_delta is None:
            score_delta = entry.total_score - recommended_score
        cells = [
            f'<span class="candidate">{escape(entry.candidate_id)}</span>{recommended_badge}',
            score_text(entry.total_score),
            _score_delta_text(score_delta),
            hard,
            *[
                escape(score_text(entry.dimension_scores.get(dimension)))
                for dimension in dimensions
            ],
            _explanations_html(entry, locale, trade_off=False),
            _explanations_html(entry, locale, trade_off=True),
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


def _explanations_html(
    entry: PlanComparisonEntry,
    locale: str,
    *,
    trade_off: bool,
) -> str:
    kind = "trade_off" if trade_off else None
    explanations = [
        explanation
        for explanation in entry.explanations
        if (explanation.kind == "trade_off") == (kind == "trade_off")
    ]
    if explanations:
        return '<div class="details"><ul>' + "".join(
            f"<li>{escape(_explanation_text(explanation, locale))}</li>"
            for explanation in explanations
        ) + "</ul></div>"
    return _list_html(entry.costs if trade_off else entry.advantages, locale)


def _explanation_text(
    explanation: PlanComparisonExplanation,
    locale: str,
) -> str:
    dimension = _dimension_label(explanation.dimension, locale)
    if explanation.kind == "best_available":
        if locale == "en":
            return (
                f'{_t("best_available", locale)}: {dimension} '
                f"({explanation.score:.1f})"
            )
        return (
            f'{_t("best_available", locale)}：{dimension}'
            f"（{explanation.score:.1f}）"
        )
    rating = _t(f"rating_{explanation.rating}", locale)
    if locale != "en":
        return f"{dimension}：{rating}（{explanation.score:.1f}）"
    return f"{dimension}: {rating} ({explanation.score:.1f})"


def _history_html(items: dict[str, str], locale: str) -> str:
    if not items:
        return escape(_t("not_available", locale))
    return '<div class="details"><ul>' + "".join(
        f"<li>{escape(_history_comparison_text(key, value, locale))}</li>"
        for key, value in items.items()
    ) + "</ul></div>"


def _metric(label: str, value: str) -> str:
    return (
        '<div class="metric">'
        f'<div class="label">{escape(label)}</div>'
        f'<div class="value">{escape(value)}</div>'
        "</div>"
    )


def _score_delta_text(value: float) -> str:
    if abs(value) < 0.000_001:
        return "0.0"
    return f"{value:+.1f}"


def _hard_constraint_summary(
    entry: PlanComparisonEntry,
    locale: str,
) -> str:
    checked = entry.hard_constraint_checked_count
    violations = entry.hard_constraint_violation_count
    if checked is None or violations is None:
        return ""
    if locale == "en":
        noun = "violation" if violations == 1 else "violations"
        return f"{checked} checked, {violations} {noun}"
    return _t("checked_violations", locale).format(
        checked=checked,
        violations=violations,
    )


def _history_comparison_text(key: object, value: object, locale: str) -> str:
    label = _localized_pair(_HISTORY_LABELS, key, locale)
    comparison = _localized_pair(_HISTORY_VALUES, value, locale)
    separator = ": " if locale == "en" else "："
    return f"{label}{separator}{comparison}"


def _recommendation_method(report: PlanComparisonReport, locale: str) -> str:
    method_code = report.metadata.get("recommendation_method_code")
    if method_code == "highest_valid_weighted_total":
        return _t("recommendation_highest_valid_weighted_total", locale)
    return str(report.metadata.get("recommendation_method", ""))


def _warning_notice(count: int, locale: str) -> str:
    """Describe attached warnings without copying potentially identifying text."""

    if locale == "en":
        noun = "warning" if count == 1 else "warnings"
        verb = "is" if count == 1 else "are"
        return (
            f"{count} {noun} {verb} attached to this candidate set. "
            "Details are omitted from comparison reports to protect privacy."
        )
    return f"此候选集包含 {count} 条警告；为保护隐私，比较报告不显示警告详情。"


def _dimension_label(key: str, locale: str) -> str:
    if key in _DIMENSION_LABELS:
        pair = _DIMENSION_LABELS[key]
        return pair[1] if locale == "en" else pair[0]
    return key.replace("_", " ")


def _localized_pair(
    values: dict[str, tuple[str, str]],
    key: object,
    locale: str,
) -> str:
    text = str(key)
    pair = values.get(text)
    if pair is None:
        return text.replace("_", " ")
    return pair[1] if locale == "en" else pair[0]


def _t(key: str, locale: str) -> str:
    pair = _TEXT[key]
    return pair[1] if locale == "en" else pair[0]
