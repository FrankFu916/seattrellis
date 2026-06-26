from __future__ import annotations

import tempfile
from pathlib import Path

from pydantic import ValidationError

try:
    import streamlit as st
except Exception as exc:  # pragma: no cover - depends on optional extra.
    from seattrellis.optional import MissingOptionalDependencyError

    raise MissingOptionalDependencyError("Streamlit web UI", "web") from exc

from seattrellis.io.json_files import InputFileError
from seattrellis.models.candidate import CandidateSet
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import list_presets
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.web.workflow import (
    assignment_rows,
    candidate_summary_rows,
    export_for_web,
    project_export_for_web,
    project_info_for_web,
    project_solve_for_web,
    project_validate_for_web,
    score_breakdown_rows,
    selected_candidate,
    selected_snapshot,
    solve_for_web,
)


def _render_result(result, output_dir: Path, *, project_path: Path | None = None) -> None:
    if isinstance(result.artifact, CandidateSet):
        st.success(
            f"生成 {len(result.artifact.candidates)} 个候选方案，"
            f"推荐 {result.artifact.recommended_candidate_id}"
        )
        if result.warnings:
            st.warning("\n".join(result.warnings))
        st.subheader("候选方案")
        st.dataframe(candidate_summary_rows(result.artifact), use_container_width=True)
        selected_id = "recommended"
        candidate = selected_candidate(result, selected_id)
        if candidate is not None:
            st.subheader("推荐方案解释")
            hard_summary = candidate.score.breakdown.hard_constraint_summary
            st.metric("总分", f"{candidate.total_score:.1f}")
            st.write(
                {
                    "hard_constraints_satisfied": hard_summary.satisfied,
                    "checked_rule_count": hard_summary.checked_rule_count,
                    "violation_count": hard_summary.violation_count,
                    "violations": hard_summary.violations,
                }
            )
            st.dataframe(score_breakdown_rows(candidate), use_container_width=True)
        with st.expander("查看所有候选方案评分"):
            for plan in result.artifact.candidates:
                st.markdown(f"**{plan.candidate_id}** · total {plan.total_score:.1f}")
                st.dataframe(score_breakdown_rows(plan), use_container_width=True)
    else:
        st.success(f"求解完成：{result.artifact.solver_status}")
        selected_id = "recommended"

    snapshot = selected_snapshot(result, selected_id)
    st.subheader("座位表")
    st.dataframe(assignment_rows(snapshot), use_container_width=True)

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
        ("png", "image/png"),
        ("excel", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ]:
        try:
            if project_path is None:
                output_path = export_for_web(
                    result,
                    output_format=output_format,
                    output_dir=output_dir,
                    candidate_id=selected_id,
                )
            else:
                output_path = project_export_for_web(
                    result,
                    project_path=project_path,
                    output_format=output_format,
                    output_dir=output_dir,
                    candidate_id=selected_id if result.is_candidate_set else None,
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


def _render_quick_solve_tab() -> None:
    preset_options = [""] + [preset.name for preset in list_presets()]
    students_file = st.file_uploader("学生名单 CSV / Excel", type=["csv", "xlsx", "xlsm"])
    layout_file = st.file_uploader("教室布局 JSON", type=["json"])
    preset_name = st.selectbox(
        "内置场景 preset",
        preset_options,
        format_func=lambda value: value or "不使用 preset",
    )
    rules_file = st.file_uploader("规则 JSON（可选，选择 preset 时作为 overlay）", type=["json"])
    history_files = st.file_uploader(
        "历史 snapshot JSON（可选，可多选）",
        type=["json"],
        accept_multiple_files=True,
    )

    st.subheader("求解设置")
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

    has_rules = bool(rules_file or preset_name)
    ready = bool(students_file and layout_file and has_rules)

    if st.button("生成座位表", type="primary", disabled=not ready):
        try:
            with tempfile.TemporaryDirectory() as input_tmpdir, tempfile.TemporaryDirectory() as output_tmpdir:
                input_root = Path(input_tmpdir)
                students_path = input_root / students_file.name
                students_path.write_bytes(students_file.getvalue())
                layout_path = input_root / layout_file.name
                layout_path.write_bytes(layout_file.getvalue())
                rules_path = None
                if rules_file is not None:
                    rules_path = input_root / rules_file.name
                    rules_path.write_bytes(rules_file.getvalue())
                history_paths = []
                for index, history_file in enumerate(history_files or [], start=1):
                    history_path = input_root / f"history-{index:02d}-{history_file.name}"
                    history_path.write_bytes(history_file.getvalue())
                    history_paths.append(history_path)

                result = solve_for_web(
                    students_path=students_path,
                    layout_path=layout_path,
                    rules_path=rules_path,
                    preset_name=preset_name or None,
                    history_paths=history_paths,
                    output_dir=output_tmpdir,
                    candidate_count=int(candidate_count),
                    seed=int(seed) if seed_enabled else None,
                    time_limit_seconds=float(time_limit_seconds),
                )
                _render_result(result, Path(output_tmpdir))
        except (
            InputFileError,
            MissingOptionalDependencyError,
            SeatTrellisSolveError,
            ValidationError,
            ValueError,
        ) as exc:
            st.error(str(exc))
    elif not ready:
        st.info("请上传学生名单、教室布局，并选择 preset 或上传规则 JSON。")


def _render_project_tab() -> None:
    project_path_text = st.text_input("Project 文件路径", value="examples/project.seattrellis.json")
    project_path = Path(project_path_text).expanduser()

    info_col, validate_col = st.columns(2)
    with info_col:
        if st.button("读取 project-info"):
            try:
                st.code(project_info_for_web(project_path=project_path))
            except (InputFileError, ValidationError, ValueError) as exc:
                st.error(str(exc))
    with validate_col:
        strict = st.checkbox("严格校验 warnings")
        if st.button("校验 project"):
            try:
                st.success(project_validate_for_web(project_path=project_path, strict=strict))
            except (InputFileError, ValidationError, ValueError) as exc:
                st.error(str(exc))

    st.subheader("Project 求解")
    use_project_candidates = st.checkbox("使用 project 默认候选数量", value=True)
    project_candidate_count = st.number_input(
        "候选方案数量",
        min_value=1,
        max_value=20,
        value=3,
        step=1,
        disabled=use_project_candidates,
        key="project_candidate_count",
    )
    project_seed_enabled = st.checkbox("自定义 project seed")
    project_seed = st.number_input(
        "project seed",
        value=42,
        step=1,
        disabled=not project_seed_enabled,
    )
    project_time_limit = st.number_input(
        "project 单次求解秒数",
        min_value=0.5,
        max_value=30.0,
        value=3.0,
        step=0.5,
    )

    if st.button("按 project 求解", type="primary"):
        try:
            with tempfile.TemporaryDirectory() as output_tmpdir:
                result = project_solve_for_web(
                    project_path=project_path,
                    candidate_count=(
                        None if use_project_candidates else int(project_candidate_count)
                    ),
                    seed=int(project_seed) if project_seed_enabled else None,
                    time_limit_seconds=float(project_time_limit),
                )
                _render_result(result, Path(output_tmpdir), project_path=project_path)
        except (
            InputFileError,
            MissingOptionalDependencyError,
            SeatTrellisSolveError,
            ValidationError,
            ValueError,
        ) as exc:
            st.error(str(exc))


st.set_page_config(page_title="SeatTrellis", layout="wide")
st.title("SeatTrellis")
st.caption("本地处理学生名单、规则和历史座位记录；不要把真实班级数据提交到公开仓库。")

quick_tab, project_tab = st.tabs(["快速排座", "Project workspace"])
with quick_tab:
    _render_quick_solve_tab()
with project_tab:
    _render_project_tab()
