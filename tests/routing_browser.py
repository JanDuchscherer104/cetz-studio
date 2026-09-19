"""Real Rust + Typst + Chromium routing round trips, never the synthetic UI transport.

Run after cargo build with Typst and Playwright installed. Evidence is written to
--output (before/proposed/after PNGs, compiled sources, and measured timings).
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--typst", default=os.environ.get("TYPST", "typst"))
    parser.add_argument("--output", type=Path, default=Path(".ci/routing"))
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cetz-routing-") as directory:
        root = Path(directory)
        source_file = root / "routing.typ"
        original = (ROOT / "examples/routing.typ").read_text(encoding="utf-8")
        source_file.write_text(original, encoding="utf-8")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        origin = f"http://127.0.0.1:{port}"
        process = None
        log = (output / "server.log").open("w", encoding="utf-8")

        def state():
            with urllib.request.urlopen(origin + "/api/state", timeout=5) as response:
                return json.load(response)

        def start():
            nonlocal process
            process = subprocess.Popen(
                [str(args.binary.resolve()), "--root", str(root), "--file", str(source_file),
                 "--port", str(port), "--typst", args.typst, "--compile-timeout", "120"],
                stdout=log, stderr=subprocess.STDOUT,
            )
            deadline = time.monotonic() + 150
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("Native server exited; inspect server.log")
                try:
                    return state()
                except (OSError, urllib.error.URLError):
                    time.sleep(0.2)
            raise TimeoutError("Native server did not open its initial Typst preview")

        def stop():
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)

        def reject(envelope, command, revision=None, session_id=None):
            request = urllib.request.Request(
                origin + "/api/edit",
                data=json.dumps({
                    "session_id": envelope["session_id"] if session_id is None else session_id,
                    "revision": envelope["snapshot"]["revision"] if revision is None else revision,
                    "command": command,
                }).encode(),
                headers={"Content-Type": "application/json", "X-Cetz-Studio-Token": envelope["token"]},
            )
            try:
                urllib.request.urlopen(request, timeout=150).close()
            except urllib.error.HTTPError as error:
                assert error.code == 409
                return json.load(error)
            raise AssertionError("Stale route was unexpectedly accepted")

        def settled(page):
            page.wait_for_function("() => !window.cetzStudioDebug().busy", timeout=150_000)

        def propose(page):
            before = time.perf_counter()
            page.locator("#route-preview").click()
            page.wait_for_function("() => !window.cetzStudioRoutingDebug().working", timeout=10_000)
            debug = page.evaluate("window.cetzStudioRoutingDebug()")
            assert debug["proposal"], page.locator("#route-status").inner_text()
            return debug, (time.perf_counter() - before) * 1000

        def geometry(page):
            # Observe the production overlay, whose points/bounds come from the
            # native instrumented SVG. This is not the proposed worker polyline.
            return page.evaluate("""() => {
              const b=window.cetzStudioDebug().basis;
              const inv=p=>{const x=p.x-b.o.x,y=p.y-b.o.y,d=b.x.x*b.y.y-b.x.y*b.y.x;
                return {x:(x*b.y.y-y*b.y.x)/d,y:(y*b.x.x-x*b.x.y)/d};};
              const r=document.querySelector('[data-node="obstacle"]');
              const n=k=>Number(r.getAttribute(k));
              const lo=inv({x:n('x')+2,y:n('y')+2}),hi=inv({x:n('x')+n('width')-2,y:n('y')+n('height')-2});
              const line=document.querySelector('.edge-hit[data-edge="e0"]');
              return {box:{left:Math.min(lo.x,hi.x),right:Math.max(lo.x,hi.x),top:Math.min(lo.y,hi.y),bottom:Math.max(lo.y,hi.y)},
                points:[...line.points].map(inv)};
            }""")

        def collisions(measured, clearance=0.0):
            box, points = measured["box"], measured["points"]
            count = 0
            tolerance = 0.02  # Native SVG serialization precision, in millimetres.
            for a, b in zip(points, points[1:]):
                horizontal = abs(a["y"] - b["y"]) < tolerance
                vertical = abs(a["x"] - b["x"]) < tolerance
                assert horizontal or vertical, (a, b)
                if horizontal:
                    count += (box["top"] - clearance + tolerance < a["y"] < box["bottom"] + clearance - tolerance
                              and max(a["x"], b["x"]) > box["left"] - clearance + tolerance
                              and min(a["x"], b["x"]) < box["right"] + clearance - tolerance)
                else:
                    count += (box["left"] - clearance + tolerance < a["x"] < box["right"] + clearance - tolerance
                              and max(a["y"], b["y"]) > box["top"] - clearance + tolerance
                              and min(a["y"], b["y"]) < box["bottom"] + clearance - tolerance)
            return count

        page = None
        playwright = None
        try:
            initial = start()
            assert initial["snapshot"]["capabilities"]["graph_gestures"], initial["snapshot"]["diagnostics"]
            playwright = sync_playwright().start()
            browser = playwright.chromium.launch(
                executable_path=os.environ.get("CHROMIUM_PATH"), headless=True,
            )
            page = browser.new_page(viewport={"width": 1700, "height": 1100})
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("dialog", lambda dialog: dialog.accept())
            page.goto(origin, wait_until="networkidle")
            page.wait_for_function("() => window.cetzStudioDebug?.().basis")
            page.locator("#edges-tab").click()
            page.locator('[data-element="e0"]').click()
            page.locator("#route-toggle").click()
            before_geometry = geometry(page)
            assert collisions(before_geometry) > 0
            page.screenshot(path=str(output / "01-before.png"), full_page=True)
            debug, preview_ms = propose(page)
            proposal = debug["proposal"]
            assert state()["snapshot"]["source"] == original
            assert state()["snapshot"]["revision"] == 0
            assert source_file.read_text(encoding="utf-8") == original
            assert page.locator('[data-route-proposal="e0"]').count() == 1
            assert len(proposal["routes"]) == 1
            route = proposal["routes"][0]
            assert route["start_port"] == "east" and route["end_port"] == "west"
            assert collisions({"box": before_geometry["box"], "points": route["preview"]}, 2) == 0
            page.screenshot(path=str(output / "02-proposed.png"), full_page=True)
            started = time.perf_counter()
            page.locator("#route-apply").click()
            settled(page)
            apply_ms = (time.perf_counter() - started) * 1000
            applied = state()["snapshot"]
            assert applied["revision"] == 1, page.locator("#status").inner_text()
            assert applied["dirty"] and applied["preview_current"]
            assert applied["diagram"]["nodes"] == initial["snapshot"]["diagram"]["nodes"]
            assert source_file.read_text(encoding="utf-8") == original
            old_lines, new_lines = original.splitlines(), applied["source"].splitlines()
            assert len(old_lines) == len(new_lines)
            changed = [(a, b) for a, b in zip(old_lines, new_lines) if a != b]
            assert len(changed) == 1 and changed[0][0].strip().startswith("f.edge(<source.east>")
            assert '["Q_h"]' not in applied["source"]  # No stringification of equations.
            assert '"-|>", [$Q_h$]' in applied["source"]
            assert collisions(geometry(page), 2) == 0
            page.screenshot(path=str(output / "03-after.png"), full_page=True)
            (output / "before.typ").write_text(original, encoding="utf-8")
            (output / "after.typ").write_text(applied["source"], encoding="utf-8")
            page.locator("#undo").click()
            settled(page)
            assert state()["snapshot"]["source"] == original
            assert not state()["snapshot"]["undo"]
            page.locator("#redo").click()
            settled(page)
            assert state()["snapshot"]["source"] == applied["source"]
            page.locator("#save").click()
            settled(page)
            assert not state()["snapshot"]["dirty"]
            assert source_file.read_text(encoding="utf-8") == applied["source"]
            backups = list((root / ".cetz-studio-backups").glob("*.bak"))
            assert len(backups) == 1 and backups[0].read_text(encoding="utf-8") == original
            stop()
            reopened = start()
            assert reopened["snapshot"]["source"] == applied["source"]
            page.reload(wait_until="networkidle")
            page.wait_for_function("() => window.cetzStudioDebug?.().basis")
            assert reopened["snapshot"]["capabilities"]["graph_gestures"]
            page.locator("#route-toggle").click()
            page.locator("#route-scope").select_option("checked")
            for edge in ("e0", "e1"):
                page.locator(f'#route-choices input[value="{edge}"]').check()
            selected, _ = propose(page)
            assert {r["edge"] for r in selected["proposal"]["routes"]} == {"e0", "e1"}
            page.locator("#route-scope").select_option("all")
            all_edges, _ = propose(page)
            assert len(all_edges["proposal"]["routes"]) == 2
            assert "unsupported edge(s) skipped" in page.locator("#route-status").inner_text()
            page.locator("#route-cancel").click()
            assert state()["snapshot"]["revision"] == 0
            page.locator("#route-scope").select_option("selected")
            page.locator("#edges-tab").click()
            page.locator('[data-element="e2"]').click()
            page.locator("#route-preview").click()
            assert "cardinal" in page.locator("#route-status").inner_text()
            assert page.locator("#route-apply").is_disabled()
            page.locator('[data-element="e0"]').click()
            page.locator("#route-clearance").fill("20")
            page.locator("#route-preview").click()
            page.wait_for_function("() => !window.cetzStudioRoutingDebug().working")
            assert not page.evaluate("window.cetzStudioRoutingDebug().proposal")
            assert "unchanged" in page.locator("#route-status").inner_text()
            assert state()["snapshot"]["revision"] == 0
            page.locator("#route-clearance").fill("2")
            stale, _ = propose(page)
            stale_command = {"kind": "route_edges", "routes": [
                {"edge": r["edge"], "points": r["points"]} for r in stale["proposal"]["routes"]]}
            page.locator("#nodes-tab").click()
            page.locator('[data-element="source"]').click()
            page.locator("#node-x").fill("1")
            page.get_by_role("button", name="Apply position", exact=True).click()
            settled(page)
            assert page.locator("#route-apply").is_disabled()
            assert not page.evaluate("window.cetzStudioRoutingDebug().proposal")
            moved = state()
            assert moved["snapshot"]["revision"] == 1
            assert "Stale" in reject(reopened, stale_command)["error"]
            assert "Stale" in reject(moved, stale_command, session_id=999)["error"]
            assert state()["snapshot"]["source"] == moved["snapshot"]["source"]
            # Delay an actual Worker message, then cancel it with a source edit.
            page.evaluate("""() => {
              window.NativeRoutingWorker=window.Worker;
              window.Worker=class extends window.NativeRoutingWorker {
                postMessage(message) { setTimeout(()=>super.postMessage(message),600); }
              };
            }""")
            page.locator("#edges-tab").click()
            page.locator('[data-element="e0"]').click()
            page.locator("#route-preview").click()
            assert page.evaluate("window.cetzStudioRoutingDebug().working")
            page.locator("#nodes-tab").click()
            page.locator('[data-element="source"]').click()
            page.locator("#node-x").fill("2")
            page.get_by_role("button", name="Apply position", exact=True).click()
            settled(page)
            page.wait_for_timeout(750)
            assert not page.evaluate("window.cetzStudioRoutingDebug().proposal")
            assert not page.evaluate("window.cetzStudioRoutingDebug().working")
            page.evaluate("window.Worker=window.NativeRoutingWorker")
            page.locator("#edges-tab").click()
            page.locator('[data-element="e0"]').click()
            propose(page)
            before_external = state()["snapshot"]
            external = applied["source"] + "\n// external writer\n"
            source_file.write_text(external, encoding="utf-8")
            page.locator("#route-apply").click()
            settled(page)
            assert "External edit" in page.locator("#status").inner_text()
            assert state()["snapshot"]["source"] == before_external["source"]
            assert state()["snapshot"]["revision"] == before_external["revision"]
            assert source_file.read_text(encoding="utf-8") == external
            assert not errors, errors
            evidence = {
                "platform": platform.platform(), "browser": browser.version,
                "fixture": "examples/routing.typ", "worker": debug["lastMetrics"],
                "preview_end_to_end_ms": preview_ms, "apply_compile_end_to_end_ms": apply_ms,
                "before_node_intersections": collisions(before_geometry),
                "after_node_intersections_at_2mm": 0,
                "edge_edge_crossing_optimization": False,
                "native_round_trip": "apply, undo, redo, save, restart, reopen",
                "rejections": ["unsupported port", "impossible clearance", "stale revision", "stale file session", "late worker", "external writer"],
            }
            (output / "metrics.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
            print(json.dumps(evidence, indent=2))
            browser.close()
        except Exception:
            try:
                if page is not None and not page.is_closed():
                    page.screenshot(path=str(output / "failure.png"), full_page=True)
                    (output / "failure-status.txt").write_text(page.locator("body").inner_text(), encoding="utf-8")
            except Exception as capture_error:
                print(f"Could not capture failure evidence: {capture_error}")
            raise
        finally:
            if playwright is not None:
                playwright.stop()
            stop()
            log.close()


if __name__ == "__main__":
    main()
