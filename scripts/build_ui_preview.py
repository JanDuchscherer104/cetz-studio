"""Build the explicitly synthetic, standalone browser interaction preview.

This embeds the production UI and test-only transport. It does not compile Rust
or Typst and cannot save a figure. The pinned frontend bundle is embedded; no runtime web assets are fetched.
"""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    html = (ROOT / "web/index.html").read_text(encoding="utf-8")
    css = (ROOT / "web/style.css").read_text(encoding="utf-8")
    app = (ROOT / "web/app.js").read_text(encoding="utf-8")
    libraries = (ROOT / "web/dist/ui.js").read_text(encoding="utf-8")
    fixture = (ROOT / "tests/ui_fixture.js").read_text(encoding="utf-8")
    demo = (ROOT / "examples/demo.typ").read_text(encoding="utf-8")
    # Escape a closing script tag should the opaque example ever contain one.
    payload = "window.CETZ_STUDIO_DEMO_SOURCE=" + json.dumps(demo).replace("</", "<\\/") + ";\n" + fixture
    if any("</script" in part.lower() for part in (app, fixture, libraries)):
        raise ValueError("Unexpected closing script tag in embedded JavaScript")
    stylesheet = '<link rel="stylesheet" href="/style.css">'
    script = '<script defer src="/app.js"></script>'
    if stylesheet not in html or script not in html:
        raise ValueError("Production asset tags changed; update the preview bundler")
    html = html.replace(stylesheet, "<style>" + css + "</style>")
    html = html.replace(script, "")
    html = html.replace('<script defer src="/ui.js"></script>', "")
    # Routing needs the real compiler/worker endpoints. The standalone fixture
    # deliberately omits that optional controller and shows no live proposals.
    html = html.replace('<script defer src="/routing.js"></script>', "")
    html = html.replace("Opening routing controls…", "Automatic routing requires the native server; unavailable in this fixture.")
    html = html.replace("</body>", "<script>" + libraries + "</script><script>" + payload + "</script><script>" + app + "</script></body>")
    destination = ROOT / "ui-preview.html"
    destination.write_text(html, encoding="utf-8")
    print(f"Built {destination}: browser fixture only; native backend not executed")


if __name__ == "__main__":
    main()
