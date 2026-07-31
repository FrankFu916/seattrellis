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
