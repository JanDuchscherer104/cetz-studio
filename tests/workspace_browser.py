"""Native workspace acceptance on disposable sources; no repository files are saved."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import tempfile
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from playwright.sync_api import sync_playwright
from native_browser import (
    ROOT, api_state, browser_snapshot, open_page, parse_args, require_file,
    running_app, screenshot, wait_condition, wait_idle, wait_revision,
)


def prepare_workspace(root: Path) -> Path:
    (root / 'figures').mkdir(parents=True)
    (root / 'typst').mkdir()
    shutil.copytree(ROOT / 'typst/cetz-studio', root / 'typst/cetz-studio')
    figure = root / 'figures/authoring.typ'
    figure.write_text('''#import "@preview/fletcher:0.5.8" as f
#import "../typst/cetz-studio/lib.typ" as studio
#set page(width: auto, height: auto, margin: 6mm)
#f.diagram(
  f.node((0mm, 0mm), [Alpha], name: <alpha>, shape: f.shapes.rect,
    fill: rgb("e8eef4"), width: 25mm, height: 12mm),
  f.node((45mm, 0mm), [Beta], name: <beta>, shape: f.shapes.rect,
    width: 25mm, height: 12mm),
  f.node((0mm, -35mm), [$x^2$], name: <equation>, width: 25mm),
  studio.card((45mm, -35mm), [Card], body: [Details], name: <card>, width: 25mm),
  f.edge(<alpha>, <beta>, "-|>", [Flow]),
)
''', encoding='utf-8')
    (root / 'figures/curves.typ').write_text(figure.read_text(encoding='utf-8').replace('#f.diagram(', '#studio.diagram(').replace('f.edge(', 'studio.edge('), encoding='utf-8')
    (root / 'figures/preview.typ').write_text('A plain Typst document', encoding='utf-8')
    (root / 'figures/broken.typ').write_text('#unknown-function()', encoding='utf-8')
    (root / 'figures/helper.typ').write_text('#let helper(x) = x', encoding='utf-8')
    (root / 'figures/controls.typ').write_text('''#import "../typst/cetz-studio/lib.typ" as studio
#let width = studio.param(20mm, min: 10mm, max: 40mm, step: 1mm)
#rect(width: width, height: 10mm)
''', encoding='utf-8')
    (root / 'notes.txt').write_text('A non-Typst project file', encoding='utf-8')
    return figure


def post_json(origin: str, path: str, body: dict) -> tuple[int, dict]:
    token = api_state(origin)['token']
    request = Request(origin + path, data=json.dumps(body).encode(), headers={
        'Content-Type': 'application/json', 'X-Cetz-Studio-Token': token,
    })
    try:
        response = urlopen(request, timeout=45)
    except HTTPError as error:
        response = error
    with response:
        return response.status, json.load(response)


def click_revision(page, selector: str) -> dict:
    before = browser_snapshot(page)['revision']
    page.locator(selector).click()
    return wait_revision(page, before)


def edit_text(page, element: str, text: str, field: str = 'title', kind: str = 'node') -> dict:
    page.locator('#nodes-tab' if kind == 'node' else '#edges-tab').click()
    page.locator(f'[data-element="{element}"]').click()
    page.locator(f'#plain-{kind}-{field} summary').click()
    page.locator(f'#{kind}-{field}-text').fill(text)
    return click_revision(page, f'#apply-{kind}-{field}')


def main() -> None:
    args = parse_args()
    binary = require_file(args.binary, '--binary')
    chromium = require_file(args.chromium, '--chromium') if args.chromium else None
    output = args.output_dir.expanduser().resolve() if args.output_dir else None
    checks: list[str] = []
    errors: list[str] = []

    def check(condition: bool, description: str) -> None:
        assert condition, description
        checks.append(description)

    with tempfile.TemporaryDirectory(prefix='cetz-workspace-') as temporary:
        root = Path(temporary) / 'project'
        figure = prepare_workspace(root)
        evidence = output or Path(temporary) / 'evidence'
        evidence.mkdir(parents=True, exist_ok=True)
        original = figure.read_text(encoding='utf-8')
        with sync_playwright() as playwright:
            launch = {'headless': True, 'args': ['--no-sandbox']}
            if chromium:
                launch['executable_path'] = str(chromium)
            browser = playwright.chromium.launch(**launch)
            try:
                with running_app(binary, root, figure, evidence / 'workspace.log', os.environ.copy()) as app:
                    page = open_page(browser, app.origin, 'workspace', errors)
                    state = browser_snapshot(page)
                    check(state['capabilities']['graph_gestures'], 'Workspace fixture has verified native graph geometry')
                    check(figure.read_text(encoding='utf-8') == original, 'Opening the workspace preserves source')
                    page.locator('[data-project-file="figures/authoring.typ"]').wait_for()
                    check(page.locator('[data-project-file="figures/authoring.typ"] .file-badge').inner_text() == 'Layout', 'Current figure has a layout capability badge')
                    check(page.locator('[data-project-file="figures/preview.typ"] .file-badge').inner_text() == 'Unchecked', 'Other files start unchecked rather than falsely incompatible')
                    page.locator('#project-filter').fill('figures/')
                    page.locator('#check-project').click()
                    wait_condition(page, "document.querySelectorAll('#project-files .file-badge.unchecked').length === 0")
                    wait_idle(page)
                    project = api_state(app.origin)['project']
                    files = {file['path']: file for file in project['files']}
                    check(files['figures/controls.typ']['capabilities']['parameters'], 'Compatibility scan discovers declared controls')
                    check(files['figures/preview.typ']['mode'] == 'render_only', 'Compatibility scan distinguishes preview-only documents')
                    check(files['figures/broken.typ']['status'] == 'error', 'Compatibility scan reports compiler errors')
                    page.locator('#project-filter').fill('authoring')
                    check(page.locator('[data-project-file]').count() == 1, 'Project filtering narrows files without changing the session')
                    page.locator('#project-filter').fill('figures/')

                    state = edit_text(page, 'alpha', 'Renamed alpha')
                    check(state['source'] in [original.replace('[Alpha]', '[Renamed alpha]'), original.replace('[Alpha]', '"Renamed alpha"')], 'Editing a node title preserves every unrelated source byte')
                    state = edit_text(page, 'card', 'Changed details', field='body')
                    check('body: [Changed details]' in state['source'] or 'body: "Changed details"' in state['source'], 'Studio card body is editable independently of its title')
                    state = edit_text(page, 'e0', 'Changed flow', field='label', kind='edge')
                    check('Changed flow' in state['source'], 'Existing edge text can be edited')
                    page.locator('#nodes-tab').click()
                    page.locator('[data-element="equation"]').click()
                    check(page.locator('#node-title-text').is_disabled(), 'Equation content remains read-only in the plain-text inspector')
                    check('[$x^2$]' in state['source'], 'Text edits preserve the equation source')

                    page.locator('#node-title-source').fill('[$x^3$ and *composed text*]')
                    state = click_revision(page, '#apply-node-title-source')
                    check('[$x^3$ and *composed text*]' in state['source'], 'Raw content editing accepts equations and composed Typst')
                    valid_source = state['source']
                    errors_before = len(errors)
                    page.locator('#node-title-source').fill('[#unknown-function()]')
                    page.locator('#apply-node-title-source').click()
                    wait_idle(page)
                    check(browser_snapshot(page)['source'] == valid_source, 'Failed content compilation preserves the valid draft')
                    check(page.locator('#node-title-source').input_value() == '[#unknown-function()]', 'Failed content input is retained for correction')
                    expected_console = 'workspace console: Failed to load resource: the server responded with a status of 409 (Conflict)'
                    check(errors[errors_before:] == [expected_console], 'Invalid content produces only the expected rejected-request console message')
                    del errors[errors_before:]
                    page.locator('#edges-tab').click()
                    page.locator('[data-element="e0"]').click()
                    page.locator('#edge-label-source').fill('[$y = x^2$]')
                    state = click_revision(page, '#apply-edge-label-source')
                    check('label: [$y = x^2$]' in state['source'], 'Raw edge labels accept math without changing arrow syntax')

                    corners_before = state['source']
                    state = click_revision(page, '#insert-waypoint')
                    check(len(state['diagram']['edges'][0]['vertices']) == 3, 'Add corner inserts a literal point in a Fletcher edge')
                    state = click_revision(page, '[data-remove-waypoint="1"]')
                    check(len(state['diagram']['edges'][0]['vertices']) == 2, 'Remove corner retains named endpoints')
                    click_revision(page, '#undo')
                    state = click_revision(page, '#undo')
                    check(state['source'] == corners_before, 'Manual corner edits undo to exact source bytes')

                    page.locator('#nodes-tab').click()
                    page.locator('[data-element="alpha"]').click()
                    before = browser_snapshot(page)
                    state = click_revision(page, '#duplicate-node')
                    new_nodes = [node for node in state['diagram']['nodes'] if node['id'] not in {n['id'] for n in before['diagram']['nodes']}]
                    check(len(new_nodes) == 1 and new_nodes[0]['id'] != 'alpha', 'Duplicate creates exactly one uniquely named node')
                    check(new_nodes[0]['position']['x'] != 0 and state['source'].count('fill: rgb("e8eef4")') == 2, 'Duplicate offsets the node and retains its style')
                    duplicate = new_nodes[0]['id']
                    page.locator('[data-element="alpha"]').click()
                    page.locator('#copy-node').click()
                    page.locator('#viewport').focus()
                    before_revision = state['revision']
                    page.keyboard.press('Control+v')
                    state = wait_revision(page, before_revision)
                    check(len(state['diagram']['nodes']) == len(before['diagram']['nodes']) + 2, 'Copy and paste duplicates within the current diagram')

                    page.locator('#add-edge').click()
                    page.locator('#edge-from').select_option('beta')
                    page.locator('#edge-to').select_option(duplicate)
                    page.locator('#new-edge-label').fill('New connection')
                    page.locator('#edge-arrow').select_option('both')
                    before_edges = len(state['diagram']['edges'])
                    state = click_revision(page, '#create-edge')
                    check(len(state['diagram']['edges']) == before_edges + 1 and 'New connection' in state['source'], 'Edge form inserts a labeled connection between the selected nodes')

                    for primitive in ['fletcher-rect', 'fletcher-ellipse', 'fletcher-diamond', 'studio-node', 'studio-card']:
                        page.locator('#add-node').click()
                        option = page.locator(f'[data-primitive="{primitive}"]')
                        check(option.is_enabled(), f'Gallery offers {primitive} with the required imports')
                        before_count = len(state['diagram']['nodes'])
                        state = click_revision(page, f'[data-primitive="{primitive}"]')
                        check(len(state['diagram']['nodes']) == before_count + 1 and state['preview_current'], f'{primitive} inserts and compiles as a real Typst node')
                    check(figure.read_text(encoding='utf-8') == original, 'Structural changes remain drafts until explicit Save')
                    expected = state['source']
                    state = click_revision(page, '#undo')
                    check(state['source'] != expected, 'Gallery insertion can be undone')
                    state = click_revision(page, '#redo')
                    check(state['source'] == expected, 'Gallery insertion can be redone')

                    page.locator('#edges-tab').click()
                    page.locator('[data-element="e0"]').click()
                    state = click_revision(page, '#delete-edge')
                    check(len(state['diagram']['edges']) == before_edges, 'Delete connection removes only the selected edge')
                    state = click_revision(page, '#undo')
                    check(state['source'] == expected, 'Edge deletion is undoable')
                    page.locator('#nodes-tab').click()
                    page.locator('[data-element="alpha"]').click()
                    page.once('dialog', lambda dialog: dialog.dismiss())
                    page.locator('#delete-node').click()
                    check(browser_snapshot(page)['source'] == expected, 'Cancelling connected-node deletion preserves the graph')
                    page.once('dialog', lambda dialog: dialog.accept())
                    state = click_revision(page, '#delete-node')
                    check(all(node['id'] != 'alpha' for node in state['diagram']['nodes']) and '<alpha>' not in state['source'], 'Confirmed node deletion also removes attached edges')
                    state = click_revision(page, '#undo')
                    check(state['source'] == expected, 'One undo restores the node and its attached edges')
                    screenshot(page, evidence, 'workspace-authoring')

                    stale = api_state(app.origin)
                    page.locator('[data-project-file="figures/preview.typ"]').click()
                    page.locator('#dirty-cancel').click()
                    check(browser_snapshot(page)['source'] == expected, 'Cancel file switching retains the complete dirty draft')
                    page.locator('[data-project-file="figures/preview.typ"]').click()
                    before_revision = browser_snapshot(page)['revision']
                    page.locator('#dirty-save').click()
                    wait_condition(page, "document.querySelector('#filename').textContent === 'preview.typ'")
                    wait_idle(page)
                    check(figure.read_text(encoding='utf-8') == expected, 'Save and continue persists the draft before switching files')
                    check(not browser_snapshot(page)['dirty'], 'Switched file opens with a clean draft')
                    check(page.locator('#add-node').is_disabled() and page.locator('#add-edge').is_disabled(), 'Preview-only documents disable unsupported structural editing')
                    status, rejected = post_json(app.origin, '/api/edit', {'session_id': stale['session_id'], 'revision': stale['snapshot']['revision'], 'command': {'kind': 'duplicate_node', 'id': 'alpha'}})
                    check(status == 409 and rejected['snapshot']['filename'] == 'preview.typ', 'A stale tab cannot apply graph commands to a different open file')

                    page.locator('[data-project-file="figures/authoring.typ"]').click()
                    wait_condition(page, "document.querySelector('#filename').textContent === 'authoring.typ'")
                    wait_idle(page)
                    check(browser_snapshot(page)['source'] == expected, 'Reopening the figure reads the saved text and inserted objects')
                    state = edit_text(page, 'alpha', 'Discard this draft')
                    page.locator('[data-project-file="figures/preview.typ"]').click()
                    page.locator('#dirty-discard').click()
                    wait_condition(page, "document.querySelector('#filename').textContent === 'preview.typ'")
                    wait_idle(page)
                    check(figure.read_text(encoding='utf-8') == expected, 'Discard and switch never writes the discarded draft to disk')
                    current = api_state(app.origin)
                    status, rejected = post_json(app.origin, '/api/project/open', {'session_id': current['session_id'], 'revision': current['snapshot']['revision'], 'path': '../outside.typ'})
                    check(status == 409, 'Project navigation refuses paths outside the chosen folder')
                    (root / 'figures/new-file.typ').write_text('A newly discovered figure', encoding='utf-8')
                    page.locator('#refresh-project').click()
                    page.locator('[data-project-file="figures/new-file.typ"]').wait_for()
                    check(page.locator('[data-project-file="figures/new-file.typ"] .file-badge').inner_text() == 'Unchecked', 'Refresh discovers a new file without claiming it was compiled')
                    page.locator('[data-project-file="figures/curves.typ"]').click()
                    wait_condition(page, "document.querySelector('#filename').textContent === 'curves.typ'")
                    wait_idle(page)
                    page.locator('#edges-tab').click()
                    page.locator('[data-element="e0"]').click()
                    before = browser_snapshot(page)
                    page.locator('#edge-route').select_option('bezier')
                    state = wait_revision(page, before['revision'])
                    check(state['diagram']['edges'][0]['route'] == 'bezier' and state['preview_current'], 'Bézier mode renders native curves with draggable controls')
                    check(page.locator('[data-waypoint]').count() == 2, 'Cubic Bézier exposes two control handles')
                    state = click_revision(page, '[data-remove-waypoint="2"]')
                    check(len(state['diagram']['edges'][0]['vertices']) == 3, 'Removing a control converts cubic to quadratic Bézier')
                    state = click_revision(page, '#insert-waypoint')
                    check(len(state['diagram']['edges'][0]['vertices']) == 4, 'Adding a control restores a cubic Bézier')
                    revision = state['revision']
                    page.locator('#edge-route').select_option('polyline')
                    state = wait_revision(page, revision)
                    check(state['diagram']['edges'][0]['route'] == 'polyline', 'Switching back retains control points as corners')
                    for _ in range(4):
                        state = click_revision(page, '#undo')
                    check(state['source'] == before['source'] and not state['dirty'], 'Route mode and control edits undo without touching disk')
                    other = Path(temporary) / 'other-project'
                    other.mkdir()
                    (other / 'a-broken.typ').write_text('#unknown-function()', encoding='utf-8')
                    (other / 'z-readable.typ').write_text('A different project', encoding='utf-8')
                    page.locator('#project-filter').fill('')
                    page.locator('#project-root').fill(str(other))
                    page.locator('#open-project').click()
                    wait_condition(page, "document.querySelector('#filename').textContent === 'a-broken.typ'")
                    wait_idle(page)
                    check(Path(api_state(app.origin)['project']['root']).resolve() == other.resolve(), 'Open folder adopts the chosen root even when its initial figure has diagnostics')
                    page.locator('[data-project-file="z-readable.typ"]').click()
                    wait_condition(page, "document.querySelector('#filename').textContent === 'z-readable.typ'")
                    wait_idle(page)
                    check(browser_snapshot(page)['preview_current'], 'Folder navigation remains usable after an initial compilation failure')
                    screenshot(page, evidence, 'workspace')
                    page.close(run_before_unload=False)
            finally:
                browser.close()
        check(not errors, f'No JavaScript errors: {errors}')
    result = {'scope': 'Native workspace authoring on disposable sources', 'passed': len(checks), 'checks': checks, 'browser_errors': errors}
    if output:
        (output / 'workspace-browser.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
