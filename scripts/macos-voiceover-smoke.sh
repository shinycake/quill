#!/usr/bin/env bash
# Read actual VoiceOver output on a fixture when the reader is already enabled.
set -euo pipefail
BIN="${1:-target/debug/quill}"
[[ "$("$BIN" --build-info)" == ui ]]
pgrep -x VoiceOver >/dev/null || { echo 'Enable VoiceOver and finish its setup before this spoken-output check.' >&2; exit 2; }
TMP="$(mktemp -d /tmp/quill-voiceover-check.XXXXXX)"
PID=''
cleanup() {
  [[ -z "$PID" ]] || kill "$PID" 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT
cat > "$TMP/activate.swift" <<'SWIFT'
import Cocoa
NSRunningApplication(processIdentifier:pid_t(CommandLine.arguments[1])!)!.activate(options:[])
RunLoop.current.run(until:Date(timeIntervalSinceNow:0.4))
SWIFT
swiftc "$TMP/activate.swift" -o "$TMP/activate"
QUILL_DEMO_LINGER_MS=60000 "$BIN" --screenshot-demo ready-profile-edit "$TMP/demo" > "$TMP/app.log" 2>&1 &
PID=$!
for _ in {1..100}; do [[ -f "$TMP/demo/.quill-ready-ready-profile-edit" ]] && break; sleep 0.1; done
"$TMP/activate" "$PID"
# VoiceOver rejects these commands when its scripting support is disabled.
osascript -e 'tell application "VoiceOver" to tell vo cursor to move to first item' >/dev/null
FOUND=false
for _ in {1..30}; do
  FRONT="$(osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true')"
  [[ "$FRONT" == "$PID" ]] || { echo 'VoiceOver fixture lost foreground; stopping.' >&2; exit 1; }
  PHRASE="$(osascript -e 'tell application "VoiceOver" to get content of last phrase')"
  case "$PHRASE" in *'Edit profile'*|*'First name'*|*'Last name'*|*'Public username'*|*'Bio'*) FOUND=true ;; esac
  osascript -e 'tell application "VoiceOver" to tell vo cursor to move right' >/dev/null
  sleep 0.2
done
[[ "$FOUND" == true ]]
echo 'PASS: actual VoiceOver phrase includes a named profile control or title'
