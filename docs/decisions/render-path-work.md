# Render-path work: DND probe, display-path verdicts, badge recount, startup sweep

Findings F2, F4, F5, F11 of the performance sweep.

## What tdesktop does
- Do Not Disturb (`notifications_manager_mac.mm`): reads the preference in-process
  (CFPreferences), only when a notification is about to be shown.
- Badge (`Session::computeUnreadBadge`) is recomputed on unread-state change, not per frame.

## What changed
- F2: `notify_focus::dnd_active` is asked only when an alert is pending
  (`needs_probe`); macOS reads `com.apple.notificationcenterui doNotDisturb`
  through `CFPreferencesGetAppBooleanValue` in the worker thread (no `defaults`
  child). Linux keeps gdbus/gsettings (zbus is an optional feature, not always
  linked), now only on demand and already off the UI thread. Windows unchanged.
- F4: `sandboxed_display_path` keeps the verdict of a plain file (final component
  not a symlink) for 60 s instead of 1 s; symlinks and misses are still
  re-checked after 1 s. `Session::upsert_file` (TDLib `updateFile`) calls
  `invalidate_display_path`, so a downloaded or rewritten file is re-checked at once.
  The sandbox still canonicalizes against the allowed roots on every miss. The
  residual window is a directory component swapped for a symlink inside the
  private TDLib files directory within 60 s, which needs write access to that
  directory already.
- F5: `tray::badge_cache::BadgeCache` memoizes `badge_count` for the window title by
  (session, `Session::revision`, chat count, `BadgePrefs`). Local optimistic unread
  edits (mark as read in the list) show in the title on the next server update.
- F11: `sweep_media_caches` renames each cache dir to `<name>.tomb-<pid>-<n>` (one
  rename, gone from its path at once) and deletes the tombstones on a background
  thread, which also removes tombstones left by an interrupted run.

## Verification
Unit tests: `needs_probe`, macOS query, badge cache revision/prefs, verdict
retention and invalidation, symlink TTL, sweep + tombstone removal. gate.sh.
Not measured: before/after `sample` of chat-list autoscroll on the 3000-chat
fixture (no release build was made).
