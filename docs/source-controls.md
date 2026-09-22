# Library-aware source controls

CeTZ Studio already uses Typst's parser. Broader editability needs better
**source interpretation**, not another grammar or a reverse SVG compiler.

The Parameters panel now has two lanes, both using the existing `SetParameter`
command, compile-before-adoption, undo and explicit save:

- Explicit top-level `studio.param(...)` declarations define the author-chosen
  editing surface. Module aliases and explicitly renamed imports are supported.
  When declarations are present, automatic inference is suppressed, including
  when a declaration is malformed. Existing authored control boundaries stay intact.
- Without declarations, reviewed library adapters expose literal display
  arguments from source calls. No wrapper insertion or source migration is needed.
  These are **source-occurrence controls**, not a claim that each occurrence
  denotes one draggable object in the rendered figure.

## Supported source forms

```typst
#import "@preview/scenery:0.1.0": camera as view, render-scene
#let preset = view.with(azimuth: 30deg)
#render-scene(objects, preset(elevation: 20deg), width: 55mm)
```

The three literal display values become controls. Editing a preset can affect
multiple consumers; an explicit override may hide that preset value. Normal
identity aliases and scoped imports such as `import c.draw: circle as disk`
retain their upstream provenance. Unknown wildcard imports invalidate inherited
provenance rather than guessing which imported names are shadowed.

Numbers, absolute/font-relative lengths, angles and percentages retain their
**authored units**. Tuple properties are edited one literal component at a time;
comments and all other expressions remain byte-for-byte unchanged. A no-op edit
preserves spelling, including parentheses and exponent notation. The existing
numeric/color/boolean frontend contract and Tweakpane widgets are reused.

| Reviewed package | Automatically recognized properties |
| --- | --- |
| CeTZ 0.5.2 | Canvas length/padding; circle/rectangle radius; literal line stroke width |
| Fletcher 0.5.8 | Diagram spacing, corner radius and two-component cell size; node size/inset/corner radius |
| Scenery 0.1.0 | Camera azimuth/elevation/distance and scene display width |
| Plotsy-3D 0.2.1 | Three-component display scale, two projection direction vectors, axis-label/rear-axis text sizes and axis/dot widths |
| Maquette 0.1.2 | Numeric camera azimuth/elevation/distance and display width/height |

Unknown package versions do not receive inferred editing permissions. Explicit
Studio declarations remain available for unsupported libraries. A detected
package import alone does not establish an editable property or graph gesture.

The fixtures under `tests/fixtures/library-controls/` contain no Studio import.
Their shapes are constructed examples, not scientific measurements. The native
integration test compiles each with its actual package, edits through Session,
checks the exact source delta and rendered change, and exercises no-op,
undo/redo/save/backup/reopen. Existing explicit-control fixtures must remain
sealed to their declared controls.

## Preserved limits

This adapter does not evaluate arbitrary expressions, follow values into other
files, invert transforms, expand loops, or edit individual instances of a
function/loop-generated figure. Function/loop bodies and conditional/contextual
constructions are skipped. A literal outside them can still be edited.

Computed arguments, unknown wrappers, rebinding/assignment, argument spreads,
ambiguous imports, malformed tuples and syntax errors fail closed. Properties
containing any unsupported tuple component stay wholly read-only. Mathematical
functions, sampling domains, datasets, meshes and calibrated poses are not
inferred controls. A display change can still affect scientific communication:
validation is not scientific approval.

Use an explicit declaration when an argument is computed:

```typst
#import "@local/cetz-studio:0.1.0" as controls
#let azimuth = controls.param(30deg, min: -180deg, max: 180deg, step: 1deg)
```

This does not make arbitrary 3D dragging possible. Such a gesture needs a proven
source-to-object mapping and an explicit axis/plane constraint. Graph mapping
continues to be owned by the existing Fletcher measurement adapter.

## Implementation and upstream reuse

`parameters::parse` and `parameters::apply` remain the small public interface.
All library knowledge is private to `parameters/adapters.rs`; lexical discovery
is private to `parameters/discover.rs`. No new renderer, package manager,
language server, protocol, trait hierarchy or runtime dependency was introduced.

The implementation uses the existing, pinned `typst-syntax` crate:
`Source`, typed `ModuleImport`, `ImportItem`, `FuncCall`, `LetBinding`, `Array`
and source spans. It reuses the existing source patcher and SHA-256 dependency
for stale-discovery rejection, including calls made outside Session. No client
can submit arbitrary source ranges through the parameter interface.

Primary research and exact API owners:

- [Typst 0.14.2 typed AST](https://github.com/typst/typst/blob/v0.14.2/crates/typst-syntax/src/ast.rs)
  provides imports, aliases, arguments, patterns, numeric units and literals.
- [Typst Source](https://github.com/typst/typst/blob/v0.14.2/crates/typst-syntax/src/source.rs)
  provides numbered spans, UTF-8 byte ranges and incremental parsing. A second
  handwritten language grammar is unnecessary.
- [CeTZ 0.5.2](https://typst.app/universe/package/cetz/0.5.2/) and
  [Fletcher 0.5.8](https://typst.app/universe/package/fletcher/0.5.8/)
  own drawing and measured graph geometry.
- [Scenery 0.1.0](https://github.com/typst/packages/tree/main/packages/preview/scenery/0.1.0)
  owns scene construction and camera projection. Its angles use Typst units.
- [Plotsy-3D 0.2.1 source](https://github.com/typst/packages/blob/main/packages/preview/plotsy-3d/0.2.1/plotsy-3d.typ)
  consumes two direction vectors in `rotation-matrix`, not a 3x3 matrix. Its
  internal CeTZ 0.4.1 remains upstream-owned; no forced dependency upgrade occurs.
- [Maquette 0.1.2](https://github.com/typst/packages/tree/main/packages/preview/maquette/0.1.2)
  uses unitless camera-angle magnitudes. Model bytes stay unchanged.

A future cross-file adapter should investigate Tinymist's existing name analysis
and compiler world before extending this static resolver into an evaluator.
Evaluated geometry may support better selection, but cannot itself determine
which source expression a gesture should modify. Parsing, evaluation, geometry
mapping and reversible editing are different contracts.

## Verification

```sh
npm ci --ignore-scripts --no-audit --no-fund && npm run build:web
cargo test --locked --test parameter_discovery
cargo test --locked --test library_controls -- --ignored
cargo test --locked --all-targets
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

Native fixture tests require Typst 0.14.2 and the pinned packages. They retain
source diffs, discovered controls and native SVGs under `.ci/library-controls/`.
The existing Ubuntu/macOS native CI runs ignored Rust tests. A configured test
is not a passing result; the PR records observed runs and remaining gaps.
