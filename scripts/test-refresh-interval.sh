#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/third_party/gpui-pre-windows/src/refresh_interval.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name refresh_interval -o "$OUT/refresh_interval" "$MODULE"
"$OUT/refresh_interval"
