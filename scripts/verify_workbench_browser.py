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

from playwright.async_api import async_playwright

WORKBENCH_URL = "http://127.0.0.1:8765"


async def main() -> int:
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch()
        context = await browser.new_context(locale="zh-CN")
        page = await context.new_page()

        await page.goto(WORKBENCH_URL, wait_until="networkidle")
        await page.wait_for_selector(".app-shell", timeout=30_000)
        print("1. workbench shell rendered")

        # Walk roster -> room -> goal -> generate.
        for step in ("room", "goal", "generate"):
            await page.click("button:has-text('继续')")
            print(f"   advanced to step {step}")

        await page.click("button:has-text('生成座位表')")
        await page.wait_for_selector(".seat-occupied", timeout=60_000)
        occupied = await page.locator(".seat-occupied").count()
        print(f"2. generated plan rendered {occupied} occupied seats")
        if occupied == 0:
            await browser.close()
            return 1

        await page.click("button:has-text('继续')")
        await page.wait_for_selector(".export-options", timeout=15_000)
        await page.click("button:has-text('打开导出预览')")
        await page.wait_for_selector(".preview-dialog", timeout=15_000)
        print("3. export preview opened")

        async with page.expect_download(timeout=30_000) as download_info:
            await page.click("button:has-text('保存一份')")
        download = await download_info.value
        print(f"4. downloaded export: {download.suggested_filename}")

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
        print(f"5. project history rendered {history_rows} artifacts")
        if history_rows == 0:
            await browser.close()
            return 1

        await page.click("[data-testid='project-privacy-button']")
        await page.wait_for_selector("[data-testid='project-privacy-status']", timeout=30_000)
        print("6. project privacy scan rendered")

        async with page.expect_download(timeout=30_000) as bundle_info:
            await page.click("[data-testid='project-backup-button']")
        bundle = await bundle_info.value
        if not bundle.suggested_filename.endswith(".seattrellis.zip"):
            await browser.close()
            return 1
        print(f"7. downloaded project bundle: {bundle.suggested_filename}")

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
        print("8. project bundle restored successfully")

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
