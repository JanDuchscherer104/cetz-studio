// Schematic fixture. Controls change presentation, never the direction data.
#import "../typst/cetz-studio/lib.typ" as studio
#import "@preview/scenery:0.1.0": build-scene, uv-sphere, sphere, edge, arrow, camera, render-scene

#set page(width: 160mm, height: 110mm, margin: 6mm, fill: white)
#set text(size: 9pt, fill: rgb("243447"))

// Numeric controls are supported today; the camera receives actual angles.
#let view-azimuth-deg = studio.param(-38, label: "Display-camera azimuth (degrees)", min: -180, max: 180, step: 1)
#let view-elevation-deg = studio.param(19, label: "Display-camera elevation (degrees)", min: -60, max: 60, step: 1)
#let view-width = studio.param(45mm, label: "Display width", min: 30mm, max: 50mm, step: 1mm)
#let show-guides = studio.param(true, label: "Display guides")

#let data = json("data/scenery-directions.json")
#let directions = data.directions.map(p => (p.at(0), p.at(1), p.at(2)))
#assert(directions.all(p => calc.abs((p.at(0)*p.at(0) + p.at(1)*p.at(1) + p.at(2)*p.at(2)) - 1) < 0.000001))
#let ink = rgb("3269A8")
#let accent = rgb("B56A19")
#let scene(guides: true) = build-scene(
  ..if guides { (
    uv-sphere((0, 0, 0), 1, segments: 12, rings: 6,
      color: rgb("D7DEE6"), fill-opacity: 100%, cull: none,
      stroke: .35pt + rgb("8D9BAA"), hidden-stroke: .2pt + rgb("C7D0DA")),
    arrow((0, 0, 0), (1.18, 0, 0), color: rgb("B23A48"), w: .015),
    arrow((0, 0, 0), (0, 1.18, 0), color: rgb("23856D"), w: .015),
    arrow((0, 0, 0), (0, 0, 1.18), color: rgb("5C4FA3"), w: .015),
  ) } else { () },
  ..directions.enumerate().map(((i, p)) => edge((0, 0, 0), p,
    color: if i == data.highlight_index { accent } else { ink }, width: .7pt)),
  ..directions.enumerate().map(((i, p)) => sphere(p, .045,
    color: if i == data.highlight_index { accent } else { ink }, specular: false)),
)

#text(size: 12pt, weight: "bold")[Display camera, fixed geometry]
#v(2mm)
#text(size: 8pt)[Schematic unit directions. No measured acquisition poses or performance claims.]
#v(4mm)
#grid(columns: (1fr, 1fr), gutter: 6mm,
  block(height: 72mm)[
    *A. Editable oblique view*
    #v(3mm)
    #align(center, render-scene(scene(guides: show-guides),
      camera(mode: "orthographic", azimuth: view-azimuth-deg * 1deg,
        elevation: view-elevation-deg * 1deg), width: view-width))
  ],
  block(height: 72mm)[
    *B. Fixed top projection*
    #v(3mm)
    #align(center, render-scene(scene(),
      camera(mode: "orthographic", azimuth: 0deg, elevation: 90deg), width: 45mm))
  ],
)
#text(size: 7.5pt)[Right-handed XYZ, +Z up; dimensionless coordinates. Top view drops Z.]
#v(1mm)
#text(size: 7.5pt)[Display widths are page sizes, not a metric scale. Each view fits its projected bounds.]
