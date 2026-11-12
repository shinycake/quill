#!/bin/bash
# Quill PR gate, run from the checkout/worktree root. Prints GATE OK / GATE FAIL.
#
# It runs every step of .github/workflows/ci.yml that can run on this Mac, in
# CI's order: the script checks and their own unit tests, fmt, clippy (core
# and UI), the core and UI tests, the standalone vendored tests, and the
# license checks. When ci.yml gains a step, add it here.
#
# Not run here (they need Linux, Windows or CI's services): the netlink
# watcher in a network namespace, the vendored Linux/Windows backend lib
# tests, the launch smokes, the quill-webview carrier tests, the Windows
# cargo check and widget runtime tests, and the gitleaks scan.
#
# GATE_BASE (default origin/main) is the PR base the hotspot check and the
# Metal renderer condition diff against.
L=$(mktemp)
BASE=${GATE_BASE:-origin/main}

# step <name> <command...>: run it; on failure show the end of its output.
step() {
  local name=$1; shift
  "$@" >$L 2>&1 || { tail -20 $L; echo "GATE FAIL $name"; exit 1; }
}
have() { [ -f "scripts/$1" ]; }
py() { if have "$1"; then step "$1" python3 "scripts/$1"; fi; }
sh_check() { if have "$1"; then step "$1" bash "scripts/$@"; fi; }
unit() { if have "$1"; then step "$1" python3 -m unittest discover -s scripts -p "$1"; fi; }

# --- script checks, in ci.yml's order -------------------------------------
sh_check check-app-name.sh
sh_check check-file-size.sh
py check-lang.py
py check-window-scope.py
unit test_dialog_hosts.py
unit test_note_hosts.py
unit test_main_frame_queues.py
py check-screen-state.py
py check-submit-settle.py
unit test_lock_surfaces.py
unit test_single_writers.py
py check-implicit-state.py
py check-stage-enums.py
py check-request-match.py
py check-owned-timers.py
py check-refcell-borrows.py
unit test_shape_guard.py
py check-click-through.py
py check-window-root.py
py check-dialog-answers.py
py check-drained-notes.py
# CI passes the PR's base; a `Hotspot-change:` trailer is read by the script.
if have check-hotspots.sh; then
  if git rev-parse -q --verify "$BASE" >/dev/null 2>&1; then
    step hotspots bash scripts/check-hotspots.sh "$BASE"
  else
    echo "gate: no $BASE here, hotspots not checked"
  fi
fi
if grep -rn 'cfg(all(test' src crates --include='*.rs' >$L 2>&1; then head -10 $L; echo "GATE FAIL cfg-form: use stacked #[cfg(test)] + #[cfg(feature = \"demo-capture\")] (CI 'test cfg form' step)"; exit 1; fi

# --- licenses (CI job `licenses`) ------------------------------------------
if command -v cargo-deny >/dev/null && [ -f deny.toml ]; then
  step deny-licenses cargo deny --locked check licenses
  step deny-advisories cargo deny --locked check advisories
fi
if command -v cargo-about >/dev/null && have third-party-licenses.sh; then
  bash scripts/third-party-licenses.sh --check >$L 2>&1 || { tail -3 $L; echo "GATE FAIL licenses: run scripts/third-party-licenses.sh and commit THIRD_PARTY_LICENSES.md"; exit 1; }
fi

# --- fmt, clippy -----------------------------------------------------------
cargo fmt --all
cargo fmt --all -- --check >$L 2>&1 || { cat $L | head -20; echo "GATE FAIL fmt"; exit 1; }
cargo clippy --no-default-features --all-targets --locked -- -D warnings >$L 2>&1 || { grep -E "^error" -A8 $L | head -40; echo "GATE FAIL clippy"; exit 1; }
cargo clippy --features demo-capture --all-targets --locked -- -D warnings >$L 2>&1 || { grep -E "^(error|warning)" -A8 $L | head -40; echo "GATE FAIL clippy-ui"; exit 1; }

# --- standalone vendored tests (std-only, built with rustc --test) ---------
for t in test-frame-idle.sh test-refresh-interval.sh test-window-list.sh test-timer-queue.sh \
         test-timer-step.sh test-gpui-message.sh test-device-recovery.sh test-input-rate.sh \
         test-notification-extras.sh test-sized-fonts.sh test-opus-fft.sh test-blocking-pool.sh; do
  sh_check "$t"
done

# --- the Metal renderer's tests, when the PR touches the renderer ----------
if [ "$(uname)" = Darwin ] && git rev-parse -q --verify "$BASE" >/dev/null 2>&1 \
   && git diff --name-only "$BASE"...HEAD | grep -qE '^third_party/gpui-pre-apple/|^\.github/workflows/ci\.yml$'; then
  have ensure-metal-toolchain.sh && step metal-toolchain bash scripts/ensure-metal-toolchain.sh
  QUILL_REQUIRE_METALLIB=1 cargo test -p gpui-pre-apple --lib --locked -- metal_renderer::tests text_system::tests::test_kept_fonts text_system::tests::test_adjacent_runs >$L 2>&1 \
    || { grep -hE "^error|^test .* FAILED|panicked at" -A2 $L | head -20; echo "GATE FAIL metal-renderer"; exit 1; }
fi

# --- tests -----------------------------------------------------------------
sum() { grep "test result" | awk '{p+=$4; f+=$6} END {print p, f}'; }
cargo test --no-default-features --locked >$L.core 2>&1; core=$(sum <$L.core)
cargo test --features demo-capture --locked --bin quill >$L.ui 2>&1; ui=$(sum <$L.ui)
set -- $core; cf=$2; set -- $ui; uf=$2
if [ "$cf" = 0 ] && [ "$uf" = 0 ]; then echo "GATE OK core=$core ui=$ui"; else grep -hE "^error|^test .* FAILED" $L.core $L.ui | head -20; grep -hE "panicked at" -A2 $L.core $L.ui | head -20; echo "GATE FAIL tests core=($core) ui=($ui) logs: $L.core $L.ui"; exit 1; fi
