#import "@preview/cetz:0.5.2" as drawing
#set page(width: 110mm, height: 60mm, margin: 8mm)
#drawing.canvas(length: 8mm, {
  import drawing.draw: circle as disk, line
  disk((1, 1), radius: 1.0, fill: rgb("D8EADD"))
  line((0, 0), (3, 2), stroke: 0.7pt)
})
