"""Exercise the actual bundled SVG sanitizer in Chromium; no fake sanitizer."""
from pathlib import Path
import json
import os
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    with sync_playwright() as p:
        options = {"headless": True}
        if os.environ.get("CHROMIUM"):
            options["executable_path"] = os.environ["CHROMIUM"]
        browser = p.chromium.launch(**options)
        try:
            page = browser.new_page()
            page.set_content('<div id="target"></div>')
            page.add_script_tag(path=str(ROOT / 'web/dist/upstream.js'))
            checks = page.evaluate(r"""() => {
              const checks = [];
              const check = (value, message) => {
                if (!value) throw new Error(message);
                checks.push(message);
              };
              const clean = content => {
                const doc = new DOMParser().parseFromString(
                  `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 80" onload="window.pwned=1">${content}</svg>`,
                  'image/svg+xml');
                const svg = CetzUpstream.sanitizeSvg(doc.documentElement);
                document.getElementById('target').replaceChildren(svg);
                return svg;
              };
              let svg = clean('<defs><path id="glyph1" d="M0 0L4 5"/><clipPath id="clip1"><rect width="5" height="5"/></clipPath><linearGradient id="paint1"><stop offset="0" stop-color="red"/></linearGradient></defs><use xlink:href="#glyph1" transform="translate(2 3)"/><rect clip-path="url(#clip1)" fill="url(#paint1)" stroke-width="0.00012345"/>');
              check(svg.getAttribute('viewBox') === '0 0 100 80', 'SVG viewport survives');
              check(!svg.hasAttribute('onload'), 'Root events are removed');
              check(svg.querySelector('use').getAttributeNS('http://www.w3.org/1999/xlink','href') === '#glyph1', 'Local Typst glyph reference survives');
              check(svg.querySelector('use').getAttribute('transform') === 'translate(2 3)', 'Geometry transforms survive');
              check(svg.querySelector('rect[clip-path]').getAttribute('clip-path') === 'url(#clip1)', 'Local clip paths survive');
              check(svg.querySelector('rect[fill]').getAttribute('fill') === 'url(#paint1)', 'Local gradients survive');
              check(svg.querySelector('[stroke-width]').getAttribute('stroke-width') === '0.00012345', 'Geometry-marker precision survives');
              svg = clean('<script>window.pwned=1</script><foreignObject><iframe src="https://example.org"/></foreignObject><style>@import "https://example.org";</style><animate attributeName="href" to="https://example.org"/><rect onclick="window.pwned=1" style="fill:url(https://example.org)"/><feImage href="https://example.org"/>');
              check(!svg.querySelector('script,foreignObject,iframe,style,animate,feImage'), 'Executable and dynamic elements are removed');
              check(!svg.querySelector('[onclick],[style]'), 'Events and inline CSS are removed');
              for (const href of ['https://example.org/a.svg#x','//example.org/x','file:///tmp/x','javascript:alert(1)','data:image/svg+xml;base64,PHN2Zy8+']) {
                svg = clean(`<use href="${href}"/><image href="${href}"/>`);
                check(!svg.querySelector('[href]'), `External/active resource rejected: ${href}`);
              }
              for (const paint of ['url(https://example.org/x)','url(//example.org/x)', 'u\\72l(https://example.org/x)']) {
                svg = clean(`<rect fill="${paint}" stroke="${paint}"/>`);
                check(!svg.querySelector('[fill],[stroke]'), `External paint rejected: ${paint}`);
              }
              svg = clean('<image href="data:image/png;base64,iVBORw0KGgo=" width="1" height="1"/>');
              check(svg.querySelector('image')?.getAttribute('href')?.startsWith('data:image/png;base64,'), 'Embedded raster inset survives');
              const first = new XMLSerializer().serializeToString(svg);
              CetzUpstream.sanitizeSvg(svg);
              check(new XMLSerializer().serializeToString(svg) === first, 'Repeated sanitization is stable');
              check(!window.pwned, 'No SVG payload executed');
              let rejected = false;
              try { CetzUpstream.sanitizeSvg(document.createElement('div')); } catch { rejected = true; }
              check(rejected, 'Non-SVG root fails closed');
              return checks;
            }""")
            print(json.dumps({"passed": len(checks), "checks": checks}, indent=2))
        finally:
            browser.close()


if __name__ == '__main__':
    main()
