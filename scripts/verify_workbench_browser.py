"""Real-browser acceptance for the React workbench.

Starts the local workspace service, drives the workbench through the
teacher flow in Chromium, and verifies that generating a plan and saving
an export produce real artifacts.  Requires ``seattrellis[web]`` plus
Playwright with Chromium installed.

Run against a locally started service::

    python scripts/verify_workbench_browser.py
"""

from __future__ import annotations

import json
import re
import sys
import tempfile
from pathlib import Path

from playwright.async_api import async_playwright, expect
from seattrellis import cli

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
            await expect(page.get_by_role("button", name="确认导入")).to_be_disabled()
            await expect(
                page.get_by_text("Map at least one Student ID or Name column.")
            ).to_have_count(0)
            await page.locator("input[name='roster-mode']").nth(1).check()
            await page.locator(".roster-preview-button").click()
            await page.wait_for_selector(".preview-result", timeout=30_000)
            if await page.locator(".preview-result .preview-ok").count() == 0:
                await browser.close()
                return 1
            await page.locator(".roster-confirm-card button.primary-button").click()
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
        await page.locator(".constraints-card .secondary-button").first.click()
        print("6. common preference and hard constraint added")
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-generate", timeout=10_000)

        advanced = page.locator("details.advanced-settings")
        await advanced.wait_for(state="visible")
        await advanced.locator("summary").first.click()
        await page.wait_for_selector("details.advanced-settings select", timeout=10_000)
        with tempfile.TemporaryDirectory(prefix="seattrellis-browser-settings-") as settings_directory:
            settings_path = Path(settings_directory)
            rules_path = settings_path / "rules.json"
            rules_document = json.loads(
                (Path(__file__).resolve().parents[1] / "examples" / "rules.json").read_text(
                    encoding="utf-8"
                )
            )
            rules_document["hard"] = {
                "fixed_seats": [],
                "must_be_adjacent": [],
                "cannot_be_adjacent": [],
                "min_distance": [],
            }
            rules_path.write_text(
                json.dumps(rules_document),
                encoding="utf-8",
            )
            history_path = settings_path / "week1.snapshot.json"
            history_path.write_text(
                (Path(__file__).resolve().parents[1] / "examples" / "history" / "week1.snapshot.json").read_text(
                    encoding="utf-8"
                ),
                encoding="utf-8",
            )
            await page.locator("[data-testid='rules-json-file']").set_input_files(str(rules_path))
            await expect(
                page.locator("textarea[aria-labelledby='custom-rules-label']")
            ).to_have_value(re.compile("schema_version"))
            await page.locator("[data-testid='history-json-files']").set_input_files(str(history_path))
            await expect(page.locator(".history-loaded")).to_contain_text("1")
            print("7. rules JSON and history snapshot loaded")
        await advanced.locator("input[type='number']").nth(0).fill("2")
        await advanced.locator("input[type='number']").nth(1).fill("5")
        await page.locator("details.advanced-settings select").select_option("fallback")
        await advanced.locator("input[type='number']").nth(2).fill("17")
        print("8. advanced generation settings applied")
        detailed = page.locator("details.detailed-rules-settings")
        await detailed.locator("summary").click()
        await detailed.locator("[data-testid='detailed-rules-toggle']").check()
        await detailed.locator("select").nth(0).select_option("high_back")
        print("9. detailed score and history rules applied")
        rotation = page.locator("details.rotation-settings")
        await rotation.locator("summary").click()
        await page.locator("[data-testid='rotation-toggle']").check()
        await page.locator("[data-testid='rotation-period-count']").fill("2")
        await page.locator("[data-testid='rotation-period-labels']").fill("第 1 周，第 2 周")
        print("10. future rotation settings applied")
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-adjust", timeout=60_000)
        await page.wait_for_selector(".seat-occupied", timeout=15_000)
        occupied = await page.locator(".seat-occupied").count()
        await page.wait_for_selector("[data-testid='rotation-plan-summary']", timeout=15_000)
        print(f"11. generated plan rendered {occupied} occupied seats and rotation summary")
        if occupied == 0:
            await browser.close()
            return 1

        await page.locator("[data-testid='rotation-period-2']").click()
        await expect(page.locator("[data-testid='rotation-period-2']")).to_have_attribute(
            "aria-pressed", "true", timeout=30_000
        )
        print("12. switched to the second rotation period")

        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector("#panel-title-export", timeout=15_000)
        await page.wait_for_selector(".export-options", timeout=15_000)
        await page.locator(".panel-actions .primary-button").click()
        await page.wait_for_selector(".preview-dialog", timeout=15_000)
        print("13. export preview opened")

        async with page.expect_download(timeout=30_000) as download_info:
            await page.locator(".preview-dialog button.primary-button").click()
        download = await download_info.value
        print(f"14. downloaded export: {download.suggested_filename}")

        # The project panel is available alongside the main teacher flow. Use
        # a fresh temporary project so migration and restore are exercised
        # without modifying a checkout or a teacher's existing files.
        with tempfile.TemporaryDirectory(prefix="seattrellis-browser-project-") as directory:
            cli.init_demo(output_dir=Path(directory) / "class", overwrite=True)
            await page.locator("[data-testid='project-root-input']").fill(directory)
            await page.locator("[data-testid='project-root-input']").press("Enter")
            await page.wait_for_function(
                """() => {
                    const select = document.querySelector('[data-testid="project-select"]');
                    return select && select.options.length > 0 && Boolean(select.value);
                }""",
                timeout=30_000,
            )
            await page.wait_for_selector("[data-testid='project-history']", timeout=30_000)
            history_rows = await page.locator("[data-testid='project-history'] .project-artifact-row").count()
            print(f"15. project history rendered {history_rows} artifacts")
            if history_rows == 0:
                await browser.close()
                return 1

            await page.click("[data-testid='project-privacy-button']")
            await page.wait_for_selector("[data-testid='project-privacy-status']", timeout=30_000)
            print("16. project privacy scan rendered")

            # Compare two historical artifacts and create a new output snapshot.
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
                    print(f"17. project comparison failed: {await compare_error.text_content()}")
                    await browser.close()
                    return 1
                await page.wait_for_selector("[data-testid='project-compare-result']", timeout=30_000)
                print("18. project history comparison rendered")
                assignment_details = page.locator("[data-testid='project-assignment-details']")
                if await assignment_details.count():
                    await assignment_details.locator("summary").click()
                    await expect(assignment_details).to_contain_text("student-")
                    print("19. anonymous assignment changes expanded")
                await page.locator("[data-testid='project-restore-artifact-button']").click()
                await page.wait_for_function(
                    """() => {
                        const status = document.querySelector('[data-testid="project-status"]');
                        return Boolean(status && /restored|恢复/.test(status.textContent || ''));
                    }""",
                    timeout=30_000,
                )
                print("20. historical artifact restored as a new plan")

            await page.locator("[data-testid='project-migration-in-place']").check()
            await page.locator("[data-testid='project-migration-preview']").click()
            await page.wait_for_selector("[data-testid='project-migration-result']", timeout=30_000)
            await page.locator("[data-testid='project-migration-apply']").click()
            await page.wait_for_selector("[data-testid='project-migration-restore']", timeout=30_000)
            await page.locator("[data-testid='project-migration-restore']").click()
            await page.wait_for_function(
                """() => {
                    const status = document.querySelector('[data-testid="project-status"]');
                    return Boolean(status && /restored|恢复/.test(status.textContent || ''));
                }""",
                timeout=30_000,
            )
            print("21. project migration backup restored with a safety copy")

            async with page.expect_download(timeout=30_000) as bundle_info:
                await page.click("[data-testid='project-backup-button']")
            bundle = await bundle_info.value
            if not bundle.suggested_filename.endswith(".seattrellis.zip"):
                await browser.close()
                return 1
            print(f"22. downloaded project bundle: {bundle.suggested_filename}")

            with tempfile.TemporaryDirectory(prefix="seattrellis-browser-restore-") as restore_directory:
                bundle_path = Path(restore_directory) / bundle.suggested_filename
                await bundle.save_as(str(bundle_path))
                restore_target = Path(restore_directory) / "restored"
                await page.locator("[data-testid='project-restore-file']").set_input_files(str(bundle_path))
                await page.locator("[data-testid='project-restore-target']").fill(str(restore_target))
                await page.click("[data-testid='project-restore-button']")
                await page.wait_for_function(
                    """() => {
                        const status = document.querySelector('[data-testid="project-status"]');
                        return Boolean(status && /restored|恢复/.test(status.textContent || ''));
                    }""",
                    timeout=30_000,
                )
                if not (restore_target / "project.seattrellis.json").exists():
                    await browser.close()
                    return 1
            print("23. project bundle restored successfully")

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
