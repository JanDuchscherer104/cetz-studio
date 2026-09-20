"""Exercise real Tweakpane widgets. Native source/save tests remain separate."""
from pathlib import Path
import json
import os
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    checks: list[str] = []
    with sync_playwright() as runtime:
        options = {"headless": True}
        if os.environ.get("CHROMIUM"):
            options["executable_path"] = os.environ["CHROMIUM"]
        browser = runtime.chromium.launch(**options)
        try:
            page = browser.new_page()
            page.set_content('<div id="controls"></div><button id="outside">Outside</button>')
            page.add_script_tag(path=str(ROOT / 'web/dist/controls.js'))
            errors: list[str] = []
            page.on('pageerror', lambda error: errors.append(str(error)))

            def check(condition: bool, name: str) -> None:
                if not condition:
                    raise AssertionError(name)
                checks.append(name)

            def mount(kind: str, value: object, **metadata: object) -> None:
                page.evaluate('''item => {
                  window.control?.dispose();
                  window.control = CetzControls.mountParameter(
                    document.getElementById('controls'), item, !!item.disabled);
                }''', {'id': 'fixture', 'kind': kind, 'value': value, **metadata})

            def value() -> object:
                return page.evaluate('window.control.value()')

            def enter(text: str) -> None:
                page.locator('#parameter-value').fill(text)
                page.locator('#outside').click()

            mount('number', 0.0000123456789)
            check(value() == 0.0000123456789, 'Mounting preserves unstepped precision')
            enter('0.125')
            check(value() == 0.125, 'Unstepped numeric input is not rounded to an invented step')
            mount('number', 0.75, min=0.25, max=2.25, step=0.5)
            enter('1.75')
            check(value() == 1.75, 'Step respects an aligned nonzero range origin')
            mount('length', 20, min=5, max=50, step=1, unit='mm')
            check(page.locator('#parameter-pane').inner_text().find('mm') >= 0, 'Length unit is explicit')
            enter('24')
            check(value() == 24, 'Length emits a number in the original source unit')
            enter('99')
            check(value() == 50, 'Upstream range widget visibly constrains an out-of-range value')
            check(page.locator('#parameter-value').input_value() == '50', 'The constrained value is displayed before Apply')
            mount('bool', True)
            page.locator('#parameter-value').uncheck()
            check(value() is False, 'Boolean widget emits JSON boolean')
            mount('color', '#AbCdEf')
            check(value() == '#AbCdEf', 'Opening a color preserves source hex spelling')
            enter('#123456')
            check(str(value()).lower() == '#123456', 'Color picker emits six-digit hex')
            enter('#ABCDEF')
            check(value() == '#AbCdEf', 'Returning to the same color preserves original spelling')
            mount('number', 2, disabled=True)
            check(page.locator('#parameter-value').is_disabled(), 'Busy/stale control is disabled')
            page.evaluate('window.control.dispose()')
            check(page.locator('#parameter-pane').count() == 0, 'Disposal removes the pane on selection changes')
            check(page.evaluate('''() => {
              try { CetzControls.mountParameter(document.getElementById('controls'),
                {kind:'vector', value:[1,2]}, false); return false; }
              catch { return true; }
            }'''), 'Unknown parameter types do not acquire editing capability')
            check(not errors, f'No widget JavaScript errors: {errors}')
            print(json.dumps({'evidence': 'real-widget-browser', 'passed': len(checks), 'checks': checks}, indent=2))
        finally:
            browser.close()


if __name__ == '__main__':
    main()
