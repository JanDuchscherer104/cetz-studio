# Third-party notices

Cetz Studio is distributed under the MIT License in `LICENSE`.

`fixtures/aria-overview.typ` is an unmodified ARIA-NBV source fixture by
Jan Duchscherer, distributed under Apache-2.0. Its original source and commit
are recorded in `fixtures/README.md`; the full license is retained in
`fixtures/LICENSE-ARIA-NBV`. It is not relicensed under MIT.

The importable Typst package composes CeTZ 0.5.2 and Fletcher 0.5.8 through
package imports. No upstream implementation or fonts are vendored. Their
upstream package licenses continue to apply independently.

The Rust dependencies declared in `Cargo.toml` are fetched from crates.io and
retain their individual upstream licenses and notices:

- anyhow
- clap
- serde and serde_json
- sha2
- similar
- tempfile
- tiny_http
- typst-syntax
- uuid
- wait-timeout
- roxmltree

The native integration path invokes the Typst CLI and fetches the pinned
Fletcher `0.5.8` Typst package. Typst, Fletcher, and their transitive package
dependencies retain their upstream copyright, license, and notice files. This
file records provenance; it does not replace notices distributed by those
projects or packages.

## Optional Rust edge routing

`pathfinding` 4.15.0 is dual MIT / Apache-2.0 and is used under MIT.
Upstream author metadata names Samuel Tardieu. The published crate and inspected
v4.15.0 tree contain the SPDX declaration in Cargo.toml but no standalone
LICENSE-MIT file. The selected MIT terms and attribution are retained in
`docs/licenses/pathfinding-MIT.txt`; upstream source and metadata: https://github.com/evenfurther/pathfinding/tree/v4.15.0.

The crate is statically linked into the Rust service. Cargo.lock records its
transitive dependencies, which retain their own package license terms. No
libavoid, libavoid-js, or ELK implementation is copied or distributed here.

## Bundled browser libraries

DOMPurify 3.4.15 (Cure53 contributors) is used under Apache-2.0. Its unchanged
license text is emitted as `web/dist/licenses.txt`, embedded in the binary and
served at `/vendor-licenses.txt`; upstream license headers are retained in the
bundle. Source and terms: https://github.com/cure53/DOMPurify/tree/3.4.15.

`esbuild-wasm` 0.25.10 (MIT) is build-time only; no bundler or Node runtime is
shipped by the application. `package-lock.json` records exact resolved packages
and integrity hashes. No fonts or upstream source copies are checked in.
