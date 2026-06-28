"""SeatTrellis Streamlit web UI — v0.4.0.

Privacy-first, local-only.  All business logic lives in ``web/workflow.py``
and ``web/components.py`` so this module stays thin.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

from pydantic import ValidationError

try:
    import streamlit as st
except Exception as exc:  # pragma: no cover
    from seattrellis.optional import MissingOptionalDependencyError

    raise MissingOptionalDependencyError("Streamlit web UI", "web") from exc

from seattrellis.io.json_files import InputFileError
from seattrellis.models.candidate import CandidateSet
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import list_presets
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.web.components import (
    PRIVACY_NOTICE_HTML,
    build_comparison_table,
    build_candidate_selector,
    build_preset_cards,
    build_seat_grid_html,
    diagnose_error,
)
from seattrellis.web.workflow import (
    WebSolveResult,
    assignment_rows,
    candidate_summary_rows,
    demo_paths,
    export_for_web,
    load_demo_layout,
    load_demo_snapshot,
    project_export_for_web,
    project_info_for_web,
    project_solve_for_web,
    project_validate_for_web,
    score_breakdown_rows,
    selected_candidate,
    selected_snapshot,
    solve_for_web,
)

# ---------------------------------------------------------------------------
# Session state init
# ---------------------------------------------------------------------------

_SS_DEFAULTS = {
    "solved": False,
    "result": None,
    "output_dir": None,
    "project_path": None,
    "layout_loaded": None,
    "current_candidate_id": "recommended",
    "demo_loaded": False,
    "demo_students_path": None,
    "demo_layout_path": None,
}


def _ss(key: str):
    """Get-or-create a session-state key."""
    if key not in st.session_state:
        st.session_state[key] = _SS_DEFAULTS.get(key)
    return st.session_state[key]


def _reset_solve_state():
    for k in ("solved", "result", "output_dir", "project_path", "layout_loaded"):
        st.session_state[k] = _SS_DEFAULTS[k]


# ---------------------------------------------------------------------------
# Render helpers
# ---------------------------------------------------------------------------


def _render_privacy_banner() -> None:
    st.markdown(PRIVACY_NOTICE_HTML, unsafe_allow_html=True)


def _render_error(exc: Exception) -> None:
    diag = diagnose_error(exc)
    st.error(f"**{diag['title']}**\n\n{diag['detail']}")


def _render_seat_map(snapshot, layout) -> None:
    """Render the classroom seat map with assignments."""
    if layout is None:
        try:
            layout = load_demo_layout()
        except Exception:
            st.info("上传教室布局 JSON 后可预览座位图。")
            return
    html = build_seat_grid_html(layout, snapshot)
    st.markdown(html, unsafe_allow_html=True)


def _render_candidate_switcher(result: WebSolveResult) -> str | None:
    """Render candidate selector and return the chosen candidate ID."""
    if not result.is_candidate_set:
        return "recommended"

    options = build_candidate_selector(result.artifact)
    labels = [opt["label"] for opt in options]
    ids = [opt["id"] for opt in options]

    current = _ss("current_candidate_id")
    try:
        idx = ids.index(current)
    except ValueError:
        idx = 0

    selected_label = st.selectbox(
        "选择候选方案",
        labels,
        index=idx,
        key="candidate_selector",
    )
    selected_idx = labels.index(selected_label)
    selected_id = ids[selected_idx]
    st.session_state["current_candidate_id"] = selected_id
    return selected_id


def _render_candidate_detail(result: WebSolveResult, candidate_id: str) -> None:
    """Render detailed score breakdown for a single candidate."""
    candidate = selected_candidate(result, candidate_id)
    if candidate is None:
        return

    b = candidate.score.breakdown
    hard = b.hard_constraint_summary

    cols = st.columns(4)
    cols[0].metric("总分", f"{candidate.total_score:.1f}")
    cols[1].metric(
        "Hard Constraints",
        "✅ 通过" if hard.satisfied else f"❌ {hard.violation_count} 违规",
    )
    cols[2].metric("可用维度", str(candidate.score.available_dimensions))
    cols[3].metric("方案ID", candidate.candidate_id)

    if hard.violations:
        st.warning(f"违规项: {hard.violations}")

    st.dataframe(score_breakdown_rows(candidate), use_container_width=True)


def _render_comparison_view(result: WebSolveResult) -> None:
    """Render the multi-candidate comparison table."""
    if not result.is_candidate_set:
        return
    with st.expander("📊 候选方案对比", expanded=False):
        comp = build_comparison_table(result.artifact)
        st.dataframe(comp["rows"], use_container_width=True)
        st.caption(
            "各维度分数为 0–100 归一化值；n/a 表示该维度不可用。"
            "⭐ 标记为推荐方案。"
        )


def _render_preset_cards() -> None:
    """Render expandable preset explanation cards."""
    with st.expander("📋 场景 Preset 说明", expanded=False):
        cards = build_preset_cards()
        cols = st.columns(2)
        for i, card in enumerate(cards):
            with cols[i % 2]:
                st.markdown(
                    f"**{card['name']}**\n"
                    f"{card['description']}\n\n"
                    f"*场景:* {card['scenario']}\n\n"
                    f"*需要:* {card['requires']}\n\n"
                    f"*降级:* {card['degradation']}"
                )
                st.divider()


def _render_file_hints() -> None:
    """Render file format and size hints."""
    with st.expander("📎 文件格式说明", expanded=False):
        st.markdown(
            """
        **支持的文件格式：**

        | 文件 | 格式 | 大小限制 |
        |------|------|----------|
        | 学生名单 | `.csv` / `.xlsx` / `.xlsm` | 建议 < 1 MB |
        | 教室布局 | `.json` | 建议 < 500 KB |
        | 规则 JSON | `.json` | 建议 < 100 KB |
        | 历史快照 | `.json` | 建议 < 1 MB / 文件 |

        **不支持：** `.xls`（旧版 Excel），请先另存为 `.xlsx` 或 CSV。

        **编码：** 所有文本文件请使用 UTF-8 编码。
        """
        )


# ---------------------------------------------------------------------------
# Export section
# ---------------------------------------------------------------------------


def _render_exports(
    result: WebSolveResult,
    output_dir: Path,
    candidate_id: str,
    project_path: Path | None = None,
) -> None:
    """Render download buttons for all export formats."""
    st.subheader("📥 导出")

    # JSON artifact download
    artifact_label = "candidate set JSON" if result.is_candidate_set else "snapshot JSON"
    st.download_button(
        f"下载 {artifact_label}",
        data=result.artifact_path.read_bytes(),
        file_name=result.artifact_path.name,
        mime="application/json",
    )
    if result.report_path is not None:
        st.download_button(
            "下载 plan report JSON",
            data=result.report_path.read_bytes(),
            file_name=result.report_path.name,
            mime="application/json",
        )

    for output_format, mime in [
        ("html", "text/html"),
        ("pdf", "application/pdf"),
        ("png", "image/png"),
        ("excel", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    ]:
        try:
            if project_path is None:
                output_path = export_for_web(
                    result,
                    output_format=output_format,
                    output_dir=output_dir,
                    candidate_id=candidate_id,
                )
            else:
                output_path = project_export_for_web(
                    result,
                    project_path=project_path,
                    output_format=output_format,
                    output_dir=output_dir,
                    candidate_id=candidate_id if result.is_candidate_set else None,
                )
        except MissingOptionalDependencyError as exc:
            st.info(str(exc))
            continue
        st.download_button(
            f"下载 {output_format.upper()}",
            data=output_path.read_bytes(),
            file_name=output_path.name,
            mime=mime,
        )


# ---------------------------------------------------------------------------
# Quick-solve tab
# ---------------------------------------------------------------------------


def _render_quick_solve_tab() -> None:
    _render_privacy_banner()

    # --- Step wizard indicators ---
    step = st.radio(
        "步骤",
        ["1. 加载数据", "2. 设置 & 求解", "3. 查看结果 & 导出"],
        horizontal=True,
        key="quick_step",
    )

    # --- Step 1: Load data ---
    if step.startswith("1"):
        _render_step_load_data()
        return

    # --- Step 2: Solve ---
    if step.startswith("2"):
        _render_step_solve()
        return

    # --- Step 3: Results ---
    if step.startswith("3"):
        _render_step_results()


def _render_step_load_data() -> None:
    st.subheader("📂 加载数据")

    # Demo one-click
    st.markdown("**快速体验**")
    demo_col1, demo_col2 = st.columns([1, 3])
    with demo_col1:
        if st.button("🚀 一键加载 Demo", type="primary", use_container_width=True):
            demo = demo_paths()
            if demo["students_csv"] and demo["layout"]:
                st.session_state["demo_loaded"] = True
                st.session_state["demo_students_path"] = str(demo["students_csv"])
                st.session_state["demo_layout_path"] = str(demo["layout"])
                st.success("Demo 数据已就绪！请切换到下一步。")
            else:
                st.error("Demo 文件不存在。请先在终端运行 `seattrellis init-demo`。")
    with demo_col2:
        st.caption("一键加载虚构示例数据，无需准备任何文件即可体验完整流程。")

    st.divider()
    st.markdown("**或手动上传**")

    _render_file_hints()

    preset_options = [""] + [preset.name for preset in list_presets()]
    students_file = st.file_uploader(
        "学生名单 CSV / Excel",
        type=["csv", "xlsx", "xlsm"],
        key="quick_students",
    )
    layout_file = st.file_uploader(
        "教室布局 JSON",
        type=["json"],
        key="quick_layout",
    )
    preset_name = st.selectbox(
        "内置场景 preset",
        preset_options,
        format_func=lambda v: v or "不使用 preset",
        key="quick_preset",
    )
    _render_preset_cards()
    rules_file = st.file_uploader(
        "规则 JSON（可选，选择 preset 时作为 overlay）",
        type=["json"],
        key="quick_rules",
    )
    history_files = st.file_uploader(
        "历史 snapshot JSON（可选，可多选）",
        type=["json"],
        accept_multiple_files=True,
        key="quick_history",
    )

    # Store files in session for next step.
    if students_file:
        st.session_state["_qf_students"] = students_file
    if layout_file:
        st.session_state["_qf_layout"] = layout_file
    if rules_file:
        st.session_state["_qf_rules"] = rules_file
    if history_files:
        st.session_state["_qf_history"] = history_files
    if preset_name:
        st.session_state["_qf_preset"] = preset_name


def _render_step_solve() -> None:
    st.subheader("⚙️ 求解设置")

    # Check data availability
    demo_loaded = _ss("demo_loaded")
    has_files = bool(_ss("_qf_students") or _ss("_qf_layout") or demo_loaded)
    has_rules = bool(_ss("_qf_rules") or _ss("_qf_preset"))

    if not has_files and not demo_loaded:
        st.warning("请先在上一步上传文件或加载 Demo 数据。")
        return

    # Solve settings
    candidate_count = st.number_input(
        "候选方案数量",
        min_value=1,
        max_value=20,
        value=3,
        step=1,
        key="quick_candidate_count",
    )
    seed_enabled = st.checkbox("自定义 seed", key="quick_seed_enabled")
    seed = st.number_input(
        "seed",
        value=42,
        step=1,
        disabled=not seed_enabled,
        key="quick_seed",
    )
    time_limit_seconds = st.number_input(
        "单次求解秒数",
        min_value=0.5,
        max_value=30.0,
        value=3.0,
        step=0.5,
        key="quick_time_limit",
    )

    ready = has_rules and has_files
    if st.button("生成座位表", type="primary", disabled=not ready):
        _reset_solve_state()
        try:
            with tempfile.TemporaryDirectory() as input_tmpdir, tempfile.TemporaryDirectory() as output_tmpdir:
                input_root = Path(input_tmpdir)

                # Determine data source: demo or uploaded.
                if demo_loaded:
                    students_path = Path(_ss("demo_students_path"))
                    layout_path = Path(_ss("demo_layout_path"))
                else:
                    sf = _ss("_qf_students")
                    lf = _ss("_qf_layout")
                    students_path = input_root / sf.name
                    students_path.write_bytes(sf.getvalue())
                    layout_path = input_root / lf.name
                    layout_path.write_bytes(lf.getvalue())

                rules_file = _ss("_qf_rules")
                rules_path = None
                if rules_file is not None:
                    rules_path = input_root / rules_file.name
                    rules_path.write_bytes(rules_file.getvalue())

                preset_name = _ss("_qf_preset") or None

                history_paths = []
                for i, hf in enumerate(_ss("_qf_history") or [], start=1):
                    hp = input_root / f"history-{i:02d}-{hf.name}"
                    hp.write_bytes(hf.getvalue())
                    history_paths.append(hp)

                result = solve_for_web(
                    students_path=students_path,
                    layout_path=layout_path,
                    rules_path=rules_path,
                    preset_name=preset_name,
                    history_paths=history_paths,
                    output_dir=output_tmpdir,
                    candidate_count=int(candidate_count),
                    seed=int(seed) if seed_enabled else None,
                    time_limit_seconds=float(time_limit_seconds),
                )
                st.session_state["solved"] = True
                st.session_state["result"] = result
                st.session_state["output_dir"] = output_tmpdir

                # Load layout for seat map.
                from seattrellis.io.json_files import load_layout

                st.session_state["layout_loaded"] = load_layout(layout_path)

                st.success("求解完成！请切换到「查看结果 & 导出」步骤。")
        except (
            InputFileError,
            MissingOptionalDependencyError,
            SeatTrellisSolveError,
            ValidationError,
            ValueError,
        ) as exc:
            _render_error(exc)


def _render_step_results() -> None:
    st.subheader("📋 结果")

    result: WebSolveResult | None = _ss("result")
    if result is None:
        st.info('请先在「设置 & 求解」步骤中点击"生成座位表"。')
        return

    output_dir = Path(_ss("output_dir"))
    layout = _ss("layout_loaded")

    # --- Success / warnings ---
    if result.is_candidate_set:
        st.success(
            f"生成 {len(result.artifact.candidates)} 个候选方案，"
            f"推荐 {result.artifact.recommended_candidate_id}"
        )
    else:
        st.success(f"求解完成：{result.artifact.solver_status}")

    if result.warnings:
        st.warning("\n".join(result.warnings))

    # --- Candidate switcher ---
    candidate_id = _render_candidate_switcher(result) or "recommended"

    # --- Seat map ---
    st.subheader("🏫 座位图")
    snapshot = selected_snapshot(result, candidate_id)
    _render_seat_map(snapshot, layout)

    # --- Candidate detail ---
    st.subheader("📊 方案详情")
    _render_candidate_detail(result, candidate_id)

    # --- Comparison view ---
    _render_comparison_view(result)

    # --- Assignment table ---
    with st.expander("📋 分配明细表", expanded=False):
        st.dataframe(assignment_rows(snapshot), use_container_width=True)

    # --- Exports ---
    _render_exports(result, output_dir, candidate_id)


# ---------------------------------------------------------------------------
# Project tab
# ---------------------------------------------------------------------------


def _render_project_tab() -> None:
    _render_privacy_banner()

    st.markdown("**Project 文件**")

    tab_mode = st.radio(
        "选择方式",
        ["输入路径", "上传文件"],
        horizontal=True,
        key="project_mode",
    )

    project_path: Path | None = None

    if tab_mode == "输入路径":
        project_path_text = st.text_input(
            "Project 文件路径",
            value="examples/project.seattrellis.json",
            key="project_path_text",
        )
        if project_path_text:
            project_path = Path(project_path_text).expanduser()
    else:
        uploaded_project = st.file_uploader(
            "上传 Project 文件 (.seattrellis.json)",
            type=["json"],
            key="project_upload",
        )
        if uploaded_project is not None:
            tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".seattrellis.json")
            tmp.write(uploaded_project.getvalue())
            tmp.close()
            project_path = Path(tmp.name)
            st.success(f"已上传: {uploaded_project.name}")

    if project_path is None:
        return

    # --- Info & Validate ---
    info_col, validate_col = st.columns(2)
    with info_col:
        if st.button("读取 project-info", key="proj_info_btn"):
            try:
                st.code(project_info_for_web(project_path=project_path))
            except (InputFileError, ValidationError, ValueError) as exc:
                _render_error(exc)
    with validate_col:
        strict = st.checkbox("严格校验 warnings", key="proj_strict")
        if st.button("校验 project", key="proj_validate_btn"):
            try:
                st.success(
                    project_validate_for_web(project_path=project_path, strict=strict)
                )
            except (InputFileError, ValidationError, ValueError) as exc:
                _render_error(exc)

    # --- Solve ---
    st.subheader("Project 求解")
    use_project_candidates = st.checkbox(
        "使用 project 默认候选数量", value=True, key="proj_use_default"
    )
    project_candidate_count = st.number_input(
        "候选方案数量",
        min_value=1,
        max_value=20,
        value=3,
        step=1,
        disabled=use_project_candidates,
        key="project_candidate_count",
    )
    project_seed_enabled = st.checkbox("自定义 project seed", key="proj_seed_enabled")
    project_seed = st.number_input(
        "project seed",
        value=42,
        step=1,
        disabled=not project_seed_enabled,
        key="proj_seed",
    )
    project_time_limit = st.number_input(
        "project 单次求解秒数",
        min_value=0.5,
        max_value=30.0,
        value=3.0,
        step=0.5,
        key="proj_time_limit",
    )

    if st.button("按 project 求解", type="primary", key="proj_solve_btn"):
        _reset_solve_state()
        try:
            with tempfile.TemporaryDirectory() as output_tmpdir:
                result = project_solve_for_web(
                    project_path=project_path,
                    candidate_count=(
                        None
                        if use_project_candidates
                        else int(project_candidate_count)
                    ),
                    seed=int(project_seed) if project_seed_enabled else None,
                    time_limit_seconds=float(project_time_limit),
                )
                st.session_state["solved"] = True
                st.session_state["result"] = result
                st.session_state["output_dir"] = output_tmpdir
                st.session_state["project_path"] = str(project_path)

                # Load layout from project.
                from seattrellis.io.project import load_project_paths

                _, paths = load_project_paths(
                    project_path, require_inputs=True, require_history=False
                )
                from seattrellis.io.json_files import load_layout

                st.session_state["layout_loaded"] = load_layout(paths.layout_path)

                st.success("求解完成！")
        except (
            InputFileError,
            MissingOptionalDependencyError,
            SeatTrellisSolveError,
            ValidationError,
            ValueError,
        ) as exc:
            _render_error(exc)

    # --- Results (if solved) ---
    result: WebSolveResult | None = _ss("result")
    proj_path_str: str | None = _ss("project_path")
    if result is not None and proj_path_str is not None:
        output_dir = Path(_ss("output_dir"))
        layout = _ss("layout_loaded")

        st.divider()
        st.subheader("📋 Project 结果")

        candidate_id = _render_candidate_switcher(result) or "recommended"
        snapshot = selected_snapshot(result, candidate_id)

        st.subheader("🏫 座位图")
        _render_seat_map(snapshot, layout)

        _render_candidate_detail(result, candidate_id)
        _render_comparison_view(result)

        with st.expander("📋 分配明细表", expanded=False):
            st.dataframe(assignment_rows(snapshot), use_container_width=True)

        _render_exports(result, output_dir, candidate_id, Path(proj_path_str))


# ---------------------------------------------------------------------------
# App entry point
# ---------------------------------------------------------------------------


st.set_page_config(
    page_title="SeatTrellis · 席序",
    page_icon="🏫",
    layout="wide",
)
st.title("🏫 SeatTrellis · 席序")
st.caption(
    "本地处理学生名单、规则和历史座位记录；"
    "不要把真实班级数据提交到公开仓库。"
)

quick_tab, project_tab = st.tabs(["快速排座", "Project workspace"])
with quick_tab:
    _render_quick_solve_tab()
with project_tab:
    _render_project_tab()
