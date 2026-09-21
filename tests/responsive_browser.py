"""Check the production UI fixture at embedded and desktop widths."""

from __future__ import annotations

import argparse
from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default=None)
    parser.add_argument("--output-dir", type=Path, default=None)
    args = parser.parse_args()
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
            for width in (492, 760, 850, 1440):
                page = browser.new_page(viewport={"width": width, "height": 900})
                try:
                    errors: list[str] = []
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.set_content((ROOT / "ui-preview.html").read_text(), wait_until="load")
                    page.wait_for_function("window.cetzStudioDebug && window.cetzStudioDebug().basis !== null")
                    check(
                        page.evaluate("document.documentElement.scrollWidth <= innerWidth && document.body.scrollWidth <= innerWidth"),
                        f"{width}px has no document-level horizontal overflow",
                    )
                    for selector in ("#render", "#zoom-out", "#zoom-in", "#fit"):
                        page.locator(selector).scroll_into_view_if_needed()
                        check(page.locator(selector).is_visible(), f"{width}px reaches {selector}")

                    if width <= 1000:
                        for name, selector in (("project", "#project-toggle"), ("elements", "#elements-toggle"), ("inspector", "#inspector-toggle")):
                            toggle = page.locator(selector)
                            check(toggle.is_visible(), f"{width}px exposes the {name} drawer toggle")
                            toggle.click()
                            panel = page.locator("#inspector-panel" if name == "inspector" else "#project-panel")
                            check(panel.is_visible(), f"{width}px opens the {name} drawer")
                            check(toggle.get_attribute("aria-expanded") == "true", f"{width}px marks {name} expanded")
                            page.keyboard.press("Escape")
                            check(page.locator(selector).evaluate("node => document.activeElement === node"), f"{width}px restores focus to {name} toggle")
                        for selector in ("#project-root", "#project-filter", "#check-project", "#nodes-tab", "#edges-tab", "#parameters-tab"):
                            page.locator("#project-toggle").click()
                            check(page.locator(selector).is_visible(), f"{width}px reaches {selector} in a drawer")
                            page.keyboard.press("Escape")
                    else:
                        check(page.locator("#panel-toggle-bar").is_hidden(), "1440px keeps the drawer bar out of the desktop layout")
                        check(page.locator("#project-panel").is_visible() and page.locator("#inspector-panel").is_visible(), "1440px keeps both sidebars visible")
                    check(not errors, f"{width}px has no browser exceptions")
                    if args.output_dir:
                        args.output_dir.mkdir(parents=True, exist_ok=True)
                        page.screenshot(path=str(args.output_dir / f"responsive-{width}.png"), full_page=True)
                finally:
                    page.close()
        finally:
            browser.close()
    print({"evidence": "responsive-browser-fixture", "passed": len(checks), "checks": checks})


if __name__ == "__main__":
    main()
