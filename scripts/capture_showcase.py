"""Capture real editor screenshots on a disposable copy of the workspace example.

Uses the same Playwright runtime and --binary/--chromium/--output-dir options as
tests/native_browser.py. Screenshots are actual Typst renders, not UI fixtures.
"""
from pathlib import Path
import os
import shutil
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tests'))
from native_browser import (ROOT, browser_snapshot, open_page, parse_args,
                            running_app, wait_idle, wait_revision)
from playwright.sync_api import sync_playwright


def main():
    args = parse_args()
    output = (args.output_dir or ROOT / 'docs/media').resolve()
    output.mkdir(parents=True, exist_ok=True)
    errors = []
    with tempfile.TemporaryDirectory(prefix='cetz-showcase-') as temporary:
        project = Path(temporary)
        shutil.copytree(ROOT / 'typst', project / 'typst')
        shutil.copytree(ROOT / 'examples', project / 'examples')
        source = project / 'examples/studio-workspace.typ'
        with sync_playwright() as playwright:
            options = {'headless': True, 'args': ['--no-sandbox']}
            if args.chromium:
                options['executable_path'] = str(args.chromium.resolve())
            browser = playwright.chromium.launch(**options)
            try:
                with running_app(args.binary.resolve(), project, source,
                                 project / 'server.log', os.environ.copy()) as app:
                    page = open_page(browser, app.origin, 'showcase', errors)
                    page.locator('#project-filter').fill('studio-')
                    page.locator('[data-element="input"]').click()
                    page.locator('#fit').click()
                    page.screenshot(path=str(output / 'studio-workspace.png'))
                    page.screenshot(path=str(output / 'step-1.png'))
                    page.locator('#node-body-source').fill('[Your ideas, in Typst.]')
                    before = browser_snapshot(page)['revision']
                    page.locator('#apply-node-body-source').click()
                    wait_revision(page, before)
                    page.screenshot(path=str(output / 'step-2.png'))
                    page.locator('#add-node').click()
                    page.screenshot(path=str(output / 'step-3.png'))
                    page.locator('#gallery-modal .modal-close').click()
                    page.locator('[data-element="transform"]').click()
                    page.locator('#node-x').fill('65')
                    before = browser_snapshot(page)['revision']
                    page.get_by_role('button', name='Apply position', exact=True).click()
                    wait_revision(page, before)
                    page.locator('#fit').click()
                    page.screenshot(path=str(output / 'step-4.png'))
                    before = browser_snapshot(page)['revision']
                    page.locator('#undo').click()
                    wait_revision(page, before)
                    page.locator('#edges-tab').click()
                    page.locator('[data-element="e1"]').click()
                    wait_idle(page)
                    page.screenshot(path=str(output / 'step-5.png'))
                    page.close(run_before_unload=False)
            finally:
                browser.close()
    assert not errors, errors


if __name__ == '__main__':
    main()
