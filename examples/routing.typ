// Synthetic routing acceptance fixture. No scientific content is migrated.
#import "@preview/fletcher:0.5.8" as f
#set page(width: auto, height: auto, margin: 8mm)
#set text(size: 10pt)
#f.diagram(
  node-stroke: 0.6pt,
  node-fill: rgb("e5eaf0"),
  node-corner-radius: 2pt,
  edge-stroke: 0.8pt,
  edge-corner-radius: 0pt,
  f.node((0mm, 0mm), [Source], name: <source>, width: 18mm, height: 12mm),
  f.node((40mm, 0mm), [Obstacle], name: <obstacle>, width: 20mm, height: 24mm),
  f.node((80mm, 0mm), [Target], name: <target>, width: 18mm, height: 12mm),
  f.edge(<source.east>, <target.west>, "-|>", [$Q_h$]),
  f.edge(<source.south>, <target.south>, "->", [$K,V$]),
  // This diagonal attachment stays manual and has a capability explanation.
  f.edge(<source.north-east>, <target.north>, "->"),
)
