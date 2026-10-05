# codex/window-layout — remembered window, resizable chat list, atomic prefs

- **Window geometry persists.** `window_state.json` lives at the app root, since it's
  app-wide rather than per account. It stores position, size, maximized, and chat list width.
  Saves are debounced 600 ms after the last move or resize (`observe_window_bounds`). On
  launch the window reopens there if its title strip still lands on a connected display.
  Otherwise it opens centered at 1200×760, replacing the hardcoded (20, 20) origin. Unusable
  stored geometry (non-finite, below 480×360) is ignored.
- **Resizable chat list.** Its right edge is a 6 px drag zone with a column-resize cursor and a
  hover hairline. The width is clamped to 240–560 px, persisted, and a double-click resets it
  to the 300 px default (was a fixed 280 px).
- **Atomic prefs writes.** Every JSON prefs file (`save_json_prefs`, which covers appearance,
  chat, media and the rest) and the window state are written to a temp file, synced, and
  renamed into place. A crash mid-write used to leave a truncated file that silently reset
  those settings to defaults.
- Tests: atomic write replacement with no temp leftovers; window state sanitizing and a legacy
  file missing the sidebar field.
