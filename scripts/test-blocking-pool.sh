#!/usr/bin/env bash
# Tests of the vendored blocking crate (third_party/blocking/QUILL-CHANGES.md).
#
# third_party/blocking is not a workspace member and its tests need
# dev-dependencies, so a copy with a workspace of its own runs them, locked
# to third_party/blocking/Cargo.lock. To update that lockfile, run
# `cargo generate-lockfile` in such a copy and copy its Cargo.lock back.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
cp -r "$ROOT/third_party/blocking" "$OUT/blocking"
printf '\n[workspace]\n' >> "$OUT/blocking/Cargo.toml"
CARGO_TARGET_DIR="$OUT/target" cargo test --locked --manifest-path "$OUT/blocking/Cargo.toml"
