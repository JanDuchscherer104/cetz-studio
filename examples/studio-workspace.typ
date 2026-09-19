// Editable demonstration, not a scientific architecture or measured result.
#import "../typst/cetz-studio/lib.typ" as studio

#set page(width: auto, height: auto, margin: 8mm)
#set text(size: 9pt)

#studio.diagram(
  studio.card((0mm, 0mm), [Input], body: [Edit this description],
    name: <input>, width: 35mm, height: 16mm),
  studio.node((55mm, 0mm), [Transform], name: <transform>,
    width: 32mm, height: 16mm),
  studio.node((55mm, -40mm), [Review], name: <review>,
    shape: studio.shapes.diamond, width: 30mm),
  studio.card((110mm, -40mm), [Output], body: [Ready to share],
    name: <output>, width: 35mm, height: 16mm),
  studio.edge(<input>, <transform>, "->", [load]),
  studio.edge(<transform>, <review>, "->", [inspect]),
  studio.edge(<review>, <output>, "->", [accept]),
)
