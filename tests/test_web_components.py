from __future__ import annotations

from pathlib import Path

import pytest

from seattrellis.io.json_files import InputFileError
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.layout import ClassroomLayout, SeatNode
from seattrellis.models.rules import RuleSet
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.web.components import (
    accessibility_styles,
    build_candidate_selector,
    build_comparison_table,
    build_data_table_html,
    build_preset_cards,
    build_privacy_notice_html,
    build_seat_grid_html,
    diagnose_error,
    layout_grid_axes,
)
from seattrellis.web.i18n import (
    LANGUAGE_OPTIONS,
    available_translation_keys,
    table_column_labels,
    translate,
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
    assert 'role="grid"' in html
    assert 'role="gridcell"' in html
    assert 'tabindex="0"' in html
    assert 'aria-label="座位 R1&lt;&quot;&amp;' in html


def test_seat_grid_and_privacy_notice_support_english() -> None:
    layout = ClassroomLayout(
        seats=[
            SeatNode(
                seat_id="R1C1",
                row=1,
                col=1,
                enabled=False,
                near_window=True,
            ),
            SeatNode(seat_id="R1C2", row=1, col=2),
        ],
    )

    html = build_seat_grid_html(layout, locale="en")
    notice = build_privacy_notice_html("en")

    assert 'aria-label="Classroom seating map"' in html
    assert 'aria-label="Seat R1C1 | disabled | Tags: near window"' in html
    assert 'tabindex="-1"' in html
    assert 'aria-disabled="true"' in html
    assert "Privacy" in notice
    assert "does not upload student information" in notice


def test_seat_grid_compacts_extreme_sparse_coordinates() -> None:
    layout = ClassroomLayout(
        seats=[
            SeatNode(seat_id="A", row=1, col=1),
            SeatNode(seat_id="B", row=10_000, col=10_000),
        ]
    )

    rows, columns = layout_grid_axes(layout.seats)
    html = build_seat_grid_html(layout)

    assert rows == [1, 10_000]
    assert columns == [1, 10_000]
    assert html.count('class="seat-cell') == 4
    assert html.count('role="gridcell"') == 2
    assert len(html) < 10_000


def test_plain_data_table_localizes_columns_and_escapes_values() -> None:
    html = build_data_table_html(
        [
            {
                "student_name": '<script>alert("x")</script>',
                "seat_id": "A1",
                "recommended": True,
            }
        ],
        columns=["student_name", "seat_id", "recommended"],
        caption="Assignment <details>",
        locale="en",
    )

    assert '<table class="seattrellis-data-table">' in html
    assert '<th scope="col">Name</th>' in html
    assert '<th scope="col">Seat</th>' in html
    assert '<th scope="col">Recommended</th>' in html
    assert "<script>" not in html
    assert "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;" in html
    assert 'aria-label="Assignment &lt;details&gt;"' in html
    assert "<td>✓</td>" in html


def test_web_app_avoids_dataframe_native_conversion() -> None:
    app_source = (
        Path(__file__).resolve().parents[1]
        / "src"
        / "seattrellis"
        / "web"
        / "app.py"
    ).read_text(encoding="utf-8")

    assert "st.dataframe(" not in app_source
    assert "st.table(" not in app_source


def test_candidate_selector_contains_recommended_only_once(
    candidate_set: CandidateSet,
) -> None:
    options = build_candidate_selector(candidate_set)
    ids = [option["id"] for option in options]

    assert ids[0] == "recommended"
    assert ids.count("recommended") == 1
    assert candidate_set.recommended_candidate_id not in ids[1:]
    assert len(options) == len(candidate_set.candidates)

    english = build_candidate_selector(candidate_set, locale="en")
    assert english[0]["label"].startswith("⭐ Recommended")


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

    english = diagnose_error(error, locale="en")
    assert english["category"] == category
    assert str(error) in english["detail"]


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
    english = build_preset_cards("en")
    assert len(english) == len(cards)
    assert english[0]["description"] != cards[0]["description"]


def test_translation_catalog_and_accessibility_styles() -> None:
    assert LANGUAGE_OPTIONS == {"简体中文": "zh", "English": "en"}
    assert translate("generate", "zh") == "生成座位表"
    assert translate("generate", "en") == "Generate seating plan"
    assert translate("candidate_result", "en", count=3, candidate_id="c-1") == (
        "Generated 3 candidates. Recommended: c-1."
    )
    assert {"generate", "privacy_body", "seat_grid_label"} <= (
        available_translation_keys()
    )
    assert table_column_labels("en")["student_name"] == "Name"

    css = accessibility_styles()
    assert ":focus-visible" in css
    assert "min-height: 44px" in css
    assert "@media (max-width: 768px)" in css
    assert "prefers-reduced-motion" in css
