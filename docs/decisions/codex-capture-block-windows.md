# Secret chats out of screen capture on Windows; plain words on Linux; Windows build fixes

**Capture blocking** (follows #412, macOS):
- **Windows:** `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`
  while a secret chat (or a secret chat's media in the viewer) is
  showing, and `WDA_NONE` otherwise. Screenshots, recordings and screen
  sharing leave the window out. Windows 10 before 2004 ignores the flag.
  The HWND comes from GPUI's raw window handle, as the macOS path gets
  its NSWindow.
- **Linux:** X11 and Wayland give apps no way to hide a window from
  capture, so nothing is blocked. A secret chat shows a quiet,
  dismissible line above the history: "Screenshots can't be blocked
  here. Linux desktops don't let apps hide a window from screen capture,
  so anything in this secret chat can be captured on this computer."
  It shows once per session.

**Windows build fixes**, found by type-checking the core library for
`x86_64-pc-windows-msvc` (`cargo check --target x86_64-pc-windows-msvc
--no-default-features`; GPUI's own build script needs the Windows SDK,
so the UI can't be cross-checked from a Mac):
- `settings::local_minutes_since_midnight` called `libc::localtime_r`,
  which doesn't exist on Windows. The core didn't compile for Windows at
  all. It now uses `local_time`.
- `local_time` had no Windows UTC offset (it returned 0), so every
  timestamp would have shown in UTC. It now converts the instant through
  `SystemTimeToTzSpecificLocalTime` (current zone rules, DST included).
- Unix-only helpers no longer warn on Windows. The self-update handoff
  is Unix-only, which is noted as debt.

`windows-sys` 0.61 (already in the lockfile) is the only new Windows
dependency. The new Windows calls were checked in an isolated crate
against the same `raw-window-handle` and `windows-sys` versions. Gate:
GATE OK.
