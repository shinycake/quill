#!/usr/bin/env bash
# Unit tests for the notification extras side channel shared by the vendored
# GPUI macOS and Windows backends (docs/decisions/codex-notify-native.md).
#
# The two crates only compile for their own OS, and the module is std-only, so
# it is built standalone with rustc --test; this runs on any host. Also checks
# that the two copies are identical.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MACOS="$ROOT/third_party/gpui-pre-macos/src/notification_extras.rs"
WINDOWS="$ROOT/third_party/gpui-pre-windows/src/notification_extras.rs"

if ! cmp -s "$MACOS" "$WINDOWS"; then
  diff -u "$MACOS" "$WINDOWS" | head -40 >&2 || true
  echo "error: the two notification_extras.rs copies differ" >&2
  exit 1
fi

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name notification_extras -o "$OUT/notification_extras" "$MACOS"
"$OUT/notification_extras"
