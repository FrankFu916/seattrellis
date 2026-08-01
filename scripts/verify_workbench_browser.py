"""Real-browser acceptance for the React workbench.

Starts the local workspace service, drives the workbench through the
teacher flow in Chromium, and verifies that generating a plan and saving
an export produce real artifacts.  Requires ``seattrellis[web]`` plus
Playwright with Chromium installed.

Run against a locally started service::

    python scripts/verify_workbench_browser.py
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

from playwright.async_api import async_playwright, expect

WORKBENCH_URL = "http://127.0.0.1:8765"


async def main() -> int:
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch()
        context = await browser.new_context(locale="zh-CN")
        page = await context.new_page()

        await page.goto(WORKBENCH_URL, wait_until="networkidle")
        await page.wait_for_selector(".app-shell", timeout=30_000)
        print("1. workbench shell rendered")

        # Use a real CSV and the full-replace path. This catches regressions in
        # the browser/API contract that a demo-only flow cannot see.
        with tempfile.TemporaryDirectory(prefix="seattrellis-browser-upload-") as directory:
            roster_path = Path(directory) / "class.csv"
            roster_path.write_text(
                "student_id,name,score,height_cm,needs\n"
                "S01,Alice,92,158,front\n"
                "S02,Bob,84,172,\n"
                "S03,Cara,78,164,hearing\n",
                encoding="utf-8",
            )
            await page.locator(".roster-import-panel input[type='file']").set_input_files(
                str(roster_path)
            )
            await page.wait_for_selector(".roster-mapping-section", timeout=30_000)
            await page.locator("input[name='roster-mode']").nth(1).check()
            await page.locator(".roster-mapping-section .secondary-button").click()
            await page.wait_for_selector(".preview-result", timeout=30_000)
            if await page.locator(".preview-result .preview-ok").count() == 0:
                await browser.close()
                return 1
            await page.locator(".preview-result button.primary-button").click()
            await page.wait_for_function(
                """() => document.querySelector('.app-header')?.textContent?.includes('3 名学生')""",
                timeout=30_000,
            )
        # Import moves the workflow to the room step. Return to the roster
        # step to verify that the imported records remain editable there.
        await page.locator(".step-navigation button").filter(has_text="名单").click()
        await page.wait_for_selector(".student-editor-row", timeout=10_000)
        # Make one in-place correction after import so the browser check also
        # covers the ordinary roster editor, not only file parsing.
        first_student_row = page.locator(".student-editor-row").nth(1)
        await first_student_row.locator("input").nth(1).fill("Alice Updated")
        await first_student_row.locator("input").nth(2).fill("93")
        print("3. CSV roster imported and student details edited")

        # Exercise the ordinary custom-room controls as well as the importer.
        await page.locator(".step-navigation button").filter(has_text="教室").click()
        await page.locator("[data-testid='custom-room-toggle']").check()
        custom_room = page.locator(".custom-room-fields")
        await custom_room.locator("input[type='number']").nth(0).fill("2")
        await custom_room.locator("input[type='number']").nth(1).fill("3")
        await custom_room.locator("input").nth(3).fill("2-3")
        print("4. custom classroom dimensions applied")

        # Use the visual editor once as well. This verifies that an irregular
        # room can be created without asking a teacher to write layout JSON.
        await page.locator("[data-testid='layout-editor-open']").click()
        await page.wait_for_selector(".layout-editor-grid", timeout=30_000)
        await page.locator(".layout-editor-grid .layout-cell").first.click()
        await page.locator(".layout-kind-button.kind-aisle").click()
        await page.locator("[data-testid='layout-editor-save']").click()
        await page.wait_for_selector(".layout-editor-status", timeout=30_000)
        print("5. visual classroom layout edited and saved")

        # Importing a roster advances the workflow to the room step.
        # Continue room -> goal -> generate.
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-goal", timeout=10_000)
        await page.locator(".preference-list input[type='checkbox']").first.check()
        await page.locator(".constraints-card .secondary-button").click()
        print("6. common preference and hard constraint added")
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-generate", timeout=10_000)

        advanced = page.locator("details.advanced-settings")
        await advanced.wait_for(state="visible")
        await advanced.locator("summary").click()
        await page.wait_for_selector("details.advanced-settings select", timeout=10_000)
        await advanced.locator("input[type='number']").nth(0).fill("2")
        await advanced.locator("input[type='number']").nth(1).fill("5")
        await page.locator("details.advanced-settings select").select_option("fallback")
        await advanced.locator("input[type='number']").nth(2).fill("17")
        print("7. advanced generation settings applied")
        detailed = page.locator("details.detailed-rules-settings")
        await detailed.locator("summary").click()
        await detailed.locator("[data-testid='detailed-rules-toggle']").check()
        await detailed.locator("select").nth(0).select_option("high_back")
        print("8. detailed score and history rules applied")
        rotation = page.locator("details.rotation-settings")
        await rotation.locator("summary").click()
        await page.locator("[data-testid='rotation-toggle']").check()
        await page.locator("[data-testid='rotation-period-count']").fill("2")
        await page.locator("[data-testid='rotation-period-labels']").fill("第 1 周，第 2 周")
        print("9. future rotation settings applied")
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-adjust", timeout=60_000)
        await page.wait_for_selector(".seat-occupied", timeout=15_000)
        occupied = await page.locator(".seat-occupied").count()
        await page.wait_for_selector("[data-testid='rotation-plan-summary']", timeout=15_000)
        print(f"10. generated plan rendered {occupied} occupied seats and rotation summary")
        if occupied == 0:
            await browser.close()
            return 1

        await page.locator("[data-testid='rotation-period-2']").click()
        await expect(page.locator("[data-testid='rotation-period-2']")).to_have_attribute(
            "aria-pressed", "true", timeout=30_000
        )
        print("11. switched to the second rotation period")

        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-export", timeout=15_000)
        await page.wait_for_selector(".export-options", timeout=15_000)
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector(".preview-dialog", timeout=15_000)
        print("12. export preview opened")

        async with page.expect_download(timeout=30_000) as download_info:
            await page.locator(".preview-dialog button.primary-button").click()
        download = await download_info.value
        print(f"13. downloaded export: {download.suggested_filename}")

        # The project panel is available alongside the main teacher flow. Use
        # the repository's example project so this check exercises the real
        # API, not only the component's demo fallback.
        project_select = page.locator("[data-testid='project-select']")
        await page.wait_for_function(
            """() => {
                const select = document.querySelector('[data-testid="project-select"]');
                return select && select.options.length > 0 && Boolean(select.value);
            }""",
            timeout=30_000,
        )
        await page.wait_for_selector("[data-testid='project-history']", timeout=30_000)
        history_rows = await page.locator("[data-testid='project-history'] .project-artifact-row").count()
        print(f"14. project history rendered {history_rows} artifacts")
        if history_rows == 0:
            await browser.close()
            return 1

        await page.click("[data-testid='project-privacy-button']")
        await page.wait_for_selector("[data-testid='project-privacy-status']", timeout=30_000)
        print("15. project privacy scan rendered")

        # Compare two historical artifacts and create a new output snapshot.
        # Older demo projects may only contain one artifact, so keep the
        # acceptance check useful for both a fresh checkout and a used project.
        compare_button = page.locator("[data-testid='project-compare-button']")
        if await compare_button.is_enabled():
            await compare_button.click()
            await page.wait_for_function(
                """() => Boolean(
                    document.querySelector('[data-testid="project-compare-result"]')
                    || document.querySelector('[data-testid="project-error"]')
                )""",
                timeout=30_000,
            )
            compare_error = page.locator("[data-testid='project-error']")
            if await compare_error.is_visible():
                print(f"15. project comparison failed: {await compare_error.text_content()}")
                await browser.close()
                return 1
            await page.wait_for_selector("[data-testid='project-compare-result']", timeout=30_000)
            print("16. project history comparison rendered")
            restore_artifact_button = page.locator(
                "[data-testid='project-restore-artifact-button']"
            )
            await restore_artifact_button.click()
            await page.wait_for_function(
                """() => {
                    const status = document.querySelector('[data-testid="project-status"]');
                    return Boolean(status && status.textContent?.includes('restored'));
                }""",
                timeout=30_000,
            )
            print("17. historical artifact restored as a new plan")
        else:
            print("16. project history comparison skipped (one artifact available)")

        async with page.expect_download(timeout=30_000) as bundle_info:
            await page.click("[data-testid='project-backup-button']")
        bundle = await bundle_info.value
        if not bundle.suggested_filename.endswith(".seattrellis.zip"):
            await browser.close()
            return 1
        print(f"18. downloaded project bundle: {bundle.suggested_filename}")

        with tempfile.TemporaryDirectory(prefix="seattrellis-browser-restore-") as directory:
            bundle_path = Path(directory) / bundle.suggested_filename
            await bundle.save_as(str(bundle_path))
            restore_target = Path(directory) / "restored"
            await page.locator("[data-testid='project-restore-file']").set_input_files(str(bundle_path))
            await page.locator("[data-testid='project-restore-target']").fill(str(restore_target))
            await page.click("[data-testid='project-restore-button']")
            await page.wait_for_function(
                """() => {
                    const status = document.querySelector('[data-testid="project-status"]');
                    return Boolean(status && status.textContent?.includes('restored'));
                }""",
                timeout=30_000,
            )
            if not (restore_target / "project.seattrellis.json").exists():
                await browser.close()
                return 1
        print("19. project bundle restored successfully")

        await browser.close()
        return 0


def asyncio_run() -> int:
    import asyncio

    return asyncio.run(main())


if __name__ == "__main__":
    try:
        raise SystemExit(asyncio_run())
    except KeyboardInterrupt:
        sys.exit(130)
