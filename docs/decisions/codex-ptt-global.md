# System-wide push-to-talk

## What tdesktop does
- `calls/group/calls_group_settings.cpp` + `base::GlobalShortcuts`: the push-to-talk shortcut works while the app is in the background.
- macOS: before enabling the shortcut it checks `base::GlobalShortcutsAllowed()`. Without access it shows a box (`lng_group_call_mac_access` + `lng_group_call_mac_input`: "allow Input Monitoring ... in Privacy Settings") with an "Open Settings" button that opens the Input Monitoring pane. It never prompts silently.

## What changed
- `src/calls/ptt_global.rs` (pure, tested): key tables from GPUI key names to macOS virtual key codes, Windows VK codes and X11 keysyms; `KeyFilter` (collapses OS auto-repeat into Down/Up edges); `Controller` (owns at most one hook: starts once, restarts on key change, stops on `None`, does not retry a failed start until asked); `linux_session` (Wayland vs X11 from env); `status_note` for Settings.
- Backends only observe keys, never swallow them, never log them:
  - macOS `ptt_global_mac.rs`: listen-only `CGEventTap` on its own run-loop thread, raw CoreGraphics/CoreFoundation FFI (no new crate). `CGPreflightListenEventAccess` gates it, so Quill never triggers the system prompt; without access Settings shows the tdesktop-style text with "Open Settings" (opens `Privacy_ListenEvent`) and "Check again", and a 2 s poll re-checks while a call needs it.
  - Windows `ptt_global_win.rs`: `WH_KEYBOARD_LL` hook (windows-sys, already a dependency; two extra features) on a thread with a message loop; always calls the next hook.
  - Linux X11 `ptt_global_x11.rs`: X11 RECORD extension through `x11rb` (same 0.13.2 GPUI already links; only the `record` feature is new, no new lockfile package, MIT/Apache-2.0). Keycodes are resolved from the live keymap.
  - Wayland (and no-display sessions): not available, falls back to in-window push-to-talk with a note in Settings. XRecord under XWayland would only see X11 windows, so it is deliberately not used there. The xdg-desktop-portal GlobalShortcuts portal was not attempted: it binds compositor-chosen triggers rather than an arbitrary held key and cannot be exercised without a Wayland desktop.
- Lifecycle (`src/ui/group_call_extras.rs`): the hook runs only while a live group call is joined with push-to-talk on (`sync_global_ptt`, called from the existing window sync and when call prefs change) and is dropped when the call ends or the option goes off. Hook events reach the existing `PushToTalk` state machine through the same press/release path the window uses. Window deactivation no longer releases the mic while the hook is active (the key-up still arrives in the background). Demo sessions never start a hook.
- Settings: the old "System-wide shortcuts are not available yet" line is now a status line from the hook state.

## Limits
- Key matching is by GPUI key name. macOS uses ANSI positions, so letters on a non-US layout may map to a different physical key; Windows and X11 follow the layout.
- Modifier-only keys can't be bound (the key capture never produces them).

## Verified
Unit tests for key tables, edge filter, controller lifecycle, session detection and notes; a macOS test that `start` never prompts and agrees with the permission state. `cargo check`/clippy of the lib for `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`; gate OK. Not verified: real key events on any OS (macOS needs the Input Monitoring grant, and no Windows or X11 machine was available), the Settings guidance rendering. For that reason the parity item `parity:calls-push-to-talk` is not declared in a fragment yet.
