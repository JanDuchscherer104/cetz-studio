// Private, temporary query projection. Never part of the saved figure.
#import "@preview/fletcher:0.5.8" as __cetz_studio_f
#let __cetz_studio_point(p) = (x: p.at(0) / 1mm, y: -p.at(1) / 1mm)
#let __cetz_studio_measure(grid, nodes, edges, options) = {
  let ctx = __cetz_studio_f.default-ctx + (target-system: "xyz", grid: grid)
  for node in nodes { ctx = __cetz_studio_f.register-node-anchors(ctx, node) }
  let measured-nodes = nodes.enumerate().map(((i, node)) => {
    let center = node.at("bounding-center", default: node.pos.xyz)
    let (x, y) = center
    let (w, h) = node.size
    // Circles use a radius, not the label's rectangular dimensions.
    if node.shape == __cetz_studio_f.shapes.circle {
      w = 2 * node.radius
      h = 2 * node.radius
    }
    let stroke = if node.stroke == none { 0pt } else { node.stroke.thickness }
    let pad = calc.max(0pt, node.outset, ..node.extrude) + stroke / 2
    let ports = if node.name == none { () } else {
      ("east", "south", "west", "north").map(side => {
        __cetz_studio_point(__cetz_studio_f.resolve-anchor(ctx, (name: node.name, anchor: side)))
      })
    }
    (
      id: if node.name == none { "@anonymous:" + str(i) } else { str(node.name) },
      bounds: (
        min: (x: (x - w/2 - pad)/1mm, y: (-y - h/2 - pad)/1mm),
        max: (x: (x + w/2 + pad)/1mm, y: (-y + h/2 + pad)/1mm),
      ),
      ports: ports,
    )
  })
  let measured-edges = edges.map(e => (
    points: e.final-vertices.map(__cetz_studio_point),
    width_mm: e.stroke.thickness / 1mm,
    corner_mm: if e.corner-radius == none { 0 } else { e.corner-radius / 1mm },
    kind: e.kind,
  ))
  [#metadata((nodes: measured-nodes, edges: measured-edges)) <cetz-studio-routing-geometry>]
}
