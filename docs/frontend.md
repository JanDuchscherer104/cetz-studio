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
PNG/JPEG/GIF/WebP images are retained. A compiler-emitted base64 SVG image is
accepted only after UTF-8 decoding, SVG parsing and a second DOMPurify pass. Its
sanitized payload is re-encoded, is limited to 1 MiB and may contain no further
SVG data image. External URLs, deeper SVG images, inline CSS, foreign content,
animation and base-URI rebinding are not permitted. Typst glyph `use` elements,
local definitions/clipping and measurement-marker attributes are retained.
Unsupported material images or paint resources fail closed and add a count-only
fidelity warning to preview status and Diagnostics; it never includes source or
encoded payloads. This does not turn Typst into an untrusted-document sandbox.

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
whereas Studio validates against min or zero. The adapter maps interaction
increments to the upstream controls, then aligns staged values to that source
grid and refreshes the displayed value before Apply. This small policy conversion
is necessary because Tweakpane has no public step-origin option; Rust still
revalidates every command. A non-grid maximum is not emitted by the slider.

Opening/applying an unchanged value preserves its original spelling and hex
case. A different colour uses Tweakpane's canonical six-digit hex string.
Selection/snapshot changes dispose the old pane; stale/busy panes are disabled.
The source does not change until a valid command is adopted.

`python tests/controls_browser.py` tests the actual installed widgets through
the same lifecycle interface (set CHROMIUM for a local executable). The existing
native/Scenery suites test source/preview invariants, actual compilation failures,
undo and save/reopen. Since widgets constrain range inputs, the Scenery suite
also sends an invalid command directly to prove server-side rejection remains.
Pointer tests, not only typed-input tests, cover integer and fractional grids,
non-grid initial values, non-grid maxima, and floating-point range endpoints.

## Drag routing

maxGraph 0.24.0's ManhattanConnector replaces the browser's handwritten A*.
The adapter constructs a disposable graph from already measured rectangles and
ports; it does not introduce a persistent graph document, history, or full
maxGraph editor UI. A small output validator checks exact endpoints, cardinal
exit/entry direction, orthogonality and obstacle intersections. Upstream fallback
paths or geometry failures that fail these checks produce no hint. There is no
second handwritten fallback algorithm.

Fractional attachment coordinates are restored after upstream 0.1-unit rounding
and the resulting segments are rechecked. Ports are represented by small
non-degenerate terminal boxes so upstream fallback inference is defined; the
requested side is exactly the measured endpoint. Missing obstacle bounds, unknown
port directions, curves and unsupported edges are not granted edit capability.

Hints are bounded to 128 known obstacles, a 4096-unit SVG extent, 1000 upstream
search iterations per edge and 128 returned vertices. Each animation frame tries
at most eight incident edges and stops starting work after 24 ms; this is not a
hard per-frame deadline because an in-flight search cannot be preempted. Selection
revision changes and drag cancellation discard the transient projection.

The server's independently measured/adoptable router remains unchanged. A browser
hint never supplies an adoptable route or source patch, and need not match the
server's crossing-aware batch result. Full canvas replacement, libavoid and
single-engine browser/server routing are not delivered by this narrow change.
Use `tests/drag_router_browser.py` for actual library cases and
`tests/drag_preview_native.py` for real pointer/source/save acceptance.
