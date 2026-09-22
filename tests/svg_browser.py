"""Exercise the production DOMPurify adapter in actual Chromium, without a server."""
from pathlib import Path
import argparse
import json
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chromium", default=None)
    args = parser.parse_args()

    with sync_playwright() as runtime:
        browser = runtime.chromium.launch(headless=True, executable_path=args.chromium)
        page = browser.new_page()
        page.add_script_tag(path=str(ROOT / "web/dist/ui.js"))
        results = page.evaluate(r"""() => {
          const checks = [];
          function check(value, name) { if (!value) throw new Error(name); checks.push(name); }
          function clean(body, attrs = '') {
            const root = new DOMParser().parseFromString(
              `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" ${attrs}>${body}</svg>`,
              'image/svg+xml').documentElement;
            return CetzUi.sanitizeSvg(root);
          }
          let result = clean('<script>window.pwned=1</script><g onclick="alert(1)"><foreignObject><div>unsafe</div></foreignObject></g>', 'onload="alert(1)"');
          let svg = result.svg;
          check(!svg.hasAttribute('onload') && !svg.querySelector('[onclick]'), 'root and nested handlers removed');
          check(!svg.querySelector('script,foreignObject') && !window.pwned, 'active content removed');
          svg = clean('<defs><path id="glyph-A" d="M0 0L1 1"/><clipPath id="clip-A"><rect width="1" height="1"/></clipPath></defs><use xlink:href="#glyph-A" fill="#123456" clip-path="url(#clip-A)"/>', 'width="100pt" height="60pt" viewBox="0 0 100 60"').svg;
          check(svg.querySelector('use').getAttributeNS('http://www.w3.org/1999/xlink', 'href') === '#glyph-A', 'Typst glyph references retained');
          check(svg.querySelector('use').getAttribute('clip-path') === 'url(#clip-A)', 'local clip reference retained');
          check(svg.getAttribute('viewBox') === '0 0 100 60', 'page geometry retained');
          check(!svg.hasAttribute('fill'), 'transparent page remains transparent');
          result = clean('<image href="https://example.invalid/pixel.png"/><use xlink:href="//example.invalid/x.svg#x"/><path fill="url(https://example.invalid/p)"/><path stroke="u\\72l(https://example.invalid/x)"/>'); svg = result.svg;
          check(!svg.querySelector('image').hasAttribute('href') && !svg.querySelector('use').hasAttributeNS('http://www.w3.org/1999/xlink', 'href'), 'external image and use blocked');
          check(!svg.querySelector('path').hasAttribute('fill') && !svg.querySelectorAll('path')[1].hasAttribute('stroke'), 'external and escaped paint URLs blocked');
          check(result.losses.some(loss => loss.kind === 'image resource' && loss.count === 1), 'blocked image reports a count-only fidelity loss');
          result = clean('<image href="data:image/png;base64,iVBORw0KGgo="/><a href="javascript:alert(1)"><text>x</text></a>'); svg = result.svg;
          check(svg.querySelector('image').getAttribute('href')?.startsWith('data:image/png;'), 'embedded raster retained');
          check(!svg.querySelector('a')?.hasAttribute('href'), 'script link blocked');
          const nested = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 6 4"><script>window.pwned=1</script><rect width="6" height="4" fill="#f60"/><image href="https://example.invalid/pixel.png"/></svg>';
          const nestedData = btoa(unescape(encodeURIComponent(nested)));
          result = clean(`<image href="data:image/svg+xml;base64,${nestedData}" width="6" height="4"/>`); svg = result.svg;
          const retainedInset = svg.querySelector('image').getAttribute('href');
          check(retainedInset?.startsWith('data:image/svg+xml;base64,'), 'sanitized compiler SVG inset retained');
          const decodedInset = decodeURIComponent(escape(atob(retainedInset.split(',')[1])));
          check(!decodedInset.includes('<script') && !decodedInset.includes('example.invalid') && decodedInset.includes('#f60'), 'embedded SVG is recursively sanitized before mounting');
          const tooDeep = btoa(unescape(encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg"><image href="${retainedInset}"/></svg>`)));
          result = clean(`<image href="data:image/svg+xml;base64,${tooDeep}"/>`); svg = result.svg;
          const shallowInset = svg.querySelector('image').getAttribute('href');
          const decodedShallowInset = decodeURIComponent(escape(atob(shallowInset.split(',')[1])));
          check(!decodedShallowInset.includes('data:image/svg+xml') && result.losses.some(loss => loss.kind === 'embedded SVG image'), 'nested embedded SVG is blocked and reported');
          result = clean('<image href="data:image/svg+xml;base64,%%%"/>'); svg = result.svg;
          check(!svg.querySelector('image').hasAttribute('href') && result.losses.some(loss => loss.kind === 'image resource'), 'malformed SVG image encoding is blocked');
          const oversizedInset = btoa(`<svg xmlns="http://www.w3.org/2000/svg">${' '.repeat(1024 * 1024)}</svg>`);
          result = clean(`<image href="data:image/svg+xml;base64,${oversizedInset}"/>`); svg = result.svg;
          check(!svg.querySelector('image').hasAttribute('href') && result.losses.some(loss => loss.kind === 'embedded SVG image' && loss.reason.includes('oversized')), 'oversized SVG image is blocked and reported');
          svg = clean('<style>svg{fill:red}</style><rect style="fill:url(https://example.invalid/x)"/><animate attributeName="href" to="javascript:alert(1)"/>', 'xml:base="https://example.invalid/"').svg;
          check(!svg.querySelector('style,animate,[style]') && !svg.hasAttribute('xml:base'), 'CSS animation and base URI blocked');
          svg = clean('<rect stroke="#a10001" stroke-width="0.00012345" x="2" y="3" width="4" height="5"/>').svg;
          check(svg.querySelector('rect').getAttribute('stroke-width') === '0.00012345', 'measurement marker retained');
          let refused = false; try { CetzUi.sanitizeSvg(document.createElement('div')); } catch { refused = true; }
          check(refused, 'non-SVG root rejected');
          return checks;
        }""")
        browser.close()
    print(json.dumps({"passed": len(results), "checks": results}, indent=2))


if __name__ == "__main__":
    main()
