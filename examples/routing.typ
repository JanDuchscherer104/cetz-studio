// Edge-only routing fixture: node positions and scientific text must not change.
#import "@preview/fletcher:0.5.8" as f
#set page(width: auto, height: auto, margin: 8mm, fill: white)
#set text(font: "New Computer Modern", size: 10pt)
#let graph = f.diagram
#let n(x, y, name, body) = f.node((x*1mm, -y*1mm), body,
  name: name, shape: f.shapes.rect, width: 24mm, height: 18mm,
  fill: rgb("e8eef4"), stroke: .7pt, corner-radius: 2pt)
#let edge = f.edge
#align(center)[*Fixed nodes · proposed edge routes*]
#v(4mm)
#graph(
  edge-stroke: .8pt, edge-corner-radius: 3pt,
  n(0, 0, <source>, [Source\ $bold(x)$]),
  n(46, 0, <obstacle>, [Keep fixed\ intervening node]),
  n(92, 0, <target>, [Target\ $f(bold(x))$]),
  edge(<source.east>, <target.west>, "-|>", [$bold(x) in RR^d$], label-pos: (0, .4)),
  edge(<source.south>, (0mm, -30mm), (92mm, -30mm), <target.south>, "--|>", [reference path]),
  edge(<source.north>, <target.north>, "-|>", [curved control], bend: 35deg),
)
