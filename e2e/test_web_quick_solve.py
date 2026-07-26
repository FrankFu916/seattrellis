"""Real-browser acceptance coverage for the primary Web workflow."""

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
    select_region_option,
)

from seattrellis.web.keys import (
    QUICK_EXPORT_DOWNLOAD_ARTIFACT,
    QUICK_EXPORT_PREFIX,
    QUICK_GENERATE_BUTTON,
    QUICK_LOAD_DEMO_BUTTON,
    QUICK_RESULTS_STATUS,
    QUICK_SOLVE_STATUS,
    export_prepare_key,
    export_prepared_download_key,
)

if TYPE_CHECKING:
    from conftest import WebServer


@pytest.mark.e2e
def test_demo_solve_and_public_export_download(
    page: Page,
    tmp_path: Path,
    web_server: WebServer,
) -> None:
    """Exercise the HTTP, WebSocket, solver, privacy, and download path."""

    open_english_app(page, web_server.url)

    region(page, QUICK_LOAD_DEMO_BUTTON).get_by_role("button").click()
    expect(
        page.get_by_text(
            "The Demo is ready with the daily preset selected. "
            "Continue to the next step.",
            exact=True,
        )
    ).to_be_visible()

    choose_quick_step(page, 2, "Configure & solve")

    region(page, QUICK_GENERATE_BUTTON).get_by_role("button").click()
    expect(region(page, QUICK_SOLVE_STATUS)).to_contain_text(
        "Solve complete. Continue to Review & export.",
        timeout=30_000,
    )
    assert_no_app_exception(page)

    choose_quick_step(page, 3, "Review & export")
    expect(region(page, QUICK_RESULTS_STATUS)).to_contain_text(
        "Generated 3 candidates. Recommended:",
        timeout=30_000,
    )

    candidate_path = download_from_region(
        page,
        QUICK_EXPORT_DOWNLOAD_ARTIFACT,
        tmp_path / "seattrellis.candidates.json",
        expected_filename="seattrellis.candidates.json",
    )
    candidate_set = json.loads(candidate_path.read_text(encoding="utf-8"))
    assert len(candidate_set["candidates"]) == 3
    assert candidate_set["recommended_candidate_id"]

    select_region_option(
        page,
        f"{QUICK_EXPORT_PREFIX}_template",
        "Public notice",
    )
    anonymize_region = region(
        page,
        f"{QUICK_EXPORT_PREFIX}_anonymize_public",
    )
    anonymize = anonymize_region.get_by_role("checkbox")
    expect(anonymize).to_have_count(1)
    anonymize_region.locator("label").click()
    expect(anonymize).to_be_checked()
    select_region_option(
        page,
        f"{QUICK_EXPORT_PREFIX}_orientation",
        "Landscape",
    )
    select_region_option(
        page,
        f"{QUICK_EXPORT_PREFIX}_locale",
        "English",
    )

    prepare_key = export_prepare_key(QUICK_EXPORT_PREFIX, "print-html")
    region(page, prepare_key).get_by_role("button").click()

    download_key = export_prepared_download_key(QUICK_EXPORT_PREFIX)
    prepared_region = region(page, download_key)
    expect(prepared_region).to_contain_text(
        "Print HTML export is ready to download.",
        timeout=30_000,
    )
    html_path = download_from_region(
        page,
        download_key,
        tmp_path / "seating.print.html",
        expected_filename="seating.print.html",
    )
    html = html_path.read_text(encoding="utf-8")

    assert "<!doctype html>" in html.lower()
    assert '<html lang="en">' in html
    assert "A4 landscape" in html
    assert "Student 01" in html

    recommended = next(
        candidate
        for candidate in candidate_set["candidates"]
        if candidate["candidate_id"]
        == candidate_set["recommended_candidate_id"]
    )
    students = recommended["snapshot"]["students"]
    private_text = {
        str(value)
        for student in students
        for field in ("name", "student_id", "notes", "vision")
        if (value := student.get(field))
    }
    private_text.update(
        str(value)
        for student in students
        for field in ("tags", "needs")
        for value in student.get(field, [])
    )
    for private_value in private_text:
        assert private_value not in html
    for student in students:
        for field in ("score", "height_cm"):
            value = student.get(field)
            if value is not None:
                assert f"<td>{value}</td>" not in html

    assert_no_app_exception(page)
    web_server.assert_healthy()
