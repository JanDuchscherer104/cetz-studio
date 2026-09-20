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
            page.add_style_tag(path=str(ROOT / 'web/style.css'))
            page.add_script_tag(path=str(ROOT / 'web/dist/ui.js'))
            errors: list[str] = []
            page.on('pageerror', lambda error: errors.append(str(error)))

            def check(condition: bool, name: str) -> None:
                if not condition:
                    raise AssertionError(name)
                checks.append(name)

            def mount(kind: str, value: object, **metadata: object) -> None:
                page.evaluate('''item => {
                  window.control?.dispose();
                  window.applied = [];
                  window.control = CetzUi.createParameterEditor(
                    document.getElementById('controls'), item, {disabled:!!item.disabled,apply:v=>window.applied.push(v)});
                }''', {'id': 'fixture', 'kind': kind, 'value': value, **metadata})

            def value() -> object:
                page.locator('#apply-parameter').click()
                return page.evaluate('window.applied.at(-1)')

            def enter(text: str) -> None:
                page.locator('#parameter-value').fill(text)
                page.locator('#outside').click()

            mount('number', 0.0000123456789)
            check(value() == 0.0000123456789, 'Mounting preserves unstepped precision')
            enter('0.125')
            check(page.evaluate('window.applied.length') == 1, 'Typing alone never applies a source command')
            check(value() == 0.125, 'Unstepped numeric input is not rounded to an invented step')
            mount('number', 0.75, min=0.25, max=2.25, step=0.5)
            enter('1.75')
            check(value() == 1.75, 'Step respects an aligned nonzero range origin')
            mount('number', 0.3, min=0, max=2, step=0.5)
            enter('0.5')
            check(value() == 0.5, 'Widget does not shift the source step origin to the initial value')
            mount('length', 20, min=5, max=50, step=1, unit='mm')
            check(page.locator('#parameter-pane').inner_text().find('mm') >= 0, 'Length unit is explicit')
            enter('24')
            check(value() == 24, 'Length emits a number in the original source unit')
            enter('99')
            check(value() == 50, 'Upstream range widget visibly constrains an out-of-range value')
            check(page.locator('#parameter-value').input_value() == '50', 'The constrained value is displayed before Apply')
            mount('bool', True)
            page.locator('label').filter(has=page.locator('#parameter-value')).click()
            check(value() is False, 'Boolean widget emits JSON boolean')
            mount('color', '#AbCdEf')
            check(value() == '#AbCdEf', 'Opening a color preserves source hex spelling')
            enter('#123456')
            check(str(value()).lower() == '#123456', 'Color picker emits six-digit hex')
            enter('#ABCDEF')
            check(value() == '#AbCdEf', 'Returning to the same color preserves original spelling')
            page.locator('#apply-parameter').hover()
            contrast = page.locator('#apply-parameter').evaluate('''button => {
              const luminance = color => {
                const rgb = color.match(/[0-9.]+/g).slice(0,3).map(v => {
                  const c=Number(v)/255;return c<=0.04045?c/12.92:((c+0.055)/1.055)**2.4;
                });
                return rgb[0]*0.2126+rgb[1]*0.7152+rgb[2]*0.0722;
              };
              const style=getComputedStyle(button),a=luminance(style.color),b=luminance(style.backgroundColor);
              return (Math.max(a,b)+0.05)/(Math.min(a,b)+0.05);
            }''')
            check(contrast >= 4.5, 'Apply text remains legible under the actual host hover styles')
            mount('number', 2, disabled=True)
            check(page.locator('#apply-parameter').is_disabled(), 'Busy/stale Apply is disabled')
            check(page.locator('#parameter-value').is_disabled(), 'Busy/stale control is disabled')
            page.evaluate('window.control.dispose()')
            check(page.locator('#parameter-pane').count() == 0, 'Disposal removes the pane on selection changes')
            check(page.evaluate('''() => {
              try { CetzUi.createParameterEditor(document.getElementById('controls'),
                {kind:'vector', value:[1,2]}, {disabled:false,apply:()=>{}}); return false; }
              catch { return true; }
            }'''), 'Unknown parameter types do not acquire editing capability')
            check(not errors, f'No widget JavaScript errors: {errors}')
            print(json.dumps({'evidence': 'real-widget-browser', 'passed': len(checks), 'checks': checks}, indent=2))
        finally:
            browser.close()


if __name__ == '__main__':
    main()
