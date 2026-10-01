#!/usr/bin/env bash
# Native menu/lifecycle check. Requires System Events accessibility permission.
set -euo pipefail
if pgrep -x quill >/dev/null; then
  echo 'Close the existing Quill process before running the tray smoke check.' >&2
  exit 1
fi
tray_smoke_log="$(mktemp)"
QUILL_TDJSON_PATH=/nonexistent/quill-tray-smoke-tdjson "${1:-target/debug/quill}" "${@:2}" >"$tray_smoke_log" 2>&1 &
tray_smoke_pid=$!
trap 'kill "$tray_smoke_pid" 2>/dev/null || true; rm -f "$tray_smoke_log"' EXIT
osascript - "$tray_smoke_pid" "${2:-}" <<'APPLESCRIPT'
on run argv
  set appPid to (item 1 of argv) as integer
  delay 3
  tell application "System Events"
    set appProcess to first process whose unix id is appPid
    tell appProcess
      if (item 2 of argv) is "--start-minimized" then
        if (count of windows) is not 0 then error "Start in tray displayed a window"
      end if
      click menu bar item 1 of menu bar 2
      click menu item "Open Quill" of menu 1 of menu bar item 1 of menu bar 2
      delay 2
      if not frontmost then error "Open Quill did not activate the app"
      keystroke "w" using command down
      delay 1
      if visible then error "Cmd-W did not hide to the tray"
      click menu bar item 1 of menu bar 2
      click menu item "Open Quill" of menu 1 of menu bar item 1 of menu bar 2
      delay 2
      if not visible then error "Open Quill did not restore the app"
      if (count of windows) is not 1 then error "Reopening did not retain the window"
      click (first button of window 1 whose subrole is "AXCloseButton")
      delay 1
      if visible then error "Native close did not hide to the tray"
      click menu bar item 1 of menu bar 2
      click menu item "Quit Quill" of menu 1 of menu bar item 1 of menu bar 2
    end tell
    delay 2
    if exists (first process whose unix id is appPid) then error "Quit did not terminate Quill"
  end tell
  return "PASS: native tray open, Cmd-W hide, reopen retained window, native close, quit"
end run
APPLESCRIPT
