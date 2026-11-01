#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/third_party/gpui-pre-windows/src/gpui_message.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name gpui_message -o "$OUT/gpui_message" "$MODULE"
"$OUT/gpui_message"
