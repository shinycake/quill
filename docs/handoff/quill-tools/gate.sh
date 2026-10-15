#!/bin/bash
# Quill PR gate, run from the checkout/worktree root: fmt, fmt --check,
# interface strings, core + UI clippy (-D warnings), core + UI tests. Prints GATE OK / GATE FAIL.
# Builds go to the external SSD when mounted (ssd-target.sh links ./target).
bash "$(dirname "$0")/ssd-target.sh" >/dev/null || { echo "GATE FAIL disk: SSD missing and not enough internal space for a local build; hold and report"; exit 1; }
# Disk floor (owner hard rule 2026-10-13): never build with under 30 GB free,
# on the internal Data volume and on the volume target/ lives on.
gb() { df -g "$1" 2>/dev/null | awk 'NR==2{print $4}'; }
for vol in /System/Volumes/Data "$(cd target 2>/dev/null && pwd -P || pwd)"; do
  free_gb=$(gb "$vol"); [ -z "$free_gb" ] && continue
  if [ "$free_gb" -lt 30 ]; then echo "GATE FAIL disk: only ${free_gb} GB free on $vol (floor 30 GB). Stop and report; do not clean other worktrees."; exit 1; fi
done
L=$(mktemp)
if [ -f scripts/check-lang.py ]; then python3 scripts/check-lang.py >$L 2>&1 || { head -20 $L; echo "GATE FAIL lang"; exit 1; }; fi
cargo fmt --all
cargo fmt --all -- --check >$L 2>&1 || { cat $L | head -20; echo "GATE FAIL fmt"; exit 1; }
cargo clippy --no-default-features --all-targets -- -D warnings >$L 2>&1 || { grep -E "^error" -A8 $L | head -40; echo "GATE FAIL clippy"; exit 1; }
cargo clippy --features ui --all-targets -- -D warnings >$L 2>&1 || { grep -E "^(error|warning)" -A8 $L | head -40; echo "GATE FAIL clippy-ui"; exit 1; }
sum() { grep "test result" | awk '{p+=$4; f+=$6} END {print p, f}'; }
cargo test --no-default-features >$L 2>&1; core=$(sum <$L)
cargo test --features demo-capture --locked --bin quill >$L 2>&1; ui=$(sum <$L)
set -- $core; cf=$2; set -- $ui; uf=$2
if [ "$cf" = 0 ] && [ "$uf" = 0 ]; then echo "GATE OK core=$core ui=$ui"; else grep -E "^error|^test .* FAILED|panicked" -A3 $L | head -30; echo "GATE FAIL tests core=($core) ui=($ui)"; exit 1; fi
