CHANGES

# Review of PR 1312 (idle-and-redraw at 7899df8, against main at 948ad901e)

## The bundle brief is missing

`sweep-briefs/bundles/B6.md` is not on `claude/sweep-briefs` (head 1abf173). That branch has B1, B2, B4, B7 to B20 and `idle-wakes.md`, and no B6 shows up in any fetched ref's history. So I checked the PR against the items it names: idle-3, idle-7, idle-8 and render-4 using their text in `sweep-briefs/idle-wakes.md`, and L33, L34, L35 and N79 using only the PR body, because no brief text for them exists on the branch. If B6 lists items the PR does not name, this review cannot see them. The owner should push B6.md, or confirm that the PR's list is the whole bundle.

## Verdict per item

| Item | Fixed at the root? | Notes |
| --- | --- | --- |
| idle-3 lone frame keeps the link 250 ms | macOS yes. X11 and Windows no | See D2 |
| idle-7 receive thread wakes 5/s | Yes, if the wake keeps working | See D4 |
| idle-8 minimized poll hides the app | Yes, the poll is deleted | Sound, see notes |
| L35 Cmd-M hides the whole app | Yes (owner default) | |
| render-4 off-screen rows redraw | Yes | Notes below |
| L33 background ringtone loops by poll | Yes | |
| L34 Dock click in full screen wait | Yes | |
| N79 App Nap delays the poll loop | Partly | See D1 |
| Found, not changed: exit aborts with a client open or closing | No | See D3 |

## Defects that stay open

### D1. N79: a poll loop still sleeps through work under App Nap

`src/ui/wake_wait.rs` `run_poll_loop` takes the activity only in the `Yield` branch. In the `Rest` branch it drops the hold (`held = None`), rests, then waits `pace.gap_left(rested)` on a plain `executor.timer` without the hold. If a poke ends a rest shorter than the gap (10 ms on screen, 40 ms for background accounts), the update that is already waiting gets a full-length gap timer with App Nap on. That is the timer N79 measured running up to 200 ms late.

Trigger: updates that arrive just faster than each pass drains them. The usual case is TDLib delivering a backlog at its own pace after a reconnect, or a busy group while Quill is hidden. Each pass empties the queue and nothing arrives during it, so the loop rests, and the next poke lands a few ms later, inside the gap. The PR's test (`a_backlog_holds_app_nap_off_once_until_the_loop_rests`) only covers `work_left: true`, so it never exercises this path. The PR's own probe (100 passes, 10 ms gaps) is the same pattern, but on the Yield path.

Proof: I added the probe `review_probe_a_poke_after_a_short_rest_waits_its_gap_under_the_hold` to `src/ui/wake_wait_tests.rs` (not pushed). It pokes right after a resting pass and asserts the hold is taken while the gap runs. It fails on the PR head and passes with the fix below (see "What ran").

Fix: in the Rest branch, after the rest, compute `gap = pace.gap_left(rested)`. If it is not zero, take the hold (`held.get_or_insert_with(&hold)`) before `executor.timer(gap).await`. If it is zero, skip the timer, so a zero-length timer is not coalesced either. The hold then covers exactly the time work is known to be waiting, and a loop at rest still holds nothing. Keep the probe as the regression test.

### D2. idle-3 is only fixed on macOS

The brief puts X11 and Windows in scope ("`third_party/gpui-pre-windows/src/frame_idle.rs:52,102` and its X11 copy do the same") and its fix sketch says: "Add the same rule to `FrameIdle::frame` in `frame_idle.rs`". The PR leaves both at the 250 ms grace "because `frame_idle.rs` must stay identical across those crates, and this PR changes neither". That is not a reason. `scripts/test-frame-idle.sh` only requires the two copies to match, so one identical edit to both satisfies it. As things stand, a focused composer on X11 or Windows still runs 250 ms of frames per caret blink: about 30 to 60 wakes a second, which is the exact waste idle-3 names.

Fix: give `FrameIdle` the `LinkPace` burst rule. `demand()` from the waker serves one frame and stays parked unless a second demand comes within `BURST_WITHIN` (100 ms) of the last woken frame, or the frame after the draw is still active (GPUI asked again). Make the same change in both copies, and port the `link_pace.rs` tests (`a_caret_blink_never_starts_the_link`, `a_timed_animation_keeps_one_link_running`, and the others) into `frame_idle.rs`. Callers: `third_party/gpui-pre-linux/src/linux/x11/window.rs:377,394` and `third_party/gpui-pre-windows/src/window.rs:290,321`. Check with `cargo check --target x86_64-pc-windows-gnu` and the script. Wayland stays out, as the brief says.

### D3. exit aborts on a TDLib worker with a client open or closing

The PR found this and did not change it: 15 to 37 aborts in 200 exits with a client open, and about 115 in 200 with a close in flight. Under the owner's rule it counts. It shows to the user whenever a quit runs past `QUIT_CLOSE_DEADLINE` (1.5 s, `src/connect/closing.rs:37`). A slow close (a large database flush, a stalled network, several accounts) then goes into `exit` with TDLib still working. TDLib's static destructors run under its own workers and it aborts. On macOS that brings up the "Quill quit unexpectedly" report on a quit the user asked for. The PR also made the exit stress test close the client first, which removes the only coverage of this case.

Fix: once a client may still be open, `exit` must not run TDLib's static destructors. In `stop_shared_at_exit` (`src/telegram/hub.rs`), after `stop_within` returns, check whether any client is not confirmed Closed. The hub's route map being non-empty is enough, or use a process-wide count kept by `ClosingSet` (its "unconfirmed" state already exists). If one is, end the process with `libc::_exit(status)`. Take `status` from a static that the few `process::exit` callers set first (`src/ui/startup_failure.rs:43` sets 1). The default is 0, since a quit through `cx.quit()` is the only way to exit with clients open. This hook is registered after TDLib's destructor and runs before it, so `_exit` here skips exactly the teardown that aborts. TDLib's SQLite and binlog are crash-safe, so this leaves the files no worse than the abort does today. Quill's own quit work (`mark_clean_exit`, settings) runs in `on_app_quit` before `exit`, so it is unaffected. One check is needed first: list which `atexit` handlers were registered before this hook, because `_exit` skips them too. Proof: keep the PR's close-first stress test and restore a variant that exits with the client open and with a close in flight. Assert 0 unclean in 200 runs each, against today's 15 to 37 and about 115.

### D4. idle-7 now depends on one TDLib behaviour that nothing in CI checks

With a 60 s `td_receive`, a clean exit relies on `td_send(0, ...)` making TDLib answer at once. If a TDLib pin bump changes that (`src/pins.rs`, watched by `scripts/tdlib-watch.py`), `stop_within(EXIT_STOP_WAIT)` gives up after 500 ms. Exit then tears TDLib down under a thread still inside `td_receive`, which is the use-after-free from #1022. Before this PR, the 0.2 s timeout bounded that risk. Both tests that prove the wake against the real library are `#[ignore]` and need `QUILL_TDJSON_PATH`, so no CI job runs them.

Fix, at no cost: `stop_shared_before_tdlib_teardown` already sends the wake and calls `receive(0.0)`, then throws the answer away. Keep it instead. If the answer is not the "Invalid TDLib instance" error for client 0, record in a static that the wake does not work, and have the receive source fall back to `0.2` s. This assumes TDLib answers an invalid client id synchronously, inside `td_send`. The PR's own ignored test relies on the same thing, so check it once against the pinned library. Also run `a_send_to_an_unmade_client_ends_a_blocked_receive` in the job that builds TDLib (`scripts/build-tdlib.sh`), so a pin bump that breaks the wake fails there.

## Bug hunt on the areas named in the brief

### A window that freezes when the display link stops: none found

I traced every way a frame can be asked for while the link is stopped, against `third_party/gpui-pre/src/window.rs` and `app.rs`:

- Dirtying a window, `on_next_frame` (`window.rs:2773`) and the throttled branch (`window.rs:1858`) all call `invalidator.wake_platform()`. The macOS waker runs a step whenever `visible && !running`, and `link_pace` does not change that.
- `draw()` now clears `demand`, so a `schedule_frame` raised during paint is dropped. That is safe only because GPUI asks again after the draw from live state: the post-frame check (`window.rs:1928-1945`) and the effect flush (`app.rs:2248-2254`) both re-read `is_dirty` and `next_frame_callbacks`. A future change that asks for a frame only during paint would stall this window. Say so in the comment on `draw` in `third_party/gpui-pre-macos/src/window.rs`, since the current comment only names the effect flush.
- A Start that fails (no screen, or occluded) leaves `running` false, so the waker still works. A missed occlusion change behaves as it did before. A lone draw no longer re-reads `occlusionState` through `start_display_link`, but `window_did_change_occlusion_state` is the source of truth there, so that changes nothing.
- The drawable release keeps one armed timer and re-arms until 3 s without activity. It does not race a restarted link, because it checks `running`.

`link_pace.rs`: 11 of 11 tests pass here. `window.rs` itself could not be compiled on Linux, so the integration rests on reading the code and on the PR's Mac runs.

### A shutdown hang: none found

- `wait_for_client` now waits with no timeout. A stop sets the flag before taking the lock that the wait checks it under, then calls `notify_all`, so it cannot be missed.
- At exit, `stop_within` is bounded at 500 ms. `TdHub::shutdown` joins without a bound, but only test hubs and a hub replaced after its thread already ended get dropped. The shared hub never is.
- The L34 wait is bounded at 30 polls. `close_all_for_quit` is bounded by `QUIT_CLOSE_DEADLINE`.
- What remains is D4: if the wake stops working, exit does not hang, it crashes.

### render-4, L33, L34, L35, idle-8: notes, no defects

- render-4: after a chat switch, the rows from the last paint belong to the old chat until the next paint. A message id that clashes with the new chat over-redraws once, and a new row grades `Later` for at most one poll. Both are harmless. Removing `join` is right: a pending `Conversation` keeps `next_due` at zero, so it is never lost behind a `ChatList` or a `Full`.
- L33: the tick ends when there is neither a call on screen nor an audible ring. The ring stopping is handled by `sync_call_sounds` on the background poll.
- L34: a raise clears `hide_after_full_screen` through `raise_main_window`, so the wait's final hide does nothing, and the tests cover both sides. The flag can only stick if the window handle is gone, and `note_main_window` resets it then.
- idle-8: deleting the poll fixes the root cause (no event, so it polled) rather than moving it to an event, and the PR explains why `orderOut` on a miniaturized window cannot hide it. The yellow button (to the Dock) and Cmd-M (hide, L35) now behave differently on a Mac with the tray on. That follows the owner default the PR cites. It is noted here and not counted.

## What ran

- `bash scripts/test-frame-idle.sh`: frame_idle 17 passed, link_pace 11 passed.
- D1 probe, `cargo test --features demo-capture --bin quill wake_wait`: on 7899df8 the probe fails at its hold assertion (13 passed, 1 failed). With the D1 fix applied locally, all 14 pass, including the PR's `a_backlog_holds_app_nap_off_once_until_the_loop_rests` and `one_update_costs_one_pass_with_no_ramp_behind_it`. The probe and the fix were not pushed.
- Not run: anything against the real libtdjson (none is in this container), Mac builds of `gpui-pre-macos` and `gpui-pre-apple`, Windows, clippy.

## To approve

Fix D1, D2 and D3, and add the D4 fallback or the CI run. Then push B6.md, or confirm that the PR's item list is the whole bundle.
