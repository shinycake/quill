#!/usr/bin/env bash
# Unit tests for the Windows timer thread's decisions
# (third_party/gpui-pre-windows/src/timer_step.rs). The crate only builds on
# Windows; timer_step and the timer_queue it uses are std-only, so they are
# built standalone under a two-line crate root; this runs on any host.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/third_party/gpui-pre-windows/src"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
cat > "$OUT/lib.rs" <<RS
#[path = "$SRC/timer_queue.rs"]
mod timer_queue;
#[path = "$SRC/timer_step.rs"]
mod timer_step;
RS
rustc --edition 2024 --test --crate-name timer_step -A dead_code -o "$OUT/timer_step" "$OUT/lib.rs"
"$OUT/timer_step" --skip timer_queue::
