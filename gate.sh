#!/bin/bash
# Quill PR gate, run from the checkout/worktree root: fmt, fmt --check,
# interface strings, core + UI clippy (-D warnings), core + UI tests. Prints GATE OK / GATE FAIL.
L=$(mktemp)
if [ -f scripts/check-lang.py ]; then python3 scripts/check-lang.py >$L 2>&1 || { head -20 $L; echo "GATE FAIL lang"; exit 1; }; fi
if grep -rn 'cfg(all(test' src crates --include='*.rs' >$L 2>&1; then head -10 $L; echo "GATE FAIL cfg-form: use stacked #[cfg(test)] + #[cfg(feature = \"demo-capture\")] (CI 'test cfg form' step)"; exit 1; fi
if command -v cargo-about >/dev/null && [ -f scripts/third-party-licenses.sh ]; then bash scripts/third-party-licenses.sh --check >$L 2>&1 || { tail -3 $L; echo "GATE FAIL licenses: run scripts/third-party-licenses.sh and commit THIRD_PARTY_LICENSES.md"; exit 1; }; fi
if [ -f scripts/check-implicit-state.py ]; then python3 scripts/check-implicit-state.py >$L 2>&1 || { head -20 $L; echo "GATE FAIL implicit-state (docs/contributing/state-machines.md)"; exit 1; }; fi
if [ -f scripts/check-file-size.sh ]; then bash scripts/check-file-size.sh >$L 2>&1 || { head -20 $L; echo "GATE FAIL file-size (docs/contributing/structure.md)"; exit 1; }; fi
for s in check-window-scope.py check-screen-state.py check-window-root.py check-click-through.py check-lang.py; do if [ -f scripts/$s ]; then python3 scripts/$s >$L 2>&1 || { tail -20 $L; echo "GATE FAIL $s"; exit 1; }; fi; done
cargo fmt --all
cargo fmt --all -- --check >$L 2>&1 || { cat $L | head -20; echo "GATE FAIL fmt"; exit 1; }
cargo clippy --no-default-features --all-targets -- -D warnings >$L 2>&1 || { grep -E "^error" -A8 $L | head -40; echo "GATE FAIL clippy"; exit 1; }
cargo clippy --features demo-capture --all-targets -- -D warnings >$L 2>&1 || { grep -E "^(error|warning)" -A8 $L | head -40; echo "GATE FAIL clippy-ui"; exit 1; }
sum() { grep "test result" | awk '{p+=$4; f+=$6} END {print p, f}'; }
cargo test --no-default-features >$L.core 2>&1; core=$(sum <$L.core)
cargo test --features demo-capture --locked --bin quill >$L.ui 2>&1; ui=$(sum <$L.ui)
set -- $core; cf=$2; set -- $ui; uf=$2
if [ "$cf" = 0 ] && [ "$uf" = 0 ]; then echo "GATE OK core=$core ui=$ui"; else grep -hE "^error|^test .* FAILED" $L.core $L.ui | head -20; grep -hE "panicked at" -A2 $L.core $L.ui | head -20; echo "GATE FAIL tests core=($core) ui=($ui) logs: $L.core $L.ui"; exit 1; fi
