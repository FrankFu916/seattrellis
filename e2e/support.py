"""Stable page operations shared by browser acceptance tests."""

from __future__ import annotations

import re
from pathlib import Path

from playwright.sync_api import Locator, Page, expect

from seattrellis.web.keys import (
    QUICK_RETAINED_UPLOADS_STATUS,
    QUICK_STEP_RADIO,
    UI_LANGUAGE_SELECT,
    widget_region_key,
)


def region(page: Page, widget_key: str) -> Locator:
    """Locate an application-owned region for one keyed widget."""

    return page.locator(f".st-key-{widget_region_key(widget_key)}")


def open_english_app(page: Page, url: str) -> None:
    """Open the app and switch the current Streamlit session to English."""

    page.goto(url, wait_until="domcontentloaded")
    expect(page).to_have_title(re.compile("SeatTrellis"))
    language = region(page, UI_LANGUAGE_SELECT).get_by_role("combobox")
    expect(language).to_have_count(1)
    language.click()
    option = page.get_by_role("option", name="English", exact=True)
    expect(option).to_be_visible()
    option.click()
    expect(
        page.get_by_role("tab", name="Quick solve", exact=True)
    ).to_be_visible()


def choose_quick_step(page: Page, number: int, label: str) -> None:
    """Choose a wizard step across Streamlit accessibility-name variants."""

    steps = region(page, QUICK_STEP_RADIO).get_by_role("radiogroup")
    visible_label = steps.locator("label").filter(
        has_text=re.compile(rf"(?:{number}\.\s*)?{re.escape(label)}$")
    )
    expect(visible_label).to_have_count(1)
    visible_label.click()


def select_region_option(
    page: Page,
    widget_key: str,
    option: str,
) -> None:
    """Select one option from a keyed Streamlit selectbox."""

    widget = region(page, widget_key)
    combobox = widget.get_by_role("combobox")
    expect(combobox).to_have_count(1)
    if combobox.input_value() == option:
        return

    # Streamlit 1.60 uses an editable React Aria combobox. Filtering by the
    # complete label avoids a flaky pointer-open operation while the page is
    # settling after a Streamlit rerun.
    combobox.fill(option)
    menu_option = page.get_by_role("option", name=option, exact=True)
    expect(menu_option).to_be_visible()
    menu_option.click()
    expect(region(page, widget_key).get_by_role("combobox")).to_have_value(
        option,
    )


def upload_file(page: Page, widget_key: str, path: Path) -> None:
    """Upload one file and wait for Streamlit to retain its display name."""

    uploader = region(page, widget_key)
    file_input = uploader.locator('input[type="file"]')
    expect(file_input).to_have_count(1)
    file_input.set_input_files(str(path))
    expect(region(page, QUICK_RETAINED_UPLOADS_STATUS)).to_contain_text(
        path.name,
        timeout=30_000,
    )


def set_number_input(page: Page, widget_key: str, value: int | float) -> None:
    """Set a keyed number input and wait for its WebSocket rerun."""

    spinbutton = region(page, widget_key).get_by_role("spinbutton")
    expect(spinbutton).to_be_enabled()
    spinbutton.fill(str(value))
    spinbutton.press("Enter")
    expect(region(page, widget_key).get_by_role("spinbutton")).to_have_value(
        str(value)
    )


def set_checkbox(page: Page, widget_key: str, *, checked: bool) -> None:
    """Set a keyed Streamlit checkbox using its visible label."""

    container = region(page, widget_key)
    checkbox = container.get_by_role("checkbox")
    expect(checkbox).to_have_count(1)
    if checkbox.is_checked() != checked:
        container.locator("label").click()
    if checked:
        expect(checkbox).to_be_checked()
    else:
        expect(checkbox).not_to_be_checked()


def download_from_region(
    page: Page,
    widget_key: str,
    destination: Path,
    *,
    expected_filename: str,
) -> Path:
    """Download one artifact from a keyed region and save it deterministically."""

    with page.expect_download(timeout=30_000) as download_info:
        region(page, widget_key).get_by_role("button").click()
    download = download_info.value
    assert download.suggested_filename == expected_filename
    destination.parent.mkdir(parents=True, exist_ok=True)
    download.save_as(destination)
    return destination


def activate_project_workspace(page: Page) -> None:
    """Show the Project tab after a Streamlit rerun."""

    tab = page.get_by_role("tab", name="Project workspace", exact=True)
    tab.click()
    expect(tab).to_have_attribute("aria-selected", "true")


def assert_no_app_exception(page: Page) -> None:
    """Assert that Streamlit did not render an application exception."""

    expect(page.locator('[data-testid="stException"]')).to_have_count(0)
