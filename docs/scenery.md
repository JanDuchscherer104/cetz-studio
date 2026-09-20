# Scenery display-control fixture

Run the existing editor on the standalone example:

```sh
cargo run --locked --release -- --file examples/studio-scenery.typ --root .
```

Select **Controls** and change the display-camera azimuth/elevation, display
width or guides. The left view changes; the right top projection is fixed.
Undo/redo operates on the draft. Only **Save source** writes a backed-up source.
This is parameter editing, not arbitrary 3D object dragging or a live agent
integration. No Studio package migration is required for other figures.

## Geometry and rendering contract

The five dimensionless unit directions in
[`scenery-directions.json`](../examples/data/scenery-directions.json) are
schematic, not measured observations. The basis is right-handed XYZ with +Z up.
The highlighted direction is fixed by the data. Both panels use orthographic
projection. The right camera has azimuth 0 degrees and elevation 90 degrees;
it drops Z. Changing the left display camera must not change the directions or
the right panel. These are display cameras, not recorded acquisition poses.

Scenery fits each projection to its requested page width. Width is therefore
not a metric scale shared across views; hiding guides may change fitting. The
native test compares the right panel's rasterized pixels as well as the exact
unchanged input data and all source bytes outside the requested control.

Imports pin Scenery 0.1.0 and use Studio's existing CeTZ 0.5.2/Fletcher 0.5.8;
CI pins Typst 0.14.2. Scenery is an optional dependency of this example, not a
new mandatory import in Studio's library. Its pure-Typst renderer produces
CeTZ vector content. Painter ordering and limited hidden-line removal are not
a z-buffer; dense or intersecting surfaces need a separately evaluated route.
No perspective or WASM backend is exercised here.

Scenery 0.1.0 uses `fill-opacity` as a transparentize amount: 100% removes the
sphere's fill. Named imports avoid shadowing Typst built-ins. The camera and
render interfaces were source-inspected in
[the published package source](https://github.com/typst/packages/tree/d3591890fe159e83e15b8fa2be73e43c5ed4770c/packages/preview/scenery/0.1.0)
and [primary documentation](https://typst.app/universe/package/scenery/).
The example is original MIT code; Scenery (MIT) and its CeTZ dependency retain
their upstream terms and are not vendored here.

## Verify

With the browser environment described in the root README:

```sh
cargo build --locked
python3 tests/scenery_browser.py --binary target/debug/cetz-studio --output-dir .ci/scenery
```

The runner copies source, data and the Studio package to a disposable project.
It exercises the real browser/server/compiler: camera/size/guide edits, source
fidelity, fixed companion pixels, undo/redo, explicit save/backup, reopen,
invalid ranges and compiler-failure rollback. It records durations, native
SVGs, screenshots and a JSON receipt. It does not rewrite repository examples
or claim that a synthetic preview is native evidence. CI runs the same test on
Ubuntu and macOS and retains the evidence even on failure.

Inspect the retained before/after SVGs or screenshots at the intended 160 mm
width for clipping, depth order and legibility. Automated pixel invariants
establish preservation, not a human or independent scientific review. Only
actual successful runs establish compatibility; the presence of this fixture
and configured CI does not by itself do so.
