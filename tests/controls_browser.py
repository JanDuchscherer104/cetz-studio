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
                    document.getElementById('controls'), item, {disabled:!!item.disabled,identity:{sessionId:item.sessionId||'session-a',objectId:item.id},apply:v=>window.applied.push(v)});
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
            mount('number', 2)
            enter('3')
            page.evaluate("window.control.update({id:'fixture',kind:'number',value:9},false,{sessionId:'session-a',objectId:'fixture'})")
            check(page.locator('#parameter-value').input_value() == '3', 'Same-session snapshot refresh retains the focused widget staging')
            check(value() == 3, 'Retained staging applies as one focused-widget transaction')
            mount('number', 2)
            enter('3')
            page.evaluate("window.control.update({id:'fixture',kind:'number',value:9},false,{sessionId:'session-b',objectId:'fixture'})")
            check(page.locator('#parameter-value').input_value() == '9', 'A new session resets stale focused-widget staging')
            check(value() == 9, 'New-session widget applies the refreshed object value')
            mount('number', 0.75, min=0.25, max=2.25, step=0.5)
            enter('1.75')
            check(value() == 1.75, 'Step respects an aligned nonzero range origin')
            mount('number', 0.3, min=0, max=2, step=0.5)
            check(value() == 0.3, 'Untouched off-grid source value is not snapped on Apply')
            enter('0.5')
            check(value() == 0.5, 'Widget does not shift the source step origin to the initial value')
            mount('length', 20, min=5, max=50, step=1, unit='mm')
            check(page.locator('#parameter-pane').inner_text().find('mm') >= 0, 'Length unit is explicit')
            enter('24')
            check(value() == 24, 'Length emits a number in the original source unit')
            enter('99')
            check(value() == 50, 'Upstream range widget visibly constrains an out-of-range value')
            check(page.locator('#parameter-value').input_value() == '50', 'The constrained value is displayed before Apply')
            # Actual pointer interaction, not only text entry: sliders must stage
            # values admitted by the server's min-or-zero step origin.
            for minimum, maximum, step, initial in [(-180, 180, 1, -38),
                    (0.25, 2.25, 0.5, 0.75), (0, 2.2, 0.5, 0.3), (0, 0.3, 0.1, 0.1)]:
                mount('number', initial, min=minimum, max=maximum, step=step)
                for fraction in [0.137, 0.433, 0.819, 1.0]:
                    track = page.locator('#parameter-pane .tp-sldv_t').bounding_box()
                    assert track is not None
                    page.mouse.click(track['x'] + fraction * track['width'], track['y'] + track['height']/2)
                    check(page.evaluate('window.applied.length') == 0, 'Slider staging emits no source command')
                    staged = float(page.locator('#parameter-value').input_value())
                    check(minimum <= staged <= maximum and abs((staged-minimum)/step-round((staged-minimum)/step)) <= 1e-7,
                          f'Pointer-staged value follows bounds/step {minimum}/{maximum}/{step}')
                    emitted = value()
                    check(emitted == staged,
                          'Apply emits the displayed step-aligned value')
                    if fraction == 1.0:
                        check(value() == staged,
                              'Repeated endpoint Apply retains the displayed staged value')
                    page.evaluate('window.applied = []')
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
