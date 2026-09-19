# Verification

The imported prototype was delivered without a native build. This repository
adds executed native verification and a real browser suite. The synthetic
`ui-preview.html` remains clearly labeled and is not native-render evidence.

## Local environment

Ubuntu Linux x86_64; Rust/Cargo 1.95.0; Typst 0.14.2; Google Chrome driven by
Python Playwright 1.63.0. Fletcher 0.5.8 and CeTZ 0.5.2 are pinned in examples
and the package. All editable test sources are disposable copies.

Local acceptance on 2026-09-19 passed: formatting, Clippy, **42 Rust contract
tests**, **5 real Typst integration tests**, package positive/negative checks,
**23 synthetic browser checks**, and **35 native browser checks** with no
JavaScript errors. The browser receipt is in
[`verification/native-browser.json`](verification/native-browser.json).

## Acceptance commands

| Check | Scope |
| --- | --- |
| `cargo fmt --all -- --check` | Rust formatting |
| `cargo clippy --locked --all-targets -- -D warnings` | Rust lint/type checking |
| `cargo test --locked --all-targets` | Parsing, source patches, units, typed controls, capabilities, rollback, save/conflict contracts |
| `cargo test --locked --all-targets -- --ignored` | Actual Typst renders, instrumented geometry, fallback, and source round trips |
| `sh tests/package_compile.sh` | Standalone CeTZ/Fletcher package examples and valid/invalid parameter/style declarations |
| `python3 tests/browser_smoke.py --chromium /path/to/chrome` | 23 production-UI checks against synthetic geometry; not native evidence |
| `python3 tests/native_browser.py --binary target/debug/cetz-studio --chromium /path/to/chrome` | 35 real browser/server/compiler checks |

Run `cargo build --locked` before browser checks so the embedded UI matches the
source. Run `python3 scripts/build_ui_preview.py` before the synthetic fixture
checks. See the README for the optional Playwright environment.

The native browser suite covers a node edit, exact draft diff, undo/redo,
explicit save, original-content backup, reopen, numeric/color/boolean controls,
compiler-failure rollback, plain CeTZ viewing, and multiple pages. It records
JavaScript errors and optionally writes screenshots and a JSON receipt with
`--output-dir`. Screenshots of these cases were visually inspected locally.

## Cross-platform evidence

The [CI workflow](.github/workflows/ci.yml) runs Rust, real Typst/package, and
native Chromium round-trip checks on both Ubuntu and macOS. Consult the
[current workflow runs](https://github.com/JanDuchscherer104/cetz-studio/actions)
for the exact commit's hosted result. Configured jobs are not passing evidence;
the local environment above does not independently establish macOS success.

## ARIA compatibility

At ARIA-NBV commit `738ef529e72da2f45ca442cb99717d4c52cb2d90`, direct native
compilation passed for the candidate-query overview and oracle-lookahead tree.
Candidate-generation geometry and target-selection procedure could not render:
referenced PNGs were Git LFS pointer text rather than hydrated images. The
52-file syntax scan is described in [architecture](docs/architecture.md).
These observations do not establish a full-corpus editing success rate.

## Remaining limits

- The save check and atomic rename cannot exclude a non-cooperating concurrent
  writer in their final interval. Source dependencies are not locked.
- Persistent compiler reuse and asynchronous compilation are not implemented.
- General generated-object dragging, structural graph editing, and arbitrary
  expressions are not supported; declared controls are the explicit edit seam.
- GitHub publication does not mean the Typst package is published on Universe.
