#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/third_party/gpui-pre-windows/src/window_list.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name window_list -o "$OUT/window_list" "$MODULE"
"$OUT/window_list"
