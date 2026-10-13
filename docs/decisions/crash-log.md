# Crash log and guarded threads

Branch `crash-log`. Step 1 of the plan in
[`no-crash-audit.md`](no-crash-audit.md) (§3.1, §6.1). No parity item.

## What tdesktop does

`Core::CrashReports` (`core/crash_reports.cpp`) writes a `tdata/working`
file at startup and removes it in `Finish()` on a normal exit. If the file is
still there on the next launch, together with a dump, tdesktop opens its
crash report window ("Last time Telegram Desktop crashed",
`core/crash_report_window.cpp`) and offers to send the report.

## What changed

- `src/crash_log.rs`: `install_panic_hook()` puts a hook in front of the
  existing one. On a panic it writes `<data root>/logs/crash-<unix-ts>.txt`
  with the panic message, file and line, thread name, `version::APP`, OS and
  architecture, and `Backtrace::force_capture()`, then calls the previous hook
  (the usual stderr message). Two panics in the same second get
  `crash-<ts>-<n>.txt`. Only the newest 10 reports are kept. The data root is
  resolved at panic time through `settings::safe_app_root`, so screenshot
  demos write into their throwaway root. Every IO error is ignored: a hook
  that panics aborts the process.
- `main` installs the hook as its first line, before any CLI or UI path.
- Clean exit and the notice (`src/ui/crash_notice.rs`): `cx.on_app_quit`
  writes `logs/clean-exit` (unix seconds). After the main window is built,
  `window.defer` checks for a crash report newer than that marker and pushes
  a kit notification, "Quill closed unexpectedly last time", with a "Show
  log" button that selects the report in the file manager
  (`platform::reveal_in_file_manager`). Pushing the notice moves the marker
  to the crash's time, so it shows once. It runs deferred because the kit's
  notification layer is read by the window root, and pushing from inside the
  root's own update would hit GPUI's re-entrancy panic (rule 7).
- `src/thread_guard.rs`: `spawn_guarded(name, f) -> io::Result<JoinHandle<Option<T>>>`
  names the thread and runs `f` under `catch_unwind`. The hook has already
  written the report; `join` gives `None` for a panicked thread. Spawning
  failures now come back as an `io::Error` instead of the panic
  `std::thread::spawn` raises.
- Migrated sites (§6.1): `single_instance` (3), `video_decode`,
  `profile_photo_camera`, `spell_dict`, `voice`, `calls/ptt_global_mac`,
  `calls/ptt_global_x11`, `calls/ptt_global_win`, `calls/level_tap`,
  `video`, `media_session/linux`, `media_session/windows_smtc`,
  `ui/notifications/spawn_call_notification` (3), `notify_focus`,
  `ui/notifications/linux_dbus`, `tray_sni`, `system_accent`, `spell_win`,
  `ui/wallpaper_cache`, `export/account/handle`, `export/handle`,
  `icon_badge` (the production one), `passcode`, `telegram/client`,
  `telegram/hub`, `ui/web_app_ui/spawn_mini_app_window`,
  `ui/spellcheck_mac`. Where a site used `std::thread::spawn` the spawn
  failure is now handled: the voice recorder returns an error and removes
  its file, the profile camera stops ffmpeg and returns an error, the
  exports return the `io::Error`, the round-video preview and the
  single-instance forwarder carry on without the thread.
- `startup_theme_mode` moved from `main.rs` to `ui/chat_theme.rs` so
  `main.rs` stays under its size baseline (the baseline went down).

## Left out

- `crates/webview-host/src/main.rs:356`: a separate helper binary that does
  not link the `quill` library. It needs its own hook; later step.
- `src/icon_badge.rs:381` and `src/single_instance.rs:320` are in tests.
- Owners that wait on a flag the thread clears (`notify_focus` `REFRESHING`,
  `system_accent` `running`, the export stages, the video decoder's `eof`)
  still see a panicked thread as "busy". The thread no longer takes anything
  else down and the panic is logged; reporting the death to each owner is a
  follow-up per site.
- No files owned by the parity workers (composer send menu, TON wallet
  rendering, revenue screens) were touched; none of the §6.1 sites are in
  them.
- Privacy: the report holds no chat data that Quill puts there, but a
  standard library panic message can quote a value. The slice-boundary panic,
  for example, quotes up to 256 characters of the string it was slicing,
  which can be message text. The file stays on this computer and is never
  sent anywhere.
- No upload or "send report" flow (tdesktop has one). Quill keeps the file
  local.

## How it was verified

- Unit tests: `crash_log` (name parsing, the 10-file cap and same-second
  serials in a temp dir, pending/acknowledge/clean-exit logic, report
  fields), `thread_guard` (value and thread name come back; a panic becomes
  `None`).
- UI test (`demo-capture`): a temp root with a crash file shows the notice
  with its "Show log" button in a real kit `Root`, and the crash is no longer
  pending afterwards.
- `cargo check --no-default-features` for `x86_64-pc-windows-msvc` and
  `x86_64-unknown-linux-gnu` (the UI build needs native toolchains this Mac
  does not have; the UI-only Windows and Linux sites are the same mechanical
  swap and are left to CI).
- `quill-tools/gate.sh`.
