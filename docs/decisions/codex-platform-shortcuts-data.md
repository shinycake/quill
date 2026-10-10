# Platform shortcuts and data settings

Cluster `platform-shortcuts-data`. Reference: tdesktop `core/shortcuts.cpp`,
`settings/sections/settings_advanced.cpp`, `platform/mac/specific_mac.mm`,
`export/view/export_view_settings.cpp`.

## What tdesktop does

- Ctrl/Cmd+L locks the app (`Command::Lock`).
- Advanced settings have "Show tray icon", "Show taskbar icon" and a
  monochrome tray icon switch. A "When the window is closed" choice (run in
  the background, close to taskbar, quit) decides what the close button does.
- On macOS, "Show warning before quitting" makes a keyboard quit require
  holding Cmd+Q (`PreventsQuit`, copied from Chromium's confirm-to-quit).
- "Try connecting through IPv6" sits in the proxy box.
- Exporting a chat offers HTML or JSON, media types, a date range and
  "Only my messages".

## What changed

- **Lock shortcut (`platform-shortcut-lock`).** `LockApp` was already
  registered, but on Ctrl/Cmd+Shift+L because Ctrl/Cmd+L focused the composer.
  Lock now takes Ctrl/Cmd+L like tdesktop, and "Focus composer" moved to the
  Shift variant (still rebindable; a saved custom binding is untouched).
  A test checks the two lock chords and that the composer chord moved.
- **Close behavior (`data-window-close`).** A "When the window is closed"
  choice, Run in the background or Quit Quill, shown whenever a tray icon
  exists. It was macOS only. The title-bar close button and Ctrl/Cmd+W now go
  through `tray::close_outcome` on every OS (pure, unit tested).
- **Tray icon switch (part of `data-tray-toggles`).** "Show tray icon"
  (default on). Turning it off drops the macOS/Windows handle or shuts down the
  Linux StatusNotifierItem; the tray dependent switches hide with it, and the
  switch itself stays visible so it can be turned back on.
- **Quit warning (`data-mac-quit-warning`).** macOS only. `quit_guard` turns
  the key repeat chain of Cmd+Q into "warn on the first press, quit after a
  1.5 s hold", with the toast "Hold ⌘Q to quit". Default on, as in tdesktop.
- **IPv6 (`data-ipv6`).** Already implemented (`setOption("prefer_ipv6")`,
  switch in the proxy box). Only the checklist was stale.
- **Chat export (not claimed).** The export menu item opens a box with HTML or
  JSON, All time / 7 days / 30 days / Last year, and "Only my messages".
  Messages carry the sender name now. Paging stops once it passes the start of
  the range. Media is a label, files are not copied, and there is no calendar
  picker, so `data-chat-export-html` stays open.

## Can GPUI hide a window on Linux and Windows?

Still no, with the vendored gpui-pre 0.3.8. `Platform::hide` is a no-op on
Linux and Windows, and `PlatformWindow` has `minimize`, `zoom`, `activate`
but no hide or unmap. The small clean option is to minimize the window
instead of hiding it:

- Windows: `activate_window` restores a minimized window (`IsIconic` then
  `SW_RESTORE`), so the tray's "Open Quill" works. The taskbar button stays.
- X11: `activate_window` sends `_NET_ACTIVE_WINDOW`, which window managers use
  to un-minimize.
- Wayland: `set_minimized` works, but compositors may refuse the activation
  request that restores it, so the tray can fail to bring the window back.
  This is the weak spot; the setting is still offered because the tray icon
  is required and the window stays in the task switcher.

A real hide needs a GPUI patch (`ShowWindow(SW_HIDE)` on Windows, XUnmapWindow
on X11). Not done here.

## Skipped, and why

- `platform-shortcut-commands`: scheduled messages, silent send, link preview
  toggle, round video, archive chat, admin log and reopen closed window each
  need their own action and context; too many for this PR.
- `platform-new-window`: a second chat window needs the session and
  navigation state split per window.
- `platform-sendto-menu`: Windows shell extension, native code and an
  installer change.
- `data-native-frame`: the window frame is chosen when the window is created
  and kit's title bar draws the chrome; needs a restart flow and testing on
  both OSes I cannot run.
- `data-tray-toggles` taskbar icon and monochrome icon: GPUI has no
  skip-taskbar call, and the tray art is not recolored. Not claimed.
- `data-proxy-extras`: auto switch, shield and share link exist; share QR, use
  system proxy and the connection type row do not. System proxy needs a
  per-OS lookup.
- `data-export-full`, `platform-data-export`: the full account export needs
  per-type options, media sizes, JSON and HTML, and resumable media download.
  Existing account export is unchanged.

## Verification

- Gate: fmt, clippy (core and ui, `-D warnings`), core and ui tests.
- Unit tests: `close_outcome`, `tray_setting_switches`, `QuitGuard` hold
  timing, export range, selection, HTML escaping and grouping, file name and
  extension, lock keybinding.
- Demo captures (English fixtures) looked at: `ready-chat-export` and
  `ready-window-settings`.
- Not verified: a live Linux or Windows session (close to minimize and tray
  restore), a real Cmd+Q hold on macOS, and a real export through TDLib.
