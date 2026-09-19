#import "../typst/cetz-studio/lib.typ" as studio

#set page(width: auto, height: auto, margin: 8mm, fill: white)
#set text(font: "New Computer Modern", size: 9pt)

#let x-gap = studio.param(42mm, label: "Horizontal gap", min: 24mm, max: 70mm, step: 1mm)
#let y-gap = studio.param(24mm, label: "Vertical gap", min: 16mm, max: 45mm, step: 1mm)
#let fill-hex = studio.param("#E8EEF4", label: "Node fill", kind: "color")
#let show-edge-labels = studio.param(true, label: "Edge labels", kind: "bool")

#let graph-theme = studio.theme(node-fill: rgb(fill-hex))
#let maybe-label(value) = if show-edge-labels { value } else { [] }

#align(center)[
  #studio.diagram(
    theme: graph-theme,
    studio.card((0mm, 0mm), [Input], body: [source data], name: <input>,
      width: 30mm, height: 13mm, theme: graph-theme),
    studio.card((x-gap, 0mm), [Transform], body: [pure operation], name: <transform>,
      width: 30mm, height: 13mm, theme: graph-theme),
    studio.card((x-gap, -y-gap), [Review], body: [human decision], name: <review>,
      width: 30mm, height: 13mm, theme: graph-theme),
    studio.card((2 * x-gap, -y-gap), [Output], body: [published result], name: <output>,
      width: 30mm, height: 13mm, theme: graph-theme),
    studio.edge(<input>, <transform>, "-|>", maybe-label([load]), theme: graph-theme),
    studio.edge(<transform>, <review>, "-|>", maybe-label([inspect]), theme: graph-theme),
    studio.edge(<review>, <output>, "-|>", maybe-label([accept]), theme: graph-theme),
  )
]
