"""Streamlit-renderable components for the SeatTrellis web UI.

All business-logic functions here accept plain data and return plain data
or HTML strings — they do not import streamlit, so they remain testable
without the ``web`` extra.
"""

from __future__ import annotations

from typing import Any

from seattrellis.models.candidate import CandidatePlan, CandidateSet
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.snapshot import SeatingSnapshot


def build_seat_grid_html(
    layout: ClassroomLayout,
    snapshot: SeatingSnapshot | None = None,
    highlight_seat_id: str | None = None,
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
    if not layout.seats:
        return "<p><em>No seats defined in layout.</em></p>"

    # Determine grid dimensions from max row/col.
    max_row = max(seat.row for seat in layout.seats)
    max_col = max(seat.col for seat in layout.seats)

    assignment_map: dict[str, str] = {}
    if snapshot is not None:
        for a in snapshot.assignments:
            assignment_map[a.seat_id] = a.student_name or a.student_key

    rows_html: list[str] = []
    for r in range(1, max_row + 1):
        cells: list[str] = []
        for c in range(1, max_col + 1):
            seat = _find_seat(layout.seats, r, c)
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
            label = student_name if student_name else seat.seat_id

            cells.append(
                f'<div class="{seat_class}" title="{_seat_tooltip(seat, student_name)}">'
                f'<span class="seat-label">{label}</span>'
                f"</div>"
            )
        rows_html.append(
            '<div class="seat-row">' + "".join(cells) + "</div>"
        )

    css = _seat_grid_css()
    return (
        f"<style>{css}</style>"
        f'<div class="seat-grid">'
        + "".join(rows_html)
        + "</div>"
    )


def build_candidate_selector(
    candidate_set: CandidateSet,
    current_id: str = "recommended",
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
            "label": f"⭐ 推荐 — {candidate_set.recommended_candidate_id}"
            f" ({rec.total_score:.1f})"
            if rec
            else "⭐ 推荐",
            "is_recommended": True,
        }
    )
    for candidate in sorted(
        candidate_set.candidates,
        key=lambda item: (-item.total_score, item.candidate_id),
    ):
        options.append(
            {
                "id": candidate.candidate_id,
                "label": f"{candidate.candidate_id} — {candidate.total_score:.1f}",
                "is_recommended": candidate.candidate_id
                == candidate_set.recommended_candidate_id,
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


def build_preset_cards() -> list[dict[str, str]]:
    """Return metadata cards for each built-in preset.

    Each card has ``name``, ``description``, ``scenario``, ``requires``,
    and ``degradation`` keys suitable for rendering as expandable cards.
    """
    return [
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


# ---------------------------------------------------------------------------
# Error diagnosis helpers
# ---------------------------------------------------------------------------

def diagnose_error(exc: Exception) -> dict[str, str]:
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
            "title": "数据格式错误",
            "detail": (
                f"输入文件的格式不符合要求。请检查：\n"
                f"1. JSON 文件是否为合法 JSON；\n"
                f"2. 字段名和类型是否与文档一致；\n"
                f"3. 必填字段是否缺失。\n\n"
                f"原始错误：{msg}"
            ),
        }

    if isinstance(exc, InputFileError):
        return {
            "category": "file_error",
            "title": "文件读取失败",
            "detail": (
                f"无法读取输入文件。请确认：\n"
                f"1. 文件路径是否正确；\n"
                f"2. 文件格式是否受支持（CSV / XLSX / JSON）；\n"
                f"3. 文件编码是否为 UTF-8。\n\n"
                f"原始错误：{msg}"
            ),
        }

    if isinstance(exc, SeatTrellisSolveError):
        return {
            "category": "solve_error",
            "title": "求解失败",
            "detail": (
                f"无法生成座位方案。可能原因：\n"
                f"1. **规则冲突**：fixed seats 与 pair rules 矛盾；\n"
                f"2. **座位不足**：启用座位数 < 学生数；\n"
                f"3. **不可行约束**：must-adjacent 与 cannot-adjacent 冲突，"
                f"或 min-distance 要求无法满足。\n\n"
                f"建议：先用 `seattrellis validate` 检查输入。\n\n"
                f"原始错误：{msg}"
            ),
        }

    if isinstance(exc, MissingOptionalDependencyError):
        return {
            "category": "missing_dependency",
            "title": "缺少可选依赖",
            "detail": (
                f"此功能需要额外的 Python 包。\n\n"
                f"{msg}\n\n"
                f"请在终端运行对应的安装命令后重试。"
            ),
        }

    if isinstance(exc, ValueError):
        return {
            "category": "value_error",
            "title": "参数错误",
            "detail": f"输入参数不符合要求。\n\n原始错误：{msg}",
        }

    # Generic fallback.
    return {
        "category": "unknown",
        "title": "未知错误",
        "detail": f"发生未预期的错误：{name}\n\n{msg}",
    }


# ---------------------------------------------------------------------------
# Privacy notice
# ---------------------------------------------------------------------------

PRIVACY_NOTICE_HTML = """
<div style="background:#f0f8f4;border:1px solid #c3e6cb;border-radius:8px;
padding:12px 16px;margin:8px 0;font-size:0.9rem;color:#155724;">
<strong>🔒 隐私提示</strong><br>
SeatTrellis 在您的电脑上本地处理数据，不会上传任何学生信息到云端。
临时文件存放在系统临时目录，关闭浏览器后自动清除。
导出的座位表文件请妥善保管，不要将包含真实学生信息的文件分享到公开平台。
</div>
"""


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _find_seat(seats: list[SeatNode], row: int, col: int) -> SeatNode | None:
    for seat in seats:
        if seat.row == row and seat.col == col:
            return seat
    return None


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


def _seat_tooltip(seat: SeatNode, student_name: str) -> str:
    parts = [f"座位 {seat.seat_id}"]
    if student_name:
        parts.append(f"学生: {student_name}")
    if not seat.enabled:
        parts.append("(禁用)")
    tags = []
    if seat.near_window:
        tags.append("靠窗")
    if seat.near_door:
        tags.append("靠门")
    if seat.near_platform:
        tags.append("讲台侧")
    if seat.near_ac:
        tags.append("空调下")
    if tags:
        parts.append("标签: " + ", ".join(tags))
    return " | ".join(parts)


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
