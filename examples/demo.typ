// Synthetic layout fixture, not a measured or scientifically certified diagram.
#import "@preview/fletcher:0.5.8" as f
#set page(width: auto, height: auto, margin: 6mm, fill: white)
#set text(size: 9pt)
#let ink = rgb("26374B")
#let input = rgb("D8EADD")
#let compute = rgb("E5DDED")
#let output = rgb("F7DED8")
#let n(x, y, name, title, body: [], fill: compute) = f.node(
  (x * 1mm, -y * 1mm),
  align(center + horizon)[#text(weight: "bold", title)\ #body],
  name: name, width: 34mm, height: 13mm, inset: 3pt,
  shape: f.shapes.rect, corner-radius: 3pt,
  stroke: .65pt + fill.darken(35%), fill: fill,
)
#let junction(x, y, name) = f.node(
  (x * 1mm, -y * 1mm), [], name: name,
  width: .7mm, height: .7mm, inset: 0pt, fill: ink, stroke: none,
)
#let graph(..args) = f.diagram(..args.pos(), ..(
  edge-stroke: .7pt + ink, edge-corner-radius: 3pt,
  mark-scale: 65%, spacing: 2pt, ..args.named(),
))
#let edge(..args) = f.edge(..args.pos(), ..(
  label-side: center,
  label-wrapper: e => box(fill: white, inset: (x: 2pt, y: 1pt))[
    #text(size: 7.5pt, e.label)
  ], ..args.named(),
))
#align(center)[#text(size: 14pt, weight: "bold")[Candidate scoring · layout fixture]]
#v(5mm)
#graph(
  n(20, 12, <physical>, [Physical inputs], body: [poses · scene statistics], fill: input),
  n(71, 12, <target>, [Target relation], body: [candidate-to-target pose], fill: input),
  n(124, 12, <state>, [Shared state], body: [scene · target · history], fill: input),
  n(20, 40, <trunk>, [Physical trunk], body: [Linear · GELU · norm]),
  n(71, 40, <query>, [Value embedding], body: [$bold(u)_i in RR^d$]),
  n(124, 40, <tokens>, [State encoders], body: [$Z in RR^(5 times d)$]),
  n(20, 65, <feasibility>, [Feasibility], body: [auxiliary prediction], fill: output),
  n(95, 65, <attention>, [Cross-attention], body: [independent queries]),
  n(95, 90, <concat>, [Concatenate], body: [$[bold(u), bold(c), bold(u) dot.o bold(c)]$]),
  n(95, 113, <decoder>, [Scalar decoder], body: [Linear · GELU · Linear]),
  n(144, 113, <value>, [Conditional value], body: [$Q_h$], fill: output),
  junction(71, 55, <query-fork>),
  edge(<physical>, <trunk>, "-|>"),
  edge(<target>, <query>, "-|>"),
  edge(<trunk>, <query>, "-|>"),
  edge(<state>, <tokens>, "-|>"),
  edge(<trunk>, <feasibility>, "-|>"),
  edge(<query>, <query-fork>, "-", [$N_q times d$]),
  edge(<query-fork>, (71mm, -65mm), <attention.west>, "-|>", [$Q$]),
  edge(<tokens>, (124mm, -65mm), <attention.east>, "-|>", [$K,V$]),
  edge(<query-fork>, (52mm, -55mm), (52mm, -90mm), <concat.west>, "-|>", [$bold(u)_i$], label-pos: (1, .5)),
  edge(<attention>, <concat>, "-|>", [$bold(c)_i$]),
  edge(<concat>, <decoder>, "-|>", [$N_q times 3d$]),
  edge(<decoder>, <value>, "-|>", [$N_q times 1$]),
)
#v(5mm)
#align(center)[#text(size: 7.5pt)[Synthetic editor fixture · no measured performance or architecture claim]]
