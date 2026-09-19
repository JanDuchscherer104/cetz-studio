// SPDX-License-Identifier: MIT
// Shared by clean package rendering and the editor instrumentation.
#let point(fletcher, points, t) = {
  if type(t) == ratio { t = float(t) }
  if type(t) == relative {
    assert(t.length == 0pt,
      message: "Bezier labels use fractional positions, not absolute lengths")
    t = float(t.ratio)
  }
  let lerp(a, b) = fletcher.cetz.vector.lerp(a, b, t)
  if points.len() == 3 {
    lerp(lerp(points.at(0), points.at(1)), lerp(points.at(1), points.at(2)))
  } else {
    let ab = lerp(points.at(0), points.at(1))
    let bc = lerp(points.at(1), points.at(2))
    let cd = lerp(points.at(2), points.at(3))
    lerp(lerp(ab, bc), lerp(bc, cd))
  }
}

#let draw-edge(fletcher, edge, nodes, debug: 0, on-anchors: none) = {
  assert(edge.final-vertices.len() in (3, 4),
    message: "Studio Bezier edges need one or two control points")
  assert(edge.extrude.len() == 1 and edge.extrude.first() == 0pt,
    message: "Studio Bezier edges do not support extruded strokes")
  assert(not edge.crossing,
    message: "Studio Bezier edges do not support crossing masks")
  assert(edge.decorations == none,
    message: "Studio Bezier edges do not support path decorations")

  let vertices = edge.final-vertices
  let controls = vertices.slice(1, -1)
  let (start, end) = (vertices.first(), vertices.last())
  let end-segments = ((start, controls.first()), (end, controls.last()))
  let dummy-lines = end-segments.map(points => fletcher.cetz.draw.line(..points))
  let intersection-objects = nodes.zip(dummy-lines).map(((candidates, dummy-line)) => {
    fletcher.cetz.draw.group(candidates.map(fletcher.draw-node-outline).join())
    dummy-line
  })

  fletcher.find-anchor-pair(intersection-objects, (start, end), anchors => {
    if on-anchors != none { on-anchors(anchors) }
    let points = (anchors.first(), ..controls, anchors.last())
    let curve(t) = point(fletcher, points, t)
    let drawing = fletcher.cetz.draw.group({
      fletcher.cetz.draw.bezier(
        anchors.first(),
        anchors.last(),
        ..controls,
        stroke: edge.stroke,
      )
      for mark in edge.marks {
        fletcher.place-mark-on-curve(mark, curve,
          stroke: edge.stroke, debug: debug >= 3)
      }
      if edge.label != none {
        fletcher.place-edge-label-on-curve(edge, curve, debug: debug)
      }
    })
    if edge.layer != 0 {
      drawing = fletcher.cetz.draw.on-layer(edge.layer, drawing)
    }
    (edge.post)(drawing)
  })
}
