#!/usr/bin/env bash
# Unit tests for the GPU device-loss retry policy in the vendored wgpu
# renderer (third_party/gpui-pre-wgpu/src/device_recovery.rs).
#
# The crate's own test target needs fonts from the Zed tree and naga, so the
# std-only module is built standalone with rustc --test; this runs on any host.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name device_recovery -o "$OUT/device_recovery" \
  "$ROOT/third_party/gpui-pre-wgpu/src/device_recovery.rs"
"$OUT/device_recovery"
