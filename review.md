CHANGES

# Second review of PR 1305, claude/sweep-sticker-timing at 5b21e6d

Scope: the four commits after 668adfac9, which answer the four findings in review.md on claude/review-sticker-timing.

All four findings are fixed at their root, and each new test fails when its fix is taken out. One thing stops the merge. Main moved after the first review (#1303, redraw-slices), and the branch now conflicts with it in `src/ui/frame_clock.rs` and `src/ui/chat_window/cover_tests.rs`. The rules say to merge origin/main before the last push. Both conflicts are small, and the steps are at the end of this file. Merge main, rerun the tests, and this is an APPROVE.

## What ran

- `cargo test --features demo-capture -- sticker_playback::player:: sticker_player frame_grid frame_clock cover_tests anim_layer layer_draws video_player` at 5b21e6d: 31 passed and 1 ignored (`sticker_store_bench`) in the lib, 233 passed in the app. Nothing failed.
- `cargo fmt --check` and `cargo clippy --features demo-capture --all-targets -- -D warnings`: both clean.
- Each fix taken out on its own, with its test kept:
  - `clip_mut` ignores the id: `a_clip_reopened_while_its_old_decode_runs_keeps_its_new_frames` fails.
  - `content_at` always returns `now`: `emoji_beside_a_clip_on_their_layer_hold_each_frame_alike_at_any_head_phase` fails.
  - The look-ahead goes back to `display_hz()`: `a_tick_draws_a_sticker_frame_due_before_the_main_windows_next_refresh` fails.
  - `workers_decode` is forced to false: `video_stickers_open_when_no_opener_thread_starts` hangs and does not fail. See note 2.
- A scratch frame grid sim, not pushed: 30 fps emoji at 120 Hz, head phases 0 to 3, clip offsets 0, 3, 5, 10 and 20 ms. Clips that keep time (30, 60 and 15 fps) and a 24 fps clip on the emoji's layer held every emoji frame 4 refreshes, at every phase and offset. This covers the `Woke::Later` path that moves the lattice to a clip's vsync, which the PR's test does not reach.
- `git merge-tree` against origin/main at 5a173f5: conflicts. See "Blocking".
- Not run: `--no-default-features` clippy, the Windows check, the full test suite, or anything on a real display. The content grid is macOS only (`display_vsync` returns `None` elsewhere), so the media-10 change does nothing on Linux or Windows. That is correct.

## Findings from the first review

### 1. media-10 at every head phase: fixed (0349666)

The root cause was that a head's frame boundaries could fall between grid ticks. The fix reads main window content at the time of the clock's last tick vsync (`content_now` and `frame_grid::content_time`). A frame then changes only on a tick, and a head starts on a tick, because `head_state.start(now)` now gets the content time. This is the suggested fix in a more general form: it also covers heads that started before the clock went onto the grid. The clip test now changes too. A clip's picture counts as a redraw only when its spacing is a whole multiple of the content frame, or the other way round (`keeps_time`). That closes the 12 fps case as well, which the first review had left for "What stays open". The test runs 24, 25 and 12 fps clips at 4 phases and 4 offsets and expects holds of exactly 4.

Hunted for a freeze or a wrong speed and found none:
- Ticks skipped by render-3 cannot stall content time. `content_time` works out the lattice point at or before the vsync from `at` and `every`. It does not need a tick to have run.
- The lattice is cleared on `Ticked(0)` before the loop breaks, so a stopped clock leaves no snap behind.
- `every` comes from the tick's demand, which is the highest fps any target asked for, so a sticker is never sampled more slowly than it plays. When a sticker first shows while the clock is ticking slower for something else, one first frame can start up to one old tick early. It catches up on the next tick.
- `changes_within` and `fps()` still read real time. Content time is at or behind real time, so at worst a tick draws when it did not need to. It never skips a frame.

### 2. render-3 look-ahead: fixed (5b21e6d)

The look-ahead now comes from the main window's own `refresh_interval`, noted in `note_frame_timing`. When that is unknown it is 16.7 ms, the longest likely refresh, so the skip errs toward drawing. The new cover test steps 1 ms at a time up to the next frame, with a 120 Hz `display_hz`, and fails without the fix.

### 3. No opener thread: fixed (3b9be8e)

When no opener starts, workers decode their own clips through `Job::Decode` and the same `opener::open`, and `next_decode` stands down. The commit also fixes a related defect: clips went to worker indices that might never have started. They now go only to live workers. When no worker starts at all, `open` leaves the clip unopened. `engine.state` then returns `None`, which `Player::frame` reads as failed (`sticker_player/mod.rs:539`), so the view rests and the clock does not spin.

### 4. A reopened clip marked failed: fixed (9e27e09)

Each opening gets an id, and opens, decodes and frames write only to the opening they were handed out for (`State::clip_mut`). Every path the first review named now goes through it: `opener::open`, `worker::open` and `worker::frame`. The test reproduces the race with two openers.

## Blocking

Merge origin/main. The conflicts:
- `src/ui/frame_clock.rs`, the tick loop. Main replaced the `else if !main_covered` branch with a `views` list that filters out covered slices and views a chat window holds. The PR adds the `idle_layer_tick` skip inside that loop. Keep main's filter and put the skip at the top of the new `for id in views` loop. `idle_layer_tick` returns `None` for anything that is not a slice painter, so the new non-slice targets still draw.
- `src/ui/chat_window/cover_tests.rs`: both sides add tests at the same place. Keep both.

Then rerun `cover_tests`, `frame_grid` and `frame_clock`, because main's change sits in the same tick path as render-3.

## Not blocking

1. The frame grid sim does not reset the thread-locals the PR adds (`ANSWERED`, and `VIDEO_ASKS` before it). Each `Sim` starts at `Instant::now()` and runs 3 simulated seconds ahead. So a sim run after a sim with a clip that keeps time, on the same test thread, sees that clip's old answers at its own vsyncs. In one test, a 24 fps clip at head phase 1 run after a 60 fps one held frames for 1, 3 and 7 refreshes. Run alone, it held every frame 4. The app is not affected, because its clock only moves forward and old answers fall behind every later vsync. But the next test that loops over a 60 fps clip and then another clip will fail for no real reason. Clear both in `Sim::new`.
2. `video_stickers_open_when_no_opener_thread_starts` hangs without the fix (`wakes.recv()` waits for good) and does not fail. CI shows that only as a timeout. A failure that names the cause would be better: for example, after the open, assert that a worker has a job for the clip, or have the waker drop its sender when the engine is idle.
