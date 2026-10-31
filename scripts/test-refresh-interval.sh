#!/usr/bin/env bash
# Unit tests for the vendored GPUI Windows backend's vsync interval
# (third_party/gpui-pre-windows/QUILL-CHANGES.md).
#
# The crate only compiles on Windows, and the module is std-only, so it is
# built standalone with rustc --test; this runs on any host.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/third_party/gpui-pre-windows/src/refresh_interval.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name refresh_interval -o "$OUT/refresh_interval" "$MODULE"
"$OUT/refresh_interval"
