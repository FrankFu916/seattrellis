from __future__ import annotations

import pytest

from seattrellis.application.class_workflow import (
    ClassDraft,
    GenerateOptions,
    generate_class_plan,
    inspect_class,
)
from seattrellis.application.room_templates import build_standard_room
from seattrellis.application.teacher_goals import TeacherGoalSelection
from seattrellis.io.json_files import InputFileError
from seattrellis.models.student import Student


def _students(count: int) -> tuple[Student, ...]:
    return tuple(
        Student(student_id=f"S{index:02d}", name=f"Student {index}")
        for index in range(1, count + 1)
    )


def _draft(*, student_count: int = 4, seat_count: int = 4) -> ClassDraft:
    return ClassDraft(
        name="Class 1",
        students=_students(student_count),
        layout=build_standard_room(rows=1, seats_per_row=seat_count),
        goal=TeacherGoalSelection(goal_id="daily-rotation"),
    )


def test_inspect_class_reports_capacity_errors_without_solving() -> None:
    readiness = inspect_class(_draft(student_count=5, seat_count=4))

    assert not readiness.ready
    assert any(
        "Not enough enabled seats" in error
        for error in readiness.validation.errors
    )


def test_class_draft_cleans_and_requires_a_class_name() -> None:
    draft = _draft()
    renamed = ClassDraft(
        name="  Class 2  ",
        students=draft.students,
        layout=draft.layout,
        goal=draft.goal,
    )

    assert renamed.name == "Class 2"
    with pytest.raises(ValueError, match="class name cannot be empty"):
        ClassDraft(
            name="  ",
            students=draft.students,
            layout=draft.layout,
            goal=draft.goal,
        )


def test_generate_class_plan_maps_hidden_defaults_to_solve_input(monkeypatch) -> None:
    captured = None
    sentinel = object()

    def fake_compute_solve(solve_input):
        nonlocal captured
        captured = solve_input
        return sentinel

    monkeypatch.setattr(
        "seattrellis.application.class_workflow.compute_solve",
        fake_compute_solve,
    )

    result = generate_class_plan(_draft())

    assert result is sentinel
    assert captured is not None
    assert captured.candidate_count == 3
    assert captured.seed is None
    assert captured.backend == "auto"
    assert captured.preset_name == "daily"
    assert captured.history_snapshots == []


def test_generate_class_plan_rejects_a_class_that_is_not_ready() -> None:
    with pytest.raises(InputFileError, match="Not enough enabled seats"):
        generate_class_plan(_draft(student_count=5, seat_count=4))


def test_generate_class_plan_runs_a_small_fallback_solve() -> None:
    result = generate_class_plan(
        _draft(student_count=4, seat_count=4),
        options=GenerateOptions(
            candidate_count=1,
            time_limit_seconds=0.5,
            backend="fallback",
        ),
    )

    assert len(result.candidate_set.candidates) == 1
    assert result.candidate_set.recommended_candidate_id


@pytest.mark.parametrize(
    ("kwargs", "message"),
    [
        ({"candidate_count": 0}, "candidate_count"),
        ({"candidate_count": 21}, "candidate_count"),
        ({"time_limit_seconds": 0.0}, "time_limit_seconds"),
        ({"backend": "unknown"}, "backend"),
    ],
)
def test_generate_options_reject_invalid_advanced_values(kwargs, message) -> None:
    with pytest.raises(ValueError, match=message):
        GenerateOptions(**kwargs)
