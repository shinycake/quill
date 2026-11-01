A Dock Quit, a log out or a shutdown now asks before ending a running export, and an export no longer leaves files in the cache or fetches them twice

## What was wrong

- L18: a Dock Quit and a log out reach AppKit's `applicationShouldTerminate:`, which GPUI left at its default, so the app ended at once and the export died with no question. Only Quill's own Quit showed the "stop the export?" box. Telegram Desktop refuses here. Qt turns the request into `Quit(QuitReason::QtQuitEvent)` (`sandbox.cpp:367`), `preventsQuit` asks, and the termination is cancelled (`application.cpp:1052-1062,1169,1210`).
- L18, a request that could not ask: the first version of this PR let such a quit through. `App::on_quit_request` allowed the quit when the app was already borrowed, which happens on Windows because broadcasts such as `WM_QUERYENDSESSION` arrive mid-update. The macOS delegate allowed it when the callback was out of its slot on re-entry, and `os_menus` allowed it when the window or the app entity could not be updated. Each of these ended a running export with no box.
- L20: quitting through Stop could leave files in TDLib's cache. The worker posts its `cancelDownloadFile` and `deleteFile` on a channel the next poll pass reads, and there was no next pass. A worker in the middle of copying a large file never saw the stop at all: `fs::copy` ran to the end, so the delete came after `close`, or never. If the app exited during that copy, the export folder kept a truncated file under its final name.
- L19: with the Stop box focused, a second quit did nothing. The focus path of the box does not pass QuillApp's own `QuitApp` handler, so in a demo the key was dead. In the real app the app-wide handler from #1299 saw it and dropped it, so Stop ran the first request. tdesktop's `stopWithConfirmation` replaces the box and its callback.
- N17: a file the export downloaded for itself was deleted from the cache as soon as it was copied. A row of the open chat that loaded afterwards, for example while the user scrolled up during the export, found the cache empty and downloaded the file again.
- N18 (owner default, Copy): an exported file could be a hard link to a cache file TDLib kept, so editing the export edited the cache.
- Found on the way: a file that failed to save kept its cache copy, because the error returned ahead of the release. This hit `save_once` and the account sections. `note_view_file` also skipped Save as HTML exports.

## What changed

- L18: new `Platform::on_quit_request` and `App::on_quit_request`. The callback answers a `QuitAnswer`: `Allow`, `Refuse`, or `Later` when the app is busy further up the stack. `App::on_quit_request` answers `Later` whenever it cannot borrow the app, so no quit goes through unasked.
  - macOS: `applicationShouldTerminate:` answers `NSTerminateCancel` on `Refuse`. On `Later` it answers `NSTerminateLater`, asks again from the main queue, and calls `replyToApplicationShouldTerminate:` with the answer. `Platform::quit` sets `quit_decided`, so Quill's own quit, restart included, is not asked again.
  - Windows: `WM_QUERYENDSESSION` is answered with FALSE on `Refuse`. On `Later` it is answered with FALSE and a new `WM_GPUI_QUIT_REQUEST_LATER` message asks from the message loop. If the app then allows the quit, it quits, so a shutdown that waits on it goes on. `ENDSESSION_CRITICAL` asks nothing, since that session ends whatever the answer.
  - `os_menus`: the quit goes ahead only when the box path ran and nothing held the exit. When the window or the app cannot be reached, it goes ahead only if no session is exporting.
  - While the Stop box is up, the main window sets a shutdown block reason ("Exporting your data", from the lang system), so the Windows shutdown screen says why Quill holds it. It is cleared when the box closes. Other platforms ignore it.
  - The answer comes from `exit_held_by_export`, the hold #1299 added, so there is one path. `ZoomPlatform` and its window forward both new methods. All of it is logged in the three `QUILL-CHANGES.md` files.
- L18, a log out stays cancelled after Stop: the app quits and the user logs out again. tdesktop does the same.
- L18, Windows differs from tdesktop: tdesktop does not hold the end of a session there (Qt `commitData` only, and `integration_win.cpp` has no `WM_ENDSESSION`). Quill asks, as the plan for this bundle says.
- L18, Linux has no change: neither Quill nor GPUI has a session-end path there. The only logind signal in `gpui-pre-linux` is `PrepareForSleep`, and tdesktop's `saveStateRequest` only sets `RestartNever`. The trait method defaults to doing nothing.
- L18, forwarded `--quit` was already held: `take_quit_request` becomes `TrayAction::Quit`, then `exit_after_exports`.
- L20, a stop now lands within one chunk: the copy goes through `<name>.part` in 1 MiB chunks and checks the new `Tdlib::is_cancelled` between them. A stop removes the part and returns `Cancelled`, and `release` then posts the delete. Nothing half written ever sits under the final name. A volume that clones (APFS through `clonefile`, btrfs and XFS through `FICLONE`) still shares the blocks at once.
- L20, cleanup outlives the process: each download made only for an export is listed by its remote id in `<account>/export-leftovers`, one small file per download, until its `deleteFile` is answered or something else keeps the file. On `authorizationStateReady` the driver looks each entry from an earlier run up with `getRemoteFile` (file ids change between runs) and deletes the file when the cache holds only the export's copy: a part of its download, or the finished copy at the recorded path. A file used since then stays. A slow copy, a crash, a forced shutdown or a delete sent just ahead of `close` therefore leaves nothing behind after the next start.
- L20, the quit: `close_all_for_quit` stops every export and waits up to `EXPORT_STOP_HOLD` (400 ms, inside the shared 1500 ms quit deadline) for the workers to end. Each link hears its worker end through a channel the worker drops, so the wait blocks on that and returns the moment the last worker is gone, with no polling. `begin_close` then sends what is queued ahead of `close`. The wait costs nothing when no export runs.
- L19: the box keeps the exit it will run, and a new request replaces it. The box stays up, as tdesktop's does, so the key shows no change on screen. Stop then runs the latest request.
- N17: workers name the chat a file belongs to on their `deleteFile`. The driver answers the worker at once, so the export is not slowed, and takes the delete over. A file the view asked for stays. A file of a chat on screen in any window is held, so a row that loads there finds it in the cache. Any other file goes at once. Held deletes go when their chat leaves the screen or when no export runs any more, minus the files the view asked for meanwhile. The cache therefore grows by at most the shown chat's media while an export runs. This replaces the first version's `held_file` window and its `should_download` override, which also went stale after a cancelled or skipped download.
- N18: an exported file is always a copy of its own. On a volume that clones it costs no space. On any other volume, NTFS and ext4 among them, each file needs its full size on disk twice for as long as the export holds it: once in TDLib's cache and once in the export folder. So an export now needs free space for everything it writes plus the cache copy of the file in hand. A nearly full disk now fails an export that the hard link used to fit. Files the export downloaded for itself leave the cache after the copy, so the double cost lasts one file at a time, or for the shown chat's held files.
- Failed saves: the release runs before the save's error returns. `note_view_file` covers every export, Save as HTML included.

## Measurements

The stop's delay, on this container's disk with a warm page cache (`rustc -O` scratch program, three runs):

| | Before | After |
| --- | --- | --- |
| A stop during a 512 MiB copy waits for | the whole copy: 157 ms, 178 ms, 267 ms | one 1 MiB chunk, slowest of 512: 0.8 ms, 0.9 ms, 7.8 ms |

The old wait grows with the file and the disk: a multi-GiB video on a slow or cold disk takes seconds, past the 400 ms hold. The new wait does not depend on the file's size. A real Mac or Windows disk was not measured.

## What ran

`cargo fmt`, and clippy with `-D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`, on the merged tree. `check-lang.py`, `check-hotspots.sh`, `check-file-size.sh`, `check-window-scope.py` and `check-app-name.sh` pass.

`cargo test --features demo-capture --locked`:

- `--lib -- export:: closing chat_state::export rich_html`: 211 passed.
- `--bin quill -- export_quit link_inbox background_accounts::closing account_export_box`: 30 passed.
- The whole `--lib` suite: 6865 passed, and only the known `atomic_write_failure_keeps_old_file` failed, the chmod 0555 test that fails as root.

No new test sleeps or waits on a timeout. The driver tests feed TDLib's answers by hand, and the copy test stops through the fake's cancel flag.

New and changed tests, each run with its fix taken out:

| Test | With the fix out |
| --- | --- |
| `a_stop_during_the_copy_leaves_no_file_and_still_releases_the_cache` | the photo sits in the export folder |
| `a_stop_mid_copy_lands_within_one_chunk_and_leaves_no_part`, `a_copy_that_runs_to_the_end_takes_the_final_name` | new code, no older version to run |
| `an_entry_outlives_its_run_until_forgotten` | new code, no older version to run |
| `a_file_an_earlier_run_left_in_the_cache_goes_on_the_next_start` | no `getRemoteFile` is sent |
| `a_file_used_since_an_earlier_run_left_it_stays_in_the_cache` | no `getRemoteFile` is sent |
| `a_cache_copy_leaves_the_list_once_its_delete_is_answered_or_the_view_keeps_it` | the entry stays listed after the answer |
| `a_link_hears_its_worker_end_at_once` | new code, no older version to run |
| `an_export_leaves_a_file_the_open_chat_shows_until_the_chat_closes` | `deletes: 1` while the chat is open |
| `a_held_delete_goes_when_the_export_stops_and_stays_for_a_file_the_view_asked_for` | both files deleted while the chat is open |
| `an_os_quit_that_finds_the_app_busy_is_asked_again_later` | the quit is allowed |
| `an_os_quit_with_the_main_window_gone_is_refused_during_an_export` | the quit is allowed |

From the first version, still passing: `an_os_quit_during_an_export_is_refused_and_stop_ends_the_app`, `an_os_quit_goes_ahead_when_nothing_holds_it` (the control), `what_a_stopped_export_left_queued_goes_out_ahead_of_close`, `closing_for_quit_stops_every_export`, `a_quit_waits_for_a_stopped_export_to_end`, `cmd_q_with_the_box_focused_becomes_what_stop_does`, `an_export_leaves_a_file_the_view_asked_for_in_the_cache`, `a_file_left_in_the_cache_after_all_stays_apart_from_the_export`, `a_file_that_could_not_be_saved_still_leaves_the_cache`.

The first version's Mac run (a `--screenshot-demo` build with a local hook, quit events sent with `NSRunningApplication.terminate()` and through the Dock menu) covered the refuse path: the box comes up, a second quit leaves one box, Stop exits with code 0. It predates the `Later` path.

## What did not run, and the lines that were only type-checked

This container cannot build or run macOS or Windows code. Every changed macOS and Windows line was type-checked with `cargo check` against the real target, and none of it was linked or run:

- `cargo check -p gpui-pre-macos --target aarch64-apple-darwin` and `cargo check --lib --no-default-features --target aarch64-apple-darwin` pass. Apple's C compiler is missing here, so a stub stood in for the C and Objective-C files of dependencies (`objc2-exception-helper`, thorvg), and an empty `stitched_shaders.metal` stood in for the Metal shaders. None of that is Quill's change. A deliberate typo in `replyToApplicationShouldTerminate` failed this check, so it does reach the changed code.
- `cargo check -p gpui-pre-windows --target x86_64-pc-windows-gnu` and `cargo check --lib --no-default-features --target x86_64-pc-windows-gnu` pass, with a stub in place of `x86_64-w64-mingw32-windres`, which only builds the icon resource.
- The `quill` binary was not checked for either target: the container ran out of disk. The binary's changed lines (`src/ui/export_quit.rs`, `src/ui/os_menus/mod.rs`, `src/ui/interface_zoom/mod.rs`) sit outside any `cfg(target_os)` block and compile on Linux.

The lines a Mac check and run should confirm:

- `third_party/gpui-pre-macos/src/platform.rs:1428-1480`: `should_terminate`, `ask_quit_request`, `ask_quit_request_later` and its `answer_later`. A Mac run should send a quit while the app is mid-update and see the box, then Stop exit and Esc keep the app.
- `src/export/copy.rs:73-86`: the `clonefile` call. A Mac run should export to an APFS folder and see the file appear at once.

The lines a Windows check and run should confirm:

- `third_party/gpui-pre-windows/src/platform.rs:1326-1377`: `handle_query_end_session`, `ask_quit_request`, `ask_quit_request_later`, `handle_quit_request_later`, plus the `quit_request_set` flag at lines 128 and 933.
- `third_party/gpui-pre-windows/src/window.rs:30` and `993-1001`: `ShutdownBlockReasonCreate` and `ShutdownBlockReasonDestroy`. A Windows run should start a shutdown during an export and see "Exporting your data" next to Quill on the shutdown screen.
- `third_party/gpui-pre-windows/src/gpui_message.rs:31`: `WM_GPUI_QUIT_REQUEST_LATER`.

Also not run here:

- No real TDLib, account, log out or shutdown. Whether TDLib finishes a `deleteFile` sent just ahead of `close` no longer matters: the next start deletes what is left. The tests check the requests on the wire, and that `getRemoteFile` takes a remote id across runs is TDLib's documented contract, not something run here.
- The `FICLONE` path on a btrfs or XFS volume. This container's disk does not clone, so the tests ran the chunked copy.
- No test covers the shutdown block reason: the test platform has no shutdown screen to read it from.

## What stays open

- A held delete waits while any export of the account runs, not only the one that downloaded the file. With two exports at once, the files of a chat on screen leave the cache when the later one ends. Nothing is fetched twice either way.
- An entry an earlier run left is cleaned only after that account reaches Ready again. An account that is never opened again keeps its leftovers, as its cache does.
