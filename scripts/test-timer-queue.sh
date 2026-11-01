#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/third_party/gpui-pre-windows/src/timer_queue.rs"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
rustc --edition 2024 --test --crate-name timer_queue -o "$OUT/timer_queue" "$MODULE"
"$OUT/timer_queue"
