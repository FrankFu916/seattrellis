"""Default Streamlit page for a teacher's everyday seating workflow.

The page intentionally exposes classroom concepts rather than solver options:
upload a roster, choose a familiar room, choose a goal, generate three plans,
then print the recommended result.  Canonical data continues to live in the
application and domain layers; session state only caches browser inputs and
the result produced for the current setup.
"""

from __future__ import annotations

import hashlib
import re
from collections.abc import Callable, MutableMapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

from seattrellis.application.class_workflow import GenerateOptions
from seattrellis.application.room_templates import (
    RoomTemplate,
    build_standard_room,
    list_room_templates,
    recommend_room_template,
)
from seattrellis.application.roster_import import ImportedRoster
from seattrellis.application.teacher_goals import (
    TeacherGoalDefinition,
    list_teacher_goals,
)
from seattrellis.models.layout import ClassroomLayout
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.service_types import ExportRequest, PageOptions, PrivacyOptions
from seattrellis.web.class_adapter import (
    build_class_draft,
    generate_class_setup,
    import_uploaded_roster,
    inspect_class_setup,
)
from seattrellis.web.components import (
    build_candidate_selector,
    build_seat_grid_html,
)
from seattrellis.web.i18n import normalize_locale, translate
from seattrellis.web.keys import (
    TEACHER_CANDIDATE_SELECT,
    TEACHER_CLASS_NAME_INPUT,
    TEACHER_EXPORT_PREFIX,
    TEACHER_GENERATE_BUTTON,
    TEACHER_GOAL_SELECT,
    TEACHER_INTERNAL_EXPORT_DOWNLOAD,
    TEACHER_INTERNAL_EXPORT_PREPARE,
    TEACHER_PUBLIC_EXPORT_DOWNLOAD,
    TEACHER_PUBLIC_EXPORT_PREPARE,
    TEACHER_RESULTS_STATUS,
    TEACHER_ROOM_AISLES_INPUT,
    TEACHER_ROOM_ROWS_INPUT,
    TEACHER_ROOM_SEATS_PER_ROW_INPUT,
    TEACHER_ROOM_TEMPLATE_SELECT,
    TEACHER_ROSTER_STATUS,
    TEACHER_ROSTER_UPLOAD,
    TEACHER_SOLVE_STATUS,
    widget_region_key,
)
from seattrellis.web.teacher_state import build_teacher_workspace_state
from seattrellis.web.workflow import (
    WebSolveResult,
    export_for_web,
    selected_snapshot,
)

TeacherPrintTemplate = Literal["public", "teacher"]
RosterImporter = Callable[[str, bytes], ImportedRoster]
PrintExporter = Callable[..., Path]


_ROSTER_CACHE_KEY = "_teacher_roster_cache"
_SETUP_SIGNATURE_KEY = "_teacher_setup_signature"
_RESULT_KEY = "_teacher_result"
_OUTPUT_DIR_KEY = "_teacher_output_dir"


@dataclass(frozen=True, slots=True)
class CachedRosterUpload:
    """Parsed roster cache that deliberately retains no uploaded file bytes."""

    fingerprint: str
    roster: ImportedRoster | None
    error_message: str | None = None

    @property
    def ready(self) -> bool:
        return self.roster is not None and self.error_message is None


@dataclass(frozen=True, slots=True)
class TeacherSetupSignature:
    """Inputs that determine whether an existing plan is still applicable."""

    class_name: str
    roster_fingerprint: str | None
    room_template_id: str | None
    goal_id: str | None


@dataclass(frozen=True, slots=True)
class PreparedTeacherExport:
    """Download payload created only after the teacher requests an export."""

    signature: tuple[str, str, str, str]
    data: bytes
    file_name: str
    mime: str = "text/html"


def roster_upload_fingerprint(filename: str, content: bytes) -> str:
    """Return a stable digest without retaining sensitive roster bytes."""

    if not isinstance(filename, str):
        raise TypeError("filename must be a string")
    if not isinstance(content, bytes):
        raise TypeError("content must be bytes")
    digest = hashlib.sha256()
    digest.update(filename.strip().encode("utf-8", errors="surrogatepass"))
    digest.update(b"\0")
    digest.update(content)
    return digest.hexdigest()


def load_cached_roster_upload(
    filename: str,
    content: bytes,
    cached: CachedRosterUpload | None = None,
    *,
    importer: RosterImporter = import_uploaded_roster,
) -> tuple[CachedRosterUpload, bool]:
    """Parse a changed upload once and reuse its summary on later reruns.

    The boolean result reports whether the browser content changed.  Failed
    imports are cached too, preventing the same malformed workbook from being
    reparsed on every Streamlit interaction.
    """

    fingerprint = roster_upload_fingerprint(filename, content)
    if cached is not None and cached.fingerprint == fingerprint:
        return cached, False

    try:
        roster = importer(filename, content)
    except Exception as exc:
        message = str(exc).strip() or exc.__class__.__name__
        return CachedRosterUpload(fingerprint, None, message), True
    return CachedRosterUpload(fingerprint, roster), True


def build_teacher_setup_signature(
    *,
    class_name: str,
    roster_fingerprint: str | None,
    room_template_id: str | None,
    goal_id: str | None,
) -> TeacherSetupSignature:
    """Normalize the inputs used to invalidate a generated teacher plan."""

    return TeacherSetupSignature(
        class_name=str(class_name).strip(),
        roster_fingerprint=roster_fingerprint,
        room_template_id=room_template_id,
        goal_id=goal_id,
    )


def invalidate_teacher_results(
    session_state: MutableMapping[str, object],
    signature: TeacherSetupSignature,
) -> bool:
    """Discard teacher-derived state when any setup input has changed.

    Roster caches and unrelated Quick Solve or Project state are intentionally
    preserved.  This keeps the teacher workflow isolated while preventing an
    old seat map or privacy export from being shown for a changed class.
    """

    previous = session_state.get(_SETUP_SIGNATURE_KEY)
    session_state[_SETUP_SIGNATURE_KEY] = signature
    if previous is None or previous == signature:
        return False

    for key in (
        _RESULT_KEY,
        _OUTPUT_DIR_KEY,
        TEACHER_CANDIDATE_SELECT,
        prepared_export_state_key("public"),
        prepared_export_state_key("teacher"),
    ):
        session_state.pop(key, None)
    return True


def prepared_export_state_key(template: TeacherPrintTemplate) -> str:
    """Return the private session key for one prepared print template."""

    normalized = _normalize_print_template(template)
    return f"_{TEACHER_EXPORT_PREFIX}_{normalized}_prepared"


def teacher_export_signature(
    result: WebSolveResult,
    *,
    candidate_id: str,
    template: TeacherPrintTemplate,
    locale: str,
) -> tuple[str, str, str, str]:
    """Describe the exact result selection represented by an export."""

    return (
        str(result.artifact_path),
        candidate_id,
        _normalize_print_template(template),
        normalize_locale(locale),
    )


def prepare_teacher_print_export(
    result: WebSolveResult,
    *,
    output_dir: str | Path,
    candidate_id: str,
    template: TeacherPrintTemplate,
    class_name: str,
    locale: str,
    exporter: PrintExporter = export_for_web,
) -> PreparedTeacherExport:
    """Create one privacy-aware Print HTML payload for an explicit download."""

    normalized_template = _normalize_print_template(template)
    normalized_locale = normalize_locale(locale)
    request = ExportRequest(
        output_format="print-html",
        template=normalized_template,
        privacy=PrivacyOptions.for_template(normalized_template),
        page=PageOptions(orientation="landscape"),
        locale=normalized_locale,
        candidate_id=candidate_id if result.is_candidate_set else None,
    )
    export_path = exporter(
        result,
        output_format="print-html",
        output_dir=Path(output_dir) / f"print-{normalized_template}",
        candidate_id=candidate_id,
        request=request,
    )
    return PreparedTeacherExport(
        signature=teacher_export_signature(
            result,
            candidate_id=candidate_id,
            template=normalized_template,
            locale=normalized_locale,
        ),
        data=Path(export_path).read_bytes(),
        file_name=teacher_export_filename(class_name, normalized_template),
    )


def teacher_export_filename(
    class_name: str,
    template: TeacherPrintTemplate,
) -> str:
    """Build a portable and recognizable file name for a prepared plan."""

    normalized_template = _normalize_print_template(template)
    safe_name = re.sub(r'[<>:"/\\|?*\x00-\x1f]+', "-", str(class_name).strip())
    safe_name = re.sub(r"\s+", "-", safe_name).strip(" .-")[:80]
    safe_name = re.sub(r"-+", "-", safe_name)
    if not safe_name:
        safe_name = "classroom"
    suffix = "public" if normalized_template == "public" else "teacher"
    return f"{safe_name}-{suffix}.html"


def render_teacher_page(
    *,
    make_persistent_tempdir: Callable[[], str],
    locale: str,
) -> None:
    """Render the default roster-to-print workflow for ordinary teachers."""

    st = _load_streamlit()
    resolved_locale = normalize_locale(locale)
    text = lambda key, **values: translate(  # noqa: E731 - concise local adapter.
        key,
        resolved_locale,
        **values,
    )

    st.header(text("teacher_home_title"))
    st.caption(text("teacher_home_caption"))

    class_name = st.text_input(
        text("teacher_class_name"),
        placeholder=text("teacher_class_name_placeholder"),
        key=TEACHER_CLASS_NAME_INPUT,
    )
    st.subheader(text("teacher_roster_title"))
    uploaded_file = st.file_uploader(
        text("teacher_roster_upload"),
        type=["csv", "xlsx", "xlsm"],
        help=text("teacher_roster_help"),
        key=TEACHER_ROSTER_UPLOAD,
    )

    roster_cache, roster_changed = _resolve_uploaded_roster(st, uploaded_file)
    roster = roster_cache.roster if roster_cache is not None else None
    if roster_cache is not None and roster_cache.error_message is not None:
        with st.container(key=widget_region_key(TEACHER_ROSTER_STATUS)):
            st.error(text("teacher_error_detail", error=roster_cache.error_message))
    elif roster is not None:
        _render_roster_summary(st, roster, text)

    room_template = _render_room_choice(
        st,
        roster,
        roster_changed=roster_changed,
        text=text,
    )
    goal = _render_goal_choice(
        st,
        roster if room_template is not None else None,
        text=text,
    )

    signature = build_teacher_setup_signature(
        class_name=class_name,
        roster_fingerprint=(roster_cache.fingerprint if roster_cache else None),
        room_template_id=(_room_identity(room_template) if room_template else None),
        goal_id=(goal.goal_id if goal else None),
    )
    invalidate_teacher_results(st.session_state, signature)

    draft = None
    readiness = None
    if (
        roster is not None
        and room_template is not None
        and goal is not None
        and class_name.strip()
    ):
        try:
            draft = build_class_draft(
                class_name=class_name,
                roster=roster,
                room_template=room_template,
                goal_id=goal.goal_id,
            )
            readiness = inspect_class_setup(draft)
        except Exception as exc:
            st.error(text("teacher_error_detail", error=exc))

    result = _stored_result(st.session_state)
    workspace = build_teacher_workspace_state(
        has_roster=roster is not None,
        has_room=room_template is not None,
        has_goal=goal is not None,
        has_plan=result is not None,
        locale=resolved_locale,
    )
    st.caption(workspace.primary_action.label)

    if readiness is not None:
        for error in readiness.validation.errors:
            st.error(text("teacher_error_detail", error=error))

    with st.container(key=widget_region_key(TEACHER_GENERATE_BUTTON)):
        generate = st.button(
            text("teacher_generate"),
            key=TEACHER_GENERATE_BUTTON,
            type="primary",
            disabled=draft is None or readiness is None or not readiness.ready,
            use_container_width=True,
        )
    if generate and draft is not None:
        with st.container(key=widget_region_key(TEACHER_SOLVE_STATUS)):
            try:
                output_dir = Path(make_persistent_tempdir())
                with st.spinner(text("teacher_generating")):
                    result = generate_class_setup(
                        draft,
                        output_dir=output_dir,
                        options=GenerateOptions(candidate_count=3),
                    )
            except Exception as exc:
                st.error(text("teacher_generate_failed", error=exc))
            else:
                st.session_state[_RESULT_KEY] = result
                st.session_state[_OUTPUT_DIR_KEY] = str(output_dir)
                st.session_state.pop(TEACHER_CANDIDATE_SELECT, None)
                _clear_prepared_exports(st.session_state)
                count = (
                    len(result.artifact.candidates) if result.is_candidate_set else 1
                )
                st.success(text("teacher_generate_success", count=count))

    result = _stored_result(st.session_state)
    output_dir_value = st.session_state.get(_OUTPUT_DIR_KEY)
    if result is not None and isinstance(output_dir_value, str):
        _render_teacher_result(
            st,
            result,
            output_dir=Path(output_dir_value),
            class_name=class_name,
            locale=resolved_locale,
            text=text,
        )


def _resolve_uploaded_roster(
    st: Any,
    uploaded_file: Any,
) -> tuple[CachedRosterUpload | None, bool]:
    cached = st.session_state.get(_ROSTER_CACHE_KEY)
    if not isinstance(cached, CachedRosterUpload):
        cached = None
    if uploaded_file is None:
        # Streamlit may temporarily report no uploader value after the teacher
        # visits another workspace.  Retain the parsed summary so returning to
        # this page does not discard or reparse the roster.
        return cached, False

    resolved, changed = load_cached_roster_upload(
        uploaded_file.name,
        uploaded_file.getvalue(),
        cached,
    )
    st.session_state[_ROSTER_CACHE_KEY] = resolved
    return resolved, changed


def _render_roster_summary(
    st: Any, roster: ImportedRoster, text: Callable[..., str]
) -> None:
    summary = roster.summary
    with st.container(key=widget_region_key(TEACHER_ROSTER_STATUS)):
        st.success(
            text(
                "teacher_roster_ready",
                name=roster.source_name or "—",
                count=summary.student_count,
            )
        )
        st.caption(
            text(
                "teacher_roster_summary",
                students=summary.student_count,
                scores=summary.score_count,
                heights=summary.height_count,
                front_needs=summary.vision_or_front_need_count,
                special_needs=summary.special_needs_count,
            )
        )
        if summary.name_only_count:
            st.caption(text("teacher_roster_name_only", count=summary.name_only_count))


def _render_room_choice(
    st: Any,
    roster: ImportedRoster | None,
    *,
    roster_changed: bool,
    text: Callable[..., str],
) -> RoomTemplate | ClassroomLayout | None:
    if roster is None:
        return None

    st.subheader(text("teacher_room_title"))
    templates = list_room_templates()
    recommendation = recommend_room_template(roster.summary.student_count)
    template_by_id = {template.template_id: template for template in templates}
    selection_ids = [*template_by_id, "custom"]
    recommended_id = recommendation.template_id if recommendation else "custom"
    if (
        roster_changed
        or st.session_state.get(TEACHER_ROOM_TEMPLATE_SELECT) not in selection_ids
    ):
        st.session_state[TEACHER_ROOM_TEMPLATE_SELECT] = recommended_id

    with st.container(key=widget_region_key(TEACHER_ROOM_TEMPLATE_SELECT)):
        selected_id = st.selectbox(
            text("teacher_room_template"),
            selection_ids,
            format_func=lambda item: (
                text("teacher_room_custom")
                if item == "custom"
                else _room_option_label(template_by_id[item])
            ),
            key=TEACHER_ROOM_TEMPLATE_SELECT,
        )
    if selected_id == "custom":
        return _render_custom_room(st, roster, text=text)

    selected = template_by_id[selected_id]
    if (
        recommendation is not None
        and selected.template_id == recommendation.template_id
    ):
        st.caption(text("teacher_room_recommended", capacity=selected.capacity))
    st.caption(
        text(
            "teacher_room_summary",
            capacity=selected.capacity,
            rows=selected.rows,
            seats_per_row=selected.seats_per_row,
        )
    )
    return selected


def _render_custom_room(
    st: Any,
    roster: ImportedRoster,
    *,
    text: Callable[..., str],
) -> ClassroomLayout | None:
    """Render a small custom-room form without exposing layout files."""

    student_count = roster.summary.student_count
    default_rows = min(20, max(1, (student_count + 7) // 8))
    columns = st.columns(2)
    rows = int(
        columns[0].number_input(
            text("teacher_room_rows"),
            min_value=1,
            max_value=20,
            value=default_rows,
            step=1,
            key=TEACHER_ROOM_ROWS_INPUT,
        )
    )
    seats_per_row = int(
        columns[1].number_input(
            text("teacher_room_seats_per_row"),
            min_value=1,
            max_value=20,
            value=8,
            step=1,
            key=TEACHER_ROOM_SEATS_PER_ROW_INPUT,
        )
    )
    aisle_options = list(range(1, seats_per_row))
    stored_aisles = st.session_state.get(TEACHER_ROOM_AISLES_INPUT, [])
    if not isinstance(stored_aisles, list):
        stored_aisles = []
    valid_aisles = [item for item in stored_aisles if item in aisle_options]
    if valid_aisles != stored_aisles:
        st.session_state[TEACHER_ROOM_AISLES_INPUT] = valid_aisles
    aisle_defaults: dict[str, Any] = {}
    if TEACHER_ROOM_AISLES_INPUT not in st.session_state:
        aisle_defaults["default"] = _central_aisle(seats_per_row)
    aisles = st.multiselect(
        text("teacher_room_aisles"),
        aisle_options,
        format_func=lambda position: text(
            "teacher_room_aisle_after",
            position=position,
        ),
        key=TEACHER_ROOM_AISLES_INPUT,
        **aisle_defaults,
    )
    capacity = rows * seats_per_row
    if capacity < student_count:
        st.error(
            text(
                "teacher_room_capacity_short",
                capacity=capacity,
                count=student_count,
            )
        )
        return None
    st.caption(
        text(
            "teacher_room_summary",
            capacity=capacity,
            rows=rows,
            seats_per_row=seats_per_row,
        )
    )
    aisle_id = "-".join(str(position) for position in aisles) or "none"
    return build_standard_room(
        rows,
        seats_per_row,
        aisles_after=tuple(aisles),
        layout_id=f"custom-{rows}x{seats_per_row}-aisles-{aisle_id}",
        name="Custom classroom",
    )


def _render_goal_choice(
    st: Any,
    roster: ImportedRoster | None,
    *,
    text: Callable[..., str],
) -> TeacherGoalDefinition | None:
    if roster is None:
        return None

    st.subheader(text("teacher_goal_title"))
    st.caption(text("teacher_goal_help"))
    goals = tuple(goal for goal in list_teacher_goals() if goal.goal_id != "custom")
    goal_by_id = {goal.goal_id: goal for goal in goals}
    labels = {
        "daily-rotation": text("teacher_goal_daily_title"),
        "fair-shuffle": text("teacher_goal_fair_title"),
        "peer-support": text("teacher_goal_peer_title"),
    }
    descriptions = {
        "daily-rotation": text("teacher_goal_daily_description"),
        "fair-shuffle": text("teacher_goal_fair_description"),
        "peer-support": text("teacher_goal_peer_description"),
    }
    with st.container(key=widget_region_key(TEACHER_GOAL_SELECT)):
        selected_id = st.radio(
            text("teacher_goal_title"),
            list(goal_by_id),
            format_func=labels.__getitem__,
            key=TEACHER_GOAL_SELECT,
            label_visibility="collapsed",
        )
    st.caption(descriptions[selected_id])
    return goal_by_id[selected_id]


def _render_teacher_result(
    st: Any,
    result: WebSolveResult,
    *,
    output_dir: Path,
    class_name: str,
    locale: str,
    text: Callable[..., str],
) -> None:
    with st.container(key=widget_region_key(TEACHER_RESULTS_STATUS)):
        st.subheader(text("teacher_results_title"))
        count = len(result.artifact.candidates) if result.is_candidate_set else 1
        st.success(text("teacher_results_summary", count=count))

        selected_id = "recommended"
        if result.is_candidate_set:
            options = build_candidate_selector(result.artifact, locale=locale)
            option_ids = [str(option["id"]) for option in options]
            labels = {str(option["id"]): str(option["label"]) for option in options}
            stored = st.session_state.get(TEACHER_CANDIDATE_SELECT, "recommended")
            if stored not in option_ids:
                st.session_state[TEACHER_CANDIDATE_SELECT] = "recommended"
            selected_id = str(
                st.session_state.get(TEACHER_CANDIDATE_SELECT, "recommended")
            )

        snapshot = selected_snapshot(result, selected_id)
        st.markdown(
            build_seat_grid_html(snapshot.layout, snapshot, locale=locale),
            unsafe_allow_html=True,
        )

        if result.is_candidate_set and len(result.artifact.candidates) > 1:
            with st.expander(text("teacher_other_candidates"), expanded=False):
                selected_id = st.selectbox(
                    text("teacher_candidate_choice"),
                    option_ids,
                    format_func=labels.__getitem__,
                    key=TEACHER_CANDIDATE_SELECT,
                )

        _render_teacher_exports(
            st,
            result,
            output_dir=output_dir,
            candidate_id=selected_id,
            class_name=class_name,
            locale=locale,
            text=text,
        )


def _render_teacher_exports(
    st: Any,
    result: WebSolveResult,
    *,
    output_dir: Path,
    candidate_id: str,
    class_name: str,
    locale: str,
    text: Callable[..., str],
) -> None:
    st.subheader(text("teacher_export_title"))
    columns = st.columns(2)
    configurations = (
        (
            columns[0],
            "public",
            "teacher_public_print",
            "teacher_public_print_help",
            TEACHER_PUBLIC_EXPORT_PREPARE,
            TEACHER_PUBLIC_EXPORT_DOWNLOAD,
        ),
        (
            columns[1],
            "teacher",
            "teacher_internal_print",
            "teacher_internal_print_help",
            TEACHER_INTERNAL_EXPORT_PREPARE,
            TEACHER_INTERNAL_EXPORT_DOWNLOAD,
        ),
    )
    for (
        column,
        template,
        title_key,
        help_key,
        prepare_key,
        download_key,
    ) in configurations:
        with column:
            st.markdown(f"**{text(title_key)}**")
            st.caption(text(help_key))
            state_key = prepared_export_state_key(template)  # type: ignore[arg-type]
            signature = teacher_export_signature(
                result,
                candidate_id=candidate_id,
                template=template,  # type: ignore[arg-type]
                locale=locale,
            )
            with st.container(key=widget_region_key(prepare_key)):
                prepare = st.button(
                    text(title_key),
                    key=prepare_key,
                    use_container_width=True,
                )
            if prepare:
                st.session_state.pop(state_key, None)
                try:
                    prepared = prepare_teacher_print_export(
                        result,
                        output_dir=output_dir,
                        candidate_id=candidate_id,
                        template=template,  # type: ignore[arg-type]
                        class_name=class_name,
                        locale=locale,
                    )
                except MissingOptionalDependencyError as exc:
                    st.info(str(exc))
                except Exception as exc:
                    st.error(text("teacher_error_detail", error=exc))
                else:
                    st.session_state[state_key] = prepared

            prepared = st.session_state.get(state_key)
            if (
                isinstance(prepared, PreparedTeacherExport)
                and prepared.signature == signature
            ):
                st.success(text("teacher_export_ready", label=text(title_key)))
                with st.container(key=widget_region_key(download_key)):
                    st.download_button(
                        text(title_key),
                        data=prepared.data,
                        file_name=prepared.file_name,
                        mime=prepared.mime,
                        key=download_key,
                        on_click="ignore",
                        use_container_width=True,
                    )


def _stored_result(session_state: MutableMapping[str, object]) -> WebSolveResult | None:
    result = session_state.get(_RESULT_KEY)
    if isinstance(result, WebSolveResult):
        return result
    session_state.pop(_RESULT_KEY, None)
    return None


def _clear_prepared_exports(session_state: MutableMapping[str, object]) -> None:
    session_state.pop(prepared_export_state_key("public"), None)
    session_state.pop(prepared_export_state_key("teacher"), None)


def _room_option_label(template: RoomTemplate) -> str:
    return f"{template.capacity} · {template.rows} × {template.seats_per_row}"


def _room_identity(room: RoomTemplate | ClassroomLayout) -> str:
    """Return a stable setup signature for a built-in or custom room."""

    if isinstance(room, RoomTemplate):
        return room.template_id
    return room.layout_id


def _central_aisle(seats_per_row: int) -> list[int]:
    """Suggest one central aisle when a row has room on both sides."""

    return [seats_per_row // 2] if seats_per_row >= 4 else []


def _normalize_print_template(template: str) -> TeacherPrintTemplate:
    normalized = str(template).strip().lower()
    if normalized not in {"public", "teacher"}:
        raise ValueError("Teacher print template must be 'public' or 'teacher'.")
    return normalized  # type: ignore[return-value]


def _load_streamlit() -> Any:
    try:
        import streamlit as st
    except Exception as exc:  # pragma: no cover - depends on optional install.
        raise MissingOptionalDependencyError("Streamlit web UI", "web") from exc
    return st
