CHANGES

# Review of the fix rounds, checked in origin/sweep-all at bcf6752e2

I checked every finding in these reviews: review-b9, review-b10, review-b13, review-b14, review-b18, review-b1-2, review-idle-and-redraw, review-sticker-timing-2, review-calls-media-2 and review-export-quit. Each "What stays open" item in the matching claude/prtext-* PR text counts as a defect, as the rules say. Only what is not fixed is listed below. Line numbers are in sweep-all.

## Fix commits that never reached sweep-all

`git cherry origin/sweep-all <branch>` shows that these fix commits are missing. sweep-all merged each branch before these commits landed, and no later merge picked them up. I checked the code as well: none of these fixes came in by another route.

- claude/fix-b13-b18: eec0b50e5, 87a971812, f2d7ac62a, 15453932f, 304729944, c1d81935e, 25a486e4a
- claude/fix-b14: 4f1726fae, bb8cc03f1, 28e0496a3
- claude/fix-b1: 7c4b3c0e1 (fork notes only)
- claude/fix-b7: 21035932a (not in this review's scope; noted only so it is not lost)

Merging these branches into sweep-all again closes most of sections 1 and 2. Rerun the tests after the merge. 25a486e4a must come with eec0b50e5 because of the MSRV.

## 1. B13 and B18 (the fix-b13-b18 commits above are missing)

- **B13 #1, L10.** `third_party/gpui-pre-wgpu/src/wgpu_renderer.rs:2403` still maps `KeepsDying` to `was_software: true`. That only *allows* a CPU adapter. `WgpuContext::new` sorts `Cpu` last, so recovery goes back onto the GPU that keeps dying. Failure: a flaky discrete GPU on a machine with lavapipe. From the 3rd loss, every recovery picks the same GPU. At the 6th loss within 10 min, `device_gone` shows the alert and Quill exits, when it should have moved to software. `QUILL-CHANGES.md:123-133` still describes it wrongly.
- **B13 #2, N50.** `src/battery_signal.rs:67-77`: `refresh` still does `FRESH.swap`, then the D-Bus `Get`, then `store(Some(answer))` unconditionally. Failure: a probe's `Get` returns the old `a`. Then a `PropertiesChanged` records `b`. Then the probe stores `a` over it. The poked wake sees `FRESH` and returns `a`, and the battery saver keeps the stale state for up to 10 min.
- **B13 #5.** `wgpu_renderer.rs:2450-2454`: `hardware_probe_at` still returns `probe.due()`, which is in the past while a look runs. Failure: during a GPU look, every frame of an animating window arms an already-due calloop timer and runs a probe step. The intended rate is one every 250 ms.
- **B13 #6, N49.** `third_party/blocking/QUILL-CHANGES.md:22-23` still cites the standalone program. There is no in-Quill count of `blocking-*` wakes or of thread spawns per `unblock`.
- **B13 #8, L09.** There is no seam and no test for "install the context only after `rebuild` succeeds" (`wgpu_renderer.rs:2587-2588`). Failure: if the install moves back above `rebuild`, every test still passes. The window then goes to `Adopt` on a context it cannot draw on, and its failure count is lost.
- **B18 D1, N67.** `src/ui/history/suggested_post_block.rs:14,50-53` still uses `civil_local(..)` and `now_unix()`. Failure: under `WallClock::chicago_repeated_hour()`, the history block names a different time from the bar and the boxes for the same post.

## 2. B14 and B1 (two of the fix-b14 commits above are missing)

- **B14 B4, L06.** `src/album.rs:36` `is_album_media` still accepts only photos and videos, and `HistoryGroup` has no file variant. Failure: Quill sends a 3-file album, and the user sees 3 bubbles. Dragging one file then drags all three (`forward_drag_source.rs`).
- **fix-b14 stays open: one caption under the last file.** tdesktop draws each file's own caption under it. Today there is no file-album bubble at all (B4).
- **B14 N1.** `src/ui/photo_editor.rs:360` `open_photo_editor` does not refuse a tap while a Send is held. Failure: Send while card 1 is still rendering, tap card 2, rotate it. `finish` (`composer_sends.rs:409-416`) then closes the editor and the rotation is lost with no note.
- **B14 N4.** `src/composer/attachments.rs:173` `append_dropped_files` still turns a picture into a Document when the tray holds files. Failure: drop a photo onto a tray that holds a PDF. The photo goes out as an uncompressed file, although `split_for_send` could now send it as its own photo group.
- **B14 B2, robustness-8, partly.** Names are now filtered at parse time. Three names still bypass that:
  - group call title: `src/telegram/envelope/calls.rs:432-436`
  - contact card first and last name: `src/telegram/envelope/message_contact.rs:175-184`
  - the invite-link preview title

  Failure: a shared contact or a voice chat titled with tag characters or Zalgo marks reaches the shaper raw.
- **fix-b14 stays open: windows stay up during the quit flush.** `src/ui/navigation.rs:566` flushes synchronously with the windows still shown. Failure: on a network home folder that hangs, quit shows a frozen window for up to the 10 s stall budget.
- **B1-2 C and fix-b1 stays open: the QuillApp entity is not kept.** The minimum fix landed: the closing set is carried over, and so is the deep link. The reopen still builds a new app (`src/main.rs:319`). Failure: after a renderer reset, the open chat and any dialogs are gone. A file PiP's "back to the viewer" goes to the dropped app, and a live-stream PiP loses its client.
- **fix-b1 stays open: a session end can reopen the main window.** `quit_asked` is set only through the `interface_zoom` wrapper. Failure: Windows destroys the main window at logoff without a quit through that wrapper. `main_lost` is then true, and Quill reopens the main window while the process is ending.
- **fix-b1 stays open: three Windows clippy lints.** They are unchanged:
  - `src/update_install/layout.rs:101`: `let mut builder` is mutated only under `cfg(unix)`.
  - `src/battery_saving.rs:125`: `ok.then(|| ..)`.
  - `spell_win.rs:78-82`: a complex return type.

  Failure: `cargo clippy --target x86_64-pc-windows-* -- -D warnings` fails.
- **fix-b1 stays open: a cover test that depends on state.** `src/ui/media_viewer/cover_tests.rs:29` `the_chat_behind_the_viewer_holds_still_until_it_closes` is unchanged and can fail in a full `--bin quill` run. It passed in my filtered run.
- **B14 B1, a leftover window.** In `src/settings/accounts/deletion.rs:60-64`, between `discard_deferred_writes_under` and `remove_dir_all`, a `write_json_deferred` from another thread for that folder can still be queued and recreate the folder. This is unlikely, because the account has already left the list, but nothing prevents it. Fix: hold a "deleted" mark on the folder in the writer, so that a submit under it is refused until the delete returns.

## 3. B10

- **#1 is still reachable: the over-cap path lets a second Quill take over.** A stuck launch now holds a client slot for up to 30 s (`UI_ANSWER`). The cap is `MAX_CLIENT_THREADS = 8` (`src/single_instance.rs:63`), and an over-cap connection is dropped with no answer (`:631-634`). The launcher reads that EOF as `NoOwner` (`:320`). It retries with the legacy header and gets EOF again (`:281`). It then unlinks the live owner's socket and binds its own (`:566-603`).

  Failure: the UI freezes and the user clicks a `tg://` link or the Dock icon 9 times within 30 s. Launches 1 to 8 are queued and release `AcquireLock`. Launch 9 gets `Acquired::Primary`, and two processes open one TDLib database.

  Fix: over the cap, write `stuck\n` (or `busy\n`, mapped to `Unresponsive`) before dropping the connection. Test: fill 8 slots against a silent heartbeat, then expect `TimedOut` from a 9th `acquire`.
- **#7 has a gap at startup.** `src/ui/link_inbox.rs:19` calls `UI.start()`. The spawned task reads `seen` only later (`:56`). Failure: a second launch that marks and pokes in that window (a double-click on the app icon during startup) is included in `seen`. The loop then sleeps 60 s with no beat, the owner answers `stuck` at 30 s, and the user gets a false "not responding". Fix: read `seen` before `UI.start()`, or beat once before the first sleep.
- **#4 and b10-2 stays open: a router change can be missed.**
  - macOS reads the link-layer address for IPv4 only (`src/net_watch/link.rs:140`). Two IPv6-only routers with the same address therefore read as one network.
  - While a new gateway's MAC is unresolved, `Path::settled` keeps the old one, so the change is seen late.
  - Windows `RESOLVING` is a single flag, so it skips the IPv6 resolve while an IPv4 resolve runs.
- **#6a.** `src/net_watch/watcher_tests.rs:414` still uses an unbounded `WAKE.wait`. The review asked for `wait_timeout`. A missed callback hangs the Windows job until the 10 min step limit instead of failing with a cause.

## 4. B9

- **#1 has a gap.** `pending_still` is cleared by `reset` and by the video path (`video_pick.rs:75`), but not by the Tgs arm or the non-adapt arm of `pick_existing_sticker` (`src/ui/sticker_set_box/creator.rs:389-399`). Failure: adapt WebP sticker A, reopen the chooser, and pick Tgs sticker B before A decodes. B is added, then A lands, and `land_still_pick` opens the editor on A, over the user's later choice. Fix: clear `pending_still` at the top of `pick_existing_sticker`.
- **#3 is partly done.** WARP timing ran. There is still no real device loss with a toast up (`dxcap -forcetdr`) and no hardware GPU numbers. The first-toast figure (47.7 ms on the branch against 13.1 ms on main) has no explanation, and the run order was not swapped to rule it out.
- **Replay intro race.** It is fixed in code with a process-wide `WRITING` mutex (`src/ui/first_run_ui.rs:116-127`). prtext-b9-2 says the race has no test, and none touches `intro_file`.

## 5. Export quit

Everything the review asked for is in the code:
- the cancellable `.part` copy, with the clone kept;
- the per-account journal, cleaned on Ready;
- held deletes for open chats;
- `NSTerminateLater` and the Windows `WM_QUERYENDSESSION` refuse-and-ask-later path, with `ShutdownBlockReason*`;
- `QuitAnswer::Later` and the `os_menus` `Err` rule.

What is not fixed:

- **prtext-export-quit-2 stays open: a held delete waits for any export of the account.** `release_held` uses `SettingsState::exporting()`, which is true for any running link. Failure: with two exports running, a file of the open chat that the first export held leaves the cache only when the second export ends.
- **prtext-export-quit-2 stays open: leftovers are cleaned only when that account reaches Ready again** (`src/connect/core/mod.rs:680`). Failure: an account that is never opened again keeps its export leftovers in the cache.
- **A delete answered with an error clears the journal.** `src/state/domains/settings/export_files.rs:122-124` calls `forget` on any answer to the `deleteFile`, with no check for an error. Failure: TDLib answers `deleteFile` with an error, and the file stays in the cache with no journal entry left to retry it. (Whether TDLib errors reach this path was not traced end to end.)
- **A `.part` file is left after a crash.** Nothing removes `<name>.part` from the export folder after a kill or a power loss during a copy. There is also no `sync_all` before the rename. Failure: a power loss right after the rename can leave a short file under the final name, which is the truncated-file case the review wanted gone.
- **Stale comment.** The doc comment at `src/connect/closing.rs:59-61` still says that a worker "copying a large file is left behind". The chunked copy has made that untrue.
- **The macOS and Windows quit paths have never run.** They were type-checked only. The macOS `answer_later` and the Windows `WM_GPUI_QUIT_REQUEST_LATER` handler re-queue themselves for as long as the app stays borrowed, with no bound. A Mac and a Windows run, as listed in prtext-export-quit-2, are still owed.

## 6. Idle, sticker timing and calls media

- **idle: B6.md is still missing** from claude/sweep-briefs. Nothing has checked the B6 items that PR 1312 does not name.
- **calls-media-2 nit, not fixed.** In `src/connect/call_capture.rs:189,193` the mic `FrameSink` and the mic `DeviceThread` share one wake signal, so `a_lost_microphone_needs_no_telegram_update` can pass with `push_event`'s poke removed if the thread's late poke lands after `woken` is read.
- **prtext-fix-sticker stays open.**
  - `ui::anim_layer::layer_tick_tests::a_sticker_tick_renders_the_layers_alone` fails on Linux when it runs with the other tests.
  - `frames_of` in `src/sticker_playback/player/tests.rs` polls against a 20 s wall-clock deadline, which breaks the rule against tests that depend on time.

Fixed and checked: idle D1 to D4 and the `draw()` comment; the sticker-timing-2 merge and notes 1 and 2; and all six items prtext-sticker-timing-2 left open. The vsync-based ones fall back on Linux and Windows by design.

## What ran (Linux, sweep-all at bcf6752e2)

- `cargo test --features demo-capture --locked --lib -- export:: chat_state::export settings::deferred settings::accounts::deletion composer::attachments single_instance net_watch telegram::hub window_title battery_signal sticker_playback`: 328 passed, 0 failed, 14 ignored.
- `quill` bin tests (`--features demo-capture`), filtered on `export_quit link_inbox sticker_set_box app_theme surfaces first_run wake_wait frame_grid cover_tests emoji_read sticker_player closing_tests device_loss suggested_post composer_sends`: 194 passed, 0 failed.
- `cargo test -p gpui-pre-linux -p quill --lib --features quill/demo-capture -- recovery_timers wayland::client::tests`: 23 passed.
- `scripts/test-frame-idle.sh`: 26 and 11 passed. `scripts/test-device-recovery.sh`: 17 passed. `scripts/test-blocking-pool.sh`: all passed.

## Not run

- No macOS or Windows build or run.
- No real TDLib. The ignored `live_exit` and `blocked_receive` tests were not run.
- No namespace tests.
- No full test suite and no clippy.
- The new failure paths above (the over-cap takeover, the startup window, the stale `pending_still`, the delete error) were found by reading the code. No probe test was written for them.
