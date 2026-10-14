#!/usr/bin/env bash
# Read actual VoiceOver output across Quill's main flows. VoiceOver must be
# on, with "Allow VoiceOver to be controlled with AppleScript" turned on in
# VoiceOver Utility > General. Each flow opens a demo fixture, walks the
# VoiceOver cursor through it (into groups and out again) and passes when
# VoiceOver spoke at least one of the flow's named controls. Everything
# spoken is kept in voiceover-phrases.txt for review.
#   scripts/macos-voiceover-smoke.sh [binary] [flow ...]
set -euo pipefail
BIN="${1:-target/debug/quill}"
shift || true
[[ "$("$BIN" --build-info)" == ui ]]
pgrep -x VoiceOver >/dev/null || { echo 'Enable VoiceOver and finish its setup before this spoken-output check.' >&2; exit 2; }
osascript -e 'tell application "VoiceOver" to get content of last phrase' >/dev/null 2>&1 \
  || { echo 'Turn on "Allow VoiceOver to be controlled with AppleScript" in VoiceOver Utility > General.' >&2; exit 2; }

# flow|phrases VoiceOver should speak (any one passes, all are reported)
FLOWS=(
  'ready-chats-composer|Search|Main menu|Chats|Demo chat A|Message history|Chat actions|Attach|Message'
  'ready-media|Open photo|Jump to bottom|Message history'
  'ready-privacy|Phone Number|Last Seen|Close'
  'ready-group-info-edit|Approve new members|Group name'
  'ready-profile-edit|Edit profile|First name|Last name|Public username|Bio'
  'ready-settings-advanced|Advanced|Experimental settings'
)
WANT=("$@")
WANT+=("")
TMP="$(mktemp -d /tmp/quill-voiceover-check.XXXXXX)"
OUT="${QUILL_VOICEOVER_LOG:-voiceover-phrases.txt}"
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
vo() { osascript -e "tell application \"VoiceOver\" to $1" 2>/dev/null || true; }
: > "$OUT"
FAILED=("")
for entry in "${FLOWS[@]}"; do
  IFS='|' read -r -a parts <<< "$entry"
  flow="${parts[0]}"
  if [[ ${#WANT[@]} -gt 1 && " ${WANT[*]} " != *" $flow "* ]]; then continue; fi
  phrases=("${parts[@]:1}")
  QUILL_DEMO_LINGER_MS=90000 "$BIN" --screenshot-demo "$flow" "$TMP/$flow" > "$TMP/$flow.log" 2>&1 &
  PID=$!
  for _ in {1..150}; do [[ -f "$TMP/$flow/.quill-ready-$flow" ]] && break; sleep 0.1; done
  "$TMP/activate" "$PID"
  vo 'tell vo cursor to move to first item'
  heard=()
  last=''
  for _ in {1..80}; do
    FRONT="$(osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true')"
    [[ "$FRONT" == "$PID" ]] || { echo "$flow: the fixture lost the foreground; stopping." >&2; exit 1; }
    PHRASE="$(vo 'get content of last phrase')"
    if [[ "$PHRASE" == "$last" ]]; then
      # The end of a container: step out and on.
      vo 'tell vo cursor to move out of item'
      vo 'tell vo cursor to move right'
    else
      heard+=("$PHRASE")
      printf '%s\t%s\n' "$flow" "$PHRASE" >> "$OUT"
      case "$PHRASE" in
        *group*|*list*|*'scroll area'*|*table*) vo 'tell vo cursor to move into item' ;;
        *) vo 'tell vo cursor to move right' ;;
      esac
    fi
    last="$PHRASE"
    sleep 0.25
  done
  found=("")
  for phrase in "${phrases[@]}"; do
    for said in ${heard[@]+"${heard[@]}"}; do
      if [[ "$said" == *"$phrase"* ]]; then found+=("$phrase"); break; fi
    done
  done
  if [[ ${#found[@]} -gt 1 ]]; then
    echo "PASS: $flow: VoiceOver spoke${found[*]} ($((${#found[@]} - 1))/${#phrases[@]})"
  else
    echo "FAIL: $flow: VoiceOver spoke none of: ${phrases[*]}" >&2
    FAILED+=("$flow")
  fi
  kill "$PID" 2>/dev/null || true
  wait "$PID" 2>/dev/null || true
  PID=''
done
echo "Every phrase spoken is in $OUT"
[[ ${#FAILED[@]} -eq 1 ]]
