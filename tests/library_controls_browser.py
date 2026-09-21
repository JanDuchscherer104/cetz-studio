#!/usr/bin/env python3
"""Exercise automatic library controls through the real browser/server/compiler.

Reuse the native harness. Fixtures are copied to a disposable project; no
repository source or scientific dataset is edited. SVG/PNG output is evidence
of native editing, not scientific validity or per-object gesture support.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile

from playwright.sync_api import sync_playwright

from native_browser import (
    ROOT, apply_parameter, browser_snapshot, open_page, require_file,
    running_app, save_source, screenshot, wait_revision,
)


@dataclass(frozen=True)
class Case:
    """One literal mutation with an exact, independently stated source delta."""

    name: str
    argument: str
    value: str
    old: str
    replacement: str


CASES = (
    Case("cetz", "length", "10", "length: 8mm", "length: 10mm"),
    Case("fletcher", "width", "32", "width: 28mm", "width: 32mm"),
    Case("scenery", "azimuth", "50", "azimuth: 30deg", "azimuth: 50deg"),
    Case("plotsy", "scale-dim[2]", "0.8", "(1, 1, 0.5)", "(1, 1, 0.8)"),
    Case("maquette", "azimuth", "60", "azimuth: 30", "azimuth: 60"),
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=ROOT / ".ci/library-controls/browser")
    args = parser.parse_args()
    binary = require_file(args.binary, "--binary")
    output = args.output_dir.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    checks: list[str] = []
    errors: list[str] = []
    complete = False

    def check(condition: bool, description: str) -> None:
        assert condition, description
        checks.append(description)

    try:
        with tempfile.TemporaryDirectory(prefix="cetz-library-controls-") as temporary:
            project = Path(temporary)
            for source in (ROOT / "tests/fixtures/library-controls").iterdir():
                shutil.copy2(source, project / source.name)
            environment = os.environ.copy()
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(headless=True, args=["--no-sandbox"])
                try:
                    for case in CASES:
                        source = project / f"{case.name}.typ"
                        original = source.read_text(encoding="utf-8")
                        expected = original.replace(case.old, case.replacement, 1)
                        check(expected != original and "studio.param" not in original,
                              f"{case.name}: fixture uses existing library calls, not declarations")
                        with running_app(binary, project, source, output / f"{case.name}.log", environment) as app:
                            page = open_page(browser, app.origin, case.name, errors)
                            before = browser_snapshot(page)
                            check(before["preview_current"] and len(before["pages"]) == 1,
                                  f"{case.name}: native single-page preview is current")
                            parameter = next(item for item in before["parameters"]
                                             if item.get("origin", {}).get("argument") == case.argument)
                            check(page.locator("#fixture-banner").is_hidden(),
                                  f"{case.name}: no synthetic preview")
                            page.locator("#parameters-tab").click()
                            page.locator(f'[data-element="{parameter["id"]}"]').click()
                            pixels = page.locator("#figure").screenshot(path=str(output / f"{case.name}-before.png"))
                            edited = apply_parameter(page, parameter["id"], case.value)
                            check(edited["source"] == expected,
                                  f"{case.name}: only the intended literal changed")
                            check(edited["preview_current"] and edited["pages"] != before["pages"],
                                  f"{case.name}: native render reflects the edit")
                            check(source.read_text(encoding="utf-8") == original,
                                  f"{case.name}: unsaved editing leaves disk untouched")
                            screenshot(page, output, f"{case.name}-controls")
                            changed_pixels = page.locator("#figure").screenshot(path=str(output / f"{case.name}-after.png"))
                            check(changed_pixels != pixels, f"{case.name}: mounted figure pixels change")
                            page.locator("#undo").click()
                            undone = wait_revision(page, edited["revision"])
                            check(undone["source"] == original, f"{case.name}: undo restores the source")
                            page.locator("#redo").click()
                            redone = wait_revision(page, undone["revision"])
                            check(redone["source"] == expected, f"{case.name}: redo restores the edit")
                            saved = save_source(page)
                            check(not saved["dirty"] and source.read_text(encoding="utf-8") == expected,
                                  f"{case.name}: explicit save persists the edit")
                            check(any(path.read_text(encoding="utf-8") == original
                                      for path in project.rglob("*.bak")),
                                  f"{case.name}: original-source backup retained")
                            (output / f"{case.name}-source.diff").write_text(edited["diff"], encoding="utf-8")
                            page.close(run_before_unload=False)
                        with running_app(binary, project, source, output / f"{case.name}-reopen.log", environment) as app:
                            page = open_page(browser, app.origin, f"{case.name}-reopen", errors)
                            reopened = browser_snapshot(page)
                            check(reopened["source"] == expected and any(
                                item.get("origin", {}).get("argument") == case.argument
                                and item["value"] == float(case.value)
                                for item in reopened["parameters"]),
                                f"{case.name}: reopening rediscovers the saved value")
                            page.close(run_before_unload=False)
                finally:
                    browser.close()
            check(not errors, f"No browser errors: {errors}")
            complete = True
    finally:
        receipt = {
            "scope": "Actual Chromium -> Rust -> pinned Typst, disposable library fixtures",
            "complete": complete,
            "checks": checks,
            "browser_errors": errors,
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "checkout_sha": os.environ.get("GITHUB_SHA"),
        }
        (output / "checks.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"PASS: {len(checks)} native library-control browser checks")


if __name__ == "__main__":
    main()
