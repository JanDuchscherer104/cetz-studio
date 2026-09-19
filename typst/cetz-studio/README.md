# Cetz Studio for Typst

`cetz-studio` is a small Typst package for diagrams that should remain ordinary,
readable Typst while also exposing a deliberate editing surface to Cetz Studio.
It composes [CeTZ 0.5.2](https://github.com/cetz-package/cetz) and
[Fletcher 0.5.8](https://github.com/Jollywatt/typst-fletcher) behind a shared
theme and a few high-leverage primitives.

The package is independent of the desktop/web editor. A document that imports
it can be compiled with Typst 0.14.2 on macOS or Linux without running Cetz
Studio.

## Status and import

Version `0.1.0` is source-available in this repository and is not yet published
to Typst Universe. Use the repository-relative import in the included examples:

```typ
#import "../typst/cetz-studio/lib.typ" as studio
```

After a future Typst Universe release, the intended import is:

```typ
// Future package import; unavailable until the package is published.
#import "@preview/cetz-studio:0.1.0" as studio
```

CeTZ and Fletcher are pinned by `lib.typ` to `0.5.2` and `0.5.8`. Typst resolves
those packages in its normal package cache.

## Editable parameters

Declare controls explicitly at top level. `param` validates basic metadata and
returns its first argument unchanged, so the file retains plain Typst semantics:

```typ
#let radius = studio.param(
  20mm,
  label: "Radius",
  min: 5mm,
  max: 50mm,
  step: 1mm,
)
#let fill-hex = studio.param("#258975", label: "Accent", kind: "color")
#let show-guides = studio.param(true, label: "Guides", kind: "bool")

#let accent = rgb(fill-hex)
```

The Typst helper accepts `number`, `int`, `float`, `length`, `ratio`, `angle`,
`bool`, `string`, and `color` declarations and otherwise infers the value's type
when `kind` is omitted. The current editor exposes the narrower supported subset:
numbers, lengths using `mm`, `cm`, `pt`, or `in`, `#rrggbb` color strings, and
booleans. Other helper values compile as normal Typst but remain source-owned in
the editor. A color string is intentionally converted by the author with
`rgb(fill-hex)`, keeping its representation and failure behavior visible.

Cetz Studio only treats direct, explicit declarations as controls. It does not
extract parameters from arbitrary helper calls, infer scientific variables, or
turn equations and loaded data into controls. Loop-generated objects remain
read-only as individual objects; changing a declared loop input such as radius,
spacing, count, or style can regenerate the loop through Typst.

## Themes

`default-theme` is a dictionary shared by the Fletcher and CeTZ primitives.
Use `theme` to make a copy with named overrides:

```typ
#let warm = studio.theme(
  accent: rgb("C16B42"),
  node-fill: rgb("FFF0E8"),
  edge-stroke: 0.9pt + rgb("8A4930"),
)
```

The stable `0.1.0` keys are:

| Key | Used by |
| --- | --- |
| `ink` | Text in cards, annotations, panels, and edge labels |
| `surface` | CeTZ panel and group backgrounds |
| `node-fill` | Fletcher nodes and cards |
| `accent` | Available to documents and derived themes |
| `muted` | Available for guides and secondary marks |
| `node-stroke` | Fletcher nodes and CeTZ framed content |
| `edge-stroke` | Fletcher diagrams and CeTZ flows/callouts |
| `radius` | Fletcher node and edge corner radius |
| `node-inset` | Fletcher node padding |
| `title-size` | Card titles |
| `body-size` | Card bodies and CeTZ annotations |

Every helper accepts either a `theme` or direct style options where that is
useful. Direct options win over theme defaults.

## Fletcher primitives

```typ
#studio.diagram(
  theme: warm,
  studio.card((0mm, 0mm), [Draft], body: [source], name: <draft>, theme: warm),
  studio.card((40mm, 0mm), [Review], body: [decision], name: <review>, theme: warm),
  studio.edge(<draft>, <review>, "-|>", [inspect], theme: warm),
)
```

- `diagram(..args)` supplies diagram-level edge defaults and forwards Fletcher
  positional content and named options.
- `node(pos, body, theme: ..., ..options)` supplies a rectangular node style
  and forwards Fletcher node options. `pos` is the ordinary Fletcher coordinate.
- `card-content(title, body: ..., ...)` produces structured content that can be
  placed in `node` or an upstream Fletcher node.
- `card(pos, title, body: ..., ...)` combines `card-content` and `node`.
- `edge(..args)` supplies a compact label wrapper and forwards Fletcher edge
  endpoints, waypoints, marks, and named options.

Literal tuple coordinates and literal waypoints are the best editing surface.
Computed coordinates, grid equations, custom render callbacks, dynamic node
generation, and unsupported routing expressions still compile through Fletcher,
but the editor may present them as read-only.

## CeTZ primitives

Use these inside `studio.canvas`. They execute normal CeTZ draw calls and can be
freely mixed with `studio.cetz.draw`:

```typ
#studio.canvas(length: 1mm, {
  import studio.cetz.draw: circle

  studio.group((-12, -8), (24, 16), {
    circle((0, 0), radius: 4)
    studio.flow((-7, 0), (7, 0))
  }, label: [A group])
  studio.annotation((0, 0), [origin])
})
```

- `canvas(..args)` forwards directly to `cetz.canvas`.
- `annotation(at, body, ...)` places compact framed content.
- `callout(at, target, body, ...)` combines an arrow and annotation.
- `panel(origin, size, body, ...)` draws a rectangular panel with centered
  content.
- `group(origin, size, body, label: ..., ...)` draws an enclosure behind an
  arbitrary block of CeTZ calls.
- `flow(..points, ...)` draws a marked polyline through two or more points.

For Bézier curves, transformations, coordinate intersections, plots, custom
marks, and other advanced features, use `studio.cetz` directly. The package also
exports `studio.fletcher` and `studio.shapes`. These exports keep the seam deep:
the Studio helpers own common composition and style while upstream packages own
their full drawing languages.

## Examples and verification

From the repository root:

```sh
sh tests/package_compile.sh
```

The check compiles:

- `examples/studio-cetz.typ`, a loop-generated CeTZ scene controlled by length,
  color, stroke, and boolean parameters;
- `examples/studio-fletcher.typ`, a generic graph using physical literal tuple
  coordinates and Studio cards/edges.

The script uses only POSIX shell and the `typst` executable, so the same command
works on macOS and Ubuntu.

## Local package installation

To try the future package-style import before publication, copy this directory
into Typst's local package namespace. On macOS:

```sh
destination="$HOME/Library/Application Support/typst/packages/local/cetz-studio/0.1.0"
mkdir -p "$destination"
cp -R typst/cetz-studio/. "$destination/"
```

On Linux:

```sh
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
destination="$data_home/typst/packages/local/cetz-studio/0.1.0"
mkdir -p "$destination"
cp -R typst/cetz-studio/. "$destination/"
```

Then import `@local/cetz-studio:0.1.0`. Run the commands from the repository
root; both forms quote paths that may contain spaces.

## Current and future interface

The functions and theme keys documented above are implemented in `0.1.0`.
Planned editor features such as automatically generated control panels,
multi-selection, constraint editing, and package-registry installation are not
part of the current Typst interface. Future versions may add higher-level
primitives only when they hide repeated behavior; the package will not mirror
every CeTZ or Fletcher function with a thin wrapper.

## License

The Cetz Studio package code is MIT licensed. It imports CeTZ
(LGPL-3.0-or-later) and Fletcher (MIT) as external Typst dependencies; their
code is not copied or vendored here.
