# Architecture and compatibility

Typst owns the program. Its compiled drawing is an observation, not a second
editable document. Rendering and reversible editing have different admission rules.

## Deep modules

The source-edit module exposes inspection, typed revision-checked commands,
validated drafts, and checked saves. Syntax ranges, units, source preservation,
and validation belong in its implementation. Callers do not construct patches.

The compiler module owns project-root/font handling, bounded subprocesses,
SVG validation, and pages. The browser owns selection, viewport state, and local
drag feedback. It consumes capabilities rather than reproducing source rules.

The Typst package owns presentation primitives and themes. Its `param` helper
leaves ordinary values intact while exposing explicit source declarations.
ARIA equations, measured inputs, notation, and domain meanings stay in ARIA.

Two real adapters live at the edit seam: literal Fletcher layout and declared
parameters in arbitrary graphics. Unknown syntax gains no edit permission merely
because it compiles. Unsupported mappings fall back to ordinary rendering.

## Why rendering does not imply reversible editing

A loop can place twelve cameras around a circle using one radius. Moving one
camera could mean changing that radius, phase, loop, or an instance override.
The image does not identify the intended change. Expose declared radius/spacing
controls and retain the program's construction.

Fletcher gesture previews add temporary markers to measure recognized nodes and
edges. Markers never enter saved source. A failed mapping uses normal compilation;
multipage output allows viewing and controls, but gestures require one verified page.

## Existing-diagram scan

On 2026-09-19, a source scan of ARIA-NBV `docs/figures` found 52 `.typ` files:
17 directly imported CeTZ, 9 had direct `#diagram(...)` calls, and 25 had loops.
These overlapping syntax counts are not a tested compatibility percentage.
The shared helpers pin Fletcher 0.5.8 and CeTZ 0.5.2.

Native compilation at ARIA commit
`738ef529e72da2f45ca442cb99717d4c52cb2d90` succeeded for the candidate-query
overview and oracle-lookahead tree. Candidate-generation geometry and the
target-selection procedure failed because their PNGs were unhydrated Git LFS
pointers in that checkout. This is an asset prerequisite, not evidence of an
editor regression. Those figures need actual assets before a rendering claim.

Candidate-query overview is an intended direct-layout case. Data-driven
candidate geometry and target-selection procedures require ordinary rendering
and explicit parameters. No bulk migration of those sources is performed.

## Useful editable properties

| Area | Controls | Contract |
| --- | --- | --- |
| Nodes | x/y, width/height, padding, corner radius | Supported direct coordinates; other values via declarations |
| Paths | Existing waypoints, ports, label segment/fraction | Proven direct Fletcher literals |
| Style | Fill, stroke width/color, opacity, text size | Declared values; no automatic inherited-theme rewrites |
| Generated figures | Radius, spacing, scale, visibility | Declared parameters preserve computed relationships |
| Automatic routing | Preview obstacle-aware routes for selected or all eligible edges | Measured bounds and cardinal ports; fixed-node remeasurement on Apply; see [routing](routing.md) |
| Content | Node titles, card bodies, edge labels as Typst expressions | One selected argument, syntax checked and compiled; literal-only plain-text shortcut |
| Structural edits | Duplicate nodes, add edges, insert gallery presets | Recognized single-page Fletcher graph; existing imports |
| Deletion | Nodes with explicit attached-edge confirmation; individual edges | Proven references, one undo transaction; final node remains protected |
| Future structural editing | Route vertices, groups | Not implemented |

Numbers, lengths, hex colors, and booleans provide the initial control interface.
Enums, gradients, arbitrary rich text, direct shape handles, and angle-specific
widgets remain future work. Scientific data and equations are not editor controls.

## Performance and simplification

The original prototype launches a new compiler for every accepted edit and for
undo/redo. Its synchronous request loop can block during compilation. Measure
cold open, warm edit-to-preview latency, SVG size, and memory before a runtime change.

The browser now retains its mounted SVG when output and gesture capabilities
are unchanged, avoiding repeated parsing and geometry reads during request-state
updates. Drag feedback stays local; compilation occurs on release. Narrow
patches preserve formatting instead of regenerating the whole source.

A future scheduler should keep HTTP responsive, coalesce obsolete work, and
adopt only matching revisions. Incremental compilation is promising, but caches
must track imports, fonts, package/compiler versions, and data files. A source-only
cache key is insufficient. Neither a persistent compiler nor a background
scheduler is claimed in 0.1. Optional edge-routing proposals use one bounded
background worker; ordinary compilation and route adoption remain synchronous.

Keep syntax/validation behind the source-edit interface. Tests exercise source
changes, rollback, capabilities, and round-trips. Avoid a general plugin framework
until another real adapter needs it.

## Reusable foundations

| Project | Useful reuse | Tradeoff |
| --- | --- | --- |
| [maxGraph](https://github.com/maxGraph/maxGraph) | Apache-2.0 graph canvas, ports, routing, groups | Spike when adding structural edits; retain Typst ownership |
| [React Flow](https://github.com/xyflow/xyflow) | MIT node/edge interactions | Adds React/build tooling; source-aware routing remains our work |
| [draw.io](https://github.com/jgraph/drawio) | Full editor with stated Apache-2.0 source terms | Larger fork; assets and trademarks have separate terms |
| [Excalidraw](https://github.com/excalidraw/excalidraw) | MIT whiteboard interactions | Scene format does not provide reversible Typst editing |
| [Tinymist](https://github.com/Myriad-Dreamin/tinymist) | Typst language and preview infrastructure | Investigate persistent compilation; CeTZ object-to-source mapping is not established |

Retain the small SVG shell for 0.1 to prove the source contract without replacing
the UI stack. None of these projects automatically solves inverse program editing.

## Distribution

One repository contains the companion application and importable Typst package.
CI targets Ubuntu/macOS; configured jobs are not passing evidence. Report local
Linux and hosted platform results separately. Pin compiler/packages and supply
fonts/assets. Universe submission is distinct from GitHub publication.

## Project navigation

The project module owns bounded file discovery, path containment, and lazy
compatibility observations. The active source session continues to own drafts,
history, compilation, and saves. File switches advance the revision and session
identity; stale browser requests cannot mutate the newly opened source.

The browser checks files sequentially on request and displays the actual
capabilities. It does not infer editability from an import name alone. Cached
checks are not dependency-aware; Refresh invalidates them. Hidden/generated
directories and symlinks are excluded from project discovery.
