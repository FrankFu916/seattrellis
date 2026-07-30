"""Teacher-facing seating goals translated into solver rules.

The Web and desktop interfaces can use these small application objects without
exposing preset names or individual solver weights to teachers.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal, Sequence

from seattrellis.models.rules import RuleSet
from seattrellis.models.student import Student
from seattrellis.presets import get_preset, preset_context_warnings

TeacherGoalId = Literal[
    "daily-rotation",
    "fair-shuffle",
    "peer-support",
    "custom",
]


@dataclass(frozen=True)
class TeacherGoalDefinition:
    """Stable product copy and defaults for one teacher-facing goal."""

    goal_id: TeacherGoalId
    preset_name: str | None
    title: str
    description: str
    default_candidate_count: int = 3


@dataclass(frozen=True)
class TeacherGoalSelection:
    """The goal selected by a teacher before class data is evaluated."""

    goal_id: TeacherGoalId = "daily-rotation"
    custom_rules: RuleSet | None = None


@dataclass(frozen=True)
class ResolvedTeacherGoal:
    """A goal resolved to an independent ruleset and contextual guidance."""

    definition: TeacherGoalDefinition
    rules: RuleSet
    preset_name: str | None
    warnings: tuple[str, ...]


_TEACHER_GOALS = (
    TeacherGoalDefinition(
        goal_id="daily-rotation",
        preset_name="daily",
        title="Daily rotation",
        description=(
            "Balance accessibility, seat rotation, recent-neighbor variety, "
            "height, and peer mixing for routine classroom use."
        ),
    ),
    TeacherGoalDefinition(
        goal_id="fair-shuffle",
        preset_name="fair-rotation",
        title="Fair shuffle",
        description="Use saved seating history to vary where students sit over time.",
    ),
    TeacherGoalDefinition(
        goal_id="peer-support",
        preset_name="balanced",
        title="Peer support",
        description="Mix different score levels across neighboring seats.",
    ),
    TeacherGoalDefinition(
        goal_id="custom",
        preset_name=None,
        title="Custom rules",
        description="Use a ruleset prepared in the advanced tools.",
    ),
)

_TEACHER_GOAL_BY_ID = {goal.goal_id: goal for goal in _TEACHER_GOALS}


def list_teacher_goals() -> tuple[TeacherGoalDefinition, ...]:
    """Return teacher-facing goals in their recommended display order."""

    return _TEACHER_GOALS


def get_teacher_goal(goal_id: str) -> TeacherGoalDefinition:
    """Look up a teacher goal, accepting underscores for configuration files."""

    normalized = goal_id.strip().lower().replace("_", "-")
    try:
        return _TEACHER_GOAL_BY_ID[normalized]  # type: ignore[index]
    except KeyError as exc:
        available = ", ".join(goal.goal_id for goal in _TEACHER_GOALS)
        raise ValueError(
            f"Unknown teacher goal {goal_id!r}. Available goals: {available}."
        ) from exc


def resolve_teacher_goal(
    selection: TeacherGoalSelection,
    *,
    students: Sequence[Student],
    history_count: int = 0,
) -> ResolvedTeacherGoal:
    """Resolve a selection to rules and warn when preferred data is unavailable."""

    if history_count < 0:
        raise ValueError("history_count must be non-negative.")

    definition = get_teacher_goal(selection.goal_id)
    if definition.goal_id == "custom":
        if selection.custom_rules is None:
            raise ValueError("The custom teacher goal requires custom_rules.")
        return ResolvedTeacherGoal(
            definition=definition,
            rules=_copy_rules(selection.custom_rules),
            preset_name=None,
            warnings=(),
        )

    if selection.custom_rules is not None:
        raise ValueError("custom_rules can only be used with the custom teacher goal.")

    if definition.preset_name is None:  # pragma: no cover - guarded by definitions.
        raise RuntimeError(f"Teacher goal {definition.goal_id!r} has no preset.")
    preset = get_preset(definition.preset_name)
    rules = _copy_rules(preset.rules)
    warnings = preset_context_warnings(
        preset,
        students,
        history_count=history_count,
        rules=rules,
    )
    return ResolvedTeacherGoal(
        definition=definition,
        rules=rules,
        preset_name=preset.name,
        warnings=tuple(warnings),
    )


def _copy_rules(rules: RuleSet) -> RuleSet:
    if hasattr(rules, "model_copy"):
        return rules.model_copy(deep=True)  # type: ignore[attr-defined,no-any-return]
    return rules.copy(deep=True)
