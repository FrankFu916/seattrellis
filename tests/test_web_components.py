from __future__ import annotations

import pytest

from seattrellis.io.json_files import InputFileError
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.web.components import (
    build_candidate_selector,
    build_comparison_table,
    build_preset_cards,
    build_seat_grid_html,
    diagnose_error,
)
from seattrellis.web.workflow import solve_for_web


@pytest.fixture
def candidate_set(tmp_path) -> CandidateSet:
    result = solve_for_web(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="daily",
        history_dir="examples/history",
        output_dir=tmp_path,
        candidate_count=3,
    )
    assert isinstance(result.artifact, CandidateSet)
    return result.artifact


def test_seat_grid_escapes_student_names_and_seat_ids() -> None:
    student = Student(student_id="S1", name='</span><script>"student"</script>')
    layout = ClassroomLayout(
        seats=[SeatNode(seat_id='R1<"&', row=1, col=1)],
    )
    snapshot = SeatingSnapshot(
        students=[student],
        layout=layout,
        rules=RuleSet(),
        assignments=[
            SeatAssignment(
                student_key=student.key,
                student_name=student.display_name,
                seat_id='R1<"&',
            )
        ],
        solver_status="FEASIBLE",
    )

    html = build_seat_grid_html(layout, snapshot)

    assert "<script>" not in html
    assert "</span><script>" not in html
    assert "&lt;/span&gt;&lt;script&gt;&quot;student&quot;&lt;/script&gt;" in html
    assert 'title="座位 R1&lt;&quot;&amp;' in html


def test_candidate_selector_contains_recommended_only_once(
    candidate_set: CandidateSet,
) -> None:
    options = build_candidate_selector(candidate_set)
    ids = [option["id"] for option in options]

    assert ids[0] == "recommended"
    assert ids.count("recommended") == 1
    assert candidate_set.recommended_candidate_id not in ids[1:]
    assert len(options) == len(candidate_set.candidates)


def test_candidate_comparison_has_one_ranked_row_per_candidate(
    candidate_set: CandidateSet,
) -> None:
    comparison = build_comparison_table(candidate_set)
    rows = comparison["rows"]

    assert comparison["columns"] == [
        "candidate_id",
        "recommended",
        "total",
        "hard_constraints",
        "fair_rotation",
        "neighbors",
        "score_balance",
        "height",
        "vision",
        "diversity",
        "stability",
    ]
    assert len(rows) == len(candidate_set.candidates)
    assert [row["total"] for row in rows] == sorted(
        (row["total"] for row in rows),
        reverse=True,
    )
    assert sum(row["recommended"] == "⭐" for row in rows) == 1


@pytest.mark.parametrize(
    ("error", "category"),
    [
        (InputFileError("cannot read file"), "file_error"),
        (MissingOptionalDependencyError("PNG export", "image"), "missing_dependency"),
        (ValueError("candidate_count is invalid"), "value_error"),
        (RuntimeError("unexpected"), "unknown"),
    ],
)
def test_error_diagnosis_uses_stable_user_facing_categories(
    error: Exception,
    category: str,
) -> None:
    diagnosis = diagnose_error(error)

    assert diagnosis["category"] == category
    assert diagnosis["title"]
    assert str(error) in diagnosis["detail"]


def test_preset_cards_cover_every_builtin_preset() -> None:
    cards = build_preset_cards()

    assert {card["name"] for card in cards} == {
        "random",
        "exam",
        "daily",
        "fair-rotation",
        "neighbor-aware",
        "balanced",
        "height-aware",
        "vision-friendly",
    }
    assert all(
        {"description", "scenario", "requires", "degradation"} <= card.keys()
        for card in cards
    )
