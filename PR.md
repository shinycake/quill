Quill on Windows keeps account databases safe and gets the main window back after a renderer failure

## What was wrong

Second review of bundle B1 (review.md on claude/review-b1-2) found these problems on the Windows GPU loss path:

- C. After a lost GPU, Windows builds a new QuillApp. The old app waits at most QUIT_CLOSE_DEADLINE for its TDLib clients. The accounts it could not confirm closed were dropped along with it, so the new app's `open_wait` said Go and could open a database that was still closing. A launch link that had not been opened yet was also lost (`pending_deep_link: None`).
- D. When the platform gave up on the main window's renderer while a PiP was open, the device still worked and one window was left, so `after_window_closed` returned Nothing. The user had a video and no way back to their chats. On Windows, closing the main window when that should quit did not call `cx.quit()`, so a quit with a PiP open did not quit.
- Open item 3. `WindowList::snapshot` allocated a `Vec` on every vsync tick and every gate wake.
- A (licenses) and B (behind main) were listed as blocking.

## What changed

- 889d88ae: not a review finding. sweep-all's Linux and Windows test builds did not compile, because the merge's import cleanup removed `Warmed`, which only macOS left unused. The one test that reads it now names it as `super::Warmed`.
- 3fa3acad (C): a dropped app hands its closing set (pending clients and unconfirmed accounts) to the next app through a main-thread registry (`take_left_by_dropped_app` in `background_accounts/closing.rs`, `ClosingSet::absorb`). It also hands the user's link it had not opened yet to the reopened window. An agent's queued link is not handed on, because the next app would take it for the user's. Tests: an account a closed window left unconfirmed gives `CloseWait::Never` in the next app, and a link goes to the next app but an agent's link does not. Both tests fail without the fix.
- c00679c8 (D): `after_window_closed` takes a `Closed` with a new `main_lost` input: the main window closed while Quill had not asked to quit. In that case Quill reopens the main window once without attaching a new device, and the PiP stays open. A second loss quits with the alert. Every quit and restart goes through `interface_zoom::ZoomPlatform`, which notes "Quill asked to quit" (`startup_failure::note_quit_asked`). The main window's id comes from the existing `MainWindow` global (`link_inbox::main_window_id`). `close_main_window` now calls `cx.quit()` on every platform when it quits, as macOS already did and as Telegram Desktop does, so a quit with a PiP or chat window open really quits. The CloseWindow action's separate quit is gone. New table rows include (PiP left, main lost, GPU works) giving Reopen.
- 67272acb and 7c4b3c0e (item 3): `snapshot` returns `SmallVec<[W; 4]>`. `window_list.rs` stays std-only for `scripts/test-window-list.sh`, so it gets a generic `collect`, and `platform.rs` adds the SmallVec `snapshot`. The fork notes record the change.

## Findings not changed, with evidence

- A: already fixed on sweep-all by 713e2f6e. `scripts/third-party-licenses.sh --check` with CI's cargo-about 0.9.2 prints "THIRD_PARTY_LICENSES.md is up to date". With cargo-about 0.9.3 it reports a diff in ring's license text, so the check depends on the pinned version.
- B: `git merge-base --is-ancestor origin/main HEAD` holds at 5a173f50, so there was nothing to merge.
- C, preferred form: keeping the QuillApp entity across windows would also keep the open chat and UI state. `QuillApp::new_with_demo` builds dozens of input states and `subscribe_in` subscriptions bound to the window it gets, so moving that into a `rebind_window` step is a large refactor. I applied the review's minimum fix instead.

## What ran

- `cargo fmt --check`, plus `cargo clippy -D warnings` for `--features demo-capture --all-targets` and for `--no-default-features --all-targets`: clean.
- `cargo test --features demo-capture` on Linux: lib 6966 passed. The bin had 2532 passed and 1 failed: `media_viewer::cover::tests::the_chat_behind_the_viewer_holds_still_until_it_closes`. That test passed on its own at 889d88ae and 3fa3acad before the full suite ran, and failed on its own at 889d88ae (no review fix in it) after the full suite ran. It depends on state an earlier run leaves behind and is not caused by these commits.
- `scripts/test-window-list.sh`: 5 passed.
- `scripts/check-hotspots.sh`: main.rs and app.rs are changed under a `Hotspot-change` trailer on 3fa3acad.
- `cargo check --target x86_64-pc-windows-gnu --features demo-capture --all-targets` with mingw-w64: 0 errors and no warnings in the touched files. Windows clippy with `-D warnings` stops on three existing lib lints outside this change: `update_install/layout.rs:101` unused mut, `battery_saving.rs:125` then_some, and `spell_win.rs:78` type complexity.

## What did not run

- Nothing ran on Windows. That covers the forced TDR, the reopen after a renderer failure with a PiP open, the quit with a PiP open, and the new `copying_up_to_four_windows_takes_no_heap` test in `gpui-pre-windows/src/platform.rs`. The fork crate's test target does not build here, because cargo will not enable GPUI's test-support for a crate outside the workspace (`render_to_image` is missing).
- The snapshot allocation was not measured as a number. The SmallVec type and the `spilled()` test show that up to 4 windows take no heap.
- No test checks that `close_main_window` calls `cx.quit()`, because the test platform does not record a quit.

## What stays open

- A reopened main window starts with a fresh UI: the open chat and dialogs are gone. A file PiP keeps playing, but its "back to the viewer" goes to the dropped app. A live stream PiP loses its client when the old app closes it. Fixing this means keeping the QuillApp entity, which is the refactor described above.
- If Windows ends the session by destroying windows without a quit going through the platform wrapper, Quill could try to reopen the main window as the process ends.
- The three Windows clippy lints and the state-dependent cover test listed above.
