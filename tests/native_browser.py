"""Exercise the real browser, Rust server, and Typst compiler end to end.

The runner copies every source into a disposable project root, starts one local
server per scenario on an ephemeral loopback port, and terminates every process
it creates. It never opens or saves the repository examples themselves.

Run with the repository's Playwright environment, for example:

    /path/to/python tests/native_browser.py \
      --binary /absolute/path/to/cetz-studio \
      --chromium /absolute/path/to/chrome \
      --output-dir /tmp/cetz-studio-native-browser
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
from typing import Any, Iterator
from urllib.error import URLError
from urllib.request import urlopen

from playwright.sync_api import Browser, Page, sync_playwright


ROOT = Path(__file__).resolve().parents[1]
HOST = "127.0.0.1"
WAIT_MS = 45_000


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument(
        "--chromium",
        type=Path,
        help="Browser executable; omit to use Playwright's managed Chromium.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="Retain screenshots, server logs, and the JSON result here.",
    )
    return parser.parse_args()


def require_file(path: Path, option: str) -> Path:
    path = path.expanduser().resolve()
    if not path.is_file():
        raise SystemExit(f"{option} is not a file: {path}")
    return path


def reserve_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind((HOST, 0))
        return int(listener.getsockname()[1])


def api_state(origin: str, timeout: float = 30.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            with urlopen(f"{origin}/api/state", timeout=1.0) as response:
                return json.load(response)
        except (OSError, URLError, TimeoutError, json.JSONDecodeError) as error:
            last_error = error
            threading.Event().wait(0.05)
    raise AssertionError(f"server did not become ready at {origin}: {last_error}")


class NativeApp:
    def __init__(
        self,
        binary: Path,
        root: Path,
        source: Path,
        log_path: Path,
        environment: dict[str, str],
    ) -> None:
        self.port = reserve_port()
        self.origin = f"http://{HOST}:{self.port}"
        self.log_path = log_path
        self._log = log_path.open("w", encoding="utf-8")
        self.process = subprocess.Popen(
            [
                str(binary),
                "--file",
                str(source),
                "--root",
                str(root),
                "--port",
                str(self.port),
                "--compile-timeout",
                "45",
            ],
            cwd=root,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=self._log,
            stderr=subprocess.STDOUT,
            text=True,
        )

    def wait_ready(self) -> None:
        deadline = time.monotonic() + 45.0
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                self._log.flush()
                details = self.log_path.read_text(encoding="utf-8", errors="replace")
                raise AssertionError(
                    f"server exited with {self.process.returncode} before readiness:\n{details}"
                )
            try:
                api_state(self.origin, timeout=0.5)
                return
            except AssertionError:
                threading.Event().wait(0.05)
        raise AssertionError(f"server readiness timed out: {self.origin}")

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self._log.close()


@contextmanager
def running_app(
    binary: Path,
    root: Path,
    source: Path,
    log_path: Path,
    environment: dict[str, str],
) -> Iterator[NativeApp]:
    app = NativeApp(binary, root, source, log_path, environment)
    try:
        app.wait_ready()
        yield app
    finally:
        app.close()


def browser_snapshot(page: Page) -> dict[str, Any]:
    return page.evaluate(
        """async () => {
            const response = await fetch('/api/state');
            if (!response.ok) throw new Error(`state request failed: ${response.status}`);
            return (await response.json()).snapshot;
        }"""
    )


def wait_condition(page: Page, expression: str, timeout_ms: int = WAIT_MS) -> None:
    """Poll through the automation protocol without requiring page `unsafe-eval`."""
    deadline = time.monotonic() + timeout_ms / 1000
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            if page.evaluate(f"() => Boolean({expression})"):
                return
        except Exception as error:  # The document may still be navigating.
            last_error = error
        threading.Event().wait(0.025)
    suffix = f": {last_error}" if last_error else ""
    raise AssertionError(f"browser condition timed out ({expression}){suffix}")


def wait_idle(page: Page) -> None:
    wait_condition(page, "window.cetzStudioDebug && !window.cetzStudioDebug().busy")


def wait_revision(page: Page, previous: int) -> dict[str, Any]:
    wait_condition(
        page,
        "window.cetzStudioDebug && !window.cetzStudioDebug().busy "
        f"&& window.cetzStudioDebug().revision > {previous}",
    )
    return browser_snapshot(page)


def open_page(
    browser: Browser,
    origin: str,
    case: str,
    browser_errors: list[str],
) -> Page:
    page = browser.new_page(
        viewport={"width": 1480, "height": 1020},
        device_scale_factor=1,
    )
    page.on("pageerror", lambda error: browser_errors.append(f"{case}: {error}"))
    page.on(
        "console",
        lambda message: browser_errors.append(f"{case} console: {message.text}")
        if message.type == "error"
        else None,
    )
    page.goto(origin, wait_until="domcontentloaded", timeout=WAIT_MS)
    wait_condition(
        page,
        "window.cetzStudioDebug && window.cetzStudioDebug().revision !== undefined",
    )
    wait_idle(page)
    return page


def screenshot(page: Page, output_dir: Path, name: str) -> None:
    page.screenshot(path=str(output_dir / f"{name}.png"), full_page=True)


def apply_parameter(page: Page, parameter_id: str, value: str | bool) -> dict[str, Any]:
    page.locator("#parameters-tab").click()
    page.locator(f'[data-element="{parameter_id}"]').click()
    field = page.locator("#parameter-value")
    if isinstance(value, bool):
        if field.is_checked() != value:
            page.locator("label").filter(has=field).click()
    else:
        field.fill(value)
    revision = int(page.evaluate("window.cetzStudioDebug().revision"))
    page.locator("#apply-parameter").click()
    return wait_revision(page, revision)


def save_source(page: Page) -> dict[str, Any]:
    revision = int(page.evaluate("window.cetzStudioDebug().revision"))
    page.locator("#save").click()
    return wait_revision(page, revision)


def prepare_project(root: Path) -> dict[str, Path]:
    examples = root / "examples"
    package = root / "typst" / "cetz-studio"
    examples.mkdir(parents=True)
    package.parent.mkdir(parents=True)
    shutil.copy2(ROOT / "examples" / "demo.typ", examples / "demo.typ")
    # Exercise the real fixed-origin warning, not only fixture-supplied text.
    demo = examples / "demo.typ"
    demo.write_text(
        demo.read_text(encoding="utf-8").replace(
            "edge(<query-fork>, (52mm, -55mm)",
            "edge((71mm, -55mm), (52mm, -55mm)",
        ),
        encoding="utf-8",
    )
    shutil.copy2(ROOT / "examples" / "studio-cetz.typ", examples / "studio-cetz.typ")
    shutil.copytree(ROOT / "typst" / "cetz-studio", package)

    view_only = examples / "view-only.typ"
    view_only.write_text(
        '#import "@preview/cetz:0.5.2" as cetz\n'
        '#set page(width: auto, height: auto, margin: 4mm)\n'
        '#cetz.canvas(length: 8mm, { import cetz.draw: circle; '
        'circle((0, 0), radius: 1, fill: rgb("D8EADD")) })\n',
        encoding="utf-8",
    )
    multipage = examples / "multipage.typ"
    multipage.write_text(
        '#set page(width: 55mm, height: 35mm, margin: 5mm)\n'
        'First page\n#pagebreak()\nSecond page\n',
        encoding="utf-8",
    )
    inset = examples / "inset.svg"
    inset.write_text(
        '<ns0:svg xmlns:ns0="http://www.w3.org/2000/svg" viewBox="0 0 40 20">'
        '<ns0:rect width="40" height="20" fill="#f60"/>'
        '<ns0:circle cx="30" cy="10" r="6" fill="#1683d8"/>'
        '</ns0:svg>\n',
        encoding="utf-8",
    )
    svg_inset = examples / "svg-inset.typ"
    svg_inset.write_text(
        '#set page(width: 70mm, height: 40mm, margin: 5mm)\n'
        'Before inset #image("inset.svg", width: 30mm) After inset\n',
        encoding="utf-8",
    )
    rollback = examples / "parameter-rollback.typ"
    rollback.write_text(
        '#import "../typst/cetz-studio/lib.typ" as studio\n'
        '#set page(width: auto, height: auto, margin: 4mm)\n'
        '#let divisor = studio.param(1, label: "Divisor", min: 0, max: 2, step: 1)\n'
        '#rect(width: (10 / divisor) * 1mm, height: 5mm)\n',
        encoding="utf-8",
    )
    return {
        "demo": examples / "demo.typ",
        "studio": examples / "studio-cetz.typ",
        "view_only": view_only,
        "multipage": multipage,
        "svg_inset": svg_inset,
        "rollback": rollback,
    }


def main() -> None:
    args = parse_args()
    binary = require_file(args.binary, "--binary")
    chromium = require_file(args.chromium, "--chromium") if args.chromium else None

    environment = os.environ.copy()

    retained_output = args.output_dir.expanduser().resolve() if args.output_dir else None
    if retained_output:
        retained_output.mkdir(parents=True, exist_ok=True)

    checks: list[str] = []
    browser_errors: list[str] = []

    def check(condition: bool, description: str) -> None:
        assert condition, description
        checks.append(description)

    with tempfile.TemporaryDirectory(prefix="cetz-studio-native-") as temporary:
        project = Path(temporary) / "project"
        project.mkdir()
        sources = prepare_project(project)
        evidence = retained_output or (Path(temporary) / "evidence")
        evidence.mkdir(parents=True, exist_ok=True)

        with sync_playwright() as playwright:
            launch_options: dict[str, Any] = {
                "headless": True,
                "args": ["--no-sandbox"],
            }
            if chromium:
                launch_options["executable_path"] = str(chromium)
            browser = playwright.chromium.launch(**launch_options)
            try:
                demo_original = sources["demo"].read_text(encoding="utf-8")
                with running_app(
                    binary, project, sources["demo"], evidence / "demo.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "demo", browser_errors)
                    state = browser_snapshot(page)
                    check(state["mode"] == "graph_editing", "Demo opens in graph-editing mode")
                    check(state["capabilities"]["graph_gestures"], "Demo exposes measured graph gestures")
                    check(page.locator("#fixture-banner").is_hidden(), "Native page is not the synthetic fixture")
                    check(
                        "moving a nearby node does not move those branch origins"
                        in page.locator("#diagnostics").text_content(),
                        "Native graph warnings reach the diagnostics panel",
                    )

                    page.locator('[data-element="trunk"]').click()
                    check(page.locator("#node-x").input_value() == "20", "Demo trunk starts at x=20 mm")
                    page.locator("#node-x").fill("25")
                    revision = state["revision"]
                    page.locator("#inspector button", has_text="Apply position").click()
                    moved = wait_revision(page, revision)
                    expected_demo = demo_original.replace("n(20, 40, <trunk>", "n(25, 40, <trunk>", 1)
                    check(moved["source"] == expected_demo, "Node move changes only the trunk x literal")
                    check("-" in moved["diff"] and "+" in moved["diff"], "Node move produces a source diff")
                    check(sources["demo"].read_text(encoding="utf-8") == demo_original, "Draft move does not write before Save")

                    page.locator("#undo").click()
                    undone = wait_revision(page, moved["revision"])
                    check(undone["source"] == demo_original and not undone["dirty"], "Undo restores the clean demo draft")
                    page.locator("#redo").click()
                    redone = wait_revision(page, undone["revision"])
                    check(redone["source"] == expected_demo, "Redo restores the validated node move")

                    saved = save_source(page)
                    backups = list(sources["demo"].parent.rglob("*.bak"))
                    check(not saved["dirty"] and sources["demo"].read_text(encoding="utf-8") == expected_demo, "Save persists the node move")
                    check(len(backups) == 1 and backups[0].read_text(encoding="utf-8") == demo_original, "Save retains an exact original-source backup")

                    page.locator("#undo").click()
                    post_save_undo = wait_revision(page, saved["revision"])
                    check(post_save_undo["source"] == demo_original and post_save_undo["dirty"], "Undo remains session-local after Save")
                    check(sources["demo"].read_text(encoding="utf-8") == expected_demo, "Post-save Undo does not silently rewrite disk")
                    screenshot(page, evidence, "demo-native")
                    page.close(run_before_unload=False)

                with running_app(
                    binary, project, sources["demo"], evidence / "demo-reopen.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "demo-reopen", browser_errors)
                    reopened = browser_snapshot(page)
                    trunk = next(node for node in reopened["diagram"]["nodes"] if node["id"] == "trunk")
                    check(trunk["position"]["x"] == 25, "Reopening the demo reads the saved node coordinate")
                    page.close(run_before_unload=False)

                studio_original = sources["studio"].read_text(encoding="utf-8")
                with running_app(
                    binary, project, sources["studio"], evidence / "studio-cetz.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "studio-cetz", browser_errors)
                    state = browser_snapshot(page)
                    check(state["mode"] == "parameter_editing", "Loop-generated CeTZ opens in parameter-editing mode")
                    check(not state["capabilities"]["graph_gestures"], "Generated CeTZ objects remain graph-read-only")
                    check({item["id"] for item in state["parameters"]} >= {"radius", "accent-hex", "show-guides"}, "Numeric, color, and boolean controls are discovered")

                    state = apply_parameter(page, "radius", "24")
                    check(state["preview_current"], "Numeric control recompiles successfully")
                    state = apply_parameter(page, "accent-hex", "#c16b42")
                    check(state["preview_current"], "Color control recompiles successfully")
                    state = apply_parameter(page, "show-guides", False)
                    check(state["preview_current"], "Boolean control recompiles successfully")

                    expected_studio = studio_original
                    expected_studio = expected_studio.replace("studio.param(20mm,", "studio.param(24mm,", 1)
                    expected_studio = expected_studio.replace('studio.param("#258975",', 'studio.param("#c16b42",', 1)
                    expected_studio = expected_studio.replace("studio.param(true, label: \"Show guides\"", "studio.param(false, label: \"Show guides\"", 1)
                    check(state["source"] == expected_studio, "Control edits replace only their three declared literals")
                    check(sources["studio"].read_text(encoding="utf-8") == studio_original, "Control drafts leave the source untouched before Save")
                    state = save_source(page)
                    check(not state["dirty"] and sources["studio"].read_text(encoding="utf-8") == expected_studio, "Control edits persist through explicit Save")
                    screenshot(page, evidence, "studio-controls")
                    page.close(run_before_unload=False)

                with running_app(
                    binary, project, sources["studio"], evidence / "studio-reopen.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "studio-reopen", browser_errors)
                    values = {item["id"]: item["value"] for item in browser_snapshot(page)["parameters"]}
                    check(values["radius"] == 24 and values["accent-hex"] == "#c16b42" and values["show-guides"] is False, "Reopening preserves numeric, color, and boolean values")
                    page.close(run_before_unload=False)

                rollback_original = sources["rollback"].read_text(encoding="utf-8")
                with running_app(
                    binary, project, sources["rollback"], evidence / "rollback.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "rollback", browser_errors)
                    before = browser_snapshot(page)
                    page.locator('[data-element="divisor"]').click()
                    page.locator("#parameter-value").fill("0")
                    page.locator("#apply-parameter").click()
                    wait_idle(page)
                    after = browser_snapshot(page)
                    check(after["revision"] == before["revision"] and after["source"] == rollback_original, "Compile failure rolls a parameter edit back")
                    check(not after["dirty"] and sources["rollback"].read_text(encoding="utf-8") == rollback_original, "Rejected parameter edit leaves disk and history clean")
                    check(page.locator("#toast").is_visible(), "Compile failure is visible in the browser")
                    # Chromium logs a rejected fetch as a console error even
                    # though the application handles the expected 409 and
                    # shows its diagnostic. It is not a JavaScript exception.
                    browser_errors[:] = [
                        error
                        for error in browser_errors
                        if not (error.startswith("rollback console:") and "409" in error)
                    ]
                    page.close(run_before_unload=False)

                with running_app(
                    binary, project, sources["view_only"], evidence / "view-only.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "view-only", browser_errors)
                    state = browser_snapshot(page)
                    check(state["mode"] == "render_only" and state["preview_current"], "Plain CeTZ opens as a current render-only preview")
                    check(not state["parameters"] and not state["capabilities"]["graph_gestures"], "Plain CeTZ exposes no invented controls or graph gestures")
                    check(page.locator("#figure svg").count() == 1 and page.locator("#save").is_disabled(), "View-only UI mounts native SVG and disables Save")
                    screenshot(page, evidence, "view-only")
                    page.close(run_before_unload=False)

                with running_app(
                    binary, project, sources["svg_inset"], evidence / "svg-inset.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "svg-inset", browser_errors)
                    inset = page.locator("#figure svg image")
                    href = inset.evaluate("image => image.getAttribute('href') || image.getAttributeNS('http://www.w3.org/1999/xlink', 'href')") if inset.count() == 1 else None
                    check(href is not None and href.startswith("data:image/svg+xml;base64,"), "Native Typst SVG inset remains mounted after recursive sanitization")
                    box = inset.bounding_box()
                    check(box is not None and box["width"] > 100 and box["height"] > 50, "Native SVG inset retains visible Typst geometry")
                    check("fidelity warning" not in page.locator("#preview-status").text_content().lower(), "Retained SVG inset does not mark the preview degraded")
                    screenshot(page, evidence, "svg-inset")
                    page.close(run_before_unload=False)

                with running_app(
                    binary, project, sources["multipage"], evidence / "multipage.log", environment
                ) as app:
                    page = open_page(browser, app.origin, "multipage", browser_errors)
                    state = browser_snapshot(page)
                    selector = page.locator("#page-select")
                    check(state["mode"] == "render_only" and len(state["pages"]) == 2, "Multipage Typst returns two bounded page previews")
                    check(selector.is_visible() and selector.locator("option").count() == 2, "Multipage selector is visible with two pages")
                    first_svg = page.locator("#figure").inner_html()
                    selector.select_option("1")
                    wait_condition(page, "document.querySelector('#page-select').value === '1'")
                    second_svg = page.locator("#figure").inner_html()
                    check(first_svg != second_svg, "Selecting page two replaces the mounted SVG")
                    screenshot(page, evidence, "multipage-second")
                    page.close(run_before_unload=False)
            finally:
                browser.close()

        check(
            not browser_errors,
            "No browser JavaScript or console errors"
            + (f": {browser_errors}" if browser_errors else ""),
        )
        result = {
            "scope": "Real Chrome -> Rust loopback server -> Typst compiler on disposable sources",
            "passed": len(checks),
            "checks": checks,
            "browser_errors": browser_errors,
            "binary": str(binary),
            "chromium": str(chromium) if chromium else "playwright-managed",
        }
        if retained_output:
            (retained_output / "native-browser.json").write_text(
                json.dumps(result, indent=2) + "\n", encoding="utf-8"
            )
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
