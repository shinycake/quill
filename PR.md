Fix the open review findings on idle-and-redraw: App Nap gaps, caret blinks on X11 and Windows, and crashes on quit

## What was wrong

The review of PR 1312 (review.md on claude/review-idle-and-redraw) left four defects open, plus one comment it asked for.

- D1 (N79). In `run_poll_loop`, a poke that ended a rest early left its update waiting out the poll gap on a plain timer with no App Nap hold. App Nap runs that timer up to 200 ms late. This happens whenever updates arrive a little slower than each pass drains them, for example a TDLib backlog after a reconnect, or a busy group while Quill is hidden.
- D2 (idle-3). On X11 and Windows, a frame waker demand on a parked frame source restarted it at the refresh rate for the whole 250 ms grace. So every caret blink in a focused composer ran 15 frames at 60 Hz, or 36 at 144 Hz. macOS already ran one.
- D3. Exit with a TDLib client open or closing aborted in TDLib's static destructors. The PR measured 15 to 37 aborts in 200 exits with a client open, and about 115 in 200 with a close in flight. A quit that runs past `QUIT_CLOSE_DEADLINE` reaches this state, and on macOS the user then sees a crash report for a quit they asked for.
- D4 (idle-7). With a 60 s `td_receive`, a clean exit depends on TDLib answering at once when Quill sends to a client id it never made. No CI job checked that against the real library.
- The macOS `draw` drops a `schedule_frame` raised during paint, and its comment did not say why that is safe.

## What changed

- D1: after a rest, a gap that is not zero takes the App Nap hold before its timer. A zero gap arms no timer at all.
- D2: `FrameIdle` (`frame_idle.rs`, the same file in both crates) now follows the `LinkPace` rule. A demand on a parked source gets one frame. That frame parks the source again, unless GPUI asked for another frame during it, or it came within 100 ms of the last woken frame that drew. A demand while a woken frame is still due asks for that frame again, so no demand is lost. On X11, the refresh timer clears what GPUI asked for before a woken frame. On Windows, a demand now invalidates the window once and leaves the vsync gate parked. A deferred re-entrant draw unparks the gate outright (`keep_frames_running`), because only a vsync invalidation retries that draw. Unlike macOS, which clears the demand in `draw`, both platforms clear it before the frame request, so a `schedule_frame` raised during paint still counts. The `link_pace` tests are ported, and the random simulation is rewritten for woken frames.
- D3: new `src/telegram/hub/exit.rs`. The hub records every client id it makes and drops it when the receive loop reads `authorizationStateClosed` for it. The exit hook already runs before TDLib's destructors. After it stops the receive thread, if any client is not confirmed Closed, it removes the scratch directories and leaves through `_exit`. The exit status comes from `set_exit_status`, which the developer tools (`--connect-smoke` can exit with a client open) and the window failure exit now call. The atexit check: Quill registers two other hooks. The call reaper registers at quit, so it has already run, and it uses `_exit` itself. The scratch directory hook can be registered earlier, so the exit hook runs it first. I did not list atexit hooks that third-party libraries register.
- D4: the wake probe at the first connect now keeps TDLib's answer. If the answer is not the error for an unknown instance, `td_receive` goes back to 0.2 s calls. The Linux package job, which builds the pinned TDLib, now runs `a_send_to_an_unmade_client_ends_a_blocked_receive` and the live exit tests against it.
- The comment on the macOS `draw` now explains why dropping a frame asked for during paint is safe, and what change would break it.
- One more commit fixes the test build on Linux: the owner's sweep-all tip removed an import that only the non-macOS decoder test uses.

## What ran

- `cargo test --features demo-capture --bin quill wake_wait`: 16 passed. With the D1 fix reverted, `a_poke_after_a_short_rest_waits_its_gap_under_the_hold` fails at its hold assertion.
- `bash scripts/test-frame-idle.sh`: frame_idle 26 passed, link_pace 11 passed, and the two copies are identical. `a_caret_blink_costs_one_frame_per_blink` models 120 blinks at 500 ms and gets 120 frames and no resume. The old rule ran each blink for the full 250 ms.
- `cargo test --lib telegram::hub`: 18 passed. The child process tests stand in for TDLib's aborting teardown with an atexit hook registered earlier. With a client open, the child ends with the quit status. With every client closed, the teardown runs. With the `_exit` disabled, the client-open test fails (the teardown ran).
- `cargo check` and `cargo clippy -D warnings` for `gpui-pre-windows --target x86_64-pc-windows-gnu`. `cargo clippy -p gpui-pre-linux -D warnings`.
- `cargo fmt --check`. `cargo clippy -D warnings` with `--features demo-capture --all-targets` and with `--no-default-features --all-targets`.

## What did not run

- Nothing ran against the real libtdjson, because none is in this container. The live exit tests (`live_exit_with_a_client_open_stays_clean`, `live_exit_with_a_close_in_flight_stays_clean`, `live_exit_after_close_stays_clean`) and the wake test were not run, so the before and after abort counts for D3 are not measured here. The new CI step runs them with 50 exits each. Locally, use `QUILL_TDJSON_PATH=<libtdjson> cargo test --lib live_exit -- --ignored --nocapture` for 200 each.
- No X11 session or Windows machine was available. The D2 frame counts come from the shared decision model, not from a running window. Also check that a 12 fps timed animation and typing still look smooth there.
- The macOS crates were not compiled. The only macOS change is the comment.

## What stays open

- review.md reports that `sweep-briefs/bundles/B6.md` is missing from claude/sweep-briefs. It is still missing, so I checked only the items PR 1312 names. The owner should push B6.md, or confirm that the PR's list is the whole bundle.
