#import "@preview/scenery:0.1.0" as scene
#set page(width: 110mm, height: 70mm, margin: 8mm)
#let camera-preset = scene.camera.with(azimuth: 30deg)
// Fixed construction, never an inferred control.
#let objects = scene.build-scene(
  scene.sphere((0, 0, 0), 0.6),
  scene.sphere((2, 0, 0), 0.4, color: rgb("DD8452")),
  scene.seg((0, 0, 0), (2, 0, 0)),
)
#scene.render-scene(objects, camera-preset(elevation: 20deg), width: 55mm)
