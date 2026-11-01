CHANGES

# Review of PR 1305, claude/sweep-sticker-timing at 668adfa, against main

I read the whole diff and the PR text. Most of the PR holds up. The opener threads, the draw-cost averaging, the grid slack and the render-3 skip all look correct. The tests pass. One finding stops the merge: the media-10 fix works at only one of the four phases an emoji head can start on. At the other three, an emoji beside a 24 or 25 fps clip still shows some frames for a single refresh. The PR's own test does not cover those phases. That finding also explains the 3 and 5 refresh holds that rose beside the 12 fps clip in the replica.

## What ran

- `cargo test --features demo-capture -- sticker_playback::player:: sticker_player frame_grid frame_clock cover_tests anim_layer layer_draws video_player` on the PR head: 29 passed and 1 ignored (`sticker_store_bench`, which needs TGS files) in the lib, 233 passed in the app. Nothing failed.
- A scratch variant of the PR's own frame grid sim, run on both builds and not pushed. It adds `emoji_start`, which starts the emoji's play head 0 to 3 refreshes plus 0.3 ms after the sim's start, at a real 120 Hz with 30 fps emoji. The patch is at the end of this file.
- The branch merges into the current main (5072e96) without conflicts. None of the 16 commits on main since the PR's last merge touch the files this PR changes.
- Not run: clippy, the full test suite, and anything on a real display.

## Findings

### 1. media-10 is fixed only when the emoji head starts on the grid phase (blocking)

- File: `src/ui/frame_grid_tests.rs:966`, the test. The cause is the interaction of `src/ui/frame_clock.rs:343` with the head start at `src/ui/sticker_player/mod.rs:564`.
- What happens: a play head starts on the first paint that shows its frame (`head_state.start(now)`). That paint can be any vsync, for example a draw for a clip, a scroll or typing. It is not always a grid vsync. The PR text accepts this for media-6 ("One head in four starts on that phase"). The new test, `a_clip_off_the_grid_on_the_emojis_layer_never_puts_their_tick_off`, starts the emoji head at the sim's `t0`, which is on the grid. That is the only phase it checks.
- What the scratch sim shows: 30 fps emoji on a 120 Hz grid beside a 24 fps clip on the same layer, offsets 0, 5, 10 and 20 ms. Each cell counts how many frames stayed up for that many refreshes, over 75 frames.

  | Emoji head start | main | this PR |
  | --- | --- | --- |
  | on the grid phase | 2: 15, 4: 45, 6: 15 | 4: 75 |
  | 1 refresh after it | 1: 15, 4: 30, 5: 15, 6: 15 | 1: 15, 4: 15, 5: 45 |
  | 2 refreshes after it | 2: 15, 4: 45, 6: 15 | 2: 15, 4: 30, 5: 30 |
  | 3 refreshes after it | 1: 15, 4: 30, 5: 15, 6: 15 | 3: 15, 4: 45, 5: 15 |

- Failure scenario: a custom emoji appears in the conversation, and its first paint is a layer draw requested by a 24 fps inline video next to it. That draw lands one refresh after a grid vsync. One emoji frame in five then stays up for 8 ms and the next for 42 ms, where 33 ms is right. On main, the 6-refresh holds go away, but the 1-refresh holds stay.
- Why it happens: the emoji's frame boundaries are offset from the grid ticks. The grid tick on G shows the old frame. A draw for the off-grid clip at G+1 or G+2 shows the new frame. Then the grid tick at G+4 draws the same frame again. `counts = false` stops the clip draw from putting the tick off, but it cannot move the boundary back onto the grid.
- On the 12 fps question: in the replica, the holds of 3 and 5 refreshes beside the 12 fps clip (which stands for 24 fps) went from 96 to 282. That is this effect at half speed, so it is a real, smaller stutter and not just noise. A real 12 fps clip at 120 Hz has an even cadence (10 refreshes), so it counts as on the grid and this fix does not touch it. The scratch sim gives the same results on both builds: holds of 2, 3 and 6 on the grid phase, and 1 and 7 one refresh after it. That case is older than this PR, but it should appear under "What stays open".
- Suggested fix: when a head starts during a grid draw, line its start up with the content grid vsync (`ContentGrid::snap`), so that its boundaries fall on grid ticks. Then extend the test to cover four head start phases, as `heads_painted_with_jitter_show_every_frame_once_at_any_phase` already does for media-6.

### 2. render-3 checks one refresh ahead using the rate of whichever window painted a sticker last

- File: `src/ui/app_slice.rs:410`.
- What happens: `idle_layer_tick` uses `1 / display_hz()` as its look-ahead. `display_hz()` is a thread-local value, set by the last window that painted a sticker (`note_display`). The check is safe only if that look-ahead is at least one refresh of the main window's display, because a skipped tick's paint would have landed within one of those refreshes.
- Failure scenario: the main window is on a 60 Hz monitor and a chat window is on a 120 Hz laptop panel, and both play stickers. A main-window tick looks only 8.3 ms ahead. When a frame comes due 8.3 to 16.7 ms after that tick, the tick is skipped, and the frame shows one refresh late: a hold of 3 refreshes followed by 1, instead of 2 and 2. This is plausible but not reproduced. The high-rate choice already shares this thread-local, but there it does not skip any draws.
- Suggested fix: take the refresh interval from the main window's own timing (`clock_display()` or its `refresh_interval`).

### 3. If an opener thread fails to start, video stickers stay in "opening" and keep the clock ticking

- File: `src/sticker_playback/player/engine.rs:198`.
- What happens: workers that fail to spawn are counted in `started`. The result of spawning an opener is discarded.
- Failure scenario: the OS refuses new threads (a thread or memory limit). No opener runs, so every WebM clip stays `ClipState::Opening` for good. Its views ask for `OPENING_FPS` with no end, so the main window keeps drawing at that rate and the sticker never appears. This is rare, but on main the same failure only lost one worker.
- Suggested fix: if no opener starts, fall back to opening decoded clips on the workers, or mark them `Failed` so their views rest.

### 4. Not blocking, and already on main: closing and reopening a clip during a decode can mark the new clip failed

- File: `src/sticker_playback/player/opener.rs:50` and `:62`.
- What happens: `open` finds the clip by key alone. Suppose a clip is closed while it decodes and then opened again with the same key. The cancelled decode returns `Err("Sticker playback cancelled")` and sets the new clip to `Failed`. With several openers, a second opener may already have finished the new clip, and then `Failed` overwrites `Ready`.
- Failure scenario: a WebM sticker scrolls out of view and back in during its 270 ms decode, and stays blank. `worker.rs` on main had the same check, so this is not a regression. An id on each `Clip`, compared before writing the result, would close it.

## Hunted and found clean

- Stickers that freeze, skip or play at the wrong speed: `DrawCost::note` cannot divide by zero (its window is at least 1) and tests from the 8th draw on. The `grid_slack` divisor is never 0, because `note_display` filters it. For 60 fps files and 60 Hz displays, the slack equals the old quarter frame.
- The #1207 tap effect: it is ticked through `tick_app` (target `None`), so the render-3 skip never applies to it. It still ticks at the display rate, and it drops after 8 to 11 draws instead of on the first.
- A window that stops drawing while something moves: the skip only applies to layer painters. It counts the same items as `demand()` does (`seq != 0`, visible, not inline). Paint and Tile content always count as changing. An opening view counts as changing. A view whose borrow fails counts as changing. A view is redrawn after 100 ms (`REPAINT_GAP`), well inside `PAUSE_GAP`. A skipped tick asks for the next tick again, so the clock keeps running. Video tiles are driven by `request_video_frame`, not by this path.
- Opener teardown: `Engine::drop` sets `stop`, cancels every clip and kills its decoder child, then wakes the threads. An opener whose clip is gone puts the store back in `freed` and exits on `stop`. Nothing blocks while it holds the lock. As on main, the threads are not joined.
- A video sticker frame landing in a dropped player: frames and opens look up the clip by key under the lock, and drop the result if the clip has gone. Workers skip clips that are not `Ready` (`next_frame`), so no worker unpacks a store that is half filled.
- Panics: none of the new code in `src` uses `unwrap`, `expect` or slice indexing.
- media-10, a small cost: `clip_redraws_off_grid` sees the off-grid clip's next request as still in flight on every draw of the layer. So while such a clip plays, no draw of that layer puts a tick off, including draws by a second clip that is on the grid. This only costs extra draws.

## Scratch sim patch (not pushed)

```diff
--- a/src/ui/frame_grid_tests.rs
+++ b/src/ui/frame_grid_tests.rs
@@ struct Sim
     emoji_fps: u32,
+    emoji_start: Duration,
@@ Sim::new
             emoji_fps: 0,
+            emoji_start: Duration::ZERO,
@@ emoji draw
-                let along = self.now.duration_since(self.t0);
+                let along = self.now.duration_since(self.t0).saturating_sub(self.emoji_start);
@@
+#[test]
+fn scratch_emoji_phase_beside_12fps() {
+    let mut display = 300;
+    for fps in [12.0, 12.5, 24.0] {
+        for start in [0u32, 1, 2, 3] {
+            for offset in [0u64, 5, 10, 20] {
+                display += 1;
+                let mut sim = Sim::new(display, 120.0, true).emoji(30).clip(fps)
+                    .on_layer(Duration::from_millis(offset));
+                sim.emoji_start = sim.period * start + Duration::from_micros(300);
+                let sim = sim.run(3.0);
+                // histogram of sim.emoji_holds(), printed
+            }
+        }
+    }
+}
```

The main build used the same change, with the old `(along * fps + 0.25).floor()` pick and a copy of `emoji_holds`.
