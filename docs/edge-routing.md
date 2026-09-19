# Optional obstacle-aware edge routing

Open `examples/routing.typ` with the native editor. Select **Route edges**, choose
a selected edge, an explicit checked selection, or all eligible edges, and set
clearance in millimetres. **Preview route** draws dashed proposed paths without
changing source or undo history. **Apply route** compiles one batch and adopts it
as one undo step. **Save source** remains the only operation that writes the
figure. Discard, Escape, any edit, or a changed source/preview cancels a proposal.

## Source and geometry contract

The first implementation is deliberately limited to verified, single-page,
axis-aligned Fletcher graphs with fixed literal node positions. It uses the real
compiler's measured node rectangles and endpoint markers, not guessed text sizes
or DOM pixel distances. Automatic ports choose among four cardinal sides;
explicit cardinal ports retain their measured attachment and outward direction.
The coordinate system is the editor's downward-positive millimetres, independent
of zoom, pan, wrapper scale, or source tuple units.

The Rust snapshot reports per-edge eligibility using exactly the admission check
used when applying a command. Computed/elastic positions, intermediate anchors,
relative routing modes, unknown geometry options, self loops, diagonal/custom
ports, incomplete direct graph structure, and missing measurements are refused
with an explanation. All-eligible mode reports how many edges it skipped. There
is no generated-source override, layout sidecar, conversion, or automatic reroute
after moving a node.

`route_edges` accepts edge IDs and numeric interior points, never replacement
source. Existing coordinate scalars retain their original units and embedded
comments. Extra tuples are inserted inside the same edge call. Existing tuples
are not deleted: shorter paths receive collinear points to reuse those source
slots without deleting trivia. Endpoint tokens, direction/mark strings, label
content and equations, style arguments, other edges, and node declarations keep
their bytes. Existing `label-pos` arguments remain unchanged, although a label's
visual location necessarily follows its new route.

Every batch is validated and compiled before source, revision, SVG, or history
is changed. A failed edge prevents the entire batch from being accepted. The
proposal captures source, SVG, revision, filename, and file-session identity;
late Worker results are ignored, and Apply sends the captured revision rather
than substituting a newer one. The server checks revision/session and checks the
on-disk source both before and after route compilation. An external edit leaves
the accepted draft intact. Existing save conflict detection and backup behavior
are unchanged. Imported dependency edits are not separately fingerprinted; avoid
concurrent writers, including edits to shared Typst libraries, during a session.

## Algorithm and limits

The original, dependency-free Worker implementation uses a coordinate-compressed
rectilinear visibility lattice over inflated node boundaries and candidate port
escape points. A* carries a horizontal/vertical heading state; cost is Manhattan
length plus a 5 mm-equivalent bend penalty. Obstacle and segment visibility are
cached. Tie ordering is deterministic. The path is simplified and its serialized
points are checked again against the inflated rectangles. Outer lanes permit
routing around the diagram; the compiled page may grow, but node coordinates do
not move.

Hard ceilings are 80 nodes, 32 requested edges, 30,000 lattice vertices per edge,
96 interior points per edge, 1,536 total points per batch, 100,000 search expansions,
8,000,000 collision checks, and a 750 ms shared Worker budget. The controller also
terminates the disposable Worker after 1.5 seconds, including startup/message
costs. Cancellation terminates it immediately. Search does not run on the browser
interaction thread or block the synchronous Rust server. These are work limits,
not guarantees of success for every diagram below the node-count limit.

**Quality limitations:** clearance measures the orthogonal centerline against
inflated measured node bounding boxes, apart from a connector's own attachment
stubs. It is not a proof about the full painted stroke, arrowhead envelope,
rounded-corner curves, or arbitrary custom renderer. Inspect the real compiled
result before saving; use extra clearance for thick strokes/large corner radii.
Routes do not avoid other edges or label boxes, minimize edge-edge crossings,
separate parallel edges, or preserve a user's previous lane choice. Multiple
routes may overlap. This is not libavoid's incremental routing/nudging behavior.

## Dependency decision (inspected 2026-09-19)

No third-party routing code, WASM, npm package, native library, or crate is added.
The implementation is original code under this repository's existing MIT license.
`Cargo.toml` and `Cargo.lock` are unchanged. Node is only a test runtime, not an
application deployment requirement. The three new browser assets total about
23 KiB uncompressed; release-binary size and memory deltas have not been measured.

| Candidate inspected | Fit and deployment | Version, license, and size evidence |
| --- | --- | --- |
| `libavoid-js` | Strong specialist fixed-node router, including capabilities this small router lacks. Requires shipping/loading WASM and glue, async initialization, lifetime management, and an appropriate WASM/CSP deployment policy. | Inspected source package declares **0.4.5**, **LGPL-2.1-or-later**. Its committed browser glue is **76,321 bytes** and WASM **485,460 bytes**, **561,781 bytes combined**, excluding source maps, notices and memory overhead. These are artifact sizes, not compressed download sizes. No claim that this checkout is npm's latest release. |
| ELK `org.eclipse.elk.alg.libavoid` / `libavoid-server` | The documented fixed-node integration uses a C++ subprocess over standard I/O and an optional ELK module. This is a materially different dependency/deployment choice from merely adding a general layout engine to browser code. It adds per-platform binary packaging; using the ELK adapter additionally adds its Java environment. | Evaluated from ELK's **2022-11-17** integration description; no ELK/server release was selected or redistributed. Libavoid's **LGPL-2.1** obligations remain relevant. Native executable/runtime sizes were not measured. |
| Rust `pathfinding` | Generic A*/graph algorithms, not a ports-and-obstacles diagram router. Would still require our geometry graph, source contract, and a cancellable background server execution path. No browser payload, but native dependency/binary cost is build-dependent. | Inspected **4.16.0**, **MIT OR Apache-2.0**, Rust **1.88** minimum. Compiled size was not measured. |

LGPL distribution needs a compliance review appropriate to the actual linkage
and packaging, including license notices and corresponding-source/replacement
requirements. Browser/WASM delivery is not a reason to assume these disappear.
This is a dependency-engineering decision, not legal clearance. A future libavoid
backend should replace the search module behind the same typed proposal/apply
contract, with pinned artifacts, notices/source distribution, and measured quality
and size comparisons. Its crossing/nudging capabilities are a reasonable reason
to revisit this choice, not something to approximate silently here.

Primary evidence:

- [libavoid-js package metadata](https://github.com/Aksem/libavoid-js/blob/master/package.json),
  inspected blob `be8b53efb341cf819baa94cf15a40f051c5015b2`.
- [libavoid-js committed distributions](https://github.com/Aksem/libavoid-js/tree/master/dist):
  `index.js` blob `f73a026bbb83de097619790cd91b081ca4f5575d`,
  `libavoid.wasm` blob `5043d8771c08f174d6366374ad472c27df2755ae`.
- [ELK: Edge Routing with Libavoid](https://eclipse.dev/elk/blog/posts/2022/22-11-17-libavoid.html).
- [pathfinding crate manifest](https://github.com/evenfurther/pathfinding/blob/main/Cargo.toml),
  inspected blob `50ea2797184b830ace414ccd56a5206099c34b22`.

## Tests and reproducible evidence

Run the pure Worker tests without npm installation:

```sh
node --test tests/routing.test.js
cargo test --locked --all-targets
TYPST=typst cargo test --locked --all-targets -- --ignored
cargo build --locked
python tests/routing_browser.py --binary target/debug/cetz-studio --output .ci/routing
```

The browser test uses the real native server and Typst, never `ui_fixture.js`.
It checks measured obstacle intersections and explicit ports; preview-only source
invariance; one-command apply/undo/redo; save/backup/restart/reopen; explicit/all
selection; unsupported and impossible requests; stale revision/file identity;
late Worker cancellation; and external source conflict rejection. The Rust tests
also cover failed compilation and exact source/trivia/unit preservation. The
existing Ubuntu/macOS CI matrix runs both suites and retains `edge-routing-*`
artifacts with `01-before.png`, `02-proposed.png`, `03-after.png`, before/after Typst
source, server logs, and `metrics.json`. Missing artifacts or a failed job are
not evidence of a successful native run.

The local Node 22.16.0 synthetic 3-node/1-edge benchmark (30 warm repetitions)
measured approximately **0.33 ms median, 0.85 ms p95** in the authoring environment.
This excludes browser messaging, compilation, and disk I/O; it is not a promise
for larger diagrams. CI's metrics separately record Worker search, end-to-end
preview and native apply/compile time, platform/browser version, and node
intersection counts. Edge-edge crossing optimization is explicitly reported as
false. Treat the screenshots and metrics from the exact PR head's successful
native jobs as the review evidence, rather than extrapolating from the local
pure-search benchmark.
