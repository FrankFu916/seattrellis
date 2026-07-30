from __future__ import annotations

from pathlib import Path

import pytest

import seattrellis.web.class_adapter as adapter
from seattrellis.application.class_workflow import GenerateOptions
from seattrellis.application.room_templates import RoomTemplate
from seattrellis.application.roster_import import import_roster_records
from seattrellis.io.json_files import (
    InputFileError,
    load_candidate_set,
    load_plan_comparison_report,
)
from seattrellis.models.candidate import CandidateSet
from seattrellis.optional import MissingOptionalDependencyError


def _roster(count: int = 2):
    return import_roster_records(
        [
            {"student_id": f"S{index}", "name": f"Student {index}"}
            for index in range(1, count + 1)
        ],
        source_name="class.csv",
    )


@pytest.mark.parametrize(
    ("filename", "temporary_name"),
    [
        ("C:\\school\\Grade 5.xlsx", "roster.xlsx"),
        ("C:\\school\\Grade 5.xlsm", "roster.xlsm"),
    ],
)
def test_uploaded_roster_uses_a_fixed_temporary_name_and_cleans_up(
    monkeypatch,
    filename,
    temporary_name,
) -> None:
    imported = _roster()
    observed_path: Path | None = None

    def fake_import(path: str | Path):
        nonlocal observed_path
        observed_path = Path(path)
        assert observed_path.name == temporary_name
        assert observed_path.read_bytes() == b"workbook bytes"
        return imported

    monkeypatch.setattr(adapter, "import_roster", fake_import)

    result = adapter.import_uploaded_roster(
        filename,
        b"workbook bytes",
    )

    assert result.source_name == Path(filename.replace("\\", "/")).name
    assert observed_path is not None
    assert not observed_path.exists()


def test_uploaded_roster_error_does_not_expose_the_temporary_path() -> None:
    with pytest.raises(InputFileError) as caught:
        adapter.import_uploaded_roster("class.csv", b"unknown\nvalue\n")

    message = str(caught.value)
    assert "class.csv" in message
    assert "seattrellis-roster-" not in message


@pytest.mark.parametrize(
    "error",
    [
        ValueError("Invalid roster values."),
        MissingOptionalDependencyError("Excel import", "excel"),
    ],
)
def test_uploaded_roster_preserves_established_import_errors(
    monkeypatch,
    error,
) -> None:
    def fail_import(_path):
        raise error

    monkeypatch.setattr(adapter, "import_roster", fail_import)

    with pytest.raises(type(error)) as caught:
        adapter.import_uploaded_roster("class.xlsx", b"workbook bytes")

    assert caught.value is error


@pytest.mark.parametrize(
    ("filename", "content", "error_type", "message"),
    [
        ("class.txt", b"name\nAlice\n", ValueError, "CSV, XLSX, or XLSM"),
        ("class.csv", b"", ValueError, "empty"),
        ("..", b"name\nAlice\n", ValueError, "valid file name"),
        ("class.csv", "name\nAlice\n", TypeError, "bytes"),
    ],
)
def test_uploaded_roster_rejects_unsafe_or_invalid_input(
    filename,
    content,
    error_type,
    message,
) -> None:
    with pytest.raises(error_type, match=message):
        adapter.import_uploaded_roster(filename, content)


def test_class_setup_chooses_the_smallest_room_and_resolves_the_goal() -> None:
    draft = adapter.build_class_draft(
        class_name="  Grade 5  ",
        roster=_roster(30),
        goal_id="fair_shuffle",
    )

    readiness = adapter.inspect_class_setup(draft)

    assert draft.name == "Grade 5"
    assert draft.layout.layout_id == "standard-30"
    assert draft.goal.goal_id == "fair-shuffle"
    assert readiness.ready
    assert readiness.resolved_goal.definition.goal_id == "fair-shuffle"


def test_class_setup_reports_a_room_that_is_too_small() -> None:
    draft = adapter.build_class_draft(
        class_name="Grade 5",
        roster=_roster(2),
        room_template=RoomTemplate(
            template_id="single-seat",
            rows=1,
            seats_per_row=1,
            aisles_after=(),
            name="Single seat",
        ),
    )

    readiness = adapter.inspect_class_setup(draft)

    assert not readiness.ready
    assert any(
        "Not enough enabled seats" in item
        for item in readiness.validation.errors
    )


def test_generate_class_setup_writes_reusable_web_artifacts(tmp_path) -> None:
    draft = adapter.build_class_draft(
        class_name="Grade 5",
        roster=_roster(2),
        room_template=RoomTemplate(
            template_id="two-seat",
            rows=1,
            seats_per_row=2,
            aisles_after=(),
            name="Two seats",
        ),
        goal_id="daily-rotation",
    )

    result = adapter.generate_class_setup(
        draft,
        output_dir=tmp_path / "results",
        options=GenerateOptions(
            candidate_count=1,
            time_limit_seconds=0.2,
            backend="fallback",
        ),
    )

    assert isinstance(result.artifact, CandidateSet)
    assert result.artifact_path.name == "seattrellis.candidates.json"
    assert result.report_path is not None
    assert result.report_path.name == "seattrellis.plan-report.json"
    saved_candidates = load_candidate_set(result.artifact_path)
    saved_report = load_plan_comparison_report(result.report_path)
    assert len(saved_candidates.candidates) == 1
    assert (
        saved_candidates.recommended_candidate_id
        == result.artifact.recommended_candidate_id
    )
    assert result.report is not None
    assert (
        saved_report.recommended_candidate_id
        == result.report.recommended_candidate_id
    )
