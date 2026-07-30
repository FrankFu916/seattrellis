"""Real-browser coverage for the default teacher workspace."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from playwright.sync_api import Page, expect

from e2e.support import (
    assert_no_app_exception,
    download_from_region,
    open_english_app,
    region,
    widget,
)
from seattrellis.web.keys import (
    TEACHER_CLASS_NAME_INPUT,
    TEACHER_GENERATE_BUTTON,
    TEACHER_GOAL_SELECT,
    TEACHER_PUBLIC_EXPORT_DOWNLOAD,
    TEACHER_PUBLIC_EXPORT_PREPARE,
    TEACHER_RESULTS_STATUS,
    TEACHER_ROOM_TEMPLATE_SELECT,
    TEACHER_ROSTER_STATUS,
    TEACHER_ROSTER_UPLOAD,
    TEACHER_SOLVE_STATUS,
)

if TYPE_CHECKING:
    from conftest import WebServer


FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures"


@pytest.mark.e2e
def test_teacher_imports_generates_and_downloads_public_plan(
    page: Page,
    tmp_path: Path,
    web_server: WebServer,
) -> None:
    """Exercise the ordinary roster-to-public-print workflow."""

    open_english_app(page, web_server.url)

    class_name = widget(page, TEACHER_CLASS_NAME_INPUT).get_by_role("textbox")
    class_name.fill("Class 7 A")
    class_name.press("Enter")

    uploader = widget(page, TEACHER_ROSTER_UPLOAD)
    file_input = uploader.locator('input[type="file"]')
    expect(file_input).to_have_count(1)
    file_input.set_input_files(str(FIXTURES / "students.csv"))

    roster_status = region(page, TEACHER_ROSTER_STATUS)
    expect(roster_status).to_contain_text("Student list ready.", timeout=30_000)
    expect(roster_status).to_contain_text("Imported 4 students.")
    expect(
        region(page, TEACHER_ROOM_TEMPLATE_SELECT).get_by_role("combobox")
    ).to_have_value("30 · 5 × 6")
    expect(region(page, TEACHER_GOAL_SELECT)).to_contain_text("Daily rotation")

    generate = region(page, TEACHER_GENERATE_BUTTON).get_by_role("button")
    expect(generate).to_be_enabled()
    generate.click()
    expect(region(page, TEACHER_SOLVE_STATUS)).to_contain_text(
        "Generated 3 seating options.",
        timeout=30_000,
    )
    expect(region(page, TEACHER_RESULTS_STATUS)).to_contain_text(
        "Found 3 seating options.",
        timeout=30_000,
    )

    region(page, TEACHER_PUBLIC_EXPORT_PREPARE).get_by_role("button").click()
    expect(region(page, TEACHER_PUBLIC_EXPORT_DOWNLOAD)).to_contain_text(
        "Public print",
        timeout=30_000,
    )
    html_path = download_from_region(
        page,
        TEACHER_PUBLIC_EXPORT_DOWNLOAD,
        tmp_path / "class-7-a-public.html",
        expected_filename="Class-7-A-public.html",
    )
    html = html_path.read_text(encoding="utf-8")

    assert "<!doctype html>" in html.lower()
    assert '<html lang="en">' in html
    assert "A4 landscape" in html
    for student_name in ("Student001", "Student002", "Student003", "Student004"):
        assert student_name in html
    for private_value in (
        "STU001",
        "STU002",
        "STU003",
        "STU004",
        "vision_front",
        "leader",
        "poor",
    ):
        assert private_value not in html
    for value in (92, 81, 76, 88, 154, 172, 160, 178):
        assert f"<td>{value}</td>" not in html

    assert_no_app_exception(page)
    web_server.assert_healthy()
