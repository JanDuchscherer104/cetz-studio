# Renderer selection and bounded recipes

This is the portable renderer-choice owner. A project's explicit accepted
contract wins over a generic preference. Select only the relevant recipe; an
accepted composition or syntax repair does not restart renderer selection.

| Construction | Prefer | Preserve |
| --- | --- | --- |
| Typed topology with Typst mathematics/composite nodes | Fletcher | Named nodes, connectivity, labels and shared typography |
| Process/state/sequence shared with Markdown tooling | Mermaid | Existing `.mmd` source and local render wrapper |
| Sparse exact geometry, rays, axes and analytic overlays | CeTZ | Explicit coordinates, projection and Typst notation |
| Sparse typed 3D scenes with hidden-line treatment | Scenery candidate | Geometry and visibility convention; fixture gate |
| Local OBJ/PLY/STL mesh inset | Maquette candidate | Camera registration, input/decimation identity, output mode |
| Explicit function or parametric 3D surface | Plotsy-3D experiment | Actual formula/domain; transitive version check |
| Dense scenes, point clouds or quantitative plots | Existing tested data renderer | Frozen scientific inputs/scales; vector annotations where useful |

## Documentation and version evidence

Inspect the actual imports, configuration and consumer first. For a material
API uncertainty, use an available Context7 resolver and the relevant verified
library; `/cetz-package/cetz` and `/fletcher-package/fletcher` were resolved in
the originating audit. They are discovery pointers, not guarantees that the
installed version is indexed. Resolve other IDs rather than guessing them.
When coverage is absent or mismatched, read exact upstream source/primary docs.
Then compile the smallest relevant fixture and inspect the actual output.

Record package/compiler versions, source identity, command, result and inspection
separately. Do not copy broad manuals or load every package on each iteration.
This bundle ships guidance, not certified support for every renderer.

## CeTZ and Fletcher

Studio's inspected baseline pins CeTZ **0.5.2** and Fletcher **0.5.8**. Use named
imports and actual shared primitives; preserve native Typst content instead of
retyping mathematics in another format. For graph editing, explicit named nodes,
physical literal positions and routes are a deliberate supported subset.
Elastic/computed coordinates require a proven adapter or remain source-owned.

Use the consumer's compile command/project root. Independent baseline examples
are available as pinned primary sources:
[CeTZ](https://github.com/JanDuchscherer104/cetz-studio/blob/59463df301d176a13e4b6b50b2f5e299c0c94865/examples/studio-cetz.typ)
and [Fletcher](https://github.com/JanDuchscherer104/cetz-studio/blob/59463df301d176a13e4b6b50b2f5e299c0c94865/examples/studio-fletcher.typ).
Their adjacent package import requires that pinned checkout; they are not
standalone installed-skill assets. For independent use, compile a small fixture
against the actual local imports rather than assuming those paths exist.
Primary documentation: [CeTZ](https://cetz-package.github.io/docs/),
[Fletcher](https://typst.app/universe/package/fletcher/).

## Mermaid

Keep the `.mmd` and existing local CLI/wrapper. Reproduce syntax failures with
the exact command and inspect the local version before changing grammar.
Use the project's symbol projection and semantic classes, if present. Render
through the existing wrapper; missing tools are an explicit gap, not permission
to upload private diagrams to an online service. Include the resulting asset
through the manuscript's format owner and inspect its destination page.
Primary documentation: [Mermaid](https://mermaid.js.org/intro/).

## Scenery

**0.1.0** is the source-inspected starting candidate, not a universal editor
compatibility claim. Use explicit imports to avoid shadowing Typst `label` or
`scale`. Begin with display azimuth/elevation, width and guide controls; numeric
degrees multiplied by `1deg` can use Studio's existing parameter subset.
The display camera is distinct from scientific acquisition poses.

Inspect painter ordering, occlusion and clipping. The default engine has no
z-buffer; perspective lacks near-plane clipping. In this version
`fill-opacity` is a transparentize amount, so 100% removes the fill. Native
example/round-trip acceptance is tracked in [Studio #26](https://github.com/JanDuchscherer104/cetz-studio/issues/26); verify its actual
result before claiming support. Primary source:
[Scenery](https://typst.app/universe/package/scenery/).

## Maquette

The source audit found an ARIA smoke fixture for **0.1.1**; this is historical
consumer evidence, not this bundle's integration proof or a request to downgrade.
Choose and pin the actual evaluated release. SVG uses painter ordering; a
z-buffered PNG can be preferable for dense or intersecting meshes. Keep vector
labels/axes registered to the same projection and viewport. Freeze units,
normalization, camera, shading, decimation and output mode. The renderer does
not own reconstruction metrics. Primary source:
[Maquette](https://typst.app/universe/package/maquette/).

## Plotsy-3D

**0.2.1** remains experimental here. Its inspected source imports CeTZ **0.4.1**,
so test isolated composition before mixing its internal drawables with Studio's
CeTZ 0.5.2. Use one genuinely defined function/parametric surface, not a smooth
substitute for measured samples. An executed negative comparison can justify
retaining the existing renderer; missing compilation is not a negative result.
Primary source: [Plotsy-3D](https://typst.app/universe/package/plotsy-3d/).
