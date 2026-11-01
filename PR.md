Helper and poll tests fail fast and assert what they claim, root-run tests pass, and a stale video shim no longer hides
## What was wrong

- L13: `helper_live_relay_round_trip` waited on `recv()` with no bound. A helper that died or went quiet before relaying hung the test until the CI step's 30 minute timeout.
- L14: the demo's folder "Mark as read" cleared the unread counts but left `Session::revision` alone, so the memoized tray, dock and title badges kept the old count. It had no test.
- N33: a quillvideo shim built from older sources (the audit found a dev build at ABI 3) ended the loader's search, so a current shim later in the list was never tried. The real decoder tests then skipped as "not available" on every run, which is how a stale dev shim goes unnoticed.
- N76: the `spoiler12` history demo had no decision entry and no doc line on its variant.
- N77: `a_picked_language_is_asked_for_by_the_pass_the_pick_wakes` and `the_first_frame_of_a_viewer_wakes_the_pass_that_asks_for_its_thumbs` asserted that `UPDATES`' epoch moved. The epoch counts every thread's pokes, so any test running in parallel that poked `UPDATES` made them pass.
- N78: the poke `pump_group_call_streams` makes when it hits its 64 events per poll cap ran in no test.
- N112: two tests that rely on a 0555 folder failed as root. `atomic_write_failure_keeps_old_file` skipped its own assert when root's write went through, but its last assert still compared the file after that write. `a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why` had no guard.

## What changed

One commit per item.

- L13: the test's relay sink pokes `wake::UPDATES`, the signal a carrier failure pokes. `next()` drains with `try_recv`, fails on any `host.poll()` failure, and waits with `wait_since` in between. The helper's own watchdog reports a quiet helper, so nothing in the test needs a timeout.
- L14: both demo list reads (folder and main or archive list) go through a new `Session::mark_chats_read_locally`, which bumps the revision.
- N33: the loader moves on to the next candidate when a library has no `qv_abi_version` or reports another ABI. The reason names the path and ends with "rebuild it with scripts/build-ffmpeg.sh". The real decoder tests fail instead of skipping when the shim they found is out of date.
- N76: an entry in `docs/decisions/codex-history-anim-layer.md` under Fixtures, and a doc line on `HistoryAnim::Spoiler12`.
- N77: `wake::Signal` keeps a per-thread poke count in test builds, `pokes_on_this_thread`. Both tests assert on it. A GPUI test runs on one thread, so only the code under test moves that count.
- N78: `a_poll_that_leaves_events_queued_wakes_the_next_one` queues 65 engine events, then checks that the first poll pokes once, the second takes the last event without a poke, and the third finds none.
- N112: the accounts test asserts the old contents before the read-only step. The log out test puts a non-empty folder where `accounts.json` goes. No rename replaces it, root's included, so the test no longer needs Unix permissions and now runs on Windows too.

## What ran

All on Linux, as root, after merging origin/main at the end.

- `cargo fmt --check`, `scripts/check-hotspots.sh`, `scripts/check-file-size.sh`, and clippy with `-D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`: clean.
- Lib tests under `wake::`, `tray::`, `group_calls::stream`, `video_decode`, `settings::accounts` and `proxy_carrier`: 212 passed, 3 ignored. App tests under `translate_ui`, `group_thumbs`, `background_accounts`, `folder` and `chat_list`: 177 passed.
- L13: both `helper_live` tests passed against a local build of `quill-webview` under xvfb (round trip median 1.46 ms). With `QUILL_CARRIER_HELPER=/bin/true` the new test fails in 0.36 s with `carrier failed: [(21, Terminated)]`. Main's version was still blocked when killed at 60 s.
- L14: `badge_cache_sees_a_folder_read_locally` fails with the bump removed (left 7, right 4).
- N33: built the shim with `scripts/build-ffmpeg.sh` (FFmpeg n9.0.2) and used a stand-in library that reports ABI 3. With the stale one first and the fresh one after it, the decoder test now decodes; main skipped with "quillvideo ABI 3 (expected 4)". With only the stale one, the test fails and names the rebuild script. All 74 `video_decode` tests pass with `QUILL_REQUIRE_VIDEO_LIB=1` on the fresh shim.
- N77: with the language pick's poke or the strip's poke removed, each test fails with "a resting poll loop runs a pass". `a_thread_counts_only_its_own_pokes` covers the counter.
- N78: with the cap poke removed, the new test fails (left 65, right 66).
- N112: both tests pass as root. Main's accounts test fails as root at its last assert.

## What did not run

- Nothing ran on macOS or Windows. No Windows target or mingw toolchain is installed here, so `cargo check --target x86_64-pc-windows-gnu` did not run. The log out test now runs on Windows and relies on a rename onto a non-empty folder failing there, which is expected but not seen.
- N33: the audit's ABI 3 file is on another machine. Whoever has a dev build under `vendor/ffmpeg/prefix` needs to run `scripts/build-ffmpeg.sh` again. The loader and test changes make the next stale shim fail the decoder tests instead of skipping.
- N76: the `spoiler12` demo was not captured. The entry quotes the measurement in the commit that added the speck layer.
- No main-thread or timing measurement: no item gives a number to compare before and after, apart from the hang in L13.

## What stays open

- The cap poke in `pump_group_call_streams` also fires when exactly 64 events were queued and none are left. That costs one spare poll and is left as is.
