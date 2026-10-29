# Native macOS shortcuts come back

## The problem

Cmd+H did not hide Quill. Two things were missing:

- GPUI builds the macOS menu bar from data and gives AppKit's standard app
  menu commands no default. Quill's app menu listed only Settings, Lock and
  Quit, so there was no Hide Quill, Hide Others, Show All or Services, and
  nothing answered Cmd+H or Cmd+Opt+H.
- GPUI passes a key to AppKit's text system only when no binding matches,
  and then drops the selector AppKit sends back. So a text field supports
  exactly the Cocoa text keys the keymap names.

Quill also bound every app shortcut twice, `cmd-x` and `ctrl-x`, on every
platform. On macOS the Ctrl copies took the Cocoa text keys: Ctrl+K opened
quick switch instead of deleting to the end of the line, Ctrl+F opened
chat search instead of moving the caret, and Ctrl+L, Ctrl+M, Ctrl+Q and
Ctrl+W locked, minimized, quit and closed. On Linux and Windows the Cmd
copies became Super chords, which belong to the desktop (Super+L locks
the session on GNOME, Super+M and Super+W are window-manager keys on
several desktops).

## What tdesktop does

`core/shortcuts.cpp` `fillDefaults` writes each shortcut once with Qt's
`ctrl`, which Qt maps to Cmd on macOS, so there is one chord per platform
and macOS keeps a real Ctrl for text editing. The folder chords are the
exception, written with `meta` to mean a real Ctrl on macOS.
`platform/mac/global_menu_mac.mm` builds the standard app menu: Services,
Hide Telegram (`lng_mac_menu_hide_telegram`), Hide Others and Show All.
Native Qt text fields keep AppKit's Ctrl text keys.

## What changed in Quill

- Every app shortcut uses `primary!`: Cmd on macOS, Ctrl elsewhere. That
  covers Quit, Settings, Close, Lock, Minimize, Focus chat list, Focus
  composer, Quick switch, Find in chat and the search result chords. The
  rebindable defaults and the shortcuts reference list follow.
- Full screen is Cmd+Ctrl+F on macOS and F11 elsewhere. macOS keeps F11
  for Show Desktop, and Super+Ctrl+F is no longer bound on Linux/Windows.
- The macOS app menu now reads Settings, Lock Quill, Services, Hide Quill
  (Cmd+H), Hide Others (Cmd+Opt+H), Show All, Quit Quill, with tdesktop's
  strings. Global handlers call `cx.hide()`, `cx.hide_other_apps()` and
  `cx.unhide_other_apps()`, so the commands work from any Quill window.
  Cmd+H and Cmd+Opt+H are fixed chrome: a custom shortcut cannot take
  them, and an open context menu lets Cmd+H through like Cmd+Q/W/M.
- The vendored gpui-base input binds the rest of Cocoa's standard Ctrl
  text keys on macOS, next to its existing Ctrl+A / Ctrl+E: Ctrl+F / B /
  N / P move the caret, Ctrl+D deletes forward, Ctrl+H deletes back,
  Ctrl+K deletes to the end of the line.

Saved custom shortcuts are untouched. A custom chord that used the other
platform's modifier still installs; only the defaults changed.

## Audit

macOS system and standard app chords, and what Quill does now:

| Chord | Standard meaning | Quill |
| --- | --- | --- |
| Cmd+H / Cmd+Opt+H | Hide / Hide Others | restored (was unbound) |
| Cmd+Q, Cmd+W, Cmd+M, Cmd+, | Quit, Close, Minimize, Settings | same |
| Cmd+Ctrl+F | Full screen | same |
| Cmd+` | Next window | not bound; AppKit handles it |
| Cmd+Tab, Cmd+Space, Cmd+Shift+3/4/5, Ctrl+arrows | system | never reach the app |
| Cmd+C/V/X/A/Z, Cmd+Shift+Z | edit | kit input and Edit menu |
| Cmd+arrows, Opt+arrows, Cmd/Opt+Delete | caret and delete | kit input |
| Ctrl+A/E/F/B/N/P/D/H/K | Cocoa text keys | kit input (Ctrl+F/K/L/M/Q/W were app shortcuts) |
| Cmd+Ctrl+Space | Character viewer | kit input |
| F11 | Show Desktop | no longer bound on macOS |

Kept on purpose, as in tdesktop: Opt+Up/Down switch chats, Cmd+Up/Down in
the composer reply to the previous/next message (the composer lets the key
through to the field when it has text), Ctrl+Tab / Ctrl+Shift+Tab switch
chats, Ctrl+Shift+Up/Down switch folders, and Ctrl+Space selects the
focused message (macOS takes it first when it is the input-source switch).

Linux and Windows: no Super chord is bound any more. The remaining Ctrl
and Alt chords do not collide with the common desktop defaults (Alt+Tab,
Alt+F4, Ctrl+Alt+T, Ctrl+Alt+L, Ctrl+Alt+arrows, Ctrl+Shift+Esc).

## Tests

`src/ui/keybindings/mod.rs`:
`shortcuts_leave_the_other_platforms_modifier_alone` checks every default,
fixed, pack and composer chord: no Ctrl+letter without Cmd on macOS, no
Super chord elsewhere. `cmd_h_hides_and_cmd_alt_h_hides_others` (macOS)
resolves Cmd+H and Cmd+Opt+H, including from inside the composer. The
fixed-chrome and conflict tests now use the platform's own chord.
