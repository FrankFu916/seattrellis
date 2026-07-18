"""Real-browser acceptance coverage for the primary Web workflow."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from playwright.sync_api import Locator, Page, expect

from seattrellis.web.keys import (
    QUICK_EXPORT_DOWNLOAD_ARTIFACT,
    QUICK_EXPORT_PREFIX,
    QUICK_GENERATE_BUTTON,
    QUICK_LOAD_DEMO_BUTTON,
    QUICK_RESULTS_STATUS,
    QUICK_SOLVE_STATUS,
    QUICK_STEP_RADIO,
    UI_LANGUAGE_SELECT,
    export_prepare_key,
    export_prepared_download_key,
    widget_region_key,
)

if TYPE_CHECKING:
    from conftest import WebServer


def _region(page: Page, widget_key: str) -> Locator:
    return page.locator(f".st-key-{widget_region_key(widget_key)}")


def _select_region_option(
    page: Page,
    widget_key: str,
    option: str,
) -> None:
    combobox = _region(page, widget_key).get_by_role("combobox")
    expect(combobox).to_have_count(1)
    combobox.click()
    listbox = page.get_by_role("listbox")
    expect(listbox).to_be_visible()
    listbox.get_by_role("option", name=option, exact=True).click()


def _assert_no_app_exception(page: Page) -> None:
    expect(page.locator('[data-testid="stException"]')).to_have_count(0)


@pytest.mark.e2e
def test_demo_solve_and_public_export_download(
    page: Page,
    tmp_path: Path,
    web_server: WebServer,
) -> None:
    """Exercise the HTTP, WebSocket, solver, privacy, and download path."""

    page.goto(web_server.url, wait_until="domcontentloaded")
    expect(page).to_have_title(re.compile("SeatTrellis"))

    language = _region(page, UI_LANGUAGE_SELECT).get_by_role("combobox")
    language.click()
    page.get_by_role("option", name="English", exact=True).click()
    expect(
        page.get_by_role("heading", name=re.compile("SeatTrellis"))
    ).to_be_visible()

    _region(page, QUICK_LOAD_DEMO_BUTTON).get_by_role("button").click()
    expect(
        page.get_by_text(
            "The Demo is ready with the daily preset selected. "
            "Continue to the next step.",
            exact=True,
        )
    ).to_be_visible()

    steps = _region(page, QUICK_STEP_RADIO).get_by_role("radiogroup")
    steps.get_by_role(
        "radio",
        name=re.compile(r"(?:2\.\s*)?Configure & solve$"),
    ).click()

    _region(page, QUICK_GENERATE_BUTTON).get_by_role("button").click()
    expect(_region(page, QUICK_SOLVE_STATUS)).to_contain_text(
        "Solve complete. Continue to Review & export.",
        timeout=30_000,
    )
    _assert_no_app_exception(page)

    steps = _region(page, QUICK_STEP_RADIO).get_by_role("radiogroup")
    steps.get_by_role(
        "radio",
        name=re.compile(r"(?:3\.\s*)?Review & export$"),
    ).click()
    expect(_region(page, QUICK_RESULTS_STATUS)).to_contain_text(
        "Generated 3 candidates. Recommended:",
        timeout=30_000,
    )

    with page.expect_download() as candidate_download_info:
        _region(
            page,
            QUICK_EXPORT_DOWNLOAD_ARTIFACT,
        ).get_by_role("button").click()
    candidate_download = candidate_download_info.value
    assert candidate_download.suggested_filename == "seattrellis.candidates.json"
    candidate_path = tmp_path / candidate_download.suggested_filename
    candidate_download.save_as(candidate_path)
    candidate_set = json.loads(candidate_path.read_text(encoding="utf-8"))
    assert len(candidate_set["candidates"]) == 3
    assert candidate_set["recommended_candidate_id"]

    _select_region_option(
        page,
        f"{QUICK_EXPORT_PREFIX}_template",
        "Public notice",
    )
    anonymize_region = _region(
        page,
        f"{QUICK_EXPORT_PREFIX}_anonymize_public",
    )
    anonymize = anonymize_region.get_by_role("checkbox")
    expect(anonymize).to_have_count(1)
    anonymize_region.locator("label").click()
    expect(anonymize).to_be_checked()
    _select_region_option(
        page,
        f"{QUICK_EXPORT_PREFIX}_orientation",
        "Landscape",
    )
    _select_region_option(
        page,
        f"{QUICK_EXPORT_PREFIX}_locale",
        "English",
    )

    prepare_key = export_prepare_key(QUICK_EXPORT_PREFIX, "print-html")
    _region(page, prepare_key).get_by_role("button").click()

    download_key = export_prepared_download_key(QUICK_EXPORT_PREFIX)
    prepared_region = _region(page, download_key)
    expect(prepared_region).to_contain_text(
        "Print HTML export is ready to download.",
        timeout=30_000,
    )
    with page.expect_download() as html_download_info:
        prepared_region.get_by_role("button").click()
    html_download = html_download_info.value
    assert html_download.suggested_filename == "seating.print.html"
    html_path = tmp_path / html_download.suggested_filename
    html_download.save_as(html_path)
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

    _assert_no_app_exception(page)
    web_server.assert_healthy()
