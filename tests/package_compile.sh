#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output_dir="${TMPDIR:-/tmp}/cetz-studio-package-check"
mkdir -p "$output_dir"

typst compile "$repo_root/examples/studio-cetz.typ" "$output_dir/studio-cetz.svg" \
  --root "$repo_root"
typst compile "$repo_root/examples/studio-fletcher.typ" "$output_dir/studio-fletcher.svg" \
  --root "$repo_root"
typst compile "$repo_root/tests/package_params.typ" "$output_dir/package-params.pdf" \
  --root "$repo_root"

if typst compile "$repo_root/tests/package_invalid_theme.typ" \
  "$output_dir/package-invalid-theme.pdf" --root "$repo_root" >/dev/null 2>&1; then
  printf '%s\n' "Expected invalid theme key compilation to fail" >&2
  exit 1
fi
if typst compile "$repo_root/tests/package_invalid_argument.typ" \
  "$output_dir/package-invalid-argument.pdf" --root "$repo_root" >/dev/null 2>&1; then
  printf '%s\n' "Expected ignored positional argument compilation to fail" >&2
  exit 1
fi

printf '%s\n' "Compiled Cetz Studio examples to $output_dir"
