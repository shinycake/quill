Stickers and emoji play evenly on fast displays, draw the window less often, and keep playing while a video sticker opens

## What was wrong

- **media-5.** One slow draw demoted a sticker to its file's rate for the rest of the clip's life. The draw cost started from the cold first draw and was tested on every draw, so one 14 ms draw among 0.8 ms ones crossed the 3.33 ms limit of a 120 grid. This explains the 3 of about 24 runs where a normal sticker played at its file's rate (107 window frames a second for 30 pictures).
- **media-6.** On the content grid, a quarter grid frame of slack is one whole 120 Hz refresh for a 30 fps emoji. A head that started one refresh after its paints' beat sat exactly on a frame boundary at every paint. Paint jitter then skipped a frame and held the next for 67 ms (4 to 11 per 945 frames). One head in four starts on that phase.
- **media-10.** A 24 or 25 fps clip on the layer of 30 fps emoji redraws the layer between the grid vsyncs the emoji tick on. The clock counted that draw as one that showed the emoji's next frame and put its tick off. That emoji frame then stayed up for 6 vsyncs (50 ms) in place of 4.
- **render-3.** A sticker at its file's rate still ticks faster than its frames change, so each frame stays up for the same number of refreshes. Every tick drew the whole window, also on the ticks where nothing changed.
- **media-4.** A WebM sticker decodes all its frames when it opens (28 to 470 ms). That ran on the sticker worker its clip was pinned to, and a worker took a pending open before any frame. Every sticker and emoji on that worker got no frames until the decode finished, and queued opens ran back to back.

## What changed

- **media-5.** The draw cost leaves out the first draw, takes the plain mean of the next 8, then a running eighth, and is tested only from the 8th draw on. The 672 px tap effect from #1207 still drops by its 9th draw (about 75 ms at 120 Hz). A demoted clip now ticks at twice its file's rate, capped at the grid: a 30 fps file ticks 60 times a second, a 60 fps file still 120. `QUILL_TRACE_STICKERS` prints the count of demoted clips (`coarse`).
- **media-6.** The grid slack is a quarter grid frame or half a refresh, whichever is less. The play head now takes the display rate. 60 fps files and 60 Hz displays keep the quarter frame from #1207. The frame grid sim picks emoji frames through the real head.
- **media-10.** Clip frame requests say whether the clip draws on the content grid. Only a layer draw for a clip on the grid puts the emoji tick off. `LayerDraws` moved to `src/ui/layer_draws.rs` so `frame_clock.rs` stays under the 1,000 line limit.
- **render-3.** Before the clock notifies an animation layer, it asks whether a draw before the next refresh would change anything. Clock-driven drawing and live tiles always change. A sticker changes only when its head, worked out on a copy, would pick another frame than its view shows. A layer that would not change keeps ticking without a draw. Frames still land on ticks, so they keep their even holds. The check needs no vsync times, so it works on macOS, Windows and Linux. A view unpainted for 100 ms paints anyway, so a slow clip never reaches the 200 ms pause gap.
- **media-4.** Clips decoded up front open on one opener thread (`opener.rs`) that fills their store. The clip's worker only unpacks the frames and needs no source for them. Workers still open TGS clips, which keep their ThorVG canvas there, but a frame now goes before an open unless 8 frames in a row already did.

The chat window cover tests compare draw counts between two phases. Their sticker heads read wall time while the frame clock ran on the executor's simulated time, so with the render-3 skip the counts depended on how fast the machine ran. Their sticker player now follows the executor's clock (`Player::follow`, test only).

The #1207 behavior and its tests stay: a clip at its file's rate still ticks evenly, and `jittered_paints_hold_each_frame_for_the_same_number_of_refreshes` and the coarse threshold test pass unchanged. No new timers: every wait stays on the frame clock.

## What ran

- `cargo fmt`, and `cargo clippy -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`, after each commit and after the merge of main.
- `scripts/check-file-size.sh` and `scripts/check-hotspots.sh`.
- New tests. Each fails without its fix, checked by running it against the old code:
  - media-5: `a_cold_first_draw_keeps_a_fast_clip_on_the_fine_grid` and `one_slow_draw_keeps_a_fast_clip_on_the_fine_grid` failed on the old averaging. `a_clip_slow_from_the_start_drops_to_its_files_rate_within_16_draws` uses the tap effect's draw times. `a_coarse_clip_ticks_twice_per_file_frame`.
  - media-6: `heads_painted_with_jitter_show_every_frame_once_at_any_phase` uses four start phases and seeded paint jitter of up to 1.5 ms through the real `head::pick`. With the old slack, phase 1 skipped or repeated on 948 of 1998 paints. Now it does on none.
  - media-10: `a_clip_off_the_grid_on_the_emojis_layer_never_puts_their_tick_off` in the frame grid sim. Before the fix, emoji holds were 2, 4, 4, 6, 4 vsyncs over and over. Now every hold is 4, at four clip phases for both 24 and 25 fps.
  - render-3: `a_clip_at_its_files_rate_paints_only_for_its_own_frames` models a 30 fps clip at its file's rate, ticked every 120 Hz refresh, with each tick up to a refresh before its paint. It drew 120 times a second before and 38 now. Exactly 30 frames show, each for 4 refreshes. `a_clip_on_the_fine_grid_paints_every_refresh` and `a_slow_clip_paints_often_enough_to_keep_playing` are also new. `a_tick_draws_a_view_only_when_its_frame_changes` covers the real `StickerView` and fails when every tick draws.
  - media-4: `a_clip_decoding_holds_back_no_frames_of_other_clips` uses one worker and holds a WebM decode open. Another clip's frame still arrives meanwhile. On the old code the decode ran on `quill-sticker-0`. The test passed 20 runs in a row. Like the other new tests, it waits on channels and the engine state, with no sleep or timeout.
- The full `cargo test --features demo-capture` before the last merge of main. Everything passed except the two tests that make a folder read-only with chmod 0555 (`atomic_write_failure_keeps_old_file` and `a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`). They fail as root in this container. The run also caught the cover tests above, fixed in their own commit and passing 4 runs in a row. After the last merge: the player, sticker, frame grid, frame clock, cover, animation layer and video player tests.

## What did not run

This cloud session has no 120 Hz display, no Mac and no Telegram account, so none of the on-device numbers were measured. The Mac A/B still needs to measure these:

- render-3 and media-5: window frames a second for the `ready-sticker-playback` sticker with a forced coarse flag (expect 107 to about 30), and CPU against the 28% measured with a sticker ticking at 120.
- media-5: the `coarse` count in `QUILL_TRACE_STICKERS` across 24 launches of `ready-sticker-playback` (expect 3 of about 24 to 0). Re-run the #1207 tap effect trace and check its frames still hold 2 refreshes each, since a demoted 60 fps clip still ticks at 120.
- media-5: a demoted 30 fps clip now ticks at 60 on a 120 Hz display, off the vsync grid without display link times. Check on the Mac that its frames still hold 4 refreshes each.
- media-6: the `a11y` emoji pace recipe across 12 runs (expect the 67 ms holds to go from 4 to 11 per 945 frames to 0).
- media-10: emoji holds beside a 24 or 25 fps clip on #1252's setup (expect 50 ms holds to go from 36 to 45 of about 235 back to the 5 to 11 seen before the grid).
- media-4: the `QUILL_TRACE_STICKERS` late count while a video pack opens, before and after.

Windows and Linux high refresh displays were not run either. Those paths are covered by the shared code and the tests.

## What stays open

- A WebM open still holds all of its raw frames at once (up to 58 MB at 448 px, inferred), and opens still queue on the one opener thread. Streaming WebM frame by frame through the shim, as tdesktop's `Media::Clip` does, would remove both.
- Play heads still read the paint's wall time. Feeding them the frame's target vsync would remove the paint jitter that media-6 works around.
- render-3 asks about the whole next refresh, so a tick can draw one refresh before a frame changes. In the model that is 8 extra draws a second for a 30 fps clip. With vsync times this could be exact on macOS.
- A demoted clip stays demoted for the rest of its life. That is unchanged and on purpose, so a clip does not flip between rates.
