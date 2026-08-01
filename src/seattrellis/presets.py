from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, Sequence

from seattrellis.io.json_files import InputFileError, parse_rules_data, read_json, write_json_model
from seattrellis.models.rules import (
    AvoidRecentNeighborsRule,
    FairRotationRule,
    HardRules,
    MentorPairingRule,
    RuleSet,
    ScoreDistributionRule,
    ScorePositionRule,
    SoftRules,
    WeightedRule,
    effective_neighbor_rule,
)
from seattrellis.models.student import Student

PresetRequirement = Literal["history", "score", "height", "vision"]


@dataclass(frozen=True)
class PresetDefinition:
    """A discoverable scenario template backed by a normal RuleSet."""

    name: str
    title: str
    description: str
    best_for: str
    rules: RuleSet
    requirements: tuple[PresetRequirement, ...] = ()

    def metadata(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "best_for": self.best_for,
            "requirements": list(self.requirements),
            "degradation": {
                requirement: _DEGRADATION_NOTES[requirement]
                for requirement in self.requirements
            },
        }


def _weighted(enabled: bool, weight: int) -> WeightedRule:
    return WeightedRule(enabled=enabled, weight=weight)


def _rules(
    *,
    vision: int = 0,
    height: int = 0,
    randomize: int = 0,
    score: int = 0,
    fair_rotation: int = 0,
    neighbors: int = 0,
    score_position: tuple[Literal["high_front", "high_back"], int] | None = None,
    score_distribution: tuple[Literal["row", "group"], int] | None = None,
    mentor_pairing: int = 0,
) -> RuleSet:
    return RuleSet(
        seed=42,
        hard=HardRules(),
        soft=SoftRules(
            vision_front=_weighted(vision > 0, vision),
            height_back=_weighted(height > 0, height),
            randomize=_weighted(randomize > 0, randomize),
            score_balance=_weighted(score > 0, score),
            score_position=ScorePositionRule(
                enabled=score_position is not None and score_position[1] > 0,
                weight=score_position[1] if score_position is not None else 1,
                direction=score_position[0] if score_position is not None else "high_front",
            ),
            score_distribution=ScoreDistributionRule(
                enabled=score_distribution is not None and score_distribution[1] > 0,
                weight=score_distribution[1] if score_distribution is not None else 1,
                scope=score_distribution[0] if score_distribution is not None else "row",
            ),
            mentor_pairing=MentorPairingRule(
                enabled=mentor_pairing > 0,
                weight=mentor_pairing if mentor_pairing > 0 else 1,
            ),
            fair_rotation=FairRotationRule(
                enabled=fair_rotation > 0,
                weight=fair_rotation,
                lookback=4,
            ),
            avoid_recent_neighbors=AvoidRecentNeighborsRule(
                enabled=neighbors > 0,
                weight=neighbors,
                lookback=4,
                max_recent_count=1,
                within_distance=2,
            ),
        ),
    )


_PRESETS = (
    PresetDefinition(
        name="random",
        title="Reproducible random",
        description="Use seeded variation without activating data-dependent preferences.",
        best_for="A neutral baseline, quick shuffle, or solver sanity check.",
        rules=_rules(randomize=10),
    ),
    PresetDefinition(
        name="exam",
        title="Exam seating",
        description="Favor a reproducible shuffle while leaving all spacing and fixed-seat decisions to explicit hard rules.",
        best_for="Exam sessions where the operator supplies any required separation constraints.",
        rules=_rules(randomize=12),
    ),
    PresetDefinition(
        name="daily",
        title="Daily classroom",
        description="Combine accessibility, height, score mixing, fair rotation, and recent-neighbor avoidance.",
        best_for="Routine classroom seating with the richest available local data.",
        rules=_rules(
            vision=20,
            height=4,
            randomize=3,
            score=4,
            fair_rotation=12,
            neighbors=12,
        ),
        requirements=("history", "score", "height", "vision"),
    ),
    PresetDefinition(
        name="fair-rotation",
        title="Fair rotation",
        description="Prioritize rotating students away from repeatedly used seat categories.",
        best_for="Classes with portable historical snapshots.",
        rules=_rules(randomize=2, fair_rotation=20),
        requirements=("history",),
    ),
    PresetDefinition(
        name="neighbor-aware",
        title="Neighbor aware",
        description="Prioritize reducing recently repeated desk-mate and neighboring relationships.",
        best_for="Classes where relationship rotation matters and history is available.",
        rules=_rules(randomize=2, neighbors=20),
        requirements=("history",),
    ),
    PresetDefinition(
        name="balanced",
        title="Peer mixing (compatibility name)",
        description="Prefer mixing different score levels across adjacent seats.",
        best_for="Cooperative learning or peer-support arrangements with score data.",
        rules=_rules(randomize=2, score=18),
        requirements=("score",),
    ),
    PresetDefinition(
        name="peer-mixing",
        title="Peer mixing",
        description="Prefer mixing different score levels across adjacent seats.",
        best_for="Cooperative learning where varied-score neighbors are useful.",
        rules=_rules(randomize=2, score=18),
        requirements=("score",),
    ),
    PresetDefinition(
        name="score-high-front",
        title="Higher scores toward the front",
        description="Use score-rank percentiles to softly favor higher-scoring students toward front rows.",
        best_for="Classes that want score position to guide, but not override, accessibility and hard rules.",
        rules=_rules(randomize=2, score_position=("high_front", 18)),
        requirements=("score",),
    ),
    PresetDefinition(
        name="score-high-back",
        title="Higher scores toward the back",
        description="Use score-rank percentiles to softly favor higher-scoring students toward back rows.",
        best_for="Classes using front rows for students who need more support.",
        rules=_rules(randomize=2, score_position=("high_back", 18)),
        requirements=("score",),
    ),
    PresetDefinition(
        name="row-score-balanced",
        title="Balanced score distribution by row",
        description="Reduce differences between the mean score ranks of occupied rows.",
        best_for="Spreading score levels across classroom rows.",
        rules=_rules(randomize=2, score_distribution=("row", 18)),
        requirements=("score",),
    ),
    PresetDefinition(
        name="group-score-balanced",
        title="Balanced score distribution by group",
        description="Reduce differences between named seat-group mean score ranks.",
        best_for="Layouts whose enabled seats all define group_id.",
        rules=_rules(randomize=2, score_distribution=("group", 18)),
        requirements=("score",),
    ),
    PresetDefinition(
        name="mentor-pairing",
        title="Mentor pairing",
        description=(
            "Deterministically pair high- and low-ranked students at neighboring "
            "desks while avoiding recent repeats when history is available."
        ),
        best_for="Peer-support lessons where teachers want explainable mentor and learner pairs.",
        rules=_rules(randomize=2, mentor_pairing=18),
        requirements=("score",),
    ),
    PresetDefinition(
        name="height-aware",
        title="Height aware",
        description="Prefer taller students toward the back while retaining reproducible variation.",
        best_for="Layouts with multiple rows and student height data.",
        rules=_rules(height=18, randomize=2),
        requirements=("height",),
    ),
    PresetDefinition(
        name="vision-friendly",
        title="Vision friendly",
        description="Prioritize front seats for students marked with vision or front-seat needs.",
        best_for="Accessibility-focused seating with vision or needs markers.",
        rules=_rules(vision=25, randomize=1),
        requirements=("vision",),
    ),
)

_PRESET_BY_NAME = {preset.name: preset for preset in _PRESETS}

_DEGRADATION_NOTES = {
    "history": "History-based preferences stay enabled but contribute no cost or score until snapshots are supplied.",
    "score": "Score-based goals stay enabled but are ignored when fewer than two students have distinct scores.",
    "height": "height_back stays enabled but is ignored when usable height or row variation is unavailable.",
    "vision": "vision_front stays enabled but is ignored when no student has a vision/front-seat marker.",
}


def list_presets() -> tuple[PresetDefinition, ...]:
    return _PRESETS


def get_preset(name: str) -> PresetDefinition:
    normalized = name.strip().lower().replace("_", "-")
    try:
        return _PRESET_BY_NAME[normalized]
    except KeyError as exc:
        available = ", ".join(preset.name for preset in _PRESETS)
        raise InputFileError(f"Unknown preset {name!r}. Available presets: {available}.") from exc


def load_rules_with_preset(
    *,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
) -> tuple[RuleSet, PresetDefinition | None]:
    rules_data = read_json(rules_path) if rules_path is not None else None
    return resolve_rules_with_preset(
        rules_data=rules_data,
        preset_name=preset_name,
        source=rules_path or "<rules>",
    )


def resolve_rules_with_preset(
    *,
    rules_data: dict[str, Any] | None = None,
    preset_name: str | None = None,
    source: str | Path = "<rules overlay>",
) -> tuple[RuleSet, PresetDefinition | None]:
    """Resolve a preset and an optional in-memory rules overlay."""
    if rules_data is None and preset_name is None:
        raise InputFileError("Provide --rules, --preset, or both.")

    preset = get_preset(preset_name) if preset_name is not None else None
    data = _model_to_data(preset.rules) if preset is not None else {}
    resolved_source: str | Path = (
        f"preset:{preset.name}" if preset is not None and rules_data is None else source
    )
    if rules_data is not None:
        data = _deep_merge(data, rules_data)
    return parse_rules_data(data, resolved_source), preset


def export_preset(name: str, output_path: str | Path | None = None) -> Path:
    preset = get_preset(name)
    output = Path(output_path) if output_path is not None else Path(f"{preset.name}.rules.json")
    return write_json_model(preset.rules, output)


def format_preset_list() -> str:
    lines = ["Available presets:"]
    lines.extend(f"- {preset.name}: {preset.description}" for preset in _PRESETS)
    return "\n".join(lines)


def format_preset(preset: PresetDefinition) -> str:
    requirements = ", ".join(preset.requirements) if preset.requirements else "none"
    lines = [
        f"Preset: {preset.name}",
        f"Title: {preset.title}",
        f"Description: {preset.description}",
        f"Best for: {preset.best_for}",
        f"Preferred data: {requirements}",
    ]
    if preset.requirements:
        lines.extend(
            ["", "Graceful degradation:"]
            + [
                f"- {requirement}: {_DEGRADATION_NOTES[requirement]}"
                for requirement in preset.requirements
            ]
        )
    lines.extend(["", "Rules JSON:", _model_to_json(preset.rules)])
    return "\n".join(lines)


def preset_context_warnings(
    preset: PresetDefinition | None,
    students: Sequence[Student],
    *,
    history_count: int = 0,
    rules: RuleSet | None = None,
) -> list[str]:
    if preset is None:
        return []
    warnings: list[str] = []
    for requirement in preset.requirements:
        if rules is not None and not _requirement_enabled(requirement, rules):
            continue
        available = _requirement_available(requirement, students, history_count)
        if not available:
            warnings.append(
                f'Preset "{preset.name}" is missing preferred {requirement} data. '
                f"{_DEGRADATION_NOTES[requirement]}"
            )
    return warnings


def _requirement_enabled(requirement: PresetRequirement, rules: RuleSet) -> bool:
    if requirement == "history":
        history_rules = (
            rules.soft.fair_rotation,
            effective_neighbor_rule(rules),
        )
        return any(rule.enabled and rule.weight > 0 for rule in history_rules)
    if requirement == "score":
        score_rules = (
            rules.soft.score_balance,
            rules.soft.score_position,
            rules.soft.score_distribution,
            rules.soft.mentor_pairing,
        )
        return any(rule.enabled and rule.weight > 0 for rule in score_rules)
    elif requirement == "height":
        rule = rules.soft.height_back
    else:
        rule = rules.soft.vision_front
    return rule.enabled and rule.weight > 0


def _requirement_available(
    requirement: PresetRequirement,
    students: Sequence[Student],
    history_count: int,
) -> bool:
    if requirement == "history":
        return history_count > 0
    if requirement == "score":
        values = {float(student.score) for student in students if student.score is not None}
        return len(values) >= 2
    if requirement == "height":
        values = {float(student.height_cm) for student in students if student.height_cm is not None}
        return len(values) >= 2
    return any(_has_vision_marker(student) for student in students)


def _has_vision_marker(student: Student) -> bool:
    if student.vision is not None:
        return True
    keywords = {
        "vision",
        "vision_front",
        "front",
        "poor",
        "low",
        "nearsighted",
        "short_sighted",
        "myopia",
        "视力",
        "近视",
        "靠前",
    }
    values = {item.lower() for item in student.tags + student.needs}
    return bool(values & keywords)


def _deep_merge(base: dict[str, Any], overlay: dict[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in overlay.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = _deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


def _model_to_data(model: RuleSet) -> dict[str, Any]:
    return model.model_dump(mode="json")


def _model_to_json(model: RuleSet) -> str:
    return model.model_dump_json(indent=2)
