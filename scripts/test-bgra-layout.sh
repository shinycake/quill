#!/usr/bin/env bash
# Checks the shims' BGRA picture layout (native/quillvideo/bgra_layout.h)
# against the FFmpeg that scripts/build-ffmpeg.sh built: swscale's
# unscaled kernels must write every column and nothing past the buffer.
# Usage: scripts/test-bgra-layout.sh [ffmpeg prefix]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${1:-$ROOT/vendor/ffmpeg/prefix}"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
cc -O1 -Wall -Wextra -Werror -I"$PREFIX/include" \
  "$ROOT/native/quillvideo/tests/bgra_layout_test.c" \
  -L"$PREFIX/lib" -lswscale -lavutil -Wl,-rpath,"$PREFIX/lib" \
  -o "$OUT/bgra_layout_test"
"$OUT/bgra_layout_test"
