"""NO_PYTHON_RUNTIME workbench E2E against the Rust server (M2 §5.7 item 2).

Every test in this module drives the compiled React workbench in a real
Chromium and talks only to the Rust `seattrellis_web` backend. The CI job
that runs these tests installs no Python package and starts no Python
process; the `rust_server` fixture additionally asserts the serving binary is
a native executable (ELF/Mach-O), not a Python interpreter.

Coverage map (revised plan §5.7 item 2):
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
FIXTURES = REPOSITORY_ROOT / "e2e-rust" / "fixtures"
STUDENTS_CSV = FIXTURES / "students.csv"


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------


def upload_and_confirm_roster(page: Page, csv: Path = STUDENTS_CSV) -> None:
    """Upload a roster file and drive the mapping -> preview -> confirm flow.

    Uses "Full replace" mode so the resulting roster is exactly the file's
    rows (the demo roster is not merged in). The spreadsheet import lives
    behind the progressive-disclosure <details> once a roster exists
    (§19.25), so the panel is opened first when needed."""
    if page.locator(".roster-import-disclosure").count():
        disclosure = page.locator(".roster-import-disclosure")
        if not disclosure.get_attribute("open"):
            disclosure.locator("summary").click()
            expect(page.locator('label.file-picker-browser-button input[type="file"]')).to_be_visible()
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
    # The workbench moves to the Room step once the roster is applied; the
    # context action bar drives the next step (D1 design).
    expect(
        page.get_by_role("button", name="Set rules")
    ).to_be_visible(timeout=15_000)


def go_to_generate_step(page: Page) -> None:
    """Walk Room -> Rules -> Generate using the context-bar action
    buttons (D1: the footer was replaced by the context action bar)."""
    page.get_by_role("button", name="Set rules", exact=True).click()
    expect(
        page.get_by_role("button", name="Generate plan", exact=True)
    ).to_be_visible(timeout=15_000)
    page.get_by_role("button", name="Generate plan", exact=True).click()
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


def click_summary_by_text(page: Page, text: str) -> None:
    """Click a <details> summary by its computed center.

    Playwright's headless shell reports a zero box for a summary inside a
    closed <details> that lives in a scroll container, even though the
    element is laid out (getBoundingClientRect returns its real height).
    Scrolling it into view and clicking the computed center works around
    that box-model quirk; real browsers click it directly.
    """
    center = page.evaluate(
        """(text) => {
          const summary = [...document.querySelectorAll('summary')].find(
            (el) => el.textContent.trim() === text,
          );
          if (!summary) {
            return null;
          }
          summary.scrollIntoView({ block: 'center' });
          const rect = summary.getBoundingClientRect();
          return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
        }""",
        text,
    )
    assert center is not None, f"summary {text!r} not found"
    page.mouse.click(center["x"], center["y"])


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
    # §19.27: the user catalog converges on the seven usable entries;
    # plain `html` stays a backend-only contract format.
    assert "svg" in formats and "print-html" in formats, f"unexpected catalogs: {formats}"
    assert "html" not in formats, f"plain html must be hidden: {formats}"


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
    # Quick-export SVG straight from the context menu: the default export
    # is print-html now (D9), so the vector path is exercised explicitly.
    export_btn = page.get_by_role("button", name="Export", exact=True)
    print("EXPORT BTN COUNT:", export_btn.count())
    print("BTN ENABLED:", export_btn.is_enabled(), "VISIBLE:", export_btn.is_visible())
    print("BTN BOX:", export_btn.bounding_box())
    export_btn.evaluate("(el) => el.click()")
    page.wait_for_timeout(600)
    print("AFTER EVAL CLICK MENU OPEN:", page.locator(".ctx-menu").count())
    page.keyboard.press("Escape")
    page.wait_for_timeout(300)
    export_btn.click()
    page.wait_for_timeout(600)
    print("AFTER REAL CLICK MENU OPEN:", page.locator(".ctx-menu").count())
    print("MENU ITEMS:", page.get_by_role("menuitem").all_text_contents())
    print("DIALOG:", page.locator(".preview-dialog").count())
    print("BACKDROP:", page.locator(".dialog-backdrop").count())
    with page.expect_download(timeout=30_000) as download_info:
        page.get_by_role("menuitem", name=re.compile("^SVG", re.IGNORECASE)).click()
    download = download_info.value
    # Close the preview dialog the quick export opened (it covers the
    # context bar) so the settings entry stays reachable.
    page.keyboard.press("Escape")
    expect(page.get_by_role("button", name="Open export preview")).to_have_count(0)
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
# 2b. export defaults: real names by default, multi-format downloads
# ---------------------------------------------------------------------------


def test_export_defaults_carry_real_names(
    page: Page, rust_server: RustServer
) -> None:
    """The default template keeps real names in every format (§19.27
    regression: the old `public` default anonymized every export to the
    literal name '学生'), and the default print-html entry downloads a
    usable document."""

    page.goto(rust_server.url)
    expect(page.get_by_text("Your local class is ready")).to_be_visible(
        timeout=15_000
    )
    upload_and_confirm_roster(page)
    go_to_generate_step(page)
    generate_seating_plan(page)

    # --- quick-export PDF from the canvas context menu ------------------
    # The canvas view carries the export menu; the export view's primary
    # action is the preview button instead.
    page.get_by_role("button", name="Export", exact=True).click()
    with page.expect_download(timeout=60_000) as pdf_info:
        page.get_by_role("menuitem", name=re.compile("^PDF", re.IGNORECASE)).click()
    pdf_download = pdf_info.value
    assert pdf_download.suggested_filename == "seat-plan.pdf", (
        f"unexpected PDF filename: {pdf_download.suggested_filename}"
    )
    pdf_path = pdf_download.path()
    assert pdf_path is not None
    data = pdf_path.read_bytes()
    assert data.startswith(b"%PDF-"), "PDF export must start with the PDF magic"
    # §19.26: the page is a rasterized Image XObject at 144 DPI - a real
    # document carries the image payload (Flate/RunLength), not a stub.
    assert len(data) > 20_000, (
        f"PDF looks like a stub: {len(data)} bytes"
    )

    # --- default print-html download keeps real names -------------------
    # Open the export view from the menu, preview, then save the default
    # format (print-html, D9).
    page.get_by_role("button", name="Export", exact=True).click()
    page.get_by_role("menuitem", name="Layout & privacy settings").click()
    expect(page.get_by_role("button", name="Open export preview")).to_be_visible()
    page.get_by_role("button", name="Open export preview").click()
    with page.expect_download(timeout=30_000) as download_info:
        page.get_by_role("button", name="Save a copy").click()
    download = download_info.value
    assert download.suggested_filename == "seat-plan.print.html", (
        f"unexpected default export filename: {download.suggested_filename}"
    )
    path = download.path()
    assert path is not None
    html = path.read_text(encoding="utf-8", errors="replace")
    assert "Student001" in html, (
        "default teacher template must keep the real student name"
    )
    assert "学生A" not in html, (
        "default export must not be anonymized (public template regression)"
    )
    assert "<!doctype html" in html.lower(), "print-html must be a document"


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
    # The workbench generates on the standard-30 template (5 rows x 6 cols,
    # aisle after column 3); the project layout must carry the same seat ids
    # so rotation reload can rebuild the editable drafts.
    import json as _json

    layout = {
        "layout_id": "standard-30",
        "name": "30-seat classroom",
        "seats": [],
    }
    for row in range(1, 6):
        grid_col = 1
        for logical_col in range(1, 7):
            layout["seats"].append(
                {
                    "seat_id": f"R{row}C{grid_col}",
                    "row": row,
                    "col": grid_col,
                    "enabled": True,
                }
            )
            grid_col += 1
            if logical_col == 3:
                # standard-30 inserts a full-length aisle after column 3.
                layout["seats"].append(
                    {
                        "seat_id": f"AISLE-R{row}C{grid_col}",
                        "row": row,
                        "col": grid_col,
                        "enabled": False,
                    }
                )
                grid_col += 1
    project.joinpath("layout.json").write_text(_json.dumps(layout))
    shutil.copyfile(FIXTURES / "rules.json", project / "rules.json")
    cli = (
        os.environ.get("SEATTRELLIS_E2E_RUST_CLI")
        or REPOSITORY_ROOT / "target" / "debug" / "seattrellis"
    )
    if not Path(cli).is_file():
        cli = REPOSITORY_ROOT / "target" / "release" / "seattrellis"
    result = subprocess.run(
        [str(cli), "project-init", "--dir", str(project)],
        check=False,
        capture_output=True,
        text=True,
        cwd=REPOSITORY_ROOT,
    )
    assert result.returncode == 0, (
        f"seattrellis project-init failed: {result.stderr}"
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
    page.get_by_role("button", name="Set rules", exact=True).click()
    expect(
        page.get_by_role("button", name="Generate plan", exact=True)
    ).to_be_visible(timeout=15_000)
    page.get_by_role("button", name="Generate plan", exact=True).click()
    expect(
        page.get_by_role("button", name="Generate seating plan")
    ).to_be_visible()
    # Rotation lives inside the collapsed "Advanced settings" fold (D4);
    # open the fold first, then the rotation settings.
    click_summary_by_text(page, "Advanced settings")

    click_summary_by_text(page, "Generate future rotation")
    # The headless shell reports zero boxes for controls inside the just-
    # opened <details>; drive the native click instead (a real click toggles
    # the checkbox and fires the change React listens to).
    page.get_by_test_id("rotation-toggle").evaluate("(el) => el.click()")

    page.get_by_test_id("rotation-period-count").evaluate(
        """(el) => {
          el.value = "2";
          el.dispatchEvent(new Event("input", { bubbles: true }));
          el.dispatchEvent(new Event("change", { bubbles: true }));
        }"""
    )

    generate_seating_plan(page)

    # --- save rotation plan into the project workspace ------------------
    # The project tools live in the History / rotation view (D7), inside a
    # collapsed "Project tools" fold; navigate there and open it.
    page.get_by_role("button", name="History / rotation").click()
    expect(page.get_by_role("tab", name="Rotation plan")).to_be_visible(
        timeout=15_000
    )
    page.get_by_role("tab", name="Rotation plan").click()
    click_summary_by_text(page, "Project tools (backup / migration / restore)")
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
    # Saving refreshes the project; wait until the rotation artifact is
    # selected and the button is enabled, or the click can land during the
    # re-render and be swallowed.
    expect(load_button).to_be_enabled(timeout=15_000)
    load_button.click()
    page.wait_for_timeout(1500)
    print("STATE:", page.evaluate("""() => {
      const main = document.querySelector('.main-workspace');
      const labels = [...document.querySelectorAll('[data-seat-id]')].map(el => el.getAttribute('aria-label')).filter(Boolean).slice(0, 6);
      return { view: main?.className ?? 'none', seats: document.querySelectorAll('[data-seat-id]').length,
               labels,
               err: document.querySelector('[role=alert]')?.textContent ?? '' };
    }"""))
    # The canvas must show an occupied seat after reload (period 1 applied).
    s1 = seat(page, "Student001")
    s1.scroll_into_view_if_needed()
    expect(s1).to_be_visible(timeout=15_000)


# ---------------------------------------------------------------------------
# 4. Regression: context switch must not leave a demo-only room id behind
# ---------------------------------------------------------------------------


def test_context_switch_then_regenerate_still_works(
    page: Page, rust_server: RustServer
) -> None:
    """Regression for the room_not_found bug: switching from a saved class back
    to the scratch workspace used to reset the room id to the demo-only
    `compact` template, so the next generate always failed with
    'Unknown room template "compact"'. Reset must pick a real catalog room."""

    page.goto(rust_server.url)
    expect(page.get_by_text("Your local class is ready")).to_be_visible(
        timeout=15_000
    )
    upload_and_confirm_roster(page)

    # First generation (exercises the normal path).
    go_to_generate_step(page)
    generate_seating_plan(page)

    # Save the scratch draft as a class (G-5) — this moves into a class
    # context without resetting the draft.
    page.get_by_role("button", name="Save as class", exact=True).click()
    page.get_by_placeholder("e.g. Class 8–3").fill("My class")
    page.get_by_role("button", name="Save & open", exact=True).click()
    expect(
        page.get_by_role("button", name="My class created this session")
    ).to_be_visible(timeout=15_000)

    # Switch back to the scratch workspace — this triggers resetWorkbench(),
    # the path that used to hard-reset the room id to "compact".
    page.get_by_role("button", name=re.compile("Scratch workspace")).first.click()
    expect(page.get_by_text("Your local class is ready")).to_be_visible(
        timeout=15_000
    )

    # Regenerate: with the fix this succeeds; before it the request carried
    # template_id "compact" and the server answered room_not_found.
    # After resetWorkbench the view returns to the roster step (demo students),
    # so walk Room -> Rules -> Generate from the context action bar. The
    # generate_seating_plan assertion (reaching the Adjust step) is the
    # regression check: a room_not_found failure would never reach it.
    page.get_by_role("button", name="Choose classroom", exact=True).click()
    go_to_generate_step(page)
    generate_seating_plan(page)
