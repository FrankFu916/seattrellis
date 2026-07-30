"""Pure view models for the teacher-facing classroom workflow.

The Streamlit adapter should derive this projection from the application
objects it already owns on every rerun.  Keeping student, room, and plan data
out of this module avoids a second source of truth and makes the same action
identifiers suitable for a future browser or desktop client.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Literal

TeacherLocale = Literal["zh", "en"]


class TeacherWorkspaceStage(str, Enum):
    """Ordered milestones in the default classroom planning workflow."""

    EMPTY = "empty"
    ROSTER_READY = "roster-ready"
    ROOM_READY = "room-ready"
    READY = "ready"
    PLAN_READY = "plan-ready"
    EDITING = "editing"
    SAVED = "saved"


class TeacherPrimaryActionId(str, Enum):
    """Stable commands that user-interface adapters can handle."""

    IMPORT_ROSTER = "import-roster"
    CONFIGURE_ROOM = "configure-room"
    CHOOSE_GOAL = "choose-goal"
    GENERATE_PLAN = "generate-plan"
    ADJUST_PLAN = "adjust-plan"
    SAVE_PLAN = "save-plan"
    EXPORT_PLAN = "export-plan"


@dataclass(frozen=True, slots=True)
class TeacherPrimaryAction:
    """One prominent next action for the current workflow stage."""

    action_id: TeacherPrimaryActionId
    label: str


@dataclass(frozen=True, slots=True)
class TeacherWorkspaceState:
    """Localized, read-only projection of the teacher workflow.

    This object deliberately contains no roster, layout, rules, plan, or edit
    session.  Those remain in their existing application and domain models.
    """

    stage: TeacherWorkspaceStage
    primary_action: TeacherPrimaryAction
    recovery_prompt: str | None


_ACTION_FOR_STAGE: dict[TeacherWorkspaceStage, TeacherPrimaryActionId] = {
    TeacherWorkspaceStage.EMPTY: TeacherPrimaryActionId.IMPORT_ROSTER,
    TeacherWorkspaceStage.ROSTER_READY: TeacherPrimaryActionId.CONFIGURE_ROOM,
    TeacherWorkspaceStage.ROOM_READY: TeacherPrimaryActionId.CHOOSE_GOAL,
    TeacherWorkspaceStage.READY: TeacherPrimaryActionId.GENERATE_PLAN,
    TeacherWorkspaceStage.PLAN_READY: TeacherPrimaryActionId.ADJUST_PLAN,
    TeacherWorkspaceStage.EDITING: TeacherPrimaryActionId.SAVE_PLAN,
    TeacherWorkspaceStage.SAVED: TeacherPrimaryActionId.EXPORT_PLAN,
}

_ACTION_COPY: dict[TeacherPrimaryActionId, tuple[str, str]] = {
    TeacherPrimaryActionId.IMPORT_ROSTER: ("导入学生名单", "Import student list"),
    TeacherPrimaryActionId.CONFIGURE_ROOM: ("设置教室", "Set up classroom"),
    TeacherPrimaryActionId.CHOOSE_GOAL: ("选择排座目标", "Choose seating goal"),
    TeacherPrimaryActionId.GENERATE_PLAN: ("生成座位表", "Generate seating plan"),
    TeacherPrimaryActionId.ADJUST_PLAN: ("调整座位", "Adjust seating"),
    TeacherPrimaryActionId.SAVE_PLAN: ("保存座位表", "Save seating plan"),
    TeacherPrimaryActionId.EXPORT_PLAN: ("打印或导出", "Print or export"),
}

_RECOVERY_COPY: dict[TeacherWorkspaceStage, tuple[str, str] | None] = {
    TeacherWorkspaceStage.EMPTY: None,
    TeacherWorkspaceStage.ROSTER_READY: (
        "已恢复学生名单。接下来设置教室。",
        "Your student list was restored. Set up the classroom next.",
    ),
    TeacherWorkspaceStage.ROOM_READY: (
        "已恢复学生名单和教室。接下来选择排座目标。",
        "Your student list and classroom were restored. Choose a seating goal next.",
    ),
    TeacherWorkspaceStage.READY: (
        "已恢复班级设置。可以继续生成座位表。",
        "Your class setup was restored. You can generate a seating plan.",
    ),
    TeacherWorkspaceStage.PLAN_READY: (
        "已恢复生成的座位表。可以继续调整。",
        "Your generated seating plan was restored. You can continue adjusting it.",
    ),
    TeacherWorkspaceStage.EDITING: (
        "已恢复尚未保存的调整。请检查后保存。",
        "Your unsaved adjustments were restored. Review and save them when ready.",
    ),
    TeacherWorkspaceStage.SAVED: (
        "已恢复已保存的座位表。可以打印或导出。",
        "Your saved seating plan was restored. You can print or export it.",
    ),
}


def build_teacher_workspace_state(
    *,
    has_roster: bool = False,
    has_room: bool = False,
    has_goal: bool = False,
    has_plan: bool = False,
    is_editing: bool = False,
    is_saved: bool = False,
    locale: str = "zh",
) -> TeacherWorkspaceState:
    """Derive the localized view state from canonical application data.

    Callers pass only availability facts and keep the actual objects in the
    application layer.  Invalid combinations fail early instead of presenting
    a misleading next action after a partial session restore.
    """

    facts = {
        "has_roster": has_roster,
        "has_room": has_room,
        "has_goal": has_goal,
        "has_plan": has_plan,
        "is_editing": is_editing,
        "is_saved": is_saved,
    }
    for name, value in facts.items():
        if not isinstance(value, bool):
            raise TypeError(f"{name} must be a boolean")

    stage = resolve_teacher_workspace_stage(
        has_roster=has_roster,
        has_room=has_room,
        has_goal=has_goal,
        has_plan=has_plan,
        is_editing=is_editing,
        is_saved=is_saved,
    )
    return TeacherWorkspaceState(
        stage=stage,
        primary_action=primary_action_for_stage(stage, locale=locale),
        recovery_prompt=recovery_prompt_for_stage(stage, locale=locale),
    )


def resolve_teacher_workspace_stage(
    *,
    has_roster: bool,
    has_room: bool,
    has_goal: bool,
    has_plan: bool,
    is_editing: bool,
    is_saved: bool,
) -> TeacherWorkspaceStage:
    """Resolve one unambiguous stage from an ordered set of milestones."""

    if has_room and not has_roster:
        raise ValueError("has_room requires has_roster")
    if has_goal and not has_room:
        raise ValueError("has_goal requires has_room")
    if has_plan and not has_goal:
        raise ValueError("has_plan requires has_goal")
    if is_editing and not has_plan:
        raise ValueError("is_editing requires has_plan")
    if is_saved and not has_plan:
        raise ValueError("is_saved requires has_plan")
    if is_editing and is_saved:
        raise ValueError("is_editing and is_saved cannot both be true")

    if is_saved:
        return TeacherWorkspaceStage.SAVED
    if is_editing:
        return TeacherWorkspaceStage.EDITING
    if has_plan:
        return TeacherWorkspaceStage.PLAN_READY
    if has_goal:
        return TeacherWorkspaceStage.READY
    if has_room:
        return TeacherWorkspaceStage.ROOM_READY
    if has_roster:
        return TeacherWorkspaceStage.ROSTER_READY
    return TeacherWorkspaceStage.EMPTY


def primary_action_for_stage(
    stage: TeacherWorkspaceStage | str,
    *,
    locale: str = "zh",
) -> TeacherPrimaryAction:
    """Return the single recommended action for ``stage``."""

    normalized_stage = _normalize_stage(stage)
    action_id = _ACTION_FOR_STAGE[normalized_stage]
    return TeacherPrimaryAction(
        action_id=action_id,
        label=_localized(_ACTION_COPY[action_id], locale),
    )


def recovery_prompt_for_stage(
    stage: TeacherWorkspaceStage | str,
    *,
    locale: str = "zh",
) -> str | None:
    """Explain what was restored and the next useful step."""

    copy = _RECOVERY_COPY[_normalize_stage(stage)]
    return None if copy is None else _localized(copy, locale)


def _normalize_stage(stage: TeacherWorkspaceStage | str) -> TeacherWorkspaceStage:
    if isinstance(stage, TeacherWorkspaceStage):
        return stage
    try:
        return TeacherWorkspaceStage(str(stage).strip().lower().replace("_", "-"))
    except ValueError as exc:
        available = ", ".join(item.value for item in TeacherWorkspaceStage)
        raise ValueError(
            f"Unknown teacher workspace stage {stage!r}. Available stages: {available}."
        ) from exc


def _localized(copy: tuple[str, str], locale: str) -> str:
    normalized = str(locale).strip().lower().replace("_", "-")
    if normalized in {"zh", "zh-cn", "zh-hans"}:
        return copy[0]
    if normalized in {"en", "en-us", "en-gb"}:
        return copy[1]
    raise ValueError(
        f"Unsupported teacher workflow locale {locale!r}. Use 'zh' or 'en'."
    )
