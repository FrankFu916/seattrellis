"""Real-browser coverage for manually uploaded quick-solve inputs."""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from playwright.sync_api import Page, expect

from e2e.support import (
    assert_no_app_exception,
    choose_quick_step,
    download_from_region,
    open_english_app,
    region,
    set_number_input,
    upload_file,
)
from seattrellis.web.keys import (
    QUICK_CANDIDATE_COUNT_INPUT,
    QUICK_EXPORT_DOWNLOAD_ARTIFACT,
    QUICK_GENERATE_BUTTON,
    QUICK_LAYOUT_UPLOAD,
    QUICK_RESULTS_STATUS,
    QUICK_RULES_UPLOAD,
    QUICK_SOLVE_STATUS,
    QUICK_STUDENTS_UPLOAD,
)

if TYPE_CHECKING:
    from conftest import WebServer


FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures"


@pytest.mark.e2e
def test_uploaded_files_solve_and_download_candidate_set(
    page: Page,
    tmp_path: Path,
    web_server: WebServer,
) -> None:
    """Prove that uploaded students, layout, and rules reach the solver."""

    open_english_app(page, web_server.url)
    upload_file(
        page,
        QUICK_STUDENTS_UPLOAD,
        FIXTURES / "students.csv",
    )
    upload_file(
        page,
        QUICK_LAYOUT_UPLOAD,
        FIXTURES / "classroom.json",
    )
    upload_file(
        page,
        QUICK_RULES_UPLOAD,
        FIXTURES / "rules.json",
    )

    choose_quick_step(page, 2, "Configure & solve")
    expect(
        region(page, QUICK_GENERATE_BUTTON).get_by_role("button")
    ).to_be_enabled()

    choose_quick_step(page, 1, "Load data")
    expect(page.get_by_text("Input files retained across steps:")).to_be_visible()
    choose_quick_step(page, 2, "Configure & solve")

    set_number_input(page, QUICK_CANDIDATE_COUNT_INPUT, 2)
    region(page, QUICK_GENERATE_BUTTON).get_by_role("button").click()
    expect(region(page, QUICK_SOLVE_STATUS)).to_contain_text(
        "Solve complete. Continue to Review & export.",
        timeout=30_000,
    )

    choose_quick_step(page, 3, "Review & export")
    expect(region(page, QUICK_RESULTS_STATUS)).to_contain_text(
        "Generated 2 candidates. Recommended:",
        timeout=30_000,
    )
    candidate_path = download_from_region(
        page,
        QUICK_EXPORT_DOWNLOAD_ARTIFACT,
        tmp_path / "uploaded.candidates.json",
        expected_filename="seattrellis.candidates.json",
    )
    candidate_set = json.loads(candidate_path.read_text(encoding="utf-8"))

    assert len(candidate_set["candidates"]) == 2
    assert candidate_set["recommended_candidate_id"]
    for candidate in candidate_set["candidates"]:
        snapshot = candidate["snapshot"]
        assert len(snapshot["students"]) == 4
        assert len(snapshot["assignments"]) == 4
        assert len(
            {assignment["seat_id"] for assignment in snapshot["assignments"]}
        ) == 4
        assignments = {
            assignment["student_key"]: assignment["seat_id"]
            for assignment in snapshot["assignments"]
        }
        assert assignments["STU001"] == "A1"
        assert candidate["hard_constraints_satisfied"] is True

    assert_no_app_exception(page)
    web_server.assert_healthy()
