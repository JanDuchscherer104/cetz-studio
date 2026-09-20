# Upstream browser components

The browser uses pinned npm dependencies bundled by esbuild into local static
assets. Node is a build/development tool, not a runtime service or browser CDN.
The source-editing and save policies stay in Rust.

```sh
npm install --ignore-scripts
npm run build:frontend
python tests/upstream_browser.py
```

DOMPurify 3.4.15 owns SVG sanitization. The small Studio hook restricts resources
to local fragment references and embedded PNG/JPEG/GIF/WebP images. Scripts,
animation, HTML integration, CSS styles and externally loaded images/paint are
not admitted. Native Typst glyph definitions, clips, gradients, transforms and
measurement markers must survive. Do not add a second sanitizer on another path.

DOMPurify is used under Apache-2.0; see its bundled license notice and
https://github.com/cure53/DOMPurify/tree/3.4.15. esbuild 0.28.2 is MIT-licensed and
used only to build the browser asset. Preserve generated license banners.

The first implementation commit contains isolated real-browser policy tests;
production mounting and native acceptance are still being integrated in the PR.
