"""SeatTrellis Streamlit web UI — v0.4.0.

Privacy-first, local-only.  All business logic lives in ``web/workflow.py``
and ``web/components.py`` so this module stays thin.
"""

from __future__ import annotations

import atexit
import hashlib
import json
import shutil
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
from seattrellis.web.config import (
    WebSessionConfig,
    dump_web_config,
    load_web_config,
)
from seattrellis.web.components import (
    accessibility_styles,
    build_comparison_table,
    build_candidate_selector,
    build_preset_cards,
    build_privacy_notice_html,
    build_seat_grid_html,
    diagnose_error,
)
from seattrellis.web.i18n import (
    LANGUAGE_OPTIONS,
    normalize_locale,
    table_column_labels,
    translate,
)
from seattrellis.web.workflow import (
    WebSolveResult,
    analyze_history_files,
    assignment_rows,
    build_rules_preview,
    candidate_summary_rows,
    demo_paths,
    export_for_web,
    load_demo_layout,
    load_demo_snapshot,
    project_export_for_web,
    project_info_for_web,
    project_solve_for_web,
    project_validate_for_web,
    parse_rules_overlay,
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
    "artifact_json": None,
    "report_json": None,
    "output_dir": None,
    "project_path": None,
    "layout_loaded": None,
    "current_candidate_id": "recommended",
    "demo_loaded": False,
    "demo_students_path": None,
    "demo_layout_path": None,
    "demo_history_dir": None,
    "_qf_rules_data": None,
    "_qf_rules_name": None,
    "_qf_config_digest": None,
    "_qf_history_quality": None,
    "ui_locale": "zh",
    "quick_step_value": "load",
    "project_mode_value": "path",
}

# Persistent temp dirs that survive Streamlit re-runs.
_PERSISTENT_DIRS: list[str] = []


def _make_persistent_tempdir() -> str:
    """Create a temp directory that persists across Streamlit re-runs.

    Registered directories are cleaned up on process exit via ``atexit``.
    """
    d = tempfile.mkdtemp(prefix="seattrellis_")
    _PERSISTENT_DIRS.append(d)
    return d


@atexit.register
def _cleanup_persistent_dirs() -> None:
    for d in _PERSISTENT_DIRS:
        shutil.rmtree(d, ignore_errors=True)
    _PERSISTENT_DIRS.clear()


def _ss(key: str):
    """Get-or-create a session-state key."""
    if key not in st.session_state:
        st.session_state[key] = _SS_DEFAULTS.get(key)
    return st.session_state[key]


def _locale() -> str:
    return normalize_locale(_ss("ui_locale"))


def _t(key: str, **values: object) -> str:
    return translate(key, _locale(), **values)


def _localized_columns(rows: list[dict[str, object]]) -> dict[str, str]:
    if not rows:
        return {}
    labels = table_column_labels(_locale())
    return {
        key: labels[key]
        for key in rows[0]
        if key in labels
    }


def _history_warnings(report) -> list[str]:
    messages: list[str] = []
    for snapshot in report.snapshots:
        if snapshot.missing_students:
            messages.append(
                _t(
                    "history_missing_students",
                    snapshot=snapshot.snapshot,
                    count=len(snapshot.missing_students),
                )
            )
        if snapshot.unknown_students:
            messages.append(
                _t(
                    "history_unknown_students",
                    snapshot=snapshot.snapshot,
                    count=len(snapshot.unknown_students),
                )
            )
        if snapshot.unknown_seats:
            messages.append(
                _t(
                    "history_unknown_seats",
                    snapshot=snapshot.snapshot,
                    seats=", ".join(snapshot.unknown_seats),
                )
            )
        if snapshot.disabled_seats:
            messages.append(
                _t(
                    "history_disabled_seats",
                    snapshot=snapshot.snapshot,
                    seats=", ".join(snapshot.disabled_seats),
                )
            )
        if not snapshot.layout_matches:
            messages.append(
                _t(
                    "history_layout_differs",
                    snapshot=snapshot.snapshot,
                )
            )
    return messages


def _reset_solve_state():
    for k in ("solved", "result", "artifact_json", "report_json",
              "output_dir", "project_path", "layout_loaded"):
        st.session_state[k] = _SS_DEFAULTS[k]


def _restore_web_config(data: bytes) -> WebSessionConfig:
    digest = hashlib.sha256(data).hexdigest()
    config = load_web_config(data)
    if config.rules_overlay is not None or config.preset_name is not None:
        build_rules_preview(
            rules_data=config.rules_overlay,
            preset_name=config.preset_name,
        )
    if _ss("_qf_config_digest") == digest:
        return config

    preset_name = config.preset_name or ""
    st.session_state[f"quick_preset_{_locale()}"] = preset_name
    if preset_name:
        st.session_state["_qf_preset"] = preset_name
    else:
        st.session_state.pop("_qf_preset", None)
    st.session_state["_qf_rules_data"] = config.rules_overlay
    st.session_state["_qf_rules_name"] = (
        "restored.rules.json" if config.rules_overlay is not None else None
    )
    st.session_state["quick_candidate_count"] = config.candidate_count
    st.session_state["quick_seed_enabled"] = config.seed is not None
    st.session_state["quick_seed"] = config.seed if config.seed is not None else 42
    st.session_state["quick_time_limit"] = config.time_limit_seconds
    st.session_state["_qf_config_digest"] = digest
    st.session_state["_qf_history_quality"] = None
    return config


def _current_rules_data() -> dict | None:
    uploaded = _ss("_qf_rules")
    if uploaded is not None:
        return parse_rules_overlay(uploaded.getvalue())
    restored = _ss("_qf_rules_data")
    return restored if isinstance(restored, dict) else None


def _materialize_quick_inputs() -> tuple[Path, Path, Path | None, list[Path]]:
    input_root = Path(_make_persistent_tempdir())
    students_file = _ss("_qf_students")
    layout_file = _ss("_qf_layout")
    demo_loaded = bool(_ss("demo_loaded"))

    if students_file is not None and layout_file is not None:
        students_path = input_root / Path(students_file.name).name
        layout_path = input_root / Path(layout_file.name).name
        students_path.write_bytes(students_file.getvalue())
        layout_path.write_bytes(layout_file.getvalue())
    elif demo_loaded:
        students_path = Path(_ss("demo_students_path"))
        layout_path = Path(_ss("demo_layout_path"))
    else:
        raise InputFileError(
            "Upload both the student list and classroom layout, or load the Demo."
        )

    rules_path: Path | None = None
    rules_file = _ss("_qf_rules")
    if rules_file is not None:
        rules_path = input_root / Path(rules_file.name).name
        rules_path.write_bytes(rules_file.getvalue())
    else:
        restored_rules = _current_rules_data()
        if restored_rules is not None:
            rules_path = input_root / "restored.rules.json"
            rules_path.write_text(
                json.dumps(restored_rules, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )

    history_paths: list[Path] = []
    for index, history_file in enumerate(_ss("_qf_history") or [], start=1):
        safe_name = Path(history_file.name).name
        history_path = input_root / f"history-{index:02d}-{safe_name}"
        history_path.write_bytes(history_file.getvalue())
        history_paths.append(history_path)
    if not history_paths and demo_loaded and _ss("demo_history_dir"):
        history_paths = sorted(
            Path(_ss("demo_history_dir")).glob("*.snapshot.json")
        )

    return students_path, layout_path, rules_path, history_paths


# ---------------------------------------------------------------------------
# Render helpers
# ---------------------------------------------------------------------------


def _render_privacy_banner() -> None:
    st.markdown(build_privacy_notice_html(_locale()), unsafe_allow_html=True)


def _render_error(exc: Exception) -> None:
    diag = diagnose_error(exc, _locale())
    st.error(f"**{diag['title']}**\n\n{diag['detail']}")


def _render_seat_map(snapshot, layout) -> None:
    """Render the classroom seat map with assignments."""
    if layout is None:
        try:
            layout = load_demo_layout()
        except Exception:
            st.info(_t("seat_map_unavailable"))
            return
    html = build_seat_grid_html(layout, snapshot, locale=_locale())
    st.markdown(html, unsafe_allow_html=True)


def _render_candidate_switcher(result: WebSolveResult, widget_key: str = "candidate_selector") -> str | None:
    """Render candidate selector and return the chosen candidate ID."""
    if not result.is_candidate_set:
        return "recommended"

    options = build_candidate_selector(result.artifact, locale=_locale())
    labels = [opt["label"] for opt in options]
    ids = [opt["id"] for opt in options]

    current = _ss("current_candidate_id")
    try:
        idx = ids.index(current)
    except ValueError:
        idx = 0

    selected_label = st.selectbox(
        _t("candidate_choice"),
        labels,
        index=idx,
        key=f"{widget_key}_{_locale()}",
    )
    try:
        selected_idx = labels.index(selected_label)
    except ValueError:
        selected_idx = 0
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
    cols[0].metric(_t("total_score"), f"{candidate.total_score:.1f}")
    cols[1].metric(
        _t("hard_constraints"),
        _t("passed")
        if hard.satisfied
        else _t("violations", count=hard.violation_count),
    )
    cols[2].metric(
        _t("available_dimensions"),
        str(candidate.score.available_dimensions),
    )
    cols[3].metric(_t("candidate_id"), candidate.candidate_id)

    if hard.violations:
        st.warning(_t("violation_items", items=hard.violations))

    rows = score_breakdown_rows(candidate)
    st.dataframe(
        rows,
        width="stretch",
        column_config=_localized_columns(rows),
    )


def _render_comparison_view(result: WebSolveResult) -> None:
    """Render the multi-candidate comparison table."""
    if not result.is_candidate_set:
        return
    with st.expander(_t("candidate_comparison"), expanded=False):
        comp = build_comparison_table(result.artifact)
        st.dataframe(
            comp["rows"],
            width="stretch",
            column_config=_localized_columns(comp["rows"]),
        )
        st.caption(_t("comparison_caption"))


def _render_preset_cards() -> None:
    """Render expandable preset explanation cards."""
    with st.expander(_t("preset_help"), expanded=False):
        cards = build_preset_cards(_locale())
        cols = st.columns(2)
        for i, card in enumerate(cards):
            with cols[i % 2]:
                st.markdown(
                    f"**{card['name']}**\n"
                    f"{card['description']}\n\n"
                    f"*{_t('scenario')}:* {card['scenario']}\n\n"
                    f"*{_t('requires')}:* {card['requires']}\n\n"
                    f"*{_t('degradation')}:* {card['degradation']}"
                )
                st.divider()


def _render_file_hints() -> None:
    """Render file format and size hints."""
    with st.expander(_t("file_help"), expanded=False):
        st.markdown(_t("file_help_body"))


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
    st.subheader(_t("exports"))

    # Use in-memory bytes when available (quick-solve tab); fall back to
    # reading from disk (project tab where files live in project outputs).
    artifact_bytes: bytes | None = _ss("artifact_json")
    report_bytes: bytes | None = _ss("report_json")

    if artifact_bytes is None:
        try:
            artifact_bytes = result.artifact_path.read_bytes()
        except (FileNotFoundError, OSError):
            st.warning(_t("artifact_missing"))
            return

    # JSON artifact download
    artifact_label = "candidate set JSON" if result.is_candidate_set else "snapshot JSON"
    st.download_button(
        _t("download", label=artifact_label),
        data=artifact_bytes,
        file_name=result.artifact_path.name,
        mime="application/json",
    )
    if result.report_path is not None:
        if report_bytes is None:
            try:
                report_bytes = result.report_path.read_bytes()
            except (FileNotFoundError, OSError):
                report_bytes = None
        if report_bytes is not None:
            st.download_button(
                _t("download", label="plan report JSON"),
                data=report_bytes,
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
        except Exception as exc:
            st.warning(
                _t(
                    "export_failed",
                    format=output_format.upper(),
                    error=exc,
                )
            )
            continue
        try:
            st.download_button(
                _t("download", label=output_format.upper()),
                data=output_path.read_bytes(),
                file_name=output_path.name,
                mime=mime,
            )
        except (FileNotFoundError, OSError) as exc:
            st.warning(_t("export_unavailable", error=exc))


# ---------------------------------------------------------------------------
# Quick-solve tab
# ---------------------------------------------------------------------------


def _render_quick_solve_tab() -> None:
    _render_privacy_banner()

    # --- Step wizard indicators ---
    step_labels = {
        "load": _t("step_load"),
        "solve": _t("step_solve"),
        "results": _t("step_results"),
    }
    current_step = _ss("quick_step_value")
    if current_step not in step_labels:
        current_step = "load"
    step = st.radio(
        _t("steps"),
        ["load", "solve", "results"],
        index=["load", "solve", "results"].index(current_step),
        format_func=step_labels.__getitem__,
        horizontal=True,
        key=f"quick_step_{_locale()}",
    )
    st.session_state["quick_step_value"] = step

    # --- Step 1: Load data ---
    if step == "load":
        _render_step_load_data()
        return

    # --- Step 2: Solve ---
    if step == "solve":
        _render_step_solve()
        return

    # --- Step 3: Results ---
    if step == "results":
        _render_step_results()


def _render_step_load_data() -> None:
    st.subheader(_t("load_data"))

    # Demo one-click
    st.markdown(_t("quick_start"))
    demo_col1, demo_col2 = st.columns([1, 3])
    with demo_col1:
        if st.button(_t("load_demo"), type="primary", width="stretch"):
            demo = demo_paths()
            if demo["students_csv"] and demo["layout"]:
                st.session_state["demo_loaded"] = True
                st.session_state["demo_students_path"] = str(demo["students_csv"])
                st.session_state["demo_layout_path"] = str(demo["layout"])
                st.session_state["demo_history_dir"] = (
                    str(demo["history_dir"]) if demo["history_dir"] else None
                )
                # Auto-select the "daily" preset so the solve button is ready.
                st.session_state["_qf_preset"] = "daily"
                st.session_state[f"quick_preset_{_locale()}"] = "daily"
                # Clear any previously uploaded files so demo takes priority.
                for k in ("_qf_students", "_qf_layout", "_qf_rules", "_qf_history"):
                    st.session_state.pop(k, None)
                for k in (
                    "quick_students",
                    "quick_layout",
                    "quick_rules",
                    "quick_history",
                ):
                    st.session_state.pop(k, None)
                st.session_state["_qf_rules_data"] = None
                st.session_state["_qf_rules_name"] = None
                st.session_state["_qf_history_quality"] = None
                st.success(_t("demo_ready"))
            else:
                st.error(_t("demo_missing"))
    with demo_col2:
        st.caption(_t("demo_caption"))

    st.divider()
    st.markdown(_t("restore_settings"))
    config_file = st.file_uploader(
        _t("web_config"),
        type=["json"],
        key="quick_config",
    )
    if config_file is not None:
        try:
            restored = _restore_web_config(config_file.getvalue())
            st.success(
                _t(
                    "settings_restored",
                    count=restored.candidate_count,
                    preset=restored.preset_name or _t("none"),
                )
            )
            st.caption(_t("inputs_still_needed"))
            if restored.contains_student_references:
                st.warning(_t("sensitive_restored_rules"))
        except (InputFileError, ValueError) as exc:
            _render_error(exc)

    st.divider()
    st.markdown(_t("manual_upload"))

    _render_file_hints()

    preset_options = [""] + [preset.name for preset in list_presets()]
    no_preset_label = _t("no_preset")
    current_preset = _ss("_qf_preset") or ""
    preset_index = (
        preset_options.index(current_preset)
        if current_preset in preset_options
        else 0
    )
    preset_widget_key = f"quick_preset_{_locale()}"
    preset_widget_index = (
        None if preset_widget_key in st.session_state else preset_index
    )
    students_file = st.file_uploader(
        _t("students_file"),
        type=["csv", "xlsx", "xlsm"],
        key="quick_students",
    )
    layout_file = st.file_uploader(
        _t("layout_file"),
        type=["json"],
        key="quick_layout",
    )
    preset_name = st.selectbox(
        _t("preset"),
        preset_options,
        index=preset_widget_index,
        format_func=lambda value: value or no_preset_label,
        key=preset_widget_key,
    )
    _render_preset_cards()
    rules_file = st.file_uploader(
        _t("rules_file"),
        type=["json"],
        key="quick_rules",
    )
    history_files = st.file_uploader(
        _t("history_files"),
        type=["json"],
        accept_multiple_files=True,
        key="quick_history",
    )

    # Store files in session for next step.
    # Always update even when cleared, so stale state doesn't linger.
    st.session_state["_qf_students"] = students_file
    st.session_state["_qf_layout"] = layout_file
    st.session_state["_qf_rules"] = rules_file
    st.session_state["_qf_history"] = history_files
    if rules_file is not None:
        st.session_state["_qf_rules_data"] = None
        st.session_state["_qf_rules_name"] = rules_file.name
    elif _ss("_qf_rules_data") is not None:
        st.caption(_t("restored_rules_in_use", name=_ss("_qf_rules_name")))
        if st.button(_t("clear_restored_rules"), key="clear_restored_rules"):
            st.session_state["_qf_rules_data"] = None
            st.session_state["_qf_rules_name"] = None
            st.rerun()
    if preset_name:
        st.session_state["_qf_preset"] = preset_name
    else:
        st.session_state.pop("_qf_preset", None)
    # If user manually uploads files, clear demo flag.
    if students_file is not None or layout_file is not None:
        st.session_state["demo_loaded"] = False
        st.session_state["_qf_history_quality"] = None


def _render_step_solve() -> None:
    st.subheader(_t("solve_settings"))

    # Check data availability
    has_uploaded_students = bool(_ss("_qf_students"))
    has_uploaded_layout = bool(_ss("_qf_layout"))
    demo_loaded = _ss("demo_loaded")

    # If the user manually uploaded files, clear demo so uploads take priority.
    if has_uploaded_students or has_uploaded_layout:
        st.session_state["demo_loaded"] = False
        demo_loaded = False

    has_files = has_uploaded_students and has_uploaded_layout or demo_loaded
    has_rules = bool(
        _ss("_qf_rules")
        or _ss("_qf_rules_data")
        or _ss("_qf_preset")
    )

    if not has_files:
        st.warning(_t("inputs_required"))
        return

    rules_data = None
    if has_rules:
        try:
            rules_data = _current_rules_data()
            rules_preview = build_rules_preview(
                rules_data=rules_data,
                preset_name=_ss("_qf_preset") or None,
            )
        except (InputFileError, ValidationError, ValueError) as exc:
            _render_error(exc)
            return
        with st.expander(_t("resolved_rules"), expanded=True):
            source_parts = []
            if rules_preview.preset_name:
                source_parts.append(f"preset: {rules_preview.preset_name}")
            if rules_preview.overlay_applied:
                source_parts.append(_t("rules_overlay"))
            st.caption(" + ".join(source_parts))
            st.code(rules_preview.json_bytes.decode("utf-8"), language="json")
            st.download_button(
                _t("download_resolved_rules"),
                data=rules_preview.json_bytes,
                file_name="resolved.rules.json",
                mime="application/json",
                key="download_resolved_rules",
            )
    else:
        st.warning(_t("rules_required"))

    has_history = bool(
        _ss("_qf_history")
        or (demo_loaded and _ss("demo_history_dir"))
    )
    if has_history:
        with st.expander(_t("history_quality"), expanded=False):
            if st.button(_t("inspect_history"), key="inspect_history"):
                try:
                    (
                        students_path,
                        layout_path,
                        _rules_path,
                        history_paths,
                    ) = _materialize_quick_inputs()
                    st.session_state["_qf_history_quality"] = analyze_history_files(
                        students_path=students_path,
                        layout_path=layout_path,
                        history_paths=history_paths,
                    )
                except (
                    InputFileError,
                    MissingOptionalDependencyError,
                    ValidationError,
                    ValueError,
                ) as exc:
                    _render_error(exc)
            quality = _ss("_qf_history_quality")
            if quality is not None:
                metric_cols = st.columns(3)
                metric_cols[0].metric(
                    _t("snapshot_count"),
                    quality.snapshot_count,
                )
                metric_cols[1].metric(
                    _t("average_coverage"),
                    f"{quality.average_coverage_percent:.1f}%",
                )
                metric_cols[2].metric(
                    _t("complete_match"),
                    f"{quality.complete_snapshot_count}/{quality.snapshot_count}",
                )
                quality_rows = quality.rows()
                st.dataframe(
                    quality_rows,
                    width="stretch",
                    column_config=_localized_columns(quality_rows),
                )
                if quality.warnings:
                    st.warning("\n".join(_history_warnings(quality)))
                else:
                    st.success(_t("history_consistent"))

    # Solve settings
    candidate_count = st.number_input(
        _t("candidate_count"),
        min_value=1,
        max_value=20,
        value=3,
        step=1,
        key="quick_candidate_count",
    )
    seed_enabled = st.checkbox(_t("custom_seed"), key="quick_seed_enabled")
    seed = st.number_input(
        "seed",
        value=42,
        step=1,
        disabled=not seed_enabled,
        key="quick_seed",
    )
    time_limit_seconds = st.number_input(
        _t("time_limit"),
        min_value=0.1,
        max_value=30.0,
        value=3.0,
        step=0.5,
        key="quick_time_limit",
    )

    try:
        config = WebSessionConfig(
            preset_name=_ss("_qf_preset") or None,
            rules_overlay=rules_data,
            candidate_count=int(candidate_count),
            seed=int(seed) if seed_enabled else None,
            time_limit_seconds=float(time_limit_seconds),
        )
        if config.contains_student_references:
            st.warning(_t("sensitive_current_rules"))
        st.download_button(
            _t("download_web_config"),
            data=dump_web_config(config),
            file_name="seattrellis.web-config.json",
            mime="application/json",
            key="download_web_config",
            help=_t("web_config_help"),
        )
    except ValueError as exc:
        _render_error(exc)

    ready = has_rules and has_files
    if st.button(_t("generate"), type="primary", disabled=not ready):
        _reset_solve_state()
        try:
            output_dir = _make_persistent_tempdir()
            (
                students_path,
                layout_path,
                rules_path,
                history_paths,
            ) = _materialize_quick_inputs()
            preset_name = _ss("_qf_preset") or None

            result = solve_for_web(
                students_path=students_path,
                layout_path=layout_path,
                rules_path=rules_path,
                preset_name=preset_name,
                history_paths=history_paths,
                output_dir=output_dir,
                candidate_count=int(candidate_count),
                seed=int(seed) if seed_enabled else None,
                time_limit_seconds=float(time_limit_seconds),
            )

            # Read artifact data into memory immediately so it survives
            # even if the temp dir is cleaned up.
            st.session_state["artifact_json"] = result.artifact_path.read_bytes()
            if result.report_path is not None:
                st.session_state["report_json"] = result.report_path.read_bytes()

            st.session_state["solved"] = True
            st.session_state["result"] = result
            st.session_state["output_dir"] = output_dir

            # Load layout for seat map.
            from seattrellis.io.json_files import load_layout

            st.session_state["layout_loaded"] = load_layout(layout_path)

            st.success(_t("solve_complete_next"))
        except (
            InputFileError,
            MissingOptionalDependencyError,
            SeatTrellisSolveError,
            ValidationError,
            ValueError,
        ) as exc:
            _render_error(exc)


def _render_step_results() -> None:
    st.subheader(_t("results"))

    result: WebSolveResult | None = _ss("result")
    if result is None:
        st.info(_t("solve_first"))
        return

    output_dir = Path(_ss("output_dir"))
    layout = _ss("layout_loaded")

    # --- Success / warnings ---
    if result.is_candidate_set:
        st.success(
            _t(
                "candidate_result",
                count=len(result.artifact.candidates),
                candidate_id=result.artifact.recommended_candidate_id,
            )
        )
    else:
        st.success(_t("single_result", status=result.artifact.solver_status))

    if result.warnings:
        st.warning("\n".join(result.warnings))

    # --- Candidate switcher ---
    candidate_id = _render_candidate_switcher(result) or "recommended"

    # --- Seat map ---
    st.subheader(_t("seat_map"))
    snapshot = selected_snapshot(result, candidate_id)
    _render_seat_map(snapshot, layout)

    # --- Candidate detail ---
    st.subheader(_t("plan_detail"))
    _render_candidate_detail(result, candidate_id)

    # --- Comparison view ---
    _render_comparison_view(result)

    # --- Assignment table ---
    with st.expander(_t("assignment_table"), expanded=False):
        rows = assignment_rows(snapshot)
        st.dataframe(
            rows,
            width="stretch",
            column_config=_localized_columns(rows),
        )

    # --- Exports ---
    _render_exports(result, output_dir, candidate_id)


# ---------------------------------------------------------------------------
# Project tab
# ---------------------------------------------------------------------------


def _render_project_tab() -> None:
    _render_privacy_banner()

    st.markdown(_t("project_file"))

    project_method_labels = {
        "path": _t("path"),
        "upload": _t("upload"),
    }
    current_project_mode = _ss("project_mode_value")
    if current_project_mode not in project_method_labels:
        current_project_mode = "path"
    tab_mode = st.radio(
        _t("project_method"),
        ["path", "upload"],
        index=["path", "upload"].index(current_project_mode),
        format_func=project_method_labels.__getitem__,
        horizontal=True,
        key=f"project_mode_{_locale()}",
    )
    st.session_state["project_mode_value"] = tab_mode

    project_path: Path | None = None

    if tab_mode == "path":
        project_path_text = st.text_input(
            _t("project_path"),
            value="examples/project.seattrellis.json",
            key="project_path_text",
        )
        if project_path_text:
            project_path = Path(project_path_text).expanduser()
    else:
        uploaded_project = st.file_uploader(
            _t("project_upload"),
            type=["json"],
            key="project_upload",
        )
        if uploaded_project is not None:
            tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".seattrellis.json")
            tmp.write(uploaded_project.getvalue())
            tmp.close()
            project_path = Path(tmp.name)
            st.success(_t("uploaded", name=uploaded_project.name))

    if project_path is None:
        return

    # --- Info & Validate ---
    info_col, validate_col = st.columns(2)
    with info_col:
        if st.button(_t("read_project"), key="proj_info_btn"):
            try:
                st.code(project_info_for_web(project_path=project_path))
            except (InputFileError, ValidationError, ValueError) as exc:
                _render_error(exc)
    with validate_col:
        strict = st.checkbox(_t("strict_warnings"), key="proj_strict")
        if st.button(_t("validate_project"), key="proj_validate_btn"):
            try:
                st.success(
                    project_validate_for_web(project_path=project_path, strict=strict)
                )
            except (InputFileError, ValidationError, ValueError) as exc:
                _render_error(exc)

    # --- Solve ---
    st.subheader(_t("project_solve"))
    use_project_candidates = st.checkbox(
        _t("project_default_candidates"),
        value=True,
        key="proj_use_default",
    )
    project_candidate_count = st.number_input(
        _t("candidate_count"),
        min_value=1,
        max_value=20,
        value=3,
        step=1,
        disabled=use_project_candidates,
        key="project_candidate_count",
    )
    project_seed_enabled = st.checkbox(
        _t("project_custom_seed"),
        key="proj_seed_enabled",
    )
    project_seed = st.number_input(
        "project seed",
        value=42,
        step=1,
        disabled=not project_seed_enabled,
        key="proj_seed",
    )
    project_time_limit = st.number_input(
        _t("project_time_limit"),
        min_value=0.5,
        max_value=30.0,
        value=3.0,
        step=0.5,
        key="proj_time_limit",
    )

    if st.button(_t("solve_project"), type="primary", key="proj_solve_btn"):
        _reset_solve_state()
        try:
            # Project output goes to the project's own output dir (persistent),
            # so we don't need a persistent temp dir here.
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
            st.session_state["output_dir"] = str(result.artifact_path.parent)
            st.session_state["project_path"] = str(project_path)
            # Clear artifact_json so exports read from project dir.
            st.session_state["artifact_json"] = None
            st.session_state["report_json"] = None

            # Load layout from project.
            from seattrellis.io.project import load_project_paths

            _, paths = load_project_paths(
                project_path, require_inputs=True, require_history=False
            )
            from seattrellis.io.json_files import load_layout

            st.session_state["layout_loaded"] = load_layout(paths.layout)

            st.success(_t("solve_complete"))
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
        st.subheader(_t("project_results"))

        candidate_id = _render_candidate_switcher(result, widget_key="project_candidate_selector") or "recommended"
        snapshot = selected_snapshot(result, candidate_id)

        st.subheader(_t("seat_map"))
        _render_seat_map(snapshot, layout)

        _render_candidate_detail(result, candidate_id)
        _render_comparison_view(result)

        with st.expander(_t("assignment_table"), expanded=False):
            rows = assignment_rows(snapshot)
            st.dataframe(
                rows,
                width="stretch",
                column_config=_localized_columns(rows),
            )

        _render_exports(result, output_dir, candidate_id, Path(proj_path_str))


# ---------------------------------------------------------------------------
# App entry point
# ---------------------------------------------------------------------------


st.set_page_config(
    page_title="SeatTrellis",
    page_icon="🏫",
    layout="wide",
)
language_label = st.sidebar.selectbox(
    "语言 / Language",
    list(LANGUAGE_OPTIONS),
    key="ui_language_choice",
)
st.session_state["ui_locale"] = LANGUAGE_OPTIONS[language_label]
st.markdown(accessibility_styles(), unsafe_allow_html=True)
st.markdown(
    f'<a class="seattrellis-skip-link" href="#seattrellis-main">'
    f'{_t("skip_to_content")}</a>'
    f'<span id="seattrellis-main" tabindex="-1"></span>',
    unsafe_allow_html=True,
)
st.title(_t("app_title"))
st.caption(_t("app_caption"))

quick_tab, project_tab = st.tabs([_t("quick_tab"), _t("project_tab")])
with quick_tab:
    _render_quick_solve_tab()
with project_tab:
    _render_project_tab()
