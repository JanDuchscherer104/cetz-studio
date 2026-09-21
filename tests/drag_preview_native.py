"""Real pointer drag -> maxGraph hint -> Rust/Typst source round trip.

Uses a disposable project. The hint is display-only; node movement and saving
still cross the existing native command interface.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile

from playwright.sync_api import sync_playwright
from native_browser import (
    browser_snapshot, open_page, running_app, save_source, wait_condition, wait_revision,
)

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--chromium', type=Path)
    parser.add_argument('--output-dir', type=Path, default=ROOT / '.ci' / 'drag-preview')
    args = parser.parse_args()
    evidence = args.output_dir.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    checks: list[str] = []
    errors: list[str] = []

    def check(condition: bool, description: str) -> None:
        if not condition:
            raise AssertionError(description)
        checks.append(description)

    with tempfile.TemporaryDirectory(prefix='studio-drag-') as tmp, sync_playwright() as runtime:
        root = Path(tmp)
        source = root / 'routing.typ'
        shutil.copy2(ROOT / 'examples/routing.typ', source)
        original = source.read_text()
        options = {'headless': True}
        if args.chromium:
            options['executable_path'] = str(args.chromium)
        browser = runtime.chromium.launch(**options)
        try:
            with running_app(args.binary.resolve(), root, source, evidence / 'server.log', os.environ.copy()) as app:
                page = open_page(browser, app.origin, 'drag-preview', errors)
                initial = browser_snapshot(page)
                check(initial['capabilities']['graph_gestures'], 'Real figure has measured gesture capabilities')
                check(page.evaluate("typeof CetzUi.routePreview === 'function'"), 'Native application loads the upstream route adapter')
                # Zoomed/subpixel coordinates must remain anchored to the actual figure.
                page.locator('#zoom-in').click()
                box = page.locator('[data-node="source"]').bounding_box()
                check(box is not None, 'Source node has a native measured hitbox')
                assert box is not None
                ppm = page.evaluate("""() => {
                  const d=cetzStudioDebug(),m=document.querySelector('#figure svg').getScreenCTM();
                  return Math.hypot(m.a*d.basis.x.x+m.c*d.basis.x.y,m.b*d.basis.x.x+m.d*d.basis.x.y);
                }""")
                x,y = box['x']+box['width']/2, box['y']+box['height']/2
                page.mouse.move(x,y)
                page.mouse.down()
                page.mouse.move(x+5*ppm,y+4*ppm,steps=4)
                wait_condition(page, 'document.querySelectorAll("[data-live-edge]").length>0')
                hints = page.locator('[data-live-edge]').evaluate_all("nodes=>nodes.map(n=>({edge:n.dataset.liveEdge,points:n.getAttribute('points').split(' ').map(p=>p.split(',').map(Number))}))")
                check(all(h['edge'] in ['e0','e1'] for h in hints), 'Only supported incident edges receive hints')
                check(all(all(abs(a[0]-b[0])<0.001 or abs(a[1]-b[1])<0.001 for a,b in zip(h['points'],h['points'][1:])) for h in hints), 'Displayed hints remain orthogonal at actual SVG coordinates')
                unchanged = browser_snapshot(page)
                check(unchanged['source']==original and unchanged['revision']==initial['revision'], 'Dragging hint does not mutate the accepted source or revision')
                check(source.read_text()==original, 'Dragging hint never writes disk')
                page.screenshot(path=str(evidence/'dragging.png'),full_page=True)
                page.mouse.up()
                moved = wait_revision(page, initial['revision'])
                check(page.locator('[data-live-edge]').count()==0, 'Release removes ephemeral hints')
                expected = original.replace('n(0, 0, <source>', 'n(5, 4, <source>', 1)
                check(moved['source']==expected, 'Release changes only the requested node literals')
                check(source.read_text()==original, 'Compiled node move still requires Save')
                page.locator('#undo').click()
                undone = wait_revision(page, moved['revision'])
                check(undone['source']==original, 'Undo restores exact initial source')
                page.locator('#redo').click()
                redone = wait_revision(page, undone['revision'])
                check(redone['source']==expected, 'Redo restores the compiled node movement')
                saved = save_source(page)
                check(not saved['dirty'] and source.read_text()==expected, 'Explicit Save persists only the node change')
                check(any(p.read_text()==original for p in root.rglob('*.bak')), 'Save retains the original backup')
                page.screenshot(path=str(evidence/'accepted.png'),full_page=True)
                page.close(run_before_unload=False)
            with running_app(args.binary.resolve(), root, source, evidence/'reopen.log', os.environ.copy()) as app:
                page = open_page(browser,app.origin,'reopen',errors)
                check(browser_snapshot(page)['source']==expected, 'Reopen reads the exact saved source')
                check(page.locator('[data-live-edge]').count()==0, 'Reopen does not retain stale hints')
                page.close(run_before_unload=False)
            check(not errors, f'No browser JavaScript errors: {errors}')
        finally:
            browser.close()
            receipt = {'evidence':'native-browser-server-typst','commit':os.environ.get('GITHUB_SHA','local'),
                       'checks':checks,'errors':errors}
            (evidence/'drag-preview.json').write_text(json.dumps(receipt,indent=2)+'\n')
            print(json.dumps(receipt,indent=2))


if __name__ == '__main__':
    main()
