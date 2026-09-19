// Cetz Studio — small, compositional primitives for CeTZ and Fletcher.
// SPDX-License-Identifier: MIT

#import "@preview/cetz:0.5.2" as cetz
#import "@preview/fletcher:0.5.8" as fletcher

/// The upstream modules remain available for advanced figures. Cetz Studio's
/// helpers are conveniences, not a replacement for either package.
#let cetz = cetz
#let fletcher = fletcher
#let shapes = fletcher.shapes

/// Shared style values used by the Fletcher and CeTZ helpers.
#let default-theme = (
  ink: rgb("243447"),
  surface: rgb("F7F9FB"),
  node-fill: rgb("E8EEF4"),
  accent: rgb("258975"),
  muted: rgb("718096"),
  node-stroke: .7pt + rgb("7A8795"),
  edge-stroke: .75pt + rgb("35495E"),
  radius: 3pt,
  node-inset: 5pt,
  title-size: 9pt,
  body-size: 8pt,
)

/// Return a theme with named overrides applied.
///
/// ```typ
/// #let warm = studio.theme(accent: rgb("C16B42"), node-fill: rgb("FFF0E8"))
/// ```
#let theme(..overrides) = {
  assert.eq(overrides.pos(), (), message: "theme accepts named overrides only")
  let named = overrides.named()
  for key in named.keys() {
    assert(key in default-theme, message: "unknown theme key: " + repr(key))
  }
  default-theme + named
}

#let _param-kind(value) = {
  let value-type = type(value)
  if value-type == int { "int" }
  else if value-type == float { "float" }
  else if value-type == length { "length" }
  else if value-type == ratio { "ratio" }
  else if value-type == angle { "angle" }
  else if value-type == bool { "bool" }
  else if value-type == str { "string" }
  else if value-type == color { "color" }
  else { none }
}

#let _number-kind(kind) = kind in ("int", "float", "number")
#let _compatible-kind(actual, expected) = actual == expected or (
  _number-kind(actual) and _number-kind(expected)
)
#let _range-kind(kind) = kind in ("int", "float", "number", "length", "ratio", "angle")
#let _zero(value) = {
  let value-type = type(value)
  if value-type == length { 0pt }
  else if value-type == ratio { 0% }
  else if value-type == angle { 0deg }
  else { 0 }
}

/// Declare a Studio-editable value while keeping plain Typst semantics.
///
/// The helper validates basic type/range metadata and returns `value` unchanged.
/// The editor may recognize explicit top-level declarations such as:
///
/// ```typ
/// #let radius = studio.param(20mm, label: "Radius", min: 5mm,
///   max: 50mm, step: 1mm)
/// ```
///
/// `kind` may be `auto`, `"number"`, `"int"`, `"float"`, `"length"`,
/// `"ratio"`, `"angle"`, `"bool"`, `"string"`, or `"color"`. Color
/// controls may use a hex string and convert it explicitly with `rgb(value)`.
#let param(value, label: none, min: none, max: none, step: none, kind: auto) = {
  let actual = _param-kind(value)
  assert(actual != none, message: "unsupported Studio parameter value: " + repr(value))
  assert(label == none or type(label) == str,
    message: "parameter label must be a string or none")

  let declared = if kind == auto { actual } else { kind }
  assert(type(declared) == str and declared in (
    "number", "int", "float", "length", "ratio", "angle",
    "bool", "string", "color",
  ), message: "unsupported Studio parameter kind: " + repr(kind))
  assert(
    _compatible-kind(actual, declared) or (declared == "color" and actual == "string"),
    message: "parameter kind " + repr(declared) + " does not match " + repr(actual),
  )

  if min != none or max != none or step != none {
    assert(_range-kind(actual),
      message: "min, max, and step are only valid for ordered numeric parameters")
  }
  if min != none {
    let min-kind = _param-kind(min)
    assert(_compatible-kind(actual, min-kind), message: "min must use the parameter's scale")
    assert(value >= min, message: "parameter value is below min")
  }
  if max != none {
    let max-kind = _param-kind(max)
    assert(_compatible-kind(actual, max-kind), message: "max must use the parameter's scale")
    assert(value <= max, message: "parameter value is above max")
  }
  if min != none and max != none {
    assert(min <= max, message: "parameter min must not exceed max")
  }
  if step != none {
    let step-kind = _param-kind(step)
    assert(_compatible-kind(actual, step-kind), message: "step must use the parameter's scale")
    assert(step > _zero(step), message: "parameter step must be positive")
  }
  value
}

/// Fletcher diagram with Studio defaults. Positional arguments and all named
/// Fletcher options pass through; explicit options override the theme.
#let diagram(..args) = {
  let selected = args.named().at("theme", default: default-theme)
  let options = args.named()
  if "theme" in options {
    let _ = options.remove("theme")
  }
  fletcher.diagram(..args.pos(), ..((
    edge-stroke: selected.edge-stroke,
    edge-corner-radius: selected.radius,
    mark-scale: 65%,
    spacing: 2pt,
  ) + options))
}

/// Fletcher node at a literal tuple coordinate.
#let node(pos, body, theme: default-theme, ..options) = {
  assert.eq(options.pos(), (), message: "node accepts no extra positional arguments")
  fletcher.node(
    pos,
    body,
    ..((
      shape: shapes.rect,
      inset: theme.node-inset,
      fill: theme.node-fill,
      stroke: theme.node-stroke,
      corner-radius: theme.radius,
    ) + options.named()),
  )
}

/// Structured content for `node`. Keeping the content form separate makes
/// composition inexpensive; `card` below combines both helpers.
#let card-content(title, body: [], theme: default-theme, alignment: center) = align(
  alignment + horizon,
  [#text(size: theme.title-size, weight: "bold", title)
  #if body != [] { linebreak(); text(size: theme.body-size, fill: theme.ink, body) }],
)

#let card(pos, title, body: [], theme: default-theme, ..options) = {
  assert.eq(options.pos(), (), message: "card accepts no extra positional arguments")
  node(
    pos,
    card-content(title, body: body, theme: theme),
    theme: theme,
    ..options.named(),
  )
}

/// Fletcher edge with compact, readable labels. Literal waypoints and all
/// Fletcher edge options pass through unchanged.
#let edge(..args) = {
  let selected = args.named().at("theme", default: default-theme)
  let options = args.named()
  if "theme" in options {
    let _ = options.remove("theme")
  }
  fletcher.edge(..args.pos(), ..((
    label-side: center,
    label-wrapper: edge => box(
      fill: white,
      inset: (x: 2pt, y: 1pt),
      radius: 1pt,
      text(size: 7.5pt, fill: selected.ink, edge.label),
    ),
  ) + options))
}

/// CeTZ canvas pass-through. Use the helpers below inside its body.
#let canvas(..args) = cetz.canvas(..args.pos(), ..args.named())

/// Place a small CeTZ annotation at a canvas coordinate.
#let annotation(at, body, theme: default-theme, anchor: "center", name: none,
  fill: white, stroke: auto, inset: 3pt, radius: 2pt) = {
  let border = if stroke == auto { theme.node-stroke } else { stroke }
  cetz.draw.content(
    at,
    box(fill: fill, stroke: border, inset: inset, radius: radius,
      text(size: theme.body-size, fill: theme.ink, body)),
    anchor: anchor,
    name: name,
  )
}

/// Connect an annotation to a target coordinate.
#let callout(at, target, body, theme: default-theme, anchor: "center",
  mark: (end: ">"), ..style) = {
  assert.eq(style.pos(), (), message: "callout accepts no extra positional arguments")
  cetz.draw.line(at, target, ..((
    stroke: theme.edge-stroke,
    mark: mark,
  ) + style.named()))
  annotation(at, body, theme: theme, anchor: anchor)
}

/// Draw a rectangular canvas panel and place content at its centre.
#let panel(origin, size, body, theme: default-theme, fill: auto, stroke: auto,
  radius: .12, name: none, ..style) = {
  assert.eq(style.pos(), (), message: "panel accepts no extra positional arguments")
  let (x, y) = origin
  let (width, height) = size
  let panel-fill = if fill == auto { theme.surface } else { fill }
  let panel-stroke = if stroke == auto { theme.node-stroke } else { stroke }
  cetz.draw.rect(
    origin,
    (x + width, y + height),
    name: name,
    ..((fill: panel-fill, stroke: panel-stroke, radius: radius) + style.named()),
  )
  cetz.draw.content(
    (x + width / 2, y + height / 2),
    text(size: theme.body-size, fill: theme.ink, body),
  )
}

/// Enclose related CeTZ drawables behind an optional label. `body` contains
/// ordinary CeTZ draw calls and keeps full access to the upstream package.
#let group(origin, size, body, label: none, theme: default-theme,
  fill: auto, stroke: auto, radius: .12) = {
  let (x, y) = origin
  let (width, height) = size
  let group-fill = if fill == auto { theme.surface } else { fill }
  let group-stroke = if stroke == auto { theme.node-stroke } else { stroke }
  cetz.draw.rect(origin, (x + width, y + height),
    fill: group-fill, stroke: group-stroke, radius: radius)
  body
  if label != none {
    cetz.draw.content(
      (x + .25, y + height - .25),
      text(size: theme.body-size, weight: "bold", fill: theme.ink, label),
      anchor: "north-west",
    )
  }
}

/// Draw a polyline through two or more CeTZ points.
#let flow(..points, theme: default-theme, mark: (end: ">"), stroke: auto,
  name: none) = {
  assert.eq(points.named(), (:), message: "unknown flow option")
  cetz.draw.line(
    ..points.pos(),
    stroke: if stroke == auto { theme.edge-stroke } else { stroke },
    mark: mark,
    name: name,
  )
}
