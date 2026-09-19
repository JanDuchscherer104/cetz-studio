#import "../typst/cetz-studio/lib.typ" as studio

#let integer = studio.param(3, min: 1, max: 5, step: 1)
#let decimal = studio.param(0.5, min: 0.0, max: 1.0, step: 0.1)
#let distance = studio.param(12mm, min: 1mm, max: 20mm, step: 1mm)
#let share = studio.param(50%, min: 0%, max: 100%, step: 5%)
#let rotation = studio.param(45deg, min: 0deg, max: 360deg, step: 5deg)
#let enabled = studio.param(true, kind: "bool")
#let color-value = studio.param("#258975", kind: "color")

#assert.eq(integer, 3)
#assert.eq(decimal, 0.5)
#assert.eq(distance, 12mm)
#assert.eq(share, 50%)
#assert.eq(rotation, 45deg)
#assert.eq(enabled, true)
#assert.eq(rgb(color-value), rgb("#258975"))

#studio.diagram(studio.node((0, 0), [Theme-free diagram]))

Parameter validation passed.
