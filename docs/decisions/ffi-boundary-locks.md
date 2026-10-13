# FFI boundary and poison-tolerant locks

Branch `ffi-boundary-locks`. Step 2 of the plan in
[`no-crash-audit.md`](no-crash-audit.md) (§3.3, and the mutex part of
§3.6), plus follow-ups from the review of [`crash-log.md`](crash-log.md).
No parity item.

## What tdesktop does

tdesktop is C++, so neither problem exists there in this form. A C++
exception that reaches a C callback is undefined behavior, and tdesktop's
callbacks (for example `Platform::GlobalShortcuts`, the tgcalls sinks) keep
their bodies small and never throw. `std::mutex` has no poisoning: a thread
that dies with a lock held leaves it locked, so tdesktop holds locks only
around plain data copies. Quill keeps the same shape: callbacks copy data
out and return, locks guard plain data, and both now survive a panic.

## What changed

- `src/thread_guard.rs`: `ffi_boundary(default, || body)` runs the body
  under `catch_unwind` and returns `default` on a panic (the panic hook has
  already written the report). `spawn_guarded` and `ffi_boundary` share a
  thread-local guard count, read by `panic_will_be_caught()`.
- Every `extern "C"` / `extern "system"` function Quill implements now
  wraps its body:
  - `calls/engine/native.rs`: the four ntgcalls trampolines
    (`signaling_trampoline`, `connection_trampoline`, `frames_trampoline`,
    `remote_source_trampoline`), default `()`. The two unsafe derefs that
    had no `// SAFETY:` comment got one.
  - `calls/ptt_global_mac.rs` `on_event` (CGEventTap): default is the
    event itself, so the key still reaches the system.
  - `calls/ptt_global_win.rs` `on_key` (`WH_KEYBOARD_LL`): the filter and
    sink run inside the boundary, and `CallNextHookEx` runs after it either
    way. Its `try_lock` uses the new `try_lock_or_recover`.
  - `telegram/ffi.rs` `redacted_native_log` has an empty body; left alone.
  - No other implementations exist (`rg 'extern "(C|system)" fn' src crates`
    finds only these and type aliases / loaded symbols).
- `src/locks.rs`: `lock_or_recover`, `read_or_recover`, `write_or_recover`
  and `try_lock_or_recover` (all `PoisonError::into_inner`), plus the
  method forms `LockExt::lock_or_recover` / `RwLockExt::{read,write}_or_recover`
  for the end of a field chain.
- Replaced every production `lock().expect(..)`, `lock().unwrap()`,
  `lock().ok()`, `lock().map(..)`, `lock().is_ok_and(..)` and ad-hoc
  `unwrap_or_else(|e| e.into_inner())` with the helpers: `platform.rs`,
  `passcode/mod.rs`, `diagnostics.rs`, `connect/sender.rs`,
  `connect/calls/mod.rs`, `connect/group_calls/{mod,join}.rs`,
  `connect/call_audio.rs`, `calls/engine/{native,types}.rs`,
  `calls/engine/ntgcalls/{mod,call_engine}.rs`, the mock engine,
  `calls/level_tap.rs`, `calls/ptt_global_win.rs`, `profile_photo_camera.rs`,
  `link_handler.rs`, `icon_badge.rs`, `local_path.rs`, `ui/wallpaper_cache.rs`,
  `media_session/{mod,linux}.rs`, `spellcheck/mod.rs`, `tray_sni.rs`,
  `deep_link_inbox.rs`, `start_files.rs`, `video_decode/mod.rs`,
  `telegram/hub.rs`, `telegram/name_accent.rs`, `voice.rs`, `spell_win.rs`,
  `paid_messages_guard.rs`, `system_accent.rs`, `export/handle.rs`,
  `export/account/handle.rs`, `ui/notifications/{mod,inline_reply,linux_dbus}.rs`,
  `ui/background_accounts/clicks.rs`, `ui/inline_playback.rs`,
  `ui/media_viewer/video.rs`, `ui/attach_bots.rs`. The per-file `lock`
  helpers that already did this now call the shared one. Test code keeps
  `unwrap`.
- Caught panics (review follow-up a): the crash-log hook asks
  `ReportKind::for_current_thread()`. Inside a guard the report is
  `panic-caught-<ts>.txt` and starts "Quill … panicked (caught; Quill kept
  running)". `pending_crash` reads only `crash-*` files, so a caught panic
  never brings up "Quill closed unexpectedly". Each kind keeps its own
  newest 10, so a burst of caught panics cannot prune a real crash report.
- Start in tray (review follow-up b): `ui/crash_notice.rs` picks the crash
  to report once, in `install`: the newest report from before this run
  (`crash_log::session_start()`, set by `install_panic_hook`, the first
  line of `main`). It shows the notice right away only when the main window
  is active. Otherwise it watches the window's activation
  (`observe_window_activation`) and, the first time the window is active,
  shows that crash and acknowledges exactly it.
- Reports written during the run: a panic on a thread no guard covers (ksni
  or zbus handlers, GPUI workers) still writes `crash-<ts>.txt` while Quill
  keeps running. Such a report is never picked as the notice
  (`pending_crash` ignores reports from `session_start` on), and on a clean
  quit `mark_clean_exit` renames it to `panic-caught-<ts>.txt`, since the
  run survived it. The clean-exit marker then moves to now, unless a crash
  from before this run was never shown (the window stayed in the tray), in
  which case the marker stays and that crash is reported next launch.
- TDLib receive threads (review follow-up c): already covered, no code
  change. `telegram/client.rs` (injected bridge) and `telegram/hub.rs`
  (shared hub) both run the loop under `spawn_guarded` with a `StopGuard`
  whose `Drop` sets `stopped` unless a shutdown was requested; it runs on
  a normal return and while a panic unwinds. Hub-routed bridges share the
  hub's `stopped` flag. `ReceiveBridge::stopped_unexpectedly()` is read by
  the foreground poll loop (`ui/notifications/mod.rs`, "Connection to
  Telegram was closed" with Retry), the background accounts
  (`ui/background_accounts/mod.rs`, status `Failed`) and
  `connect/live.rs` shutdown. Tests: `telegram/hub/tests.rs`
  `a_dead_receive_thread_marks_every_bridge_stopped` and the client test
  around `client.rs:373`.

## Left out

- Most `if let Ok(guard) = m.lock()` / `let Ok(..) = .. else` /
  `match m.lock()` sites never panic: on a poisoned lock they skip their
  work. The ones in files this PR already rewrote were converted to the
  recovering helpers: `local_path.rs` (the verdict, root and directory
  caches), `profile_photo_camera.rs` (the frame slot and the child in
  `Drop`), `system_accent.rs` (the Linux reader thread) and
  `calls/ptt_global_win.rs` (setting and clearing the hook context). The
  rest stay as they are (`advanced/{experimental,update_channel}.rs`, `fast_buttons.rs`,
  `file_prefs.rs`, `notify_prefs.rs`, `web_app/trust.rs`, `media_tools.rs`,
  `video/mod.rs`, `voice.rs`, `voice_input.rs`, `paid_messages_guard.rs`,
  `export/*/handle.rs` progress writers, `sticker_playback.rs`,
  `ui/notifications/{inline_reply,spawn_call_notification}.rs`,
  `ui/background_accounts/{clicks,calls}.rs`, `ui/round_record.rs`,
  `ui/sticker_playback.rs`, `ui/frame_profile.rs`). Converting them changes
  behavior from "skip" to "recover" and is a follow-up per site.
- `crates/webview-host` is a separate binary that does not link `quill`;
  its one `if let Ok` lock is unchanged.
- Follow-up (rule 9): `block2` blocks (AppKit, AVFoundation,
  MediaPlayer and notification completion handlers) and WinRT event
  handler closures (`windows` crate `TypedEventHandler`s for SMTC and media
  playback) are also C-to-Rust entry points. They are not
  `extern "C" fn`s, so this PR does not wrap them; a later PR should put
  their bodies in `ffi_boundary` too.
- No files owned by in-flight parity work (composer send menu, TON wallet
  rendering, channel and bot revenue screens) were touched.

## How it was verified

- Unit tests: `thread_guard` (`ffi_boundary` returns the body's value, a
  panic becomes the default, the guard count is set inside guards and
  drops after a panic), `locks` (a mutex poisoned by a panicking holder
  still locks and keeps its data; `try_lock_or_recover` returns `None` only
  while held; a poisoned `RwLock` still reads and writes), `crash_log`
  (`panic-caught-` naming, caught reports never pending and capped on their
  own, the report kind follows the guards, a clean quit keeps an unseen
  crash from before the run pending, reports from during the run are never
  pending and become caught reports at a clean quit), `crash_notice` (`NoticeGate` opens once, on the first
  activation).
- UI test (`demo-capture`): a deactivated window with a pending crash shows
  no notice and leaves the crash pending; after `activate_window` the
  notice is there and the crash is acknowledged.
- `cargo check --no-default-features` for `x86_64-pc-windows-msvc` and
  `x86_64-unknown-linux-gnu` (the UI build needs native toolchains this
  Mac does not have, so `ptt_global_win.rs`, `spell_win.rs`, `tray_sni.rs`
  and `media_session/linux.rs` are left to CI).
- `quill-tools/gate.sh`.
