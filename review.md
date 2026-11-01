APPROVE

# Second review of PR 1306 (calls-media), commits 66a44011b..25f8bffec

Scope: the five fix commits on top of the first review's head (66a4401), up to 25f8bff. Nothing on the PR branch was changed. The checks below ran in a separate local worktree. The revert probes were undone after they ran.

## Findings 1 to 5

| # | Commit | Fixed |
|---|--------|-------|
| 1 | 580f548 | Yes |
| 2 | 795a70b | Yes |
| 3 | d9d87a8 | Yes |
| 4 | ab3d72b | Yes (one nit below) |
| 5 | 25f8bff | Yes |

### 1. A pause lost while the share opens

`finish_open` now returns `Ok(active.paused)`, and `pump_shared_screen` calls `set_screen_paused(paused)` with that value instead of `false`. A `Paused` read by `screen_change` during the open sets `active.paused = true` and sends one toggle. After the open, `set_screen_paused(true)` turns off the new feeder's forwarding. It sends nothing more, because `changed` is false and `was == paused`. When nothing paused, the call is the same no-op as before. The new test `a_window_hidden_while_its_share_opens_stays_paused` asserts `paused()` and exactly one `is_paused: true` toggle. With the old `set_screen_paused(false)` put back, it fails at `call_screen.rs:537`.

### 2. Visibility from a replaced window

The callback compares `window.window_handle()` with `app.group_call.window` and returns when they differ. The test `a_replaced_call_window_hiding_leaves_the_large_tile` swaps in another handle, hides the old window, and expects the pinned tile to stay large. With the guard disabled, it fails at `group_call_tiles_tests.rs:176`. The existing visible, hidden and closed test still passes, so the normal path still works.

### 3. Device thread failures wake the poll loop

An `Answering` guard owns the sender. Its `Drop` drops the sender first and then pokes. So the answer, or the hang-up, can be read before the poke, on success, on error and on panic. The panic case works because the workspace sets `panic = "deny"` as a lint, not `panic = "abort"`, so unwinding runs `Drop`. The spawn-failure branch now pokes after it sends. `a_panicked_open_answers_and_wakes_the_poll_loop` uses its own `Signal`. With the poke removed from `Drop`, it fails at `device_thread.rs:166`.

### 4. The lost-mic test depends on parallel tests

`FrameSink::waking` gives each sink its own signal, and `new` keeps `UPDATES`. `CallCapture::inject_wake` passes the injected signal to the mic sink and to the mic `DeviceThread`. No other test pokes the test's static `WAKE`. With the poke removed from `push_event`, the test fails at `call_capture.rs:380`. Over 400 sequential runs and 200 runs with the CPU loaded, it never passed without the fix.

Nit, not blocking: the mic device thread pokes the same `WAKE` right after it sends `Ok`. `pump_until_mic_answers` can return in the moment between that send and that poke. If the thread is then preempted until after the test reads `woken`, the late poke would satisfy `assert_ne!` without `push_event`'s poke. The window is a few instructions wide, and I could not make it happen. Giving the sink and the device thread different signals in the test would close it.

### 5. Device warming for calls you are not in

The gate is now `active_call.is_some()`, or a tracked group call with `is_joined`. The renamed test first tracks an unjoined chat and asserts no warm, then joins and asserts one warm. With the old gate put back, it fails at `call_capture.rs:472`. Warming for a group call now starts on join rather than when the call is tracked. That is fine, because the mic opens only after join anyway.

## New bugs

None found. Things I checked:

- `set_screen_paused` during the open still sends `toggleGroupCallScreenSharingIsPaused(true)` before the feeder exists. That is how the PR already behaved at 66a4401, not something these commits added. The fix only stops the later unpause.
- Between `CaptureFeeder::start` and `set_forwarding(false)` in the same pump, the feeder can forward the few frames already in the sink. That is a gap of microseconds and not visible to the user.
- `DeviceThread::open` has a new `wake` parameter. All callers pass one: the mic passes the injected or default signal, and screen and system audio pass `UPDATES`. All other `FrameSink::new` callers keep `UPDATES`.
- No new `unwrap`, `expect`, `panic` or indexing in `src` outside tests. None of the touched files are in `scripts/hotspots.txt`. Each commit has the required trailer and one finding.

## What ran

Linux x86_64, at 25f8bff:

- `cargo fmt --check`: clean.
- `cargo clippy --features demo-capture --all-targets -- -D warnings`: clean.
- `cargo clippy --no-default-features --all-targets -- -D warnings`: clean.
- Library tests: `call_capture` 11 passed, `call_screen` 15 passed, `device_thread` 5 passed, `feeder` 32 passed.
- Binary tests (`--features demo-capture`): `group_call` 60 passed.
- Revert probes: each of the five new or changed tests failed with its fix undone.

## Not verified

- Windows and macOS compile. The changes are platform-neutral Rust.
- Real window occlusion and real device panics.
