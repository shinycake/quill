#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/third_party/gpui-pre-apple/src/sized_fonts.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name sized_fonts -o "$OUT/sized_fonts" "$MODULE"
"$OUT/sized_fonts"
