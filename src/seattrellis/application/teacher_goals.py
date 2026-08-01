"""Teacher-facing seating goals translated into solver rules.

The Web and desktop interfaces can use these small application objects without
exposing preset names or individual solver weights to teachers.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from typing import Any, Literal, Mapping, Sequence

from seattrellis.models.rules import HardRules, RuleSet
from seattrellis.models.student import Student
from seattrellis.presets import get_preset, preset_context_warnings

TeacherGoalId = Literal[
    "daily-rotation",
    "quick-shuffle",
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
    hard_rules: HardRules | None = None
    rules_overlay: Mapping[str, Any] | None = None


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
        goal_id="quick-shuffle",
        preset_name="random",
        title="Quick shuffle",
        description=(
            "Create a neutral shuffle without relying on scores or saved history."
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
        rules = _copy_rules(selection.custom_rules)
        rules = _apply_rules_overlay(rules, selection.rules_overlay)
        rules = _append_hard_rules(rules, selection.hard_rules)
        return ResolvedTeacherGoal(
            definition=definition,
            rules=rules,
            preset_name=None,
            warnings=(),
        )

    if selection.custom_rules is not None:
        raise ValueError("custom_rules can only be used with the custom teacher goal.")

    if definition.preset_name is None:  # pragma: no cover - guarded by definitions.
        raise RuntimeError(f"Teacher goal {definition.goal_id!r} has no preset.")
    preset = get_preset(definition.preset_name)
    rules = _copy_rules(preset.rules)
    rules = _apply_rules_overlay(rules, selection.rules_overlay)
    rules = _append_hard_rules(rules, selection.hard_rules)
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
    return rules.model_copy(deep=True)


def _append_hard_rules(rules: RuleSet, extra: HardRules | None) -> RuleSet:
    """Add teacher-entered hard constraints without replacing preset rules."""

    if extra is None:
        return rules
    rules.hard.fixed_seats.extend(deepcopy(extra.fixed_seats))
    rules.hard.must_be_adjacent.extend(deepcopy(extra.must_be_adjacent))
    rules.hard.cannot_be_adjacent.extend(deepcopy(extra.cannot_be_adjacent))
    rules.hard.min_distance.extend(deepcopy(extra.min_distance))
    return rules


def _apply_rules_overlay(
    rules: RuleSet,
    overlay: Mapping[str, Any] | None,
) -> RuleSet:
    """Merge a partial JSON overlay into a resolved preset and revalidate it."""

    if overlay is None:
        return rules
    if not isinstance(overlay, Mapping):
        raise ValueError("rules_overlay must be an object.")

    data = rules.model_dump(mode="json")
    _deep_merge(data, overlay)
    return RuleSet.model_validate(data)


def _deep_merge(target: dict[str, Any], patch: Mapping[str, Any]) -> None:
    for key, value in patch.items():
        if isinstance(value, Mapping) and isinstance(target.get(key), dict):
            _deep_merge(target[key], value)
        else:
            target[key] = deepcopy(value)
