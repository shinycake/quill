Stickers and emoji play evenly on fast displays, draw the window less often, and keep playing while a video sticker opens

## What was wrong

- media-5: One slow draw demoted a sticker to its file's rate for the rest of the clip's life. The draw cost started from the cold first draw and was tested on every draw, so one 14 ms draw among 0.8 ms ones crossed the 3.33 ms limit of a 120 grid. This fits the 3 of about 24 runs where a normal sticker played at its file's rate (107 window frames a second for 30 pictures).
- media-6: On the content grid, a quarter grid frame of slack is one whole 120 Hz refresh for a 30 fps emoji. A head that started one refresh after its paints' beat sat exactly on a frame boundary at every paint. Paint jitter then skipped a frame and held the next for 67 ms (4 to 11 per 945 frames). One head in four starts on that phase.
- media-10: A 24 or 25 fps clip on the layer of 30 fps emoji redraws the layer between the grid vsyncs the emoji tick on. The clock counted that draw as one that showed the emoji's next frame and put its tick off. That emoji frame then stayed up for 6 vsyncs (50 ms) in place of 4. Not counting those draws fixed it only for a head that started on a grid tick. A head starts on whatever paint first shows its frame (a clip's draw, a scroll), so its frame boundaries can fall on any refresh. Then a clip's draw one or two refreshes after a boundary showed the next frame early, and the next tick held it again: holds of 1 and 5, 2 and 5, or 3 and 5 refreshes in place of 4, at three of the four phases a head can start on. A 12 fps clip, ten refreshes apart at 120 Hz, comes a whole number of grid ticks apart, so its draws still counted and put the emoji's tick off, holding frames 2, 3 and 6 refreshes at every phase. This is the stutter behind the holds of 3 and 5 refreshes that rose beside the 12 fps clip in the replica below.
- render-3: A sticker at its file's rate still ticks at the display's rate, so each frame stays up for the same number of refreshes. Every tick drew the whole window, also on the ticks where nothing changed.
- media-4: A WebM sticker decodes all its frames when it opens (28 to 470 ms). That ran on the sticker worker its clip was pinned to, and a worker took a pending open before any frame. Every sticker and emoji on that worker got no frames until the decode finished.
- Found in review of this branch:
  - The render-3 skip looked one refresh ahead at the rate of whichever window last painted a sticker. With the main window on a 60 Hz monitor and a chat window on a 120 Hz panel, a tick looked only 8.3 ms ahead, skipped a frame due 8.3 to 16.7 ms later, and showed it a refresh late.
  - If no opener thread started (a thread or memory limit), every WebM clip stayed opening for good. Its views kept the clock ticking at the opening rate, and the sticker never showed.
  - Older than this branch: a clip closed during its decode and opened again under the same key took the old decode's result. The cancelled decode marked the new clip failed, also after another opener had made it ready, and the sticker stayed blank (a WebM sticker scrolled out and back in during its decode).

## What changed

- media-5: The draw cost leaves out the first draw, takes the plain mean of the next 8, then a running eighth, and is tested only from the 8th draw on. The 672 px tap effect from #1207 still drops early (draw 8 to 11 in the runs below, about 75 ms at 120 Hz). A demoted clip keeps ticking at the display's rate, as #1207 left it. It stays demoted for the rest of its life, so it does not flip between rates. `QUILL_TRACE_STICKERS` prints the count of demoted clips (`coarse`).
- media-6: The grid slack is a quarter grid frame or half a refresh, whichever is less. The play head now takes the display rate. 60 fps files and 60 Hz displays keep the quarter frame from #1207. The frame grid sim picks emoji frames through the real head.
- media-10: While the frame clock ticks on the content grid, the main window's play heads read the time of the clock's last tick vsync at or before the frame they paint for, not the paint's own time (`frame_grid::content_time`, `frame_clock::content_now`). A frame then changes only on a tick, so a head holds each frame alike whatever refresh it started on. A clip's picture puts the emoji's tick off, and counts as a tick itself, only when the clip keeps time with the emoji: its pictures come steadily a whole number of emoji frames apart, or the other way round (`frame_clock::clip_frame_keeps_time`, from `Cadence::even_spacing`). A 30 fps clip still shares its window frames with the emoji, and a 60 fps one still leaves the clock idle. 24, 25 and 12 fps clips no longer move the emoji's ticks. Off the grid, in chat windows and on displays without vsync times (Windows, Linux), heads read the paint time as before. `LayerDraws` moved to `src/ui/layer_draws.rs`.
- render-3: Before the clock notifies an animation layer, it asks whether a draw before the main window's next refresh would change anything. Clock-driven drawing and live tiles always change. A sticker changes only when its head, worked out on a copy, would pick another frame than its view shows. A layer that would not change keeps ticking without a draw. The refresh is the main window's own, noted as it renders, and 60 Hz when its display does not report one. The check needs no vsync times, so it works on macOS, Windows and Linux. A view unpainted for 100 ms paints anyway, so a slow clip never reaches the 200 ms pause gap.
- media-4: Clips decoded up front open on opener threads, one per worker (`opener.rs`), so as many decodes run at once as before. An opener fills the clip's store, and the clip's worker only unpacks the frames. Workers still open TGS clips, which keep their ThorVG canvas there, but a frame now goes before an open unless 8 frames in a row already did. If no opener thread starts, the workers decode their own clips. Clips go only to workers that started.
- Each opening of a clip has an id. An open, a decode or a frame lands only in the opening it was handed out for, so a reopened clip keeps its own result.

The chat window cover tests compare draw counts between two phases. Their sticker heads read wall time while the frame clock ran on the executor's simulated time, so with the render-3 skip the counts depended on how fast the machine ran. Their sticker player now follows the executor's clock (`Player::follow`, test only).

## Measurements

All numbers in the tables are relative: fastrel builds of main at 021d736f1 and of this branch at 668adfa (before the review fixes), both with local trace hooks, in alternating runs on a 60 Hz Mac. Compare instructions, since the same work ran at anywhere from 1.5 to 3.1 GHz and CPU percent moved with the clock. The review fixes were made in a Linux cloud container with no display, so these runs were not repeated on the final head.

No number here comes from a 120 Hz display. The 120 Hz cases ran on a half-speed replica: the 60 Hz display with the fine grid forced on, a 15 fps file standing for 30 fps and a 30 fps effect for 60 fps, with the demotion limit scaled to match. Ticks, slack and look-ahead are the same in refreshes. Window frames a second come out at half the real rate, paint jitter is half as large against a refresh, and the workers carry half the load.

render-3 and the demoted clip's tick, on the replica, 4 runs of 8 s each on a quiet Mac (load 3 to 4):

| | main | ticking at twice the file's rate | this branch |
| --- | --- | --- | --- |
| Window frames a second | 57 to 59 | 15 to 19 | 16 to 30 |
| Pictures a second | 14.9 | 14.8 | 14.9 |
| Instructions, M/s | 247 to 257 | 88 to 103 | 96 to 147 |
| Cycles, M/s | 351 to 416 | 128 to 164 | 140 to 241 |
| Frames held exactly 4 refreshes | 91 to 98% | 79 to 88% | 92 to 100% |

An earlier commit here ticked a demoted clip at twice its file's rate. The middle column is why that is gone: the clock runs free at 92% of a frame, so at that rate ticks beat against the frames' starts. The wakes it saved cost almost nothing (at about 15 draws a second, 32 and 61 wakes a second both came to about 95 M instructions a second), so the skip keeps nearly all of the gain. A window draw costs about 3.6 M instructions (fit from these runs), and a picture costs one or two draws depending on the head's phase, because a tick can draw one refresh before a frame changes. That phase is the 16 to 30 spread. On a real 120 Hz display expect about double these frame rates.

media-5, demotion: `coarse` stayed 0 on both builds across 48 launches each, 24 quiet and 24 beside a `cargo clippy` run (load 4 to 5), with the display reported as 120 Hz. The slowest draw was 6 ms. The 3 in 24 did not reproduce on this Mac, so this fix rests on its tests.

#1207 tap effect, on the replica, 3 runs each: main demoted on the cold first draw and this branch at draw 8 to 11. After that frames hold 2 refreshes: 172 to 176 of about 182 frames here, the others mostly the 5 to 7 fine grid frames before the drop, against 175 to 176 of about 179 on main. Late paints were 1 on each. At the shipped 60 Hz rate every frame holds 1 refresh, apart from the stall below. Captures mid-effect look the same on both builds. The effect asks for ticks at the display's rate, which the content grid does not fit, so the clock ticks it off the grid and the media-10 change leaves its heads on paint time.

The tap effect ticks the whole app from outside the layers, so render-3 does not cover it: 60 draws a second for 30 pictures on the replica on both builds.

media-6, on the replica (15 fps emoji beside a 15 fps clip on a forced grid), 16 s runs at four start phases, 28 runs per build: holds of 8 refreshes, the replica's 67 ms, went from 36 of 4733 frames to 2 of 4759, and skipped frames from 24 to 2. On main all 36 came at one phase, in 2 of its 7 runs. Here that phase had none, and the 2 came in one run at another phase.

media-10, on the replica at 668adfa (15 fps emoji beside a 12 or 12.5 fps clip on a forced grid), 3 runs of 16 s: holds of 6 refreshes, the replica's 50 ms, went from 141 to 9 of 715 frames beside the 12 fps clip and from 119 to 7 of 715 beside the 12.5 fps clip. Holds of 3 or 5 refreshes rose from 96 to 282 beside the 12 fps clip while holds of 1 or 2 fell from 143 to 50. That rise is the head phase effect above, which the final head fixes; the replica was not run on it.

media-10 on the final head, in the frame grid sim: 30 fps emoji at 120 Hz beside a clip on their layer, the emoji's head started 0 to 3 refreshes plus 0.3 ms after the sim's start, clip offsets of 0, 5, 10 and 20 ms, 3 s each. Each cell counts emoji frames by how many refreshes they stayed up, over the four offsets:

| Clip, head phase | 668adfa | final head |
| --- | --- | --- |
| 24 fps, on the grid phase | 4: 300 | 4: 300 |
| 24 fps, 1 refresh after | 1: 60, 4: 60, 5: 180 | 4: 300 |
| 24 fps, 2 refreshes after | 2: 60, 4: 120, 5: 120 | 4: 300 |
| 24 fps, 3 refreshes after | 3: 60, 4: 180, 5: 60 | 4: 300 |
| 25 fps, on the grid phase | 4: 300 | 4: 300 |
| 25 fps, 1 refresh after | 1: 49, 4: 98, 5: 150 | 4: 300 |
| 25 fps, 2 refreshes after | 2: 50, 4: 148, 5: 101 | 4: 300 |
| 25 fps, 3 refreshes after | 3: 51, 4: 198, 5: 51 | 4: 300 |
| 12 fps, on the grid phase | 2: 40, 3: 19, 4: 190, 6: 50 | 4: 300 |
| 12 fps, 1 refresh after | 1: 20, 2: 40, 3: 9, 4: 160, 5: 10, 6: 37, 7: 20 | 4: 300 |
| 12 fps, 2 refreshes after | 2: 40, 3: 20, 4: 190, 6: 46 | 4: 300 |
| 12 fps, 3 refreshes after | 1: 20, 2: 40, 3: 10, 4: 160, 5: 10, 6: 40, 7: 20 | 4: 300 |

Beside 30, 15 and 60 fps clips on the layer, which keep time with the emoji, every frame held 4 refreshes at all four phases too (296 to 300 frames a cell).

The render-3 look-ahead, in the new app test (a 60 Hz main window, a sticker painted last at 120 Hz): before the fix a tick drew only in the last 9 ms before a frame came due, and skipped from 16 ms to 10 ms before it. Now every tick in the last 16 ms draws.

media-4, with a 3 s 512 px VP9 clip that decodes in about 270 ms, 5 runs each:

| | main | this branch |
| --- | --- | --- |
| One forced worker, 15 tiles hovered: late paints | 49 to 72 | 0 |
| Same: longest hold of the clips playing beside it | 186 to 238 ms | 50 to 88 ms |
| 3 workers, five opens in 160 ms: late paints | 19 to 22 | 0 |
| Same: longest hold of the clips playing beside it | 448 to 518 ms | 67 to 83 ms |
| Same: first paint of the last clip hovered | 430 to 499 ms | 446 to 522 ms |

A first version with a single opener thread took 388 to 1170 ms on that last row, which is why there is now one per worker. With one forced worker a first frame took the same on both builds (WebM median 303 and 296 ms, TGS 29 ms on each), so `FRAMES_BEFORE_OPEN` costs nothing visible here. A 50 ms decode stays inside the ready queue of a 30 fps clip: 1 late paint in 10 runs, on main.

Controls at the real 60 Hz with no switches, 4 runs each, show no measurable difference:

| Scene | Window frames a second | Instructions, M/s |
| --- | --- | --- |
| Sticker playback | 28.5 to 28.6, here 28.4 to 28.6 | 206 to 217, here 206 to 219 |
| Video, emoji and stickers | 41.7 to 44.3, here 42.6 to 45.1 | 350 to 364, here 352 to 372 |
| 60 fps sticker | 56.9 to 60.0, here 57.8 to 59.6 | 309 to 322, here 314 to 321 |

One 60 fps run here held 47 frames for 2 refreshes while painting every refresh. Nothing in this branch changes that path at 60 Hz (no content grid there), so it looks like a head started close to a frame boundary, though that is not proven.

## Tests

- On the final head: `cargo fmt --check`, `cargo clippy -- -D warnings` with `--features demo-capture --all-targets` and with `--no-default-features --all-targets`, `scripts/check-hotspots.sh` and `scripts/check-file-size.sh`, all clean.
- On the final head: `cargo test --lib sticker_playback::player::` (31 passed, 1 ignored: `sticker_store_bench`, which needs TGS files), the sticker player, frame grid, frame clock, animation layer, layer draws and video player tests in the app crate (233 passed), and the chat window cover tests (9 passed).
- On the final head: the full `cargo test --features demo-capture --no-fail-fast`. Everything passed apart from the two tests that make a folder read-only with chmod 0555 (`atomic_write_failure_keeps_old_file` and `a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`), which fail as root in the cloud container.
- New tests. Each fails without its fix, and none sleeps or waits on a timeout or the wall clock:
  - media-5: `a_cold_first_draw_keeps_a_fast_clip_on_the_fine_grid` and `one_slow_draw_keeps_a_fast_clip_on_the_fine_grid` failed on the old averaging. `a_clip_slow_from_the_start_drops_to_its_files_rate_within_16_draws` uses the tap effect's draw times.
  - media-6: `heads_painted_with_jitter_show_every_frame_once_at_any_phase` uses four start phases and seeded paint jitter of up to 1.5 ms through the real `head::pick`. With the old slack, phase 1 skipped or repeated on 948 of 1998 paints. Now it does on none.
  - media-10: `emoji_beside_a_clip_on_their_layer_hold_each_frame_alike_at_any_head_phase` in the frame grid sim runs all four head phases beside 24, 25 and 12 fps clips at four offsets, and asks every emoji hold to be 4 refreshes. At 668adfa it failed first at 24 fps, phase 1 (holds 5, 4, 1, 5, 5, ...). The sim now runs the clock's tick lattice, the content time and the clip pictures that keep time, through the app's own functions. The existing sim tests for sharing window frames with a 30 fps clip, a 60 fps clip leaving the clock idle, and a clip starting and stopping beside the emoji pass unchanged.
  - render-3: `a_clip_at_its_files_rate_paints_only_for_its_own_frames` models a 30 fps and a 60 fps clip with a clock running free at 92% of a frame, each tick asking about the next refresh. Exactly the clip's frames show, each for the same number of refreshes, on far fewer draws. At twice the file's rate its holds come out 3, 4 and 5. `a_clip_on_the_fine_grid_paints_every_refresh` and `a_slow_clip_paints_often_enough_to_keep_playing` are also new. `a_tick_draws_a_view_only_when_its_frame_changes` covers the real `StickerView` and fails when every tick draws. `a_tick_draws_a_sticker_frame_due_before_the_main_windows_next_refresh` opens the demo with two looping stickers, notes a 120 Hz window as the last to paint a sticker, and steps the player's clock 1 ms at a time: every tick in the last 16 ms before a frame must draw.
  - media-4: `a_clip_decoding_holds_back_no_frames_of_other_clips` uses one worker and holds a WebM decode open, and another clip's frame still arrives. On the old code the decode ran on `quill-sticker-0`. `clips_decoding_at_once_do_not_wait_for_each_other` needs two decodes running together and hangs with a single opener. `video_stickers_open_when_no_opener_thread_starts` starts the engine with a spawner that refuses the opener threads, and hangs without the fallback (it blocks on the waker's channel; stopped here by a 30 s shell timeout). These tests wait on channels, a barrier and the engine state.
  - Reopening: `a_clip_reopened_while_its_old_decode_runs_keeps_its_new_frames` closes a WebM clip while its decode is held, opens it again, lets the second opener finish it, then lets the old decode return cancelled. Without the ids the clip ended `Failed("Sticker playback cancelled")`.

## What could not be verified

- Anything on a real 120 Hz display: window frames, CPU (the 107 to 30 and the 28% from the finding) and frame holds for render-3, media-5, media-6, media-10 and #1207. The replica numbers stand in for them, and the replica was not run on the final head: the media-10 content time and the render-3 look-ahead rest on the frame grid sim and the app tests.
- That the display link's present time read while painting is the vsync of the frame being painted, which content time and the layer draw bookkeeping both assume, as the content grid did before.
- media-5's 3 in 24 demotions, which did not happen on the Mac on either build.
- media-4 with real sticker packs. The clip was synthetic, and no account was used.
- A machine that refuses new threads. The opener fallback ran only with a test spawner.
- Windows and Linux high refresh displays. They have no vsync times, so the content grid and content time stay off there. The render-3 look-ahead is covered by the shared code and the tests.

## What stays open

- In about half of the 3 s tap effects, on main and here alike (5 of 9 runs each), a play misses one refresh, paints, then misses two. Window frames stop for those refreshes although each takes 3 ms. Its cause is not known.
- The tap effect's ticks draw every refresh: it ticks the whole app, outside the layers render-3 checks.
- A WebM open still holds all of its raw frames at once (up to 58 MB at 448 px, inferred). Streaming WebM frame by frame through the shim, as tdesktop's `Media::Clip` does, would remove that.
- Play heads still read a paint's wall time, on the grid moved back to the tick's vsync. Feeding them the frame's target vsync would remove the paint jitter that media-6 works around.
- render-3 asks about the whole next refresh, so a tick can draw one refresh before a frame changes. With vsync times this could be exact on macOS.
- The fine grid choice and the grid slack of a head still read the refresh rate of the window that last painted a sticker. With windows on displays of different rates, a head can pick its frames for the other display's rate. No skipped draw comes of it.
