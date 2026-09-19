# Native editor media

The PNG and animated GIF show the real Rust/browser application rendering
`examples/studio-workspace.typ` with Typst 0.14.2, CeTZ 0.5.2, and Fletcher 0.5.8.
The capture edits a disposable copy; no scientific diagram or data is used.

Reproduce the frames with the Playwright runtime used by the browser tests:

```sh
python scripts/capture_showcase.py --binary target/debug/cetz-studio \
  --output-dir /path/to/frames
```

The GIF contains frames `step-1.png` through `step-5.png`, resized to 1110×765
with durations 2400, 2400, 2200, 2200, and 2400 milliseconds. Pillow can assemble
the frames with `save_all=True`, `loop=0`, and `optimize=True`.

These images illustrate the implemented workflow. Compatibility depends on the
source and available dependencies; they are not an exhaustive compatibility test.
