"""Execute the production canvas against the explicit test-only API fixture.

This validates browser interaction and serialization, NOT Rust compilation,
Typst rendering, backend span edits, disk safety, or end-to-end round-trips.
Run: python tests/browser_smoke.py [--chromium /usr/bin/chromium]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    args = parser.parse_args()
    checks: list[str] = []
    errors: list[str] = []

    def check(condition: bool, description: str) -> None:
        assert condition, description
        checks.append(description)

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=args.chromium, headless=True, args=["--no-sandbox"])
        page = browser.new_page(viewport={"width": 1480, "height": 1020}, device_scale_factor=1)
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.set_content((ROOT / "ui-preview.html").read_text(), wait_until="load")
        page.wait_for_function("window.cetzStudioDebug && window.cetzStudioDebug().basis !== null")

        def idle() -> None:
            page.wait_for_function("!window.cetzStudioDebug().busy")

        def command() -> dict:
            return page.evaluate("window.cetzStudioFixtureRequests.at(-1).body.command")

        def drag(selector: str, dx_mm: float, dy_mm: float) -> None:
            target = page.locator(selector)
            box = target.bounding_box()
            assert box
            # In this fixture, physical millimetres map through the exact SVG
            # calibration and the paper's current CSS zoom.
            ppm = page.evaluate("""() => {
                const d=cetzStudioDebug(), m=document.querySelector('#figure svg').getScreenCTM();
                const a=new DOMPoint(d.basis.o.x,d.basis.o.y).matrixTransform(m);
                const b=new DOMPoint(d.basis.o.x+d.basis.x.x,d.basis.o.y+d.basis.x.y).matrixTransform(m);
                return Math.hypot(a.x-b.x,a.y-b.y);
            }""")
            x, y = box["x"] + box["width"] / 2, box["y"] + box["height"] / 2
            page.mouse.move(x, y)
            page.mouse.down()
            page.mouse.move(x + dx_mm * ppm, y + dy_mm * ppm, steps=8)
            page.mouse.up()
            idle()

        check(page.locator("#fixture-banner").is_visible(), "Preview visibly identifies itself as an API/geometry fixture")
        check(page.locator("#save").is_disabled(), "Fixture cannot save source")
        check(page.locator("[data-node]").count() == 12, "Twelve rendered node hitboxes are mapped")
        check(page.evaluate("cetzStudioDebug().locked.length") == 0, "Measured/source coordinate checks accept this fixture")
        check(page.locator('#figure [stroke="#a00000"]').count() == 0, "Calibration marker is removed from displayed SVG")

        viewport = page.locator("#viewport")
        check(viewport.evaluate("node => node.classList.contains('grid-visible')"), "Visual grid is enabled by default")
        major_before = viewport.evaluate("node => getComputedStyle(node).getPropertyValue('--grid-major-x')")
        page.locator("#zoom-in").click()
        major_zoomed = viewport.evaluate("node => getComputedStyle(node).getPropertyValue('--grid-major-x')")
        check(major_before != major_zoomed, "Visual grid spacing follows canvas zoom")
        page.locator("#fit").click()
        page.locator("#show-grid").uncheck()
        check(not viewport.evaluate("node => node.classList.contains('grid-visible')"), "Grid toggle hides the visual grid without changing snap settings")
        page.locator("#show-grid").check()
        check(page.locator("#figure .studio-page-background").count() == 1, "Rendered page background is identified conservatively")
        page.locator("#transparent-page").check()
        check(page.locator("#paper").evaluate("node => node.classList.contains('transparent-page')"), "Transparency toggle exposes the visual grid through the page")
        check(page.locator("#figure .studio-page-background").evaluate("node => getComputedStyle(node).visibility === 'hidden'"), "Transparency hides only the rendered page backdrop")
        page.locator("#transparent-page").uncheck()
        check(page.locator("#figure .studio-page-background").evaluate("node => getComputedStyle(node).visibility !== 'hidden'"), "Opaque mode restores the rendered page backdrop")

        page.locator("#grid-step").select_option("2")
        trunk = page.locator('[data-node="trunk"]')
        box = trunk.bounding_box()
        assert box
        ppm = page.evaluate("""() => {
            const d=cetzStudioDebug(), m=document.querySelector('#figure svg').getScreenCTM();
            const a=new DOMPoint(d.basis.o.x,d.basis.o.y).matrixTransform(m);
            const b=new DOMPoint(d.basis.o.x+d.basis.x.x,d.basis.o.y+d.basis.x.y).matrixTransform(m);
            return Math.hypot(a.x-b.x,a.y-b.y);
        }""")
        x, y = box["x"] + box["width"] / 2, box["y"] + box["height"] / 2
        page.mouse.move(x, y); page.mouse.down(); page.mouse.move(x + 50.6 * ppm, y, steps=8)
        check(page.locator('.alignment-guide[data-align-axis="x"]').count() == 1, "Node drag previews a measured center or edge alignment guide")
        page.mouse.up(); idle()
        check(command() == {"kind": "move_node", "id": "trunk", "x": 71, "y": 40}, "Object alignment takes priority over the nearest grid point")
        page.locator("#undo").click(); idle()

        box = trunk.bounding_box(); assert box
        x, y = box["x"] + box["width"] / 2, box["y"] + box["height"] / 2
        page.keyboard.down("Alt")
        page.mouse.move(x, y); page.mouse.down(); page.mouse.move(x + 50.6 * ppm, y, steps=8)
        check(page.locator(".alignment-guide").count() == 0, "Alt temporarily suppresses object alignment guides")
        page.mouse.up(); page.keyboard.up("Alt"); idle()
        c = command()
        check(c["kind"] == "move_node" and c["id"] == "trunk" and abs(c["x"] - 70.6) < .01, "Alt bypasses both grid and object snapping")
        page.locator("#undo").click(); idle()
        page.locator("#grid-step").select_option("1")

        page.locator('[data-element="trunk"]').click()
        drag('[data-node="trunk"]', 10, 5)
        c = command()
        check(c == {"kind": "move_node", "id": "trunk", "x": 30, "y": 45}, "Node drag emits the exact millimetre command")
        check("source" not in page.evaluate("cetzStudioFixtureRequests.at(-1).body"), "Browser sends commands, never replacement source")
        check(page.locator("#node-x").input_value() == "30", "Inspector follows accepted fixture position")
        page.locator("#undo").click(); idle()
        check(page.locator("#node-x").input_value() == "20", "Undo restores displayed fixture position")
        page.locator("#redo").click(); idle()
        check(page.locator("#node-x").input_value() == "30", "Redo restores displayed fixture position")

        page.locator("#edges-tab").click()
        page.locator('[data-element="e8"]').click()
        check(page.locator('[data-waypoint="e8:1"]').count() == 1, "Selecting a routed edge exposes waypoint handles")
        drag('[data-waypoint="e8:1"]', 5, 0)
        check(command() == {"kind": "move_waypoint", "edge": "e8", "vertex": 1, "x": 57, "y": 55}, "Waypoint drag identifies the exact edge and vertex")
        page.locator("#undo").click(); idle()
        drag('[data-segment="e8:1"]', 7, 0)
        check(command() == {"kind": "move_segment", "edge": "e8", "segment": 1, "delta": 7}, "Orthogonal segment drag emits a perpendicular displacement")
        drag('[data-label="e8"]', 0, 8.75)
        c = command()
        check(c["kind"] == "set_label" and c["edge"] == "e8" and c["segment"] == 1 and abs(c["fraction"] - .75) < .005, "Label drag projects onto the selected route segment")
        page.locator("#inspector .property-row select").nth(1).select_option("north"); idle()
        check(command() == {"kind": "set_port", "edge": "e8", "end": "end", "port": "north"}, "Port control changes only the requested endpoint port")

        page.locator("#nodes-tab").click(); page.locator('[data-element="trunk"]').click()
        revision = page.evaluate("cetzStudioDebug().revision")
        page.evaluate("window.CETZ_STUDIO_TEST_FAIL_NEXT=true")
        page.locator("#viewport").focus();page.keyboard.press("ArrowRight");idle()
        check(page.evaluate("cetzStudioDebug().revision") == revision, "Rejected API command leaves the displayed revision unchanged")
        check("Injected compiler failure" in page.locator("#toast").inner_text(), "Rejected compiler result is visible, not silently accepted")
        check(page.locator("#node-x").input_value() == "30", "Rejected command restores prior displayed coordinates")
        page.keyboard.press("Escape")

        page.locator("#search").fill("query")
        check(page.locator("#elements .element").count() == 2, "Element search filters without changing graph membership")
        page.locator("#search").fill("")
        page.locator("#edges-tab").click();page.locator('[data-element="e8"]').click()
        z = page.locator("#zoom").inner_text();page.locator("#zoom-in").click()
        check(page.locator("#zoom").inner_text() != z, "Zoom controls update the canvas scale")
        page.locator("#fit").click()
        # Space-pan must also work when the gesture starts over a node.
        before=page.locator("#paper").get_attribute("style")
        page.keyboard.down("Space")
        box=page.locator('[data-node="attention"]').bounding_box();assert box
        page.mouse.move(box["x"]+box["width"]/2,box["y"]+box["height"]/2);page.mouse.down();page.mouse.move(box["x"]+box["width"]/2+30,box["y"]+box["height"]/2+20,steps=5);page.mouse.up();page.keyboard.up("Space")
        check(page.locator("#paper").get_attribute("style") != before, "Space-drag pans even when starting over a node")
        page.locator("#fit").click()
        check(page.locator("#save").is_disabled(), "Save stays disabled after all fixture edits")
        blocked_svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10"><image href="https://example.invalid/pixel.png" width="20" height="10"/></svg>'
        preview = (ROOT / "ui-preview.html").read_text(encoding="utf-8")
        preview = preview.replace("window.CETZ_STUDIO_DEMO_SOURCE=", f"window.CETZ_STUDIO_FIXTURE_SVG={json.dumps(blocked_svg)};\\nwindow.CETZ_STUDIO_DEMO_SOURCE=", 1)
        page.set_content(preview, wait_until="load")
        page.wait_for_function("window.cetzStudioDebug && document.querySelector('#preview-status').textContent.includes('fidelity warning')")
        diagnostics = page.locator("#diagnostics").text_content()
        check("Preview fidelity warning:" in diagnostics and "example.invalid" not in diagnostics and "data:image" not in diagnostics, "Blocked visual resource produces count-only fidelity diagnostics")
        page.set_content((ROOT / "ui-preview.html").read_text(encoding="utf-8"), wait_until="load")
        page.wait_for_function("window.cetzStudioDebug && document.querySelector('#preview-status').textContent === 'UI fixture'")
        check("fidelity warning" not in page.locator("#preview-status").text_content().lower(), "Clean subsequent preview clears the fidelity warning")
        check(not errors, "No browser JavaScript exceptions")
        (ROOT / "verification").mkdir(exist_ok=True)
        # The screenshot is of this explicitly labelled fixture, not the server.
        page.locator("#status").evaluate("(node, count) => node.textContent = `Browser smoke: ${count} checks passed · UI fixture only`", len(checks))
        page.screenshot(path=str(ROOT / "verification/ui-fixture.png"), full_page=True)
        browser.close()
    result={"scope":"Production browser code with synthetic API/geometry fixture; Rust and Typst NOT executed", "passed":len(checks),"checks":checks,"browser_errors":errors}
    (ROOT / "verification/browser-smoke.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result,indent=2))

if __name__ == "__main__":
    main()
