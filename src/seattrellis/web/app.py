"""SeatTrellis Streamlit web UI — v0.4.0.

Privacy-first, local-only. Business logic lives in ``web/workflow.py``;
stateful editing controls live in ``web/interactive_panels.py``.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

try:
    from pydantic.v1 import ValidationError
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import ValidationError

try:
    import streamlit as st
except Exception as exc:  # pragma: no cover
    from seattrellis.optional import MissingOptionalDependencyError

    raise MissingOptionalDependencyError("Streamlit web UI", "web") from exc

from seattrellis.io.json_files import InputFileError
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.project import SeatTrellisProject
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import list_presets
from seattrellis.service_types import ExportRequest, PageOptions, PrivacyOptions
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
    build_data_table_html,
    build_preset_cards,
    build_privacy_notice_html,
    build_seat_grid_html,
    diagnose_error,
)
from seattrellis.web.i18n import (
    LANGUAGE_OPTIONS,
    normalize_locale,
    translate,
)
from seattrellis.web.interactive_panels import (
    render_manual_edit_panel as _render_manual_edit_panel,
    render_repair_panel as _render_repair_panel,
)
from seattrellis.web.keys import (
    APP_WORKSPACE_SELECT,
    PROJECT_CANDIDATE_COUNT_INPUT,
    PROJECT_CANDIDATE_SELECT,
    PROJECT_EXPORT_PREFIX,
    PROJECT_EXPORT_DOWNLOAD_ARTIFACT,
    PROJECT_EXPORT_DOWNLOAD_REPORT,
    PROJECT_INFO_BUTTON,
    PROJECT_INFO_STATUS,
    PROJECT_MODE_RADIO,
    PROJECT_PATH_INPUT,
    PROJECT_PATH_STATUS,
    PROJECT_RESULTS_STATUS,
    PROJECT_SEED_ENABLED,
    PROJECT_SEED_INPUT,
    PROJECT_SOLVE_BUTTON,
    PROJECT_SOLVE_STATUS,
    PROJECT_STRICT_CHECKBOX,
    PROJECT_TIME_LIMIT_INPUT,
    PROJECT_UPLOAD_INPUT,
    PROJECT_USE_DEFAULT_CANDIDATES,
    PROJECT_VALIDATE_BUTTON,
    PROJECT_VALIDATE_STATUS,
    QUICK_CANDIDATE_SELECT,
    QUICK_CANDIDATE_COUNT_INPUT,
    QUICK_CLEAR_UPLOADS_BUTTON,
    QUICK_CONFIG_UPLOAD,
    QUICK_EXPORT_ALL_CANDIDATES_CHECKBOX,
    QUICK_EXPORT_DOWNLOAD_ARTIFACT,
    QUICK_EXPORT_DOWNLOAD_REPORT,
    QUICK_EXPORT_FORMAT_SELECT,
    QUICK_EXPORT_PREFIX,
    QUICK_GENERATE_BUTTON,
    QUICK_HISTORY_UPLOAD,
    QUICK_INSPECT_HISTORY_BUTTON,
    QUICK_LAYOUT_UPLOAD,
    QUICK_LOAD_DEMO_BUTTON,
    QUICK_PRESET_SELECT,
    QUICK_RETAINED_UPLOADS_STATUS,
    QUICK_RESULTS_STATUS,
    QUICK_RULES_UPLOAD,
    QUICK_SOLVE_STATUS,
    QUICK_STUDENTS_UPLOAD,
    QUICK_STEP_RADIO,
    UI_LANGUAGE_SELECT,
    export_prepare_key,
    export_prepared_download_key,
    export_prepared_state_key,
    widget_region_key,
)
from seattrellis.web.teacher_page import render_teacher_page
from seattrellis.web.tempfiles import (
    discard_persistent_tempdir as _discard_persistent_tempdir,
    make_persistent_tempdir as _make_persistent_tempdir,
)
from seattrellis.web.workflow import (
    WebSolveResult,
    analyze_history_files,
    assignment_rows,
    build_rules_preview,
    candidate_summary_rows,
    demo_paths,
    expand_user_path,
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
    "result_origin": None,
    "demo_loaded": False,
    "demo_students_path": None,
    "demo_layout_path": None,
    "demo_history_dir": None,
    "_qf_rules_data": None,
    "_qf_rules_name": None,
    "_qf_config_digest": None,
    "_qf_history_quality": None,
    "_qf_students": None,
    "_qf_layout": None,
    "_qf_rules": None,
    "_qf_history": None,
    "ui_locale": "zh",
    "quick_step_value": "load",
    "project_mode_value": "path",
    "_quick_editing_draft": None,
    "_project_editing_draft": None,
}

def _ss(key: str):
    """Get-or-create a session-state key."""
    if key not in st.session_state:
        st.session_state[key] = _SS_DEFAULTS.get(key)
    return st.session_state[key]


def _locale() -> str:
    return normalize_locale(_ss("ui_locale"))


def _t(key: str, **values: object) -> str:
    return translate(key, _locale(), **values)


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


def _reset_solve_state(origin: str, *, replace_active: bool = False) -> None:
    """Discard derived state for one workspace without clearing the other."""

    export_prefixes = {
        "quick": QUICK_EXPORT_PREFIX,
        "project": PROJECT_EXPORT_PREFIX,
    }
    try:
        export_prefix = export_prefixes[origin]
    except KeyError as exc:
        raise ValueError(f"Unknown solve-state origin: {origin}") from exc
    st.session_state.pop(export_prepared_state_key(export_prefix), None)
    st.session_state[f"_{origin}_editing_draft"] = None

    active_origin = _ss("result_origin")
    if replace_active and active_origin not in (None, origin):
        try:
            active_export_prefix = export_prefixes[active_origin]
        except KeyError as exc:
            raise ValueError(
                f"Unknown active solve-state origin: {active_origin}"
            ) from exc
        st.session_state.pop(
            export_prepared_state_key(active_export_prefix),
            None,
        )
        st.session_state[f"_{active_origin}_editing_draft"] = None
    if not replace_active and active_origin not in (None, origin):
        return

    for key in (
        "solved",
        "result",
        "artifact_json",
        "report_json",
        "output_dir",
        "project_path",
        "layout_loaded",
        "current_candidate_id",
        "result_origin",
    ):
        st.session_state[key] = _SS_DEFAULTS[key]


def _invalidate_quick_solve() -> None:
    """Clear results derived from quick-solve inputs or settings."""

    st.session_state["_qf_history_quality"] = None
    _reset_solve_state("quick")


def _invalidate_project_solve() -> None:
    """Clear results when the selected Project source changes."""

    _reset_solve_state("project")


def _sync_quick_upload(widget_key: str, cache_key: str) -> None:
    """Persist an upload across wizard steps and invalidate stale results."""

    st.session_state[cache_key] = st.session_state.get(widget_key)
    _invalidate_quick_solve()


def _clear_quick_uploads() -> None:
    """Discard retained quick-solve files without changing other settings."""

    for widget_key, cache_key in (
        (QUICK_STUDENTS_UPLOAD, "_qf_students"),
        (QUICK_LAYOUT_UPLOAD, "_qf_layout"),
        (QUICK_RULES_UPLOAD, "_qf_rules"),
        (QUICK_HISTORY_UPLOAD, "_qf_history"),
    ):
        st.session_state.pop(widget_key, None)
        st.session_state[cache_key] = None
    if _ss("_qf_rules_data") is None:
        st.session_state["_qf_rules_name"] = None
    _invalidate_quick_solve()


def _retained_upload_names() -> list[str]:
    """Return safe display names for files retained across wizard steps."""

    retained: list[str] = []
    for cache_key in ("_qf_students", "_qf_layout", "_qf_rules"):
        uploaded = _ss(cache_key)
        if uploaded is not None:
            retained.append(Path(uploaded.name).name)
    retained.extend(
        Path(uploaded.name).name
        for uploaded in (_ss("_qf_history") or [])
    )
    return retained


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

    _invalidate_quick_solve()
    preset_name = config.preset_name or ""
    st.session_state[QUICK_PRESET_SELECT] = preset_name
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
        students_suffix = Path(students_file.name).suffix.lower() or ".csv"
        students_path = input_root / f"students{students_suffix}"
        layout_path = input_root / "layout.json"
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
        rules_path = input_root / "rules.json"
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
        history_path = input_root / f"history-{index:02d}.snapshot.json"
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


def _render_candidate_switcher(
    result: WebSolveResult,
    widget_key: str = QUICK_CANDIDATE_SELECT,
    state_key: str = "current_candidate_id",
) -> str | None:
    """Render candidate selector and return the chosen candidate ID."""
    if not result.is_candidate_set:
        return "recommended"

    options = build_candidate_selector(result.artifact, locale=_locale())
    ids = [opt["id"] for opt in options]
    labels_by_id = {opt["id"]: opt["label"] for opt in options}

    current = _ss(state_key)
    try:
        idx = ids.index(current)
    except ValueError:
        idx = 0

    with st.container(key=widget_region_key(widget_key)):
        selected_id = st.selectbox(
            _t("candidate_choice"),
            ids,
            index=idx,
            format_func=labels_by_id.__getitem__,
            key=widget_key,
        )
    st.session_state[state_key] = selected_id
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
    st.markdown(
        build_data_table_html(
            rows,
            caption=_t("plan_detail"),
            locale=_locale(),
        ),
        unsafe_allow_html=True,
    )


def _render_comparison_view(result: WebSolveResult) -> None:
    """Render the multi-candidate comparison table."""
    if not result.is_candidate_set:
        return
    with st.expander(_t("candidate_comparison"), expanded=False):
        comp = build_comparison_table(result.artifact)
        st.markdown(
            build_data_table_html(
                comp["rows"],
                columns=comp["columns"],
                caption=_t("candidate_comparison"),
                locale=_locale(),
            ),
            unsafe_allow_html=True,
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
    export_key = (
        PROJECT_EXPORT_PREFIX
        if project_path is not None
        else QUICK_EXPORT_PREFIX
    )
    export_formats = {
        "print-html": ("Print HTML", "text/html"),
        "pdf": ("PDF", "application/pdf"),
        "docx": (
            "DOCX",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        "html": ("HTML", "text/html"),
        "png": ("PNG", "image/png"),
        "excel": (
            "Excel",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    }
    configurable_formats = {"print-html", "pdf", "docx"}
    with st.expander(_t("export_settings"), expanded=True):
        export_format_key = (
            f"{export_key}_format"
            if project_path is not None
            else QUICK_EXPORT_FORMAT_SELECT
        )
        output_format = st.selectbox(
            _t("export_format"),
            list(export_formats),
            format_func=lambda value: export_formats[value][0],
            key=export_format_key,
        )
        export_label, mime = export_formats[output_format]
        st.caption(_t("export_on_demand"))
        supports_candidate_report = (
            result.is_candidate_set and output_format in {"html", "print-html"}
        )
        candidate_scope = "selected"
        if result.is_candidate_set:
            all_candidates_key = (
                f"{export_key}_all_candidates"
                if project_path is not None
                else QUICK_EXPORT_ALL_CANDIDATES_CHECKBOX
            )
            all_candidates = st.checkbox(
                _t("export_all_candidates"),
                value=False,
                disabled=not supports_candidate_report,
                key=all_candidates_key,
                help=_t("export_all_candidates_help"),
            )
            candidate_scope = "all" if all_candidates and supports_candidate_report else "selected"

        template_labels = {
            "public": _t("template_public"),
            "teacher": _t("template_teacher"),
            "report": _t("template_report"),
        }
        supports_privacy_options = output_format in configurable_formats
        if not supports_privacy_options:
            st.info(_t("export_privacy_unsupported"))
        template_options = ["public", "teacher"]
        if result.is_candidate_set:
            template_options.append("report")
        template_key = f"{export_key}_template"
        if (
            template_key in st.session_state
            and st.session_state[template_key] not in template_options
        ):
            st.session_state[template_key] = "public"
        with st.container(key=widget_region_key(template_key)):
            template = st.selectbox(
                _t("export_template"),
                template_options,
                format_func=template_labels.__getitem__,
                key=template_key,
                disabled=not supports_privacy_options,
            )
        defaults = PrivacyOptions.for_template(template)
        st.caption(_t("privacy_defaults"))
        privacy_columns = st.columns(2)
        with privacy_columns[0]:
            hide_scores = st.checkbox(
                _t("hide_scores"),
                value=defaults.hide_scores,
                disabled=defaults.hide_scores or not supports_privacy_options,
                key=f"{export_key}_hide_scores_{template}",
            )
            hide_notes = st.checkbox(
                _t("hide_notes"),
                value=defaults.hide_notes,
                disabled=defaults.hide_notes or not supports_privacy_options,
                key=f"{export_key}_hide_notes_{template}",
            )
            hide_special_needs = st.checkbox(
                _t("hide_special_needs"),
                value=defaults.hide_special_needs,
                disabled=(
                    defaults.hide_special_needs or not supports_privacy_options
                ),
                key=f"{export_key}_hide_needs_{template}",
            )
        with privacy_columns[1]:
            hide_height = st.checkbox(
                _t("hide_height"),
                value=not defaults.show_height,
                disabled=not defaults.show_height or not supports_privacy_options,
                key=f"{export_key}_hide_height_{template}",
            )
            hide_vision = st.checkbox(
                _t("hide_vision"),
                value=not defaults.show_vision,
                disabled=not defaults.show_vision or not supports_privacy_options,
                key=f"{export_key}_hide_vision_{template}",
            )
            anonymize_key = f"{export_key}_anonymize_{template}"
            with st.container(key=widget_region_key(anonymize_key)):
                anonymize = st.checkbox(
                    _t("anonymize_names"),
                    value=False,
                    disabled=not supports_privacy_options,
                    key=anonymize_key,
                )

        page_columns = st.columns(3)
        with page_columns[0]:
            orientation_key = f"{export_key}_orientation"
            with st.container(key=widget_region_key(orientation_key)):
                orientation = st.selectbox(
                    _t("page_orientation"),
                    ["portrait", "landscape"],
                    format_func=lambda value: _t(f"orientation_{value}"),
                    key=orientation_key,
                    disabled=not supports_privacy_options,
                )
        with page_columns[1]:
            page_scale = st.number_input(
                _t("page_scale"),
                min_value=0.5,
                max_value=2.0,
                value=1.0,
                step=0.1,
                key=f"{export_key}_page_scale",
                disabled=not supports_privacy_options,
            )
        with page_columns[2]:
            locale_key = f"{export_key}_locale"
            with st.container(key=widget_region_key(locale_key)):
                export_locale = st.selectbox(
                    _t("export_locale"),
                    ["zh", "en"],
                    index=0 if _locale() == "zh" else 1,
                    format_func=(
                        lambda value: "简体中文"
                        if value == "zh"
                        else "English"
                    ),
                    key=locale_key,
                    disabled=not supports_privacy_options,
                )

    privacy = PrivacyOptions(
        hide_scores=hide_scores,
        hide_notes=hide_notes,
        hide_special_needs=hide_special_needs,
        anonymize=anonymize,
        show_height=not hide_height,
        show_vision=not hide_vision,
    )
    page = PageOptions(
        orientation=orientation,
        scale=float(page_scale),
    )

    # Prefer the bytes captured when the result was created. Derived results
    # can still fall back to their session-scoped artifact on disk.
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
    artifact_download_key = (
        PROJECT_EXPORT_DOWNLOAD_ARTIFACT
        if project_path is not None
        else QUICK_EXPORT_DOWNLOAD_ARTIFACT
    )
    with st.container(key=widget_region_key(artifact_download_key)):
        st.download_button(
            _t("download", label=artifact_label),
            data=artifact_bytes,
            file_name=result.artifact_path.name,
            mime="application/json",
            key=artifact_download_key,
            on_click="ignore",
        )
    if result.report_path is not None:
        if report_bytes is None:
            try:
                report_bytes = result.report_path.read_bytes()
            except (FileNotFoundError, OSError):
                report_bytes = None
        if report_bytes is not None:
            report_download_key = (
                PROJECT_EXPORT_DOWNLOAD_REPORT
                if project_path is not None
                else QUICK_EXPORT_DOWNLOAD_REPORT
            )
            with st.container(key=widget_region_key(report_download_key)):
                st.download_button(
                    _t("download", label="plan report JSON"),
                    data=report_bytes,
                    file_name=result.report_path.name,
                    mime="application/json",
                    key=report_download_key,
                    on_click="ignore",
                )

    export_signature = (
        str(result.artifact_path),
        str(result.report_path) if result.report_path is not None else "",
        str(project_path) if project_path is not None else "",
        output_format,
        candidate_id,
        candidate_scope,
        template,
        privacy.hide_scores,
        privacy.hide_notes,
        privacy.hide_special_needs,
        privacy.anonymize,
        privacy.show_height,
        privacy.show_vision,
        page.orientation,
        page.scale,
        export_locale,
    )
    prepared_key = export_prepared_state_key(export_key)
    prepare_widget_key = export_prepare_key(export_key, output_format)
    with st.container(key=widget_region_key(prepare_widget_key)):
        prepare_requested = st.button(
            _t("prepare_export", label=export_label),
            key=prepare_widget_key,
        )
    if prepare_requested:
        st.session_state.pop(prepared_key, None)
        try:
            request = None
            if output_format in configurable_formats or candidate_scope == "all":
                request = ExportRequest(
                    output_format=output_format,
                    template=template,
                    privacy=privacy if output_format in configurable_formats else None,
                    page=page,
                    locale=export_locale,
                    candidate_id=(
                        candidate_id
                        if result.is_candidate_set and candidate_scope == "selected"
                        else None
                    ),
                    candidate_scope=candidate_scope,
                )
            if project_path is None:
                output_path = export_for_web(
                    result,
                    output_format=output_format,
                    output_dir=output_dir,
                    candidate_id=candidate_id,
                    request=request,
                )
            else:
                output_path = project_export_for_web(
                    result,
                    project_path=project_path,
                    output_format=output_format,
                    output_dir=output_dir,
                    candidate_id=candidate_id if result.is_candidate_set else None,
                    request=request,
                )
        except MissingOptionalDependencyError as exc:
            st.info(str(exc))
        except Exception as exc:
            st.warning(
                _t(
                    "export_failed",
                    format=output_format.upper(),
                    error=exc,
                )
            )
        else:
            try:
                st.session_state[prepared_key] = {
                    "signature": export_signature,
                    "label": export_label,
                    "data": output_path.read_bytes(),
                    "file_name": output_path.name,
                    "mime": mime,
                }
            except (FileNotFoundError, OSError) as exc:
                st.warning(_t("export_unavailable", error=exc))

    prepared = st.session_state.get(prepared_key)
    if prepared and prepared.get("signature") == export_signature:
        download_widget_key = export_prepared_download_key(export_key)
        with st.container(key=widget_region_key(download_widget_key)):
            st.success(_t("export_ready", label=prepared["label"]))
            try:
                st.download_button(
                    _t("download", label=prepared["label"]),
                    data=prepared["data"],
                    file_name=prepared["file_name"],
                    mime=prepared["mime"],
                    key=download_widget_key,
                    on_click="ignore",
                )
            except (KeyError, TypeError) as exc:
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
    with st.container(key=widget_region_key(QUICK_STEP_RADIO)):
        step = st.radio(
            _t("steps"),
            ["load", "solve", "results"],
            index=["load", "solve", "results"].index(current_step),
            format_func=step_labels.__getitem__,
            horizontal=True,
            key=QUICK_STEP_RADIO,
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
        with st.container(key=widget_region_key(QUICK_LOAD_DEMO_BUTTON)):
            load_demo = st.button(
                _t("load_demo"),
                type="primary",
                width="stretch",
                key=QUICK_LOAD_DEMO_BUTTON,
            )
        if load_demo:
            demo = demo_paths()
            if demo["students_csv"] and demo["layout"]:
                _invalidate_quick_solve()
                st.session_state["demo_loaded"] = True
                st.session_state["demo_students_path"] = str(demo["students_csv"])
                st.session_state["demo_layout_path"] = str(demo["layout"])
                st.session_state["demo_history_dir"] = (
                    str(demo["history_dir"]) if demo["history_dir"] else None
                )
                # Auto-select the "daily" preset so the solve button is ready.
                st.session_state["_qf_preset"] = "daily"
                st.session_state[QUICK_PRESET_SELECT] = "daily"
                # Clear any previously uploaded files so demo takes priority.
                for k in ("_qf_students", "_qf_layout", "_qf_rules", "_qf_history"):
                    st.session_state.pop(k, None)
                for k in (
                    QUICK_STUDENTS_UPLOAD,
                    QUICK_LAYOUT_UPLOAD,
                    QUICK_RULES_UPLOAD,
                    QUICK_HISTORY_UPLOAD,
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
    with st.container(key=widget_region_key(QUICK_CONFIG_UPLOAD)):
        config_file = st.file_uploader(
            _t("web_config"),
            type=["json"],
            key=QUICK_CONFIG_UPLOAD,
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
    preset_widget_index = (
        None if QUICK_PRESET_SELECT in st.session_state else preset_index
    )
    with st.container(key=widget_region_key(QUICK_STUDENTS_UPLOAD)):
        st.file_uploader(
            _t("students_file"),
            type=["csv", "xlsx", "xlsm"],
            key=QUICK_STUDENTS_UPLOAD,
            on_change=_sync_quick_upload,
            args=(QUICK_STUDENTS_UPLOAD, "_qf_students"),
        )
    with st.container(key=widget_region_key(QUICK_LAYOUT_UPLOAD)):
        st.file_uploader(
            _t("layout_file"),
            type=["json"],
            key=QUICK_LAYOUT_UPLOAD,
            on_change=_sync_quick_upload,
            args=(QUICK_LAYOUT_UPLOAD, "_qf_layout"),
        )
    with st.container(key=widget_region_key(QUICK_PRESET_SELECT)):
        preset_name = st.selectbox(
            _t("preset"),
            preset_options,
            index=preset_widget_index,
            format_func=lambda value: value or no_preset_label,
            key=QUICK_PRESET_SELECT,
            on_change=_invalidate_quick_solve,
        )
    _render_preset_cards()
    with st.container(key=widget_region_key(QUICK_RULES_UPLOAD)):
        st.file_uploader(
            _t("rules_file"),
            type=["json"],
            key=QUICK_RULES_UPLOAD,
            on_change=_sync_quick_upload,
            args=(QUICK_RULES_UPLOAD, "_qf_rules"),
        )
    with st.container(key=widget_region_key(QUICK_HISTORY_UPLOAD)):
        st.file_uploader(
            _t("history_files"),
            type=["json"],
            accept_multiple_files=True,
            key=QUICK_HISTORY_UPLOAD,
            on_change=_sync_quick_upload,
            args=(QUICK_HISTORY_UPLOAD, "_qf_history"),
        )

    retained_uploads = _retained_upload_names()
    if retained_uploads:
        with st.container(
            key=widget_region_key(QUICK_RETAINED_UPLOADS_STATUS)
        ):
            st.caption(
                _t(
                    "retained_uploads",
                    names=", ".join(retained_uploads),
                )
            )
        with st.container(
            key=widget_region_key(QUICK_CLEAR_UPLOADS_BUTTON)
        ):
            clear_uploads = st.button(
                _t("clear_uploads"),
                key=QUICK_CLEAR_UPLOADS_BUTTON,
            )
        if clear_uploads:
            _clear_quick_uploads()
            st.rerun()

    if _ss("_qf_rules") is not None:
        st.session_state["_qf_rules_data"] = None
        st.session_state["_qf_rules_name"] = _ss("_qf_rules").name
    elif _ss("_qf_rules_data") is not None:
        st.caption(_t("restored_rules_in_use", name=_ss("_qf_rules_name")))
        if st.button(_t("clear_restored_rules"), key="clear_restored_rules"):
            st.session_state["_qf_rules_data"] = None
            st.session_state["_qf_rules_name"] = None
            _invalidate_quick_solve()
            st.rerun()
    if preset_name:
        st.session_state["_qf_preset"] = preset_name
    else:
        st.session_state.pop("_qf_preset", None)
    # If user manually uploads files, clear demo flag.
    if _ss("_qf_students") is not None or _ss("_qf_layout") is not None:
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
            with st.container(
                key=widget_region_key(QUICK_INSPECT_HISTORY_BUTTON)
            ):
                inspect_history = st.button(
                    _t("inspect_history"),
                    key=QUICK_INSPECT_HISTORY_BUTTON,
                )
            if inspect_history:
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
                st.markdown(
                    build_data_table_html(
                        quality_rows,
                        caption=_t("history_quality"),
                        locale=_locale(),
                    ),
                    unsafe_allow_html=True,
                )
                if quality.warnings:
                    st.warning("\n".join(_history_warnings(quality)))
                else:
                    st.success(_t("history_consistent"))

    # Solve settings
    with st.container(key=widget_region_key(QUICK_CANDIDATE_COUNT_INPUT)):
        candidate_count = st.number_input(
            _t("candidate_count"),
            min_value=1,
            max_value=20,
            value=3,
            step=1,
            key=QUICK_CANDIDATE_COUNT_INPUT,
            on_change=_invalidate_quick_solve,
        )
    seed_enabled = st.checkbox(
        _t("custom_seed"),
        key="quick_seed_enabled",
        on_change=_invalidate_quick_solve,
    )
    seed = st.number_input(
        "seed",
        value=42,
        step=1,
        disabled=not seed_enabled,
        key="quick_seed",
        on_change=_invalidate_quick_solve,
    )
    time_limit_seconds = st.number_input(
        _t("time_limit"),
        min_value=0.1,
        max_value=30.0,
        value=3.0,
        step=0.5,
        key="quick_time_limit",
        on_change=_invalidate_quick_solve,
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
    with st.container(key=widget_region_key(QUICK_GENERATE_BUTTON)):
        generate_requested = st.button(
            _t("generate"),
            type="primary",
            disabled=not ready,
            key=QUICK_GENERATE_BUTTON,
        )
    if generate_requested:
        _reset_solve_state("quick", replace_active=True)
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
            st.session_state["result_origin"] = "quick"
            st.session_state["output_dir"] = output_dir

            # Load layout for seat map.
            from seattrellis.io.json_files import load_layout

            st.session_state["layout_loaded"] = load_layout(layout_path)

            with st.container(key=widget_region_key(QUICK_SOLVE_STATUS)):
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

    result: WebSolveResult | None = (
        _ss("result") if _ss("result_origin") == "quick" else None
    )
    if result is None:
        st.info(_t("solve_first"))
        return

    output_dir = Path(_ss("output_dir"))
    layout = _ss("layout_loaded")

    # --- Success / warnings ---
    with st.container(key=widget_region_key(QUICK_RESULTS_STATUS)):
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
        st.markdown(
            build_data_table_html(
                rows,
                caption=_t("assignment_table"),
                locale=_locale(),
            ),
            unsafe_allow_html=True,
        )

    _render_manual_edit_panel(
        result,
        candidate_id,
        output_dir=output_dir,
        translate=_t,
        render_error=_render_error,
    )
    _render_repair_panel(
        result,
        candidate_id,
        output_dir=output_dir,
        translate=_t,
        render_error=_render_error,
        quick_history_paths=lambda: _materialize_quick_inputs()[3],
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
    with st.container(key=widget_region_key(PROJECT_MODE_RADIO)):
        tab_mode = st.radio(
            _t("project_method"),
            ["path", "upload"],
            index=["path", "upload"].index(current_project_mode),
            format_func=project_method_labels.__getitem__,
            horizontal=True,
            key=PROJECT_MODE_RADIO,
            on_change=_invalidate_project_solve,
        )
    st.session_state["project_mode_value"] = tab_mode

    project_path: Path | None = None

    if tab_mode == "path":
        with st.container(key=widget_region_key(PROJECT_PATH_INPUT)):
            project_path_text = st.text_input(
                _t("project_path"),
                value="examples/project.seattrellis.json",
                key=PROJECT_PATH_INPUT,
                on_change=_invalidate_project_solve,
            )
        if project_path_text:
            try:
                project_path = expand_user_path(project_path_text)
            except ValueError as exc:
                with st.container(
                    key=widget_region_key(PROJECT_PATH_STATUS)
                ):
                    _render_error(exc)
    else:
        with st.container(key=widget_region_key(PROJECT_UPLOAD_INPUT)):
            uploaded_project = st.file_uploader(
                _t("project_upload"),
                type=["json"],
                key=PROJECT_UPLOAD_INPUT,
            )
        if uploaded_project is not None:
            try:
                raw_project = json.loads(uploaded_project.getvalue())
                if hasattr(SeatTrellisProject, "model_validate"):
                    project = SeatTrellisProject.model_validate(  # type: ignore[attr-defined]
                        raw_project
                    )
                    project_data = project.model_dump(mode="json")  # type: ignore[attr-defined]
                else:
                    project = SeatTrellisProject.parse_obj(raw_project)
                    project_data = json.loads(project.json())
                st.success(_t("uploaded", name=uploaded_project.name))
                st.code(
                    json.dumps(project_data, ensure_ascii=False, indent=2),
                    language="json",
                )
                st.info(_t("project_upload_manifest_only"))
            except (UnicodeDecodeError, ValidationError, ValueError) as exc:
                _render_error(exc)
            return

    if project_path is None:
        return

    # --- Info & Validate ---
    info_col, validate_col = st.columns(2)
    with info_col:
        with st.container(key=widget_region_key(PROJECT_INFO_BUTTON)):
            info_requested = st.button(
                _t("read_project"),
                key=PROJECT_INFO_BUTTON,
            )
        if info_requested:
            with st.container(key=widget_region_key(PROJECT_INFO_STATUS)):
                try:
                    st.code(project_info_for_web(project_path=project_path))
                except (InputFileError, ValidationError, ValueError) as exc:
                    _render_error(exc)
    with validate_col:
        with st.container(key=widget_region_key(PROJECT_STRICT_CHECKBOX)):
            strict = st.checkbox(
                _t("strict_warnings"),
                key=PROJECT_STRICT_CHECKBOX,
            )
        with st.container(key=widget_region_key(PROJECT_VALIDATE_BUTTON)):
            validate_requested = st.button(
                _t("validate_project"),
                key=PROJECT_VALIDATE_BUTTON,
            )
        if validate_requested:
            with st.container(
                key=widget_region_key(PROJECT_VALIDATE_STATUS)
            ):
                try:
                    st.success(
                        project_validate_for_web(
                            project_path=project_path,
                            strict=strict,
                        )
                    )
                except (InputFileError, ValidationError, ValueError) as exc:
                    _render_error(exc)

    # --- Solve ---
    st.subheader(_t("project_solve"))
    with st.container(
        key=widget_region_key(PROJECT_USE_DEFAULT_CANDIDATES)
    ):
        use_project_candidates = st.checkbox(
            _t("project_default_candidates"),
            value=True,
            key=PROJECT_USE_DEFAULT_CANDIDATES,
            on_change=_invalidate_project_solve,
        )
    with st.container(key=widget_region_key(PROJECT_CANDIDATE_COUNT_INPUT)):
        project_candidate_count = st.number_input(
            _t("candidate_count"),
            min_value=1,
            max_value=20,
            value=3,
            step=1,
            disabled=use_project_candidates,
            key=PROJECT_CANDIDATE_COUNT_INPUT,
            on_change=_invalidate_project_solve,
        )
    with st.container(key=widget_region_key(PROJECT_SEED_ENABLED)):
        project_seed_enabled = st.checkbox(
            _t("project_custom_seed"),
            key=PROJECT_SEED_ENABLED,
            on_change=_invalidate_project_solve,
        )
    with st.container(key=widget_region_key(PROJECT_SEED_INPUT)):
        project_seed = st.number_input(
            "project seed",
            value=42,
            step=1,
            disabled=not project_seed_enabled,
            key=PROJECT_SEED_INPUT,
            on_change=_invalidate_project_solve,
        )
    with st.container(key=widget_region_key(PROJECT_TIME_LIMIT_INPUT)):
        project_time_limit = st.number_input(
            _t("project_time_limit"),
            min_value=0.5,
            max_value=30.0,
            value=3.0,
            step=0.5,
            key=PROJECT_TIME_LIMIT_INPUT,
            on_change=_invalidate_project_solve,
        )

    with st.container(key=widget_region_key(PROJECT_SOLVE_BUTTON)):
        solve_requested = st.button(
            _t("solve_project"),
            type="primary",
            key=PROJECT_SOLVE_BUTTON,
        )
    if solve_requested:
        _reset_solve_state("project", replace_active=True)
        with st.container(key=widget_region_key(PROJECT_SOLVE_STATUS)):
            try:
                # Each browser session owns its result files. The Project
                # inputs remain shared, but one session cannot overwrite
                # another session's displayed or exported result.
                output_dir = _make_persistent_tempdir()
                result = project_solve_for_web(
                    project_path=project_path,
                    output_dir=output_dir,
                    candidate_count=(
                        None
                        if use_project_candidates
                        else int(project_candidate_count)
                    ),
                    seed=(
                        int(project_seed)
                        if project_seed_enabled
                        else None
                    ),
                    time_limit_seconds=float(project_time_limit),
                )

                from seattrellis.io.project import load_project_paths

                _, paths = load_project_paths(
                    project_path,
                    require_inputs=True,
                    require_history=False,
                )
                from seattrellis.io.json_files import load_layout

                layout = load_layout(paths.layout)

                st.session_state["solved"] = True
                st.session_state["result"] = result
                st.session_state["result_origin"] = "project"
                st.session_state["output_dir"] = output_dir
                st.session_state["project_path"] = str(project_path)
                st.session_state["artifact_json"] = result.artifact_path.read_bytes()
                st.session_state["report_json"] = (
                    result.report_path.read_bytes()
                    if result.report_path is not None
                    else None
                )
                st.session_state["layout_loaded"] = layout
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
    result: WebSolveResult | None = (
        _ss("result") if _ss("result_origin") == "project" else None
    )
    proj_path_str: str | None = _ss("project_path")
    if result is not None and proj_path_str is not None:
        output_dir = Path(_ss("output_dir"))
        layout = _ss("layout_loaded")

        st.divider()
        with st.container(key=widget_region_key(PROJECT_RESULTS_STATUS)):
            st.subheader(_t("project_results"))
            if result.is_candidate_set:
                st.success(
                    _t(
                        "candidate_result",
                        count=len(result.artifact.candidates),
                        candidate_id=result.artifact.recommended_candidate_id,
                    )
                )
            else:
                st.success(
                    _t(
                        "single_result",
                        status=result.artifact.solver_status,
                    )
                )

        candidate_id = (
            _render_candidate_switcher(
                result,
                widget_key=PROJECT_CANDIDATE_SELECT,
            )
            or "recommended"
        )
        snapshot = selected_snapshot(result, candidate_id)

        st.subheader(_t("seat_map"))
        _render_seat_map(snapshot, layout)

        _render_candidate_detail(result, candidate_id)
        _render_comparison_view(result)

        with st.expander(_t("assignment_table"), expanded=False):
            rows = assignment_rows(snapshot)
            st.markdown(
                build_data_table_html(
                    rows,
                    caption=_t("assignment_table"),
                    locale=_locale(),
                ),
                unsafe_allow_html=True,
            )

        _render_manual_edit_panel(
            result,
            candidate_id,
            output_dir=output_dir,
            translate=_t,
            render_error=_render_error,
            project=True,
        )
        _render_repair_panel(
            result,
            candidate_id,
            output_dir=output_dir,
            translate=_t,
            render_error=_render_error,
            project_path=Path(proj_path_str),
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
with st.sidebar:
    with st.container(key=widget_region_key(UI_LANGUAGE_SELECT)):
        language_label = st.selectbox(
            "语言 / Language",
            list(LANGUAGE_OPTIONS),
            key=UI_LANGUAGE_SELECT,
        )
    st.session_state["ui_locale"] = LANGUAGE_OPTIONS[language_label]
    with st.container(key=widget_region_key(APP_WORKSPACE_SELECT)):
        workspace = st.radio(
            _t("workspace_choice"),
            ["teacher", "advanced"],
            format_func=lambda value: _t(f"workspace_{value}"),
            key=APP_WORKSPACE_SELECT,
        )
st.markdown(accessibility_styles(), unsafe_allow_html=True)
st.markdown(
    f'<a class="seattrellis-skip-link" href="#seattrellis-main">'
    f'{_t("skip_to_content")}</a>'
    f'<span id="seattrellis-main" tabindex="-1"></span>',
    unsafe_allow_html=True,
)
st.title(_t("app_title"))
st.caption(_t("app_caption"))

if workspace == "teacher":
    render_teacher_page(
        make_persistent_tempdir=_make_persistent_tempdir,
        discard_persistent_tempdir=_discard_persistent_tempdir,
        locale=_locale(),
    )
else:
    quick_tab, project_tab = st.tabs([_t("quick_tab"), _t("project_tab")])
    with quick_tab:
        _render_quick_solve_tab()
    with project_tab:
        _render_project_tab()
