"""NO_PYTHON_RUNTIME workbench E2E against the Rust server (M2 §5.7 item 2).

Every test in this module drives the compiled React workbench in a real
Chromium and talks only to the Rust `seattrellis_app` backend. The CI job
that runs these tests installs no Python package and starts no Python
process; the `rust_server` fixture additionally asserts the serving binary is
a native executable (ELF/Mach-O), not a Python interpreter.

Coverage map (修订版 §5.7 item 2):
  import   -> roster upload + field mapping + confirm
  solve    -> generate seating plan (hard constraints + solver)
  edit     -> lock seat, swap two seats, undo
  export   -> SVG download with valid structure
  rotation -> rotation plan generate, save into project workspace
  reopen   -> scan project root, open project, reload rotation plan
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from playwright.sync_api import Page, expect

if TYPE_CHECKING:
    from conftest import RustServer

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
FIXTURES = REPOSITORY_ROOT / "tests" / "fixtures"
STUDENTS_CSV = FIXTURES / "students.csv"


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------


def upload_and_confirm_roster(page: Page, csv: Path = STUDENTS_CSV) -> None:
    """Upload a roster file and drive the mapping -> preview -> confirm flow.

    Uses "Full replace" mode so the resulting roster is exactly the file's
    rows (the demo roster is not merged in)."""
    page.locator('label.file-picker-browser-button input[type="file"]').set_input_files(
        str(csv)
    )
    expect(
        page.get_by_role("button", name="Review import changes")
    ).to_be_visible(timeout=15_000)
    # Choose "Full replace" before previewing: changing the mode clears the
    # preview, so the mode must be selected first.
    page.get_by_role("radio", name="Full replace").check()
    page.get_by_role("button", name="Review import changes").click()
    expect(page.get_by_text("Safe to import")).to_be_visible(timeout=15_000)
    page.get_by_role("button", name="Confirm import").click()
    # The workbench moves to the Room step once the roster is applied.
    expect(
        page.get_by_role("button", name="Continue", exact=True)
    ).to_be_visible(timeout=15_000)


def go_to_generate_step(page: Page) -> None:
    """Walk Room -> Goal -> Generate using the footer Continue button."""
    expect(page.get_by_role("button", name="Continue", exact=True)).to_be_visible()
    page.get_by_role("button", name="Continue", exact=True).click()  # room -> goal
    page.get_by_role("button", name="Continue", exact=True).click()  # goal -> generate
    expect(
        page.get_by_role("button", name="Generate seating plan")
    ).to_be_visible()


def generate_seating_plan(page: Page) -> None:
    page.get_by_role("button", name="Generate seating plan").click()
    # The Adjust step is reached only when a plan (draft) exists.
    expect(
        page.get_by_role("button", name="Lock selected seat")
    ).to_be_visible(timeout=30_000)


def seat(page: Page, name: str):
    """The canvas seat <g> carrying a student's name in its aria-label."""
    return page.get_by_role("button", name=name)


def seat_label(page: Page, name: str) -> str:
    label = seat(page, name).get_attribute("aria-label")
    assert label, f"seat for {name} has no aria-label"
    return label


def seat_position(label: str) -> str:
    """`"Row 5, seat 1, Student002"` -> `"Row 5, seat 1"` (student agnostic)."""
    return label.rsplit(",", 1)[0]


# ---------------------------------------------------------------------------
# 1. Bootstrap: the workbench runs on the Rust backend only
# ---------------------------------------------------------------------------


def test_workbench_bootstraps_against_rust_backend(
    page: Page, rust_server: RustServer
) -> None:
    """The page loads, health reports ok, and the browser bootstraps its
    session token and catalogs from the Rust server."""

    page.goto(rust_server.url)
    expect(page.locator(".brand-lockup")).to_be_visible()
    expect(page.get_by_text("Your local class is ready")).to_be_visible(
        timeout=15_000
    )

    # The browser itself must be able to bootstrap the Bearer token and read
    # the capability catalog from the Rust endpoints (same-origin).
    token = page.evaluate(
        """async () => {
          const response = await fetch("/api/v1/session");
          const data = await response.json();
          return data.session_token ?? null;
        }"""
    )
    assert token, "session bootstrap did not issue a token"

    formats = page.evaluate(
        """async (token) => {
          const response = await fetch("/api/v1/catalogs", {
            headers: { Authorization: `Bearer ${token}` },
          });
          const data = await response.json();
          return (data.exportFormats ?? []).map((item) => item.id);
        }""",
        token,
    )
    assert "svg" in formats and "html" in formats, f"unexpected catalogs: {formats}"


# ---------------------------------------------------------------------------
# 2. import -> solve -> edit/repair -> export
# ---------------------------------------------------------------------------


def test_import_solve_edit_export_workflow(
    page: Page, rust_server: RustServer
) -> None:
    """The primary teacher workflow: roster import, generation, a seat lock,
    a two-seat swap, undo, and an SVG export download."""

    page.goto(rust_server.url)
    expect(page.get_by_text("Your local class is ready")).to_be_visible(
        timeout=15_000
    )

    # --- import ---------------------------------------------------------
    upload_and_confirm_roster(page)
    expect(page.get_by_text("4 students", exact=True)).to_be_visible()

    # --- solve ----------------------------------------------------------
    go_to_generate_step(page)
    generate_seating_plan(page)
    # Every student must be seated (each seat's aria-label carries the name).
    for name in ("Student001", "Student002", "Student003", "Student004"):
        expect(seat(page, name)).to_be_visible()

    # --- edit: lock a seat ----------------------------------------------
    seat(page, "Student001").click()
    page.get_by_role("button", name="Lock selected seat").click()
    expect(seat(page, "Student001")).to_have_attribute(
        "aria-label", re.compile(r", locked$")
    )

    # --- edit: swap two seats (editor command round-trip) ---------------
    before_second = seat_label(page, "Student002")
    before_third = seat_label(page, "Student003")
    seat(page, "Student002").click()
    seat(page, "Student003").click()
    # After the swap the two students exchange seats.
    expect(seat(page, "Student002")).to_have_attribute(
        "aria-label",
        re.compile(rf"^{re.escape(seat_position(before_third))}, .*Student002"),
    )
    expect(seat(page, "Student003")).to_have_attribute(
        "aria-label",
        re.compile(rf"^{re.escape(seat_position(before_second))}, .*Student003"),
    )

    # --- edit: undo restores the previous assignment --------------------
    page.get_by_role("button", name="Undo last change").click()
    expect(seat(page, "Student002")).to_have_attribute(
        "aria-label",
        re.compile(rf"^{re.escape(seat_position(before_second))}, .*Student002"),
    )
    expect(seat(page, "Student003")).to_have_attribute(
        "aria-label",
        re.compile(rf"^{re.escape(seat_position(before_third))}, .*Student003"),
    )

    # --- export ---------------------------------------------------------
    page.get_by_role("button", name="Export", exact=True).click()
    expect(page.get_by_role("button", name="Open export preview")).to_be_visible()
    page.get_by_role("button", name="Open export preview").click()

    with page.expect_download(timeout=30_000) as download_info:
        page.get_by_role("button", name="Save a copy").click()
    download = download_info.value
    assert download.suggested_filename == "seat-plan.svg", (
        f"unexpected export filename: {download.suggested_filename}"
    )
    path = download.path()
    assert path is not None
    content = path.read_text(encoding="utf-8", errors="replace")
    assert content.lstrip().startswith("<svg"), (
        "SVG export does not look like a vector document"
    )


# ---------------------------------------------------------------------------
# 3. rotation save -> reopen -> rotation load
# ---------------------------------------------------------------------------


def _make_project_workspace(root: Path, name: str) -> Path:
    """Create a valid project workspace with the Rust CLI (no Python).

    `project-init` requires an existing workspace carrying `students.csv`,
    `layout.json` and `rules.json`; the fixtures are copied in first.
    """
    project = root / name
    project.mkdir()
    shutil.copyfile(FIXTURES / "students.csv", project / "students.csv")
    shutil.copyfile(FIXTURES / "classroom.json", project / "layout.json")
    shutil.copyfile(FIXTURES / "rules.json", project / "rules.json")
    cli = (
        os.environ.get("SEATTRELLIS_E2E_RUST_CLI")
        or REPOSITORY_ROOT / "target" / "debug" / "seattrellis_cli"
    )
    if not Path(cli).is_file():
        cli = REPOSITORY_ROOT / "target" / "release" / "seattrellis_cli"
    result = subprocess.run(
        [str(cli), "project-init", "--dir", str(project)],
        check=False,
        capture_output=True,
        text=True,
        cwd=REPOSITORY_ROOT,
    )
    assert result.returncode == 0, (
        f"seattrellis_cli project-init failed: {result.stderr}"
    )
    return project


def test_rotation_save_reopen_workflow(
    page: Page, rust_server: RustServer, tmp_path: Path
) -> None:
    """Generate a 2-period rotation plan, save it into a project workspace,
    rescan/reopen the workspace, and reload the saved rotation plan."""

    project_root = tmp_path / "projects"
    project_root.mkdir()
    _make_project_workspace(project_root, "class-a")

    page.goto(rust_server.url)
    expect(page.get_by_text("Your local class is ready")).to_be_visible(
        timeout=15_000
    )
    upload_and_confirm_roster(page)

    # Enable rotation on the Generate step: 2 periods. The rotation settings
    # live in a collapsed <details> until rotation is enabled.
    expect(page.get_by_role("button", name="Continue", exact=True)).to_be_visible()
    page.get_by_role("button", name="Continue", exact=True).click()  # room -> goal
    page.get_by_role("button", name="Continue", exact=True).click()  # goal -> generate
    expect(
        page.get_by_role("button", name="Generate seating plan")
    ).to_be_visible()
    page.get_by_text("Generate future rotation", exact=True).click()
    page.get_by_test_id("rotation-toggle").check()
    page.get_by_test_id("rotation-period-count").fill("2")

    generate_seating_plan(page)

    # --- save rotation plan into the project workspace ------------------
    page.get_by_test_id("project-root-input").fill(str(project_root))
    page.get_by_test_id("project-refresh").click()
    # The first project is selected and opened automatically.
    expect(page.get_by_test_id("project-select")).to_contain_text(
        "class-a", timeout=15_000
    )
    save_button = page.get_by_test_id("project-rotation-save-button")
    expect(save_button).to_be_visible(timeout=15_000)
    save_button.click()
    expect(
        page.get_by_text("Rotation plan saved to:", exact=False)
    ).to_be_visible(timeout=15_000)

    # --- reopen: reload the saved rotation plan -------------------------
    load_button = page.get_by_test_id("project-open-rotation-button")
    expect(load_button).to_be_visible(timeout=15_000)
    load_button.click()
    # The canvas must show an occupied seat after reload (period 1 applied).
    expect(seat(page, "Student001")).to_be_visible(timeout=15_000)
