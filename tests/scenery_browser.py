"""Native Scenery control round trips using the existing browser test interface.

All writes use a disposable project. Pixel comparison rasterizes the server's
actual SVG in Chromium; it is not a synthetic fixture or a scientific review.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from typing import Any

from playwright.sync_api import Browser, Page, sync_playwright

from native_browser import (
    apply_parameter,
    browser_snapshot,
    open_page,
    require_file,
    running_app,
    save_source,
    screenshot,
    wait_idle,
    wait_revision,
)

ROOT = Path(__file__).resolve().parents[1]


def regions(browser: Browser, svg: str) -> tuple[bytes, bytes]:
    """Rasterize fixed halves of the actual, fixed-size native page."""
    page = browser.new_page(viewport={"width": 1100, "height": 900}, device_scale_factor=1)
    try:
        encoded = base64.b64encode(svg.encode()).decode("ascii")
        page.set_content(
            '<body style="margin:0;background:white">'
            '<img id="native" style="display:block;width:960px" '
            f'src="data:image/svg+xml;base64,{encoded}"></body>'
        )
        page.locator("#native").evaluate("image => image.decode()")
        box = page.locator("#native").bounding_box()
        if box is None:
            raise AssertionError("Native SVG has no browser bounds")
        if abs(box["height"] - 660) > 1:
            raise AssertionError(f"Expected the 160 by 110 mm fixture, got {box}")
        halves = [
            page.screenshot(clip={"x": x, "y": 0, "width": 480, "height": 660})
            for x in (0, 480)
        ]
        return halves[0], halves[1]
    finally:
        page.close()


def reject_parameter(page: Page, identifier: str, value: str) -> dict[str, Any]:
    """Attempt an invalid change through the same controls as the user."""
    page.locator("#parameters-tab").click()
    page.locator(f'[data-element="{identifier}"]').click()
    page.locator("#parameter-value").fill(value)
    page.locator("#apply-parameter").click()
    wait_idle(page)
    return browser_snapshot(page)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--chromium", type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    binary = require_file(args.binary, "--binary")
    chromium = require_file(args.chromium, "--chromium") if args.chromium else None
    checks: list[str] = []
    durations: dict[str, float] = {}
    errors: list[str] = []

    def check(condition: bool, description: str) -> None:
        if not condition:
            raise AssertionError(description)
        checks.append(description)

    with tempfile.TemporaryDirectory(prefix="studio-scenery-") as temporary:
        root = Path(temporary) / "project"
        examples = root / "examples"
        examples.mkdir(parents=True)
        shutil.copytree(ROOT / "typst" / "cetz-studio", root / "typst" / "cetz-studio")
        shutil.copytree(ROOT / "examples" / "data", examples / "data")
        source = examples / "studio-scenery.typ"
        shutil.copy2(ROOT / "examples" / source.name, source)
        original = source.read_text(encoding="utf-8")
        data_file = examples / "data" / "scenery-directions.json"
        data_before = data_file.read_bytes()
        evidence = args.output_dir.resolve() if args.output_dir else Path(temporary) / "evidence"
        evidence.mkdir(parents=True, exist_ok=True)
        environment = os.environ.copy()
        compiler = subprocess.check_output(["typst", "--version"], text=True).strip()
        receipt: dict[str, Any] = {
            "evidence": "native-browser-server-typst",
            "compiler": compiler,
            "scenery": "0.1.0",
            "platform": os.uname().sysname,
            "commit": os.environ.get("GITHUB_SHA", "local; record checkout separately"),
            "input_sha256": hashlib.sha256(data_before).hexdigest(),
            "source_sha256": hashlib.sha256(original.encode()).hexdigest(),
            "checks": checks,
            "seconds": durations,
            "visual_review": "not established by automated tests",
            "status": "failed",
        }
        try:
            with sync_playwright() as playwright:
                options: dict[str, Any] = {"headless": True, "args": ["--no-sandbox"]}
                if chromium:
                    options["executable_path"] = str(chromium)
                browser = playwright.chromium.launch(**options)
                try:
                    started = time.monotonic()
                    with running_app(binary, root, source, evidence / "session.log", environment) as app:
                        page = open_page(browser, app.origin, "scenery", errors)
                        durations["open"] = time.monotonic() - started
                        state = browser_snapshot(page)
                        check(state["mode"] == "parameter_editing", "Scenery opens with declared controls")
                        check(not state["capabilities"]["graph_gestures"], "No arbitrary 3D graph gestures")
                        check(len(state["pages"]) == 1, "Fixture fits its single fixed-size page")
                        check(page.locator("#fixture-banner").is_hidden(), "Preview is native, not synthetic")
                        check(len(state["parameters"]) == 4, "Exactly four display controls")
                        before_left, before_right = regions(browser, state["svg"])
                        (evidence / "before.svg").write_text(state["svg"], encoding="utf-8")
                        screenshot(page, evidence, "before")

                        expected = original
                        previous_left = before_left
                        changes = [
                            ("view-azimuth-deg", "-18", "studio.param(-38,", "studio.param(-18,"),
                            ("view-elevation-deg", "32", "studio.param(19,", "studio.param(32,"),
                            ("view-width", "40", "studio.param(45mm,", "studio.param(40mm,"),
                            ("show-guides", False, "studio.param(true,", "studio.param(false,"),
                        ]
                        for identifier, value, old, new in changes:
                            started = time.monotonic()
                            state = apply_parameter(page, identifier, value)
                            durations[identifier] = time.monotonic() - started
                            expected = expected.replace(old, new, 1)
                            check(state["source"] == expected, f"{identifier}: only requested literal changed")
                            check(source.read_text(encoding="utf-8") == original, f"{identifier}: disk untouched")
                            check(data_file.read_bytes() == data_before, f"{identifier}: input data unchanged")
                            left, right = regions(browser, state["svg"])
                            check(right == before_right, f"{identifier}: fixed companion pixels unchanged")
                            check(left != previous_left, f"{identifier}: editable view changes")
                            previous_left = left

                        (evidence / "after.svg").write_text(state["svg"], encoding="utf-8")
                        screenshot(page, evidence, "after")
                        before_invalid = state
                        error_start = len(errors)
                        state = reject_parameter(page, "view-azimuth-deg", "181")
                        check(state["source"] == before_invalid["source"] and state["revision"] == before_invalid["revision"], "Out-of-range control preserves draft/revision")
                        check(source.read_text(encoding="utf-8") == original, "Rejected value never writes disk")
                        # Filter only the expected rejected-request console message
                        # from this operation; retain all JavaScript exceptions.
                        errors[error_start:] = [e for e in errors[error_start:]
                            if not (e.startswith("scenery console:") and "409" in e)]

                        page.locator("#undo").click()
                        undone = wait_revision(page, state["revision"])
                        check(undone["source"] == expected.replace("studio.param(false,", "studio.param(true,", 1), "Undo restores guides exactly")
                        page.locator("#redo").click()
                        redone = wait_revision(page, undone["revision"])
                        check(redone["source"] == expected, "Redo restores the validated draft")
                        saved = save_source(page)
                        check(not saved["dirty"] and source.read_text(encoding="utf-8") == expected, "Explicit Save persists source")
                        backups = list(examples.rglob("*.bak"))
                        check(len(backups) == 1 and backups[0].read_text(encoding="utf-8") == original, "Backup retains exact original")
                        page.close(run_before_unload=False)

                    with running_app(binary, root, source, evidence / "reopen.log", environment) as app:
                        page = open_page(browser, app.origin, "reopen", errors)
                        reopened = browser_snapshot(page)
                        check(reopened["source"] == expected and not reopened["dirty"], "Reopen reads saved control values")
                        check(regions(browser, reopened["svg"])[1] == before_right, "Reopened companion pixels match baseline")
                        page.close(run_before_unload=False)

                    # Inject failure only in a disposable source, never in the shipped fixture.
                    rollback = examples / "scenery-rollback.typ"
                    rollback.write_text(original.replace(
                        '#let data = json(',
                        '#assert(view-azimuth-deg != 13, message: "deliberate test failure")\n#let data = json(', 1
                    ), encoding="utf-8")
                    with running_app(binary, root, rollback, evidence / "rollback.log", environment) as app:
                        page = open_page(browser, app.origin, "rollback", errors)
                        prior = browser_snapshot(page)
                        error_start = len(errors)
                        rejected = reject_parameter(page, "view-azimuth-deg", "13")
                        check(rejected["revision"] == prior["revision"] and rejected["source"] == prior["source"], "Compiler failure retains accepted draft/revision")
                        check(rejected["svg"] == prior["svg"], "Compiler failure retains accepted preview")
                        check(rollback.read_text(encoding="utf-8") == prior["source"], "Compiler failure retains disk source")
                        check(page.locator("#toast").is_visible(), "Compiler failure is visible to the user")
                        errors[error_start:] = [e for e in errors[error_start:]
                            if not (e.startswith("rollback console:") and "409" in e)]
                        page.close(run_before_unload=False)
                    check(not errors, f"No browser JavaScript errors: {errors}")
                    check(data_file.read_bytes() == data_before, "Frozen data unchanged after complete workflow")
                    receipt["status"] = "passed"
                finally:
                    browser.close()
        except Exception as error:
            receipt["error"] = str(error)
            raise
        finally:
            (evidence / "scenery-browser.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
            print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
