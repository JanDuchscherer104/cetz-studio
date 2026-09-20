# Cetz Studio contributor guidance

Typst source, executable code and tests own behavior. Read the current issue,
its comments and the applicable owner below before changing a contract. This
repository is an independent, generic figure tool; consumer projects retain
scientific notation, data, evidence and manuscript decisions.

## Route to the smallest owner

| Task | Start here | Relevant proof |
| --- | --- | --- |
| Drafts, revisions, undo or saving | [session](src/session.rs), then [architecture](docs/architecture.md) | Session and native round-trip tests |
| Supported source edits or controls | [edit](src/edit.rs), [model](src/model.rs), [parameters](src/parameters.rs) | Literal/source-preservation, rejection and native edit tests |
| Compiler, roots, fonts, pages or SVG validation | [render](src/render.rs) | Real Typst tests, including fallback and bounded failures |
| Project discovery or file switching | [project](src/project.rs) and request handling in [main](src/main.rs) | Project and workspace browser tests |
| Selection, viewport or authoring UI | [web](web), then [graph authoring](docs/graph-authoring.md) | Native browser/workspace tests |
| Obstacle-aware routes | [routing contract](docs/routing.md), then [routing](src/routing) | Native routing and fixed-geometry round trips |
| Themes, drawing primitives or Typst declarations | [package contract](typst/cetz-studio/README.md), then [library](typst/cetz-studio/lib.typ) | Package compile tests and real consumer renders |
| Installation or public capability claims | [README](README.md), exact source and [verification](VERIFICATION.md) | Changed commands/links and actual workflow evidence |

[Test sources](tests) and [CI](.github/workflows/ci.yml) define executable checks;
[verification](VERIFICATION.md) explains their scope and local setup. Choose the
smallest relevant proof, then check affected consumers. A CI configuration is
not a passing run. Keep synthetic UI, native compiler/browser, hosted-platform
and human/agent trial evidence distinct.

## Preserve the editing contract

- Browser commands cross the existing session/edit interface. External coding
  agents collaborate through reviewable source changes rather than an embedded
  chat, voice or shared-session transport. Capabilities come from verified
  source mappings, not an import name or SVG appearance. Unsupported
  constructions keep a preview or declared-control lane.
- Keep source patches, units, validation and save/conflict behavior behind their
  existing owners. Add a primitive or adapter for demonstrated reuse, not a
  wrapper for every upstream function.
- Preserve the accepted draft on invalid edits and compilation failures. Disk
  writes remain explicit, revision-checked and backed up. Retain the documented
  check-to-rename race; do not claim transactional dependency locking.
- Separate display properties from scientific inputs. Calibrated geometry,
  measurements, metrics and claims belong to the consumer project. Human visual
  selection, scientific approval, saving and manuscript inclusion are different
  decisions.

## Bounded changes and delivery

Inspect the working tree and retain unrelated work. Keep each change tied to
one issue outcome; source code, focused tests and necessary documentation travel
together. Reuse upstream packages at their actual pinned versions and prove
version-sensitive behavior with a small fixture. Package loading alone is not
reversible-editing support.

Keep conversation, voice input and agent orchestration in external coding-agent
clients. Do not add embedded collaboration UI or session transport without an
explicitly accepted change to this boundary. A skill or loopback URL does not
establish connectivity. Keep credentials, private transcripts and agent runtime
state out of commits.

Before publishing, review the complete diff and current target branch. Report
exact commits, checks actually run, artifacts and remaining acceptance in the
PR and owning issue. Leave issues open until their delivery/merge criteria are
met. The current user's authorization determines permitted pushes, PRs, merges
and cross-repository changes; an issue is not an additional grant.
