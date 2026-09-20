# Verification

The imported prototype was delivered without a native build. This repository
adds executed native verification and a real browser suite. The synthetic
`ui-preview.html` remains clearly labeled and is not native-render evidence.

## Local environment

Ubuntu Linux x86_64; Rust/Cargo 1.95.0; Typst 0.14.2; Google Chrome driven by
Python Playwright 1.63.0. Fletcher 0.5.8 and CeTZ 0.5.2 are pinned in examples
and the package. All editable test sources are disposable copies.

Initial scaffold acceptance on 2026-09-19 passed: formatting, Clippy, **42 Rust contract
tests**, **5 real Typst integration tests**, package positive/negative checks,
**23 synthetic browser checks**, and **35 native browser checks** with no
JavaScript errors. The browser receipt is in
[`verification/native-browser.json`](verification/native-browser.json).

The subsequent workspace/routing integration passed **81 default Rust tests**,
**10 real Typst tests**, **62 native workspace checks**, and **38 native routing
checks** locally. The original 35 native browser scenarios still pass. Workspace
coverage includes raw maths/content edits, rejected input recovery, all gallery
presets, node/edge deletion, manual corners, quadratic/cubic Bézier controls,
undo, file switching, and save boundaries. See
[`verification/workspace-browser.json`](verification/workspace-browser.json).

The integrated project browser, authoring/deletion, and native routing changes
passed hosted Ubuntu and macOS checks before merging. Manual-curve and showcase
PRs run the same matrix; use the commit-specific workflow results below for their
final status. These records describe executed tests, not universal editability.

The 2026-09-20 architecture-wrapper and transparent-page change passed **58
contract tests**, **10 real Typst tests**, **34 synthetic browser checks**, and
the unchanged **35 native browser checks** locally. A disposable copy of the
ARIA Chapter 04 attention/decoder pipeline also accepted an `architecture-node`
insertion, recompiled to an 11-node preview, and left the original source clean.

## Acceptance commands

| Check | Scope |
| --- | --- |
| `cargo fmt --all -- --check` | Rust formatting |
| `cargo clippy --locked --all-targets -- -D warnings` | Rust lint/type checking |
| `cargo test --locked --all-targets` | Parsing, source patches, units, typed controls, capabilities, rollback, save/conflict contracts |
| `cargo test --locked --all-targets -- --ignored` | Actual Typst renders, instrumented geometry, fallback, and source round trips |
| `sh tests/package_compile.sh` | Standalone CeTZ/Fletcher package examples and valid/invalid parameter/style declarations |
| `python3 tests/browser_smoke.py --chromium /path/to/chrome` | 34 production-UI checks against synthetic geometry; not native evidence |
| `python3 tests/native_browser.py --binary target/debug/cetz-studio --chromium /path/to/chrome` | 35 real browser/server/compiler checks |
| `python3 tests/workspace_browser.py --binary target/debug/cetz-studio` | Project switching, graph/content authoring, deletion, manual routes |
| `python3 tests/routing_browser.py --binary target/debug/cetz-studio` | Proposed routes, fixed geometry, source fidelity, stale/conflict refusal |

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

At the 2026-09-20 ARIA checkout, all 13 Chapter 04 sources containing a direct
`graph(...)` or `diagram(...)` call compiled after their tracked LFS assets were
hydrated. Studio recognized 12 as named graph structures. Ten use literal
millimetre nodes and expose layout editing; two use Fletcher's elastic unitless
grid and retain content editing while positions remain read-only. The remaining
historical rollout/replay figure uses anonymous local wrapper aliases and opens
as a native preview. These observations do not establish a full-corpus editing
success rate.

## Remaining limits

- The save check and atomic rename cannot exclude a non-cooperating concurrent
  writer in their final interval. Source dependencies are not locked.
- Persistent compiler reuse and general asynchronous compilation are not
  implemented. Routing proposals run in one bounded background worker.
- Generated-object overrides, arbitrary CeTZ canvas insertion, and deleting the
  final graph node are not supported. Declared controls remain the edit seam for
  generated drawings; recognized content arguments have a direct Typst editor.
- GitHub publication does not mean the Typst package is published on Universe.
