#!/usr/bin/env bash
# Native acceptance, not a substitute fixture. No source or baseline rewriting.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { echo 'Cargo is required; no native check ran.' >&2; exit 1; }
TYPST="${TYPST:-typst}"
export TYPST
command -v "$TYPST" >/dev/null || { echo 'Typst is required; native render checks cannot be skipped by this script.' >&2; exit 1; }
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets -- --ignored
cargo run --locked -- --inspect --file fixtures/aria-overview.typ --root .
sh tests/package_compile.sh
printf '\nNative gates passed. A browser walkthrough against the real server is still required.\n'
