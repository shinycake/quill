A Dock Quit or a log out now asks before ending a running export, and quitting after Stop leaves nothing in the cache

## What was wrong

- L18: a Dock Quit and a log out reach AppKit's `applicationShouldTerminate:`, which GPUI left at its default, so the app ended at once and the export died with no question. Only Quill's own Quit showed the "stop the export?" box. Telegram Desktop refuses here. Qt turns the request into `Quit(QuitReason::QtQuitEvent)` (`sandbox.cpp:367`), `preventsQuit` asks, and the termination is cancelled (`application.cpp:1052-1062,1169,1210`).
- L20: quitting through Stop could leave one file in TDLib's cache. The worker posts its `cancelDownloadFile` and `deleteFile` on a channel the next poll pass reads, and there was no next pass.
- L19: with the Stop box focused, a second quit did nothing. The focus path of the box does not pass QuillApp's own `QuitApp` handler, so in a demo the key was dead. In the real app the app-wide handler from #1299 saw it and dropped it, so Stop ran the first request. tdesktop's `stopWithConfirmation` replaces the box and its callback.
- N17: a file the export finished before the open chat asked for it was deleted, then fetched again. Views filter what they ask for through `should_download`, and the export's own transfer makes that false: active with no record of ours, or complete. So `note_view_file` never ran and the delete went through.
- N18 (owner default, Copy): an exported file could be a hard link to a cache file TDLib kept, so editing the export edited the cache.
- Found on the way: a file that failed to save kept its cache copy, because the error returned ahead of the release. This hit `save_once` and the account sections. `note_view_file` also skipped Save as HTML exports.

## What changed

- L18: new `Platform::on_quit_request` and `App::on_quit_request`. macOS answers `applicationShouldTerminate:` with `NSTerminateCancel` when the app says no. `Platform::quit` sets `quit_decided`, so Quill's own quit, restart included, is not asked again. Windows answers `WM_QUERYENDSESSION` with FALSE when the app says no and asks nothing for `ENDSESSION_CRITICAL`, since that session ends whatever it answers. `ZoomPlatform` forwards the method. The answer comes from `exit_held_by_export`, the hold #1299 added, so there is one path. All of it is logged in the three `QUILL-CHANGES.md` files.
- L18, a log out stays cancelled after Stop: the app quits and the user logs out again. tdesktop does the same.
- L18, Windows differs from tdesktop: tdesktop does not hold the end of a session there (Qt `commitData` only, and `integration_win.cpp` has no `WM_ENDSESSION`). Quill asks, as the plan for this bundle says.
- L18, Linux has no change: neither Quill nor GPUI has a session-end path there. The only logind signal in `gpui-pre-linux` is `PrepareForSleep`, and tdesktop's `saveStateRequest` only sets `RestartNever`. The trait method defaults to doing nothing.
- L18, forwarded `--quit` was already held: `take_quit_request` becomes `TrayAction::Quit`, then `exit_after_exports`.
- L20: `close_all_for_quit` stops every export and waits up to `EXPORT_STOP_HOLD` (400 ms, inside the shared 1500 ms quit deadline) for the workers to end. `begin_close` then sends what is queued ahead of `close`. The wait costs nothing when no export runs. A worker in the middle of copying a large file past the hold is left behind.
- L19: the box keeps the exit it will run, and a new request replaces it. The box stays up, as tdesktop's does, so the key shows no change on screen. Stop then runs the latest request.
- N17: while a worker holds a file no view has spoken for, `should_download` says yes. The ask reaches `note_view_file`, and from then on the normal answer holds, so TDLib gets no second request. A row that loads only after the delete still fetches once, the same as with no export.
- N18: `place_into` always uses `fs::copy`, which clones on APFS and costs no space there.
- Failed saves: the release runs before the save's error returns. `note_view_file` goes through `SettingsState::export_links_mut`, which covers Save as HTML.

## Measurements

None needed. These are logic fixes with no speed or memory claim.

## Tests

`cargo test --features demo-capture --locked`:

- `--lib -- export:: closing chat_state::export rich_html`: 202 passed.
- `--bin quill -- export_quit link_inbox background_accounts::closing`: 26 passed.

Each test below failed with its fix taken out. `an_os_quit_goes_ahead_when_nothing_holds_it` is the control and passes either way.

| Test | With the fix out |
| --- | --- |
| `an_os_quit_during_an_export_is_refused_and_stop_ends_the_app` | the quit is allowed |
| `what_a_stopped_export_left_queued_goes_out_ahead_of_close` | sent `["setOption", "close"]` only |
| `closing_for_quit_stops_every_export` | the links stay uncancelled |
| `a_quit_waits_for_a_stopped_export_to_end` | the worker is still running when the wait returns |
| `cmd_q_with_the_box_focused_becomes_what_stop_does` | Stop runs the first request |
| `an_export_leaves_a_file_the_open_chat_rides_along_on` | `deletes: 1` |
| `a_file_left_in_the_cache_after_all_stays_apart_from_the_export` | the cache file reads back `edited` |
| `a_file_that_could_not_be_saved_still_leaves_the_cache` | `deleteFile` count 0 |

On this Mac I ran a `--screenshot-demo` build (fastrel profile) with a local hook, left out of this PR, that turns on the app shell and starts an export no driver answers. The quit event is `NSRunningApplication.terminate()`.

- Nothing exporting, quit event: exits with code 0.
- Export running, the new answer switched off by the hook: exits with no box, which is main's behaviour.
- Export running, quit event: the app stays and shows the box. A second event leaves one box. Cmd-Q with the box up leaves it up. Return on Stop exits with code 0, so `quit_decided` lets the app's own quit through.
- Export running, Dock menu Quit pressed through Accessibility: the app stays and shows the box. Esc closes the box and the app runs on. A quit event brings the box back, and Stop exits with code 0.
- I looked at captures of the box in light and dark.

`cargo check -p gpui-pre-windows --target x86_64-pc-windows-msvc` is clean. `quick-gate.sh` printed QUICK OK: `check-lang.py`, `check-hotspots.sh`, `check-file-size.sh`, `check-window-scope.py`, `check-app-name.sh`, `cargo fmt`, and clippy with `-D warnings` for `--features demo-capture --all-targets`.

## Could not verify

- Whether real TDLib finishes a `deleteFile` sent just ahead of `close`. There is no real account here. The test checks only the order on the wire.
- An actual OS log out. `NSRunningApplication.terminate()` sends the same quit Apple event, and that is what ran.
- Windows and Linux runs. The Windows code compiles with `cargo check -p gpui-pre-windows --target x86_64-pc-windows-msvc`. It has not run.
