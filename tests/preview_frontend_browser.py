"""Exercise tentative preview lifecycle against the synthetic HTTP transport.

This is browser behavior evidence only; Rust and Typst are not executed.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default=None)
    args = parser.parse_args()
    html = (ROOT / "ui-preview.html").read_text(encoding="utf-8")
    html = html.replace(
        "window.CETZ_STUDIO_DEMO_SOURCE=",
        "window.CETZ_STUDIO_PREVIEW_TEST=true;window.CETZ_STUDIO_DEMO_SOURCE=",
        1,
    )
    checks: list[str] = []

    def check(condition: bool, description: str) -> None:
        if not condition:
            raise AssertionError(description)
        checks.append(description)

    with sync_playwright() as runtime:
        options = {"headless": True}
        if args.chromium:
            options["executable_path"] = args.chromium
        browser = runtime.chromium.launch(**options)
        try:
            page = browser.new_page(viewport={"width": 1480, "height": 1020})
            errors: list[str] = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.set_content(html, wait_until="load")
            page.wait_for_function("window.cetzStudioDebug && window.cetzStudioDebug().previewReady")
            page.locator("#nodes-tab").click()
            page.locator('[data-element="physical"]').click()
            accepted_fill = page.locator('#figure rect[fill="#d8eadd"]').count()
            check(accepted_fill > 0, "Accepted image is mounted before a tentative preview")
            check(page.locator("#overlay .node-hit").count() > 0, "Accepted geometry exposes hit targets before a tentative preview")

            source = page.locator("#node-title-source")
            source.fill("[It's fine]")
            page.wait_for_function("window.cetzStudioFixtureRequests.some(request => request.url === '/api/preview')")
            page.wait_for_function("window.cetzStudioDebug().previewVisible")
            check(page.locator('#figure rect[fill="#ffb000"]').count() > 0, "Completed candidate image is displayed")
            check(page.locator("#overlay .node-hit").count() == 0, "Tentative geometry has no accepted-source hit targets")
            check(page.locator("#node-title-source").input_value() == "[It's fine]", "Apostrophes in valid Typst content reach the preview scheduler")

            requests_before_selection = page.evaluate("window.cetzStudioFixtureRequests.filter(request => request.url === '/api/preview').length")
            page.locator('[data-element="target"]').click()
            page.wait_for_function("!window.cetzStudioDebug().previewVisible && window.cetzStudioDebug().preview.state === 'cancelled'")
            page.wait_for_timeout(500)
            requests_after_selection = page.evaluate("window.cetzStudioFixtureRequests.filter(request => request.url === '/api/preview').length")
            check(requests_after_selection == requests_before_selection, "Detached text fields cannot preview after selection changes")
            check(page.locator('#figure rect[fill="#d8eadd"]').count() > 0, "Selection changes restore the accepted image")
            check(page.locator("#overlay .node-hit").count() > 0, "Selection changes restore accepted geometry hit targets")
            check(not errors, f"No browser JavaScript errors: {errors}")
            print({"evidence": "tentative-preview-browser-fixture", "passed": len(checks), "checks": checks})
        finally:
            browser.close()


if __name__ == "__main__":
    main()
