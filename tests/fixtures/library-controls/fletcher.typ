#import "@preview/fletcher:0.5.8": diagram, node, edge
#set page(width: 140mm, height: 70mm, margin: 8mm)
#diagram(
  cell-size: (40mm, 25mm), spacing: 5pt,
  node((0, 0), [$Q_1$], name: <a>, width: 28mm),
  node((1, 0), [$Q_2$], name: <b>, width: 28mm),
  edge(<a>, <b>, "-|>"),
)
