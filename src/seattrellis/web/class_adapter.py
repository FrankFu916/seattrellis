"""Web boundary for the teacher-oriented classroom workflow.

The functions in this module do not import Streamlit.  They translate browser
uploads and simple form values into the application-layer objects while
retaining the existing Web result shape used by editing and export features.
"""

from __future__ import annotations

from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Sequence

from seattrellis.application.class_workflow import (
    ClassDraft,
    ClassReadiness,
    GenerateOptions,
    generate_class_plan,
    inspect_class,
)
from seattrellis.application.room_templates import (
    RoomTemplate,
    build_room_from_template,
    recommend_room_template,
)
from seattrellis.application.roster_import import ImportedRoster, import_roster
from seattrellis.application.teacher_goals import (
    TeacherGoalSelection,
    get_teacher_goal,
)
from seattrellis.io.json_files import InputFileError, write_json_model
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.web.workflow import WebSolveResult


SUPPORTED_ROSTER_SUFFIXES = frozenset({".csv", ".xlsx", ".xlsm"})
MAX_ROSTER_UPLOAD_BYTES = 20 * 1024 * 1024


def import_uploaded_roster(filename: str, content: bytes) -> ImportedRoster:
    """Import an uploaded CSV or modern Excel file through the established parser.

    Browser-provided names are used only for a safe display name and suffix.
    The bytes are written under a fixed name in a short-lived directory, so a
    path supplied by a browser cannot escape the temporary workspace.  Import
    exceptions keep their existing types and diagnostics, but internal paths
    are replaced before they reach the interface.
    """

    display_name, suffix = _safe_upload_name(filename)
    if not isinstance(content, bytes):
        raise TypeError("content must be bytes.")
    if not content:
        raise ValueError("The uploaded roster is empty.")
    if len(content) > MAX_ROSTER_UPLOAD_BYTES:
        raise ValueError("The uploaded roster is larger than 20 MB.")

    try:
        with TemporaryDirectory(prefix="seattrellis-roster-") as temporary_dir:
            upload_path = Path(temporary_dir) / f"roster{suffix}"
            upload_path.write_bytes(content)
            try:
                imported = import_roster(upload_path)
            except InputFileError as exc:
                # Preserve the established exception for the shared Web error
                # renderer, while keeping the private temporary path internal.
                exc.args = (
                    str(exc).replace(str(upload_path), display_name),
                )
                raise
    except OSError as exc:
        raise InputFileError(
            "Could not prepare the uploaded roster for import."
        ) from exc

    return replace(imported, source_name=display_name)


def build_class_draft(
    *,
    class_name: str,
    roster: ImportedRoster,
    room_template: str | int | RoomTemplate | ClassroomLayout | None = None,
    goal_id: str = "daily-rotation",
    history_snapshots: Sequence[SeatingSnapshot] = (),
) -> ClassDraft:
    """Build a class draft from teacher-facing room and goal choices.

    When no room is selected, the smallest built-in template that fits the
    roster is used.  Classes larger than the standard templates are left to
    the custom room tools rather than silently creating an unsuitable layout.
    """

    selected_room = room_template
    if selected_room is None:
        selected_room = recommend_room_template(roster.summary.student_count)
        if selected_room is None:
            raise ValueError(
                "No standard room can hold this class. Choose a custom room "
                "in the advanced tools."
            )

    layout = (
        selected_room
        if isinstance(selected_room, ClassroomLayout)
        else build_room_from_template(selected_room)
    )
    goal = get_teacher_goal(goal_id)
    return ClassDraft(
        name=class_name,
        students=roster.students,
        layout=layout,
        goal=TeacherGoalSelection(goal_id=goal.goal_id),
        history_snapshots=tuple(history_snapshots),
    )


def inspect_class_setup(draft: ClassDraft) -> ClassReadiness:
    """Inspect a teacher-facing class without starting the solver."""

    return inspect_class(draft)


def generate_class_setup(
    draft: ClassDraft,
    *,
    output_dir: str | Path,
    options: GenerateOptions | None = None,
) -> WebSolveResult:
    """Generate and persist a class plan in the existing Web result format.

    The caller owns ``output_dir`` and therefore controls how long result files
    survive Web reruns.  A candidate set is retained even for a single result,
    allowing the editing, repair, comparison, and export paths to share one
    representation.
    """

    solve_output = generate_class_plan(draft, options=options)
    output_root = Path(output_dir)
    artifact_path = write_json_model(
        solve_output.candidate_set,
        output_root / "seattrellis.candidates.json",
    )

    report = solve_output.plan_comparison_report
    report_path = (
        write_json_model(report, output_root / "seattrellis.plan-report.json")
        if report is not None
        else None
    )
    return WebSolveResult(
        artifact_path=artifact_path,
        artifact=solve_output.candidate_set,
        report_path=report_path,
        report=report,
        summary=solve_output.summary,
    )


def _safe_upload_name(filename: str) -> tuple[str, str]:
    if not isinstance(filename, str):
        raise TypeError("filename must be a string.")
    # Browser uploads may contain either POSIX or Windows separators.  Keep
    # only the final component and never use it as the temporary file path.
    display_name = filename.replace("\\", "/").rsplit("/", 1)[-1].strip()
    if (
        not display_name
        or display_name in {".", ".."}
        or any(ord(character) < 32 for character in display_name)
    ):
        raise ValueError("The uploaded roster must have a valid file name.")
    suffix = Path(display_name).suffix.lower()
    if suffix not in SUPPORTED_ROSTER_SUFFIXES:
        raise ValueError("The student roster must be a CSV, XLSX, or XLSM file.")
    return display_name, suffix
