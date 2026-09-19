#import "../typst/cetz-studio/lib.typ" as studio

#set page(width: auto, height: auto, margin: 8mm, fill: white)
#set text(font: "New Computer Modern", size: 9pt)

// Explicit top-level parameters are the editor's supported control surface.
#let radius = studio.param(20mm, label: "Radius", min: 5mm, max: 50mm, step: 1mm)
#let spacing = studio.param(9mm, label: "Node spacing", min: 4mm, max: 16mm, step: 1mm)
#let accent-hex = studio.param("#258975", label: "Accent", kind: "color")
#let stroke-width = studio.param(0.8pt, label: "Stroke", min: 0.2pt, max: 2pt, step: 0.1pt)
#let show-guides = studio.param(true, label: "Show guides", kind: "bool")

#let accent = rgb(accent-hex)
#let scene-theme = studio.theme(accent: accent, edge-stroke: stroke-width + accent)
#let r = radius / 1mm
#let gap = spacing / 1mm

#align(center)[
  #studio.canvas(length: 1mm, padding: 1mm, {
    import studio.cetz.draw: circle, content, line

    studio.group((-r - gap, -r - gap), (2 * (r + gap), 2 * (r + gap)), {
      if show-guides {
        circle((0, 0), radius: r, stroke: .35pt + scene-theme.muted, fill: none)
        line((-r, 0), (r, 0), stroke: .25pt + scene-theme.muted)
        line((0, -r), (0, r), stroke: .25pt + scene-theme.muted)
      }

      for index in range(6) {
        let angle = index * 60deg
        let point = (r * calc.cos(angle), r * calc.sin(angle))
        circle(point, radius: 1.7, fill: accent.lighten(65%),
          stroke: stroke-width + accent)
        content(point, text(size: 7pt, weight: "bold", str(index + 1)))
      }

      studio.flow((0, 0), (r * .72, r * .42), theme: scene-theme)
    }, label: [Loop-generated scene], theme: scene-theme,
    )
    studio.annotation((0, 0), [origin], theme: scene-theme)
    studio.callout((r + gap + 5, r / 2), (r * .86, r * .5), [editable callout],
      theme: scene-theme, anchor: "west")
  })
]
