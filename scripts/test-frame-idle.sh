#!/usr/bin/env bash
# Unit tests for the idle-frame decision shared by the vendored GPUI X11 and
# Windows backends (docs/decisions/codex-idle-frames-x11-windows.md).
#
# The two crates only compile for their own OS, and the module is std-only, so
# it is built standalone with rustc --test; this runs on any host. Also checks
# that the two copies are identical.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINUX="$ROOT/third_party/gpui-pre-linux/src/linux/x11/frame_idle.rs"
WINDOWS="$ROOT/third_party/gpui-pre-windows/src/frame_idle.rs"

if ! cmp -s "$LINUX" "$WINDOWS"; then
  diff -u "$LINUX" "$WINDOWS" | head -40 >&2 || true
  echo "error: the two frame_idle.rs copies differ" >&2
  exit 1
fi

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name frame_idle -o "$OUT/frame_idle" "$LINUX"
"$OUT/frame_idle"
