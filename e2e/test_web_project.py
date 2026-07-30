"""Real-browser coverage for the local Project workspace."""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from playwright.sync_api import Page, expect

from e2e.support import (
    activate_advanced_tools,
    activate_project_workspace,
    assert_no_app_exception,
    download_from_region,
    open_english_app,
    region,
    select_region_option,
    set_checkbox,
    set_number_input,
)
from seattrellis.demo import create_demo_files
from seattrellis.web.keys import (
    PROJECT_CANDIDATE_COUNT_INPUT,
    PROJECT_CANDIDATE_SELECT,
    PROJECT_EXPORT_DOWNLOAD_ARTIFACT,
    PROJECT_EXPORT_PREFIX,
    PROJECT_INFO_BUTTON,
    PROJECT_INFO_STATUS,
    PROJECT_PATH_INPUT,
    PROJECT_RESULTS_STATUS,
    PROJECT_SOLVE_BUTTON,
    PROJECT_SOLVE_STATUS,
    PROJECT_USE_DEFAULT_CANDIDATES,
    PROJECT_VALIDATE_BUTTON,
    PROJECT_VALIDATE_STATUS,
    export_prepare_key,
    export_prepared_download_key,
)

if TYPE_CHECKING:
    from conftest import WebServer


@pytest.mark.e2e
def test_project_path_validates_solves_and_exports_selected_candidate(
    page: Page,
    tmp_path: Path,
    web_server: WebServer,
) -> None:
    """Exercise Project path resolution, validation, solving, and export."""

    project_files = create_demo_files(
        tmp_path / "project-workspace",
        overwrite=True,
    )
    project_path = project_files["project"].resolve()

    open_english_app(page, web_server.url)
    activate_advanced_tools(page)
    activate_project_workspace(page)
    path_input = region(page, PROJECT_PATH_INPUT).get_by_role("textbox")
    path_input.fill(str(project_path))
    path_input.press("Enter")
    activate_project_workspace(page)
    expect(
        region(page, PROJECT_PATH_INPUT).get_by_role("textbox")
    ).to_have_value(str(project_path))

    region(page, PROJECT_INFO_BUTTON).get_by_role("button").click()
    expect(region(page, PROJECT_INFO_STATUS)).to_contain_text(
        "Project: Demo Class"
    )
    activate_project_workspace(page)

    region(page, PROJECT_VALIDATE_BUTTON).get_by_role("button").click()
    expect(region(page, PROJECT_VALIDATE_STATUS)).to_contain_text(
        "Validation passed.",
        timeout=30_000,
    )
    activate_project_workspace(page)

    set_checkbox(
        page,
        PROJECT_USE_DEFAULT_CANDIDATES,
        checked=False,
    )
    activate_project_workspace(page)
    set_number_input(page, PROJECT_CANDIDATE_COUNT_INPUT, 2)
    activate_project_workspace(page)

    region(page, PROJECT_SOLVE_BUTTON).get_by_role("button").click()
    expect(region(page, PROJECT_SOLVE_STATUS)).to_contain_text(
        "Solve complete.",
        timeout=30_000,
    )
    activate_project_workspace(page)
    expect(region(page, PROJECT_RESULTS_STATUS)).to_contain_text(
        "Generated 2 candidates. Recommended:",
        timeout=30_000,
    )

    candidate_path = download_from_region(
        page,
        PROJECT_EXPORT_DOWNLOAD_ARTIFACT,
        tmp_path / "project.candidates.json",
        expected_filename="seattrellis.candidates.json",
    )
    candidate_set = json.loads(candidate_path.read_text(encoding="utf-8"))
    assert len(candidate_set["candidates"]) == 2
    recommended_id = candidate_set["recommended_candidate_id"]
    selected = next(
        candidate
        for candidate in candidate_set["candidates"]
        if candidate["candidate_id"] != recommended_id
    )
    selected_id = selected["candidate_id"]
    selected_label = f"{selected_id} — {selected['score']['total']:.1f}"

    activate_project_workspace(page)
    select_region_option(page, PROJECT_CANDIDATE_SELECT, selected_label)
    activate_project_workspace(page)
    select_region_option(
        page,
        f"{PROJECT_EXPORT_PREFIX}_template",
        "Explanation report",
    )
    activate_project_workspace(page)

    prepare_key = export_prepare_key(PROJECT_EXPORT_PREFIX, "print-html")
    region(page, prepare_key).get_by_role("button").click()
    download_key = export_prepared_download_key(PROJECT_EXPORT_PREFIX)
    expect(region(page, download_key)).to_contain_text(
        "Print HTML export is ready to download.",
        timeout=30_000,
    )
    activate_project_workspace(page)
    html_path = download_from_region(
        page,
        download_key,
        tmp_path / "project-seating.print.html",
        expected_filename="project-seating.print.html",
    )
    html = html_path.read_text(encoding="utf-8")

    assert "<!doctype html>" in html.lower()
    assert '<html lang="en">' in html
    assert selected_id in html
    assert "Plan explanation" in html
    output_path = project_path.parent / "outputs" / "project-seating.print.html"
    assert not output_path.exists()

    assert_no_app_exception(page)
    web_server.assert_healthy()
