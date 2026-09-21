#import "@preview/plotsy-3d:0.2.1": plot-3d-surface as plot
#set page(width: 110mm, height: 90mm, margin: 8mm)
#set text(size: 7pt)
// The mathematical function, domain and sampling are source-owned.
#let surface(x, y) = 1 + x * y
#let shade(..args) = rgb("6375AF")
#plot(surface, color-func: shade,
  xdomain: (0, 2), ydomain: (0, 2), subdivisions: 1,
  axis-step: (1, 1, 1),
  scale-dim: (1, 1, 0.5),
  rotation-matrix: ((-2, 2, 4), (0, -1, 0)),
  axis-label-size: 1.5em,
)
