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
