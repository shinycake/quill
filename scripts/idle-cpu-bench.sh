#!/bin/bash
# Idle CPU of a Quill build on screenshot-demo fixtures (macOS; uses `top`).
# Background: docs/decisions/codex-idle-cpu.md.
#
#   scripts/idle-cpu-bench.sh <quill binary> [settle seconds, default 20]
#
# Each scenario launches the fixture, waits for its ready marker plus the
# settle time, then prints the mean %CPU of 30 one-second `top` samples
# (the first sample, cumulative since launch, is dropped). A window
# launched from a terminal never comes to the front, so "active" runs set
# QUILL_ASSUME_ACTIVE=1; "inactive" runs hand key status to a second
# window (QUILL_DEMO_DEACTIVATE=1). QUILL_DEMO_UPDATE_STREAM=10 feeds ten
# synthetic TDLib updates a second (src/ui/demo_stream.rs). The window must
# stay at least partly uncovered (macOS stops frames for an occluded window);
# QUILL_DEMO_WINDOW_ORIGIN=x,y moves it. Run nothing else heavy meanwhile,
# and compare builds in alternating runs.
set -u
BIN=$1
SETTLE=${2:-20}

measure() {
  local label=$1 kind=$2
  shift 2
  local out pid cpu
  out=$(mktemp -d)
  env "$@" QUILL_DEMO_LINGER_MS=$(((SETTLE + 45) * 1000)) \
    "$BIN" --screenshot-demo "$kind" "$out" >"$out/log.txt" 2>&1 &
  pid=$!
  for _ in $(seq 1 120); do
    [ -e "$out/.quill-ready-$kind" ] && break
    sleep 0.5
  done
  sleep "$SETTLE"
  cpu=$(top -l 31 -s 1 -stats pid,cpu -pid "$pid" |
    awk -v p="$pid" '$1 == p { n++; if (n > 1) { s += $2; c++ } } END { printf "%.2f", (c ? s / c : 0) }')
  printf '%-22s %6s%%\n' "$label" "$cpu"
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  rm -rf "$out"
}

measure active-idle ready-chats QUILL_ASSUME_ACTIVE=1
measure active-stream10 ready-chats QUILL_ASSUME_ACTIVE=1 QUILL_DEMO_UPDATE_STREAM=10
measure active-stream10-big ready-chats QUILL_ASSUME_ACTIVE=1 QUILL_DEMO_UPDATE_STREAM=10 \
  QUILL_DEMO_STRESS=60,400 QUILL_DEMO_STRESS_REDRAW=0
measure inactive-idle ready-chats QUILL_DEMO_DEACTIVATE=1
measure inactive-stream10 ready-chats QUILL_DEMO_DEACTIVATE=1 QUILL_DEMO_UPDATE_STREAM=10
measure active-typing ready-typing QUILL_ASSUME_ACTIVE=1
measure active-stickers ready-sticker-playback QUILL_ASSUME_ACTIVE=1
measure active-gif ready-gif-playback QUILL_ASSUME_ACTIVE=1
