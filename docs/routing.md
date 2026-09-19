# Optional edge-only routing

Choose an edge, or choose **Checked edges** / **All eligible edges**, set the
clearance, and press **Preview routes**. Orange dashed paths are proposals over
the existing Typst render. They do not alter the draft. **Apply proposal**
compiles the candidate, remeasures it, and adopts all selected routes as one undo
entry only if node bounds and ports are unchanged. **Save source** remains the
only operation that writes the figure, with the existing conflict/backup rules.

Changing source revision, opening another file, an external active-file edit,
or a discarded proposal prevents later adoption. A failed batch has no partial
success. Ordinary node/waypoint editing never invokes the router automatically.

## Source and geometry contract

`routing/measure.typ` is injected only into a temporary sibling query source.
The pinned Fletcher 0.5.8 post-layout callback supplies all resolved node bounds
(including anonymous obstacles), cardinal ports, edge vertices, stroke widths,
and corner radii. The browser never supplies obstacle geometry. Measured bounds
include outset, extrusion, and stroke; circles use their actual radius. Other
shapes use Fletcher's declared size. Custom shapes that draw outside those bounds
are not certified by this adapter. This is not a generic CeTZ drawing router.

Routes have named endpoints and physical, literal interior waypoints. Explicit
north/east/south/west ports stay attached to the exact original node and port.
Bare named endpoints choose the facing cardinal direction from the measured
endpoint displacement; their source reference remains bare. The outward exit
corridor fixes the approach direction. Diagonal ports, self-loops, curves, loops,
elastic/computed waypoints, intermediate named vertices, custom snapping/shift
options, and unknown edge options are reported as unsupported. No coordinates
are inferred from a nearby branch or junction.

Patches replace only interior coordinate expressions and their own comma tokens.
Whitespace and comments between arguments survive; a comment inside a replaced
coordinate expression makes that edge ineligible. Labels, equations, marks,
styles, endpoint identities, node source and unrelated source bytes survive.
Segment-indexed literal label positions are remapped by their fraction of the
old path length; scalar and absent positions remain unchanged. This remapping
is an explicit layout change, not a claim that label collisions are solved.

Apply remeasures the candidate and refuses changes in node positions, extents,
ports, or edge stroke/corner style. Active-file conflict checks run before and
after compilation. This supplements, but does not replace, the existing save
contract. Arbitrary concurrent writes to imported files are not locked; rerender
and preview again after changing dependencies. Pause other file writers when
saving, as with the existing check-to-rename save window.

## Dependency decision

Evaluated on 19 September 2026 against primary upstream documentation/source:

| Option | What is reused | Deployment / decision |
| --- | --- | --- |
| [libavoid-js 0.5.0-beta.5](https://github.com/Aksem/libavoid-js) | Dedicated incremental obstacle-aware routing, C++ libavoid compiled to WebAssembly | Requires a JavaScript/WASM runtime and sidecar assets. Upstream specifies LGPL-2.1-or-later; distributing it requires preserving the applicable notices and source/relinking obligations. Attractive for future crossing/nudging features, but not selected for this native Rust service. |
| [ELK libavoid integration](https://eclipse.dev/elk/blog/posts/2022/22-11-17-libavoid.html) | Fixed-node orthogonal routing | This documented integration uses a Java module and a separate native `libavoid-server`; it is not a routing backend automatically included in elkjs. Adds runtime/process packaging beyond this service. Not selected. |
| [pathfinding 4.15.0](https://github.com/evenfurther/pathfinding/tree/v4.15.0) | Tested A* implementation over a caller-supplied implicit graph | Selected, exactly pinned in Cargo.toml and resolved in Cargo.lock. Dual MIT / Apache-2.0; used under MIT. Rust MSRV 1.87, below the pinned Typst parser's Rust 1.89 requirement. No WASM, C++ compiler, Node build, or new runtime service. |

The selected crate is a search foundation, **not a complete diagram router**.
This repository owns the small visibility-grid, port, clearance and bend-cost
adapter. Existing `typst-syntax`, `tempfile`, `wait-timeout`, `serde`/`serde_json`,
and the installed Typst CLI remain the owners of parsing, files, subprocesses
and rendering. Third-party terms are recorded in `THIRD_PARTY_NOTICES.md`.

The locked new runtime packages are `pathfinding`, `integer-sqrt`,
`num-traits`, `deprecate-until`, `thiserror` and `thiserror-impl`; shared
`indexmap`/`rustc-hash` and their existing dependencies are reused. Cargo.lock is
the actual dependency owner. Production delivery remains one native editor
binary plus the already-required Typst compiler/packages. No routing asset is
fetched at runtime. Release size and timings are measurements, not guarantees
across architectures or toolchains; CI emits the binary size and native fixture
latencies alongside screenshots.

## Algorithm and bounds

The rectilinear visibility grid uses the x/y coordinates of obstacle boundaries
and the two port exits, not a pixel raster. Obstacles are inflated by requested
clearance + half edge width + corner radius + 0.002 mm rounding guard. Only each
endpoint's own outward corridor may cross its inflated rectangle. The remainder
of the path cannot reenter endpoint boxes. A* minimizes path length in rounded-up
micrometres plus a 5 mm-equivalent bend penalty, with deterministic neighbour
order. Simplification removes only zero-length/collinear non-reversing segments.
Rounded corners keep their existing style; the extra radius margin protects the
inside of a fillet. Arrowhead footprints are not independently optimized.

Limits per proposal: 128 measured nodes, 32 selected edges, 70,000 grid points,
50,000 expanded states across the batch, 500 ms search budget, and 128 vertices
per result. Search returns failure rather than a partial path on exhaustion.
The existing configured Typst timeout separately bounds geometry acquisition.

Exactly one background proposal worker is allowed. HTTP state reads and canvas
navigation remain responsive during measurement/search. Discard invalidates a
running job; it does not kill Typst mid-query, and a new job waits until that
bounded worker finishes. Apply uses the existing synchronous compile transaction;
this change does not introduce a general compiler scheduler.

Edges are routed independently. Crossings, collinear edge overlap, label
avoidance, global bundling, and automatic node layout are deliberately excluded.
A larger clearance or fixed port can make a route unavailable. Choose a different
port or lower clearance explicitly; the router does not silently move nodes.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets -- --ignored
cargo build --locked
python tests/routing_browser.py --binary /absolute/path/to/cetz-studio
```

Rust tests cover intervening obstacles, preserved endpoint direction, byte-scoped
patches, comments, label remapping, unsupported expressions, invalid requests,
blocked ports and bounded exhaustion. Native Typst tests cover compile → apply →
undo → redo → save → reopen, fixed measured geometry, external edits, stale
revision and failed compiler adoption. Native browser tests exercise the actual
controls, batch selection, responsive state/zoom during a deliberately delayed
real query, one worker slot, stale results and explicit saving. They run in the
existing Ubuntu/macOS matrix and emit before/proposal/after screenshots and
latency JSON. These fixtures establish editor behavior, not scientific results.
