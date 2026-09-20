# Library-backed frontend

The source-edit/session modules still own Typst admission, source patches, units,
compilation and saving. Browser libraries own generic presentation machinery.

## Build and distribution

From the repository root, run `npm ci --ignore-scripts` and `npm run build:web`
before Cargo. Node.js 22+ is a developer/CI dependency, not an application server.
`esbuild-wasm` bundles `web/src/index.js` into the local `web/dist/ui.js` asset.
Rust embeds it and the upstream license text. No CDN or runtime package download
is used. Generated assets are not authored or checked in; the npm lock is.
The synthetic UI builder embeds this same bundle and remains synthetic evidence.

## SVG policy

DOMPurify owns the SVG sanitization algorithm, including the root element.
Studio adds a resource policy: same-document fragment references and base64
PNG/JPEG/GIF/WebP images only. External URLs, nested SVG data images, inline CSS,
foreign content, animation and base-URI rebinding are not permitted. Typst glyph
`use` elements, local definitions/clipping and measurement-marker attributes
are retained. Unsupported sanitization fails closed rather than falling back to
our old filter. This does not turn Typst into an untrusted-document sandbox.

## Verification and updates

Run `python tests/svg_browser.py` (optionally `--chromium /path/to/chromium`)
and the existing native browser, workspace, routing and Scenery suites after a
version change. Inspect actual native renders; a sanitizer unit test alone does
not establish font, image or source-mapping fidelity. Package loading never
grants edit capabilities. Update the exact package and lock together, retain
licenses and review upstream security changes. Rebuild before any Cargo build.

The declared-control view has a small lifecycle interface in `web/src/controls.js`
so its widgets can be replaced independently of the source-command contract.
It does not expose additional parameter types or a new editable document.

## Declared controls

Tweakpane 4.0.5 owns numeric/length fields and sliders, the colour picker,
boolean widgets and the Apply button. `createParameterEditor` is presentation
only: Rust still validates declarations, admitted values, source units and steps,
then compiles before adoption. There is no new parameter type or source model.

Typing or dragging a widget stages a value without a request. Apply submits the
existing typed command; Save remains separate. Ranges clamp values visibly
before Apply. Tweakpane's step rounding uses the initial value as its origin,
whereas Studio validates against min or zero, so the adapter maps declared step
to keyboard/pointer increments and leaves step admission to Rust. It does not
install a second rounding/validation algorithm.

Opening/applying an unchanged value preserves its original spelling and hex
case. A different colour uses Tweakpane's canonical six-digit hex string.
Selection/snapshot changes dispose the old pane; stale/busy panes are disabled.
The source does not change until a valid command is adopted.

`python tests/controls_browser.py` tests the actual installed widgets through
the same lifecycle interface (set CHROMIUM for a local executable). The existing
native/Scenery suites test source/preview invariants, actual compilation failures,
undo and save/reopen. Since widgets constrain range inputs, the Scenery suite
also sends an invalid command directly to prove server-side rejection remains.
