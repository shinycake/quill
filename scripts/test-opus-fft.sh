#!/usr/bin/env bash
# Unit tests for the FFT in the vendored opus-decoder
# (docs/decisions/opus-decode-cpu.md).
#
# third_party/opus-decoder is not a workspace member, so `cargo test` does not
# reach it. Its FFT module is std-only, so it is built standalone with
# rustc --test.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FFT="$ROOT/third_party/opus-decoder/src/celt/kiss_fft.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name kiss_fft -o "$OUT/kiss_fft" "$FFT"
"$OUT/kiss_fft"
