"""Verify Transparent mode against actual Typst page and foreground geometry."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import tempfile

from playwright.sync_api import sync_playwright

from native_browser import browser_snapshot, open_page, running_app


ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--chromium", type=Path)
    args = parser.parse_args()
    checks: list[str] = []
    errors: list[str] = []

    def check(condition: bool, description: str) -> None:
        if not condition:
            raise AssertionError(description)
        checks.append(description)

    with tempfile.TemporaryDirectory(prefix="studio-transparent-") as temporary:
        root = Path(temporary) / "project"
        root.mkdir()
        source = root / "transparent.typ"
        source.write_text(
            "#set page(width: 40mm, height: 30mm)\n"
            "#rect(width: 10mm, height: 10mm, fill: red)\n",
            encoding="utf-8",
        )
        log_path = root / "session.log"
        environment = os.environ.copy()
        with sync_playwright() as runtime:
            options = {"headless": True, "args": ["--no-sandbox"]}
            if args.chromium:
                options["executable_path"] = str(args.chromium)
            browser = runtime.chromium.launch(**options)
            try:
                with running_app(args.binary, root, source, log_path, environment) as app:
                    page = open_page(browser, app.origin, "transparent", errors)
                    try:
                        before = browser_snapshot(page)
                        check(page.locator("#fixture-banner").is_hidden(), "Native page is not the synthetic fixture")
                        shapes = page.locator("#figure svg > *")
                        check(shapes.count() >= 2, "Typst emits a page backdrop and foreground geometry")
                        check(page.locator("#figure .studio-page-background").count() == 1, "Typst page backdrop is identified by geometry")
                        check(page.locator("#figure svg > *:not(.studio-page-background)").count() >= 1, "Foreground geometry remains visible")
                        page.locator("#transparent-page").check()
                        check(page.locator("#paper").evaluate("node => node.classList.contains('transparent-page')"), "Transparent mode updates only the display class")
                        check(page.locator("#figure .studio-page-background").evaluate("node => getComputedStyle(node).visibility === 'hidden'"), "Transparent mode hides the page backdrop")
                        after = browser_snapshot(page)
                        check(after["revision"] == before["revision"] and not after["dirty"], "Transparent mode creates no source revision or dirty draft")
                        page.locator("#transparent-page").uncheck()
                        check(page.locator("#figure .studio-page-background").evaluate("node => getComputedStyle(node).visibility !== 'hidden'"), "Opaque mode restores the page backdrop")
                    finally:
                        page.close(run_before_unload=False)
            finally:
                browser.close()
    check(not errors, f"No native browser errors: {errors}")
    print({"evidence": "native-typst-browser", "compiler": "Typst 0.15.1", "passed": len(checks), "checks": checks})


if __name__ == "__main__":
    main()
