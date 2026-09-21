#import "@preview/maquette:0.1.2": render-obj
#set page(width: 110mm, height: 90mm, margin: 8mm)
#render-obj(read("tetra.obj", encoding: none),
  azimuth: 30, elevation: 20, width: 50mm, format: "svg",
)
