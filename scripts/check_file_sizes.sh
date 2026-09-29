#!/usr/bin/env bash
# check_file_sizes.sh — no dumping grounds.
# Fails if any .rs file under src/, crates/, or tests/ exceeds the hard cap, or if a waived file grows
# past its recorded line count. Waivers only shrink: each split PR lowers
# (or removes) its entry here.
set -euo pipefail
cd "$(dirname "$0")/.."

HARD_CAP=2000

# "path:lines" — grandfathered violators, recorded 2026-09-29.
# Waiver-bump rule (Idan 2026-09-29): a baseline may grow ONLY for
# type-coherent extensions that cannot live in a named module (new enum
# variants, new match arms on existing matches, new methods on existing
# types); the bump is exact and the PR body says why the code can't live
# elsewhere.
WAIVERS=(
  "src/ui/mod.rs:48748"
  "src/connect.rs:24405"
  "src/state.rs:21311"
  "src/telegram/envelope.rs:18346"
  "src/telegram/requests.rs:10407"
  # Slice A9 (2026-09-29): +54 type-coherent lines — `mod
  # account_lifecycle` decl/re-export (2), `QuillApp.account_lifecycle`
  # field + doc + ctor init (5), `DialogKind::AccountLifecycle` variant (2),
  # `dialog_is_open` / `dialog_builder` arms (2), `KINDS` entry + size (2),
  # sidebar entry (15), `ScreenshotDemo::ReadyAccountLifecycle` variant +
  # dispatch arm + fixture block (26). No new structs/impls/functions here.
  "src/calls/engine.rs:3212"
  "tests/replay.rs:3505"
)

waiver_for() {
  local f="$1" w
  for w in "${WAIVERS[@]}"; do
    [[ "${w%%:*}" == "$f" ]] && { echo "${w##*:}"; return 0; }
  done
  return 1
}

fail=0
while IFS= read -r f; do
  lines=$(wc -l < "$f")
  if cap=$(waiver_for "$f"); then
    if (( lines > cap )); then
      echo "FAIL: $f has $lines lines, over waived $cap (waivers only shrink)"
      fail=1
    fi
  elif (( lines > HARD_CAP )); then
    echo "FAIL: $f has $lines lines, over hard cap $HARD_CAP (split it into named modules)"
    fail=1
  fi
done < <(find src crates tests -name '*.rs' | sort)

(( fail == 0 )) && echo "file sizes OK"
exit "$fail"
