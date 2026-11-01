# sticker-timing

Start a new branch claude/sweep-sticker-timing from main. Fix each finding below. They touch the streaming sticker player and the frame clock. Keep the behavior and tests of #1207 (coarse rate plus grid slack, which fixed a tap effect stutter). Any wait on the frame clock stays there, with no new timers. A/B on a Mac happens after you finish, so say which numbers still need it. Text branch name: claude/prtext-sticker-timing.

## render-3: A sticker demoted to its file's rate still draws the window 120 times a second
- Severity: P2 measurable waste
- Kind: cpu, power
- Platforms: macOS confirmed. Windows and Linux on displays above 61 Hz by the same code.
- Where: `src/ui/sticker_player/view.rs:105-125`: `display_high` keeps `timeline.tick_fps(true)` for a clip `engine.coarse(&key)` holds to its file rate. Coarse comes from `src/sticker_playback/player/engine.rs:83` `too_slow_for_grid`. The fps reaches the clock through `src/ui/anim_layer.rs:240` and `tick_animation_layer`.
- Trigger: a sticker whose frames cost more than 40% of a grid frame, on a 120 Hz display.
- Effect and size: measured in `memory-sweep-2026-10-29` notes: 107 frames/s for 30 pictures/s, demoted in 1 of 14 and 3 of about 24 runs. Inferred: three of every four draws paint the same picture, so about 75% of that clip's window draws are waste, with the per-tick cost of render-1.
- Why it is real: the comment says this is on purpose, "so each of its frames stays up for the same number of refreshes". That goal holds; the cost is a window draw per refresh when only the timer needs to run at that rate. `frame_grid::clock_tick` could pace it but `tick_wait` skips the grid unless a clip shares the display ("Alone, content keeps its own timing"), and `display_vsync` is macOS only.
- Fix sketch: keep the clock ticking at the display rate, but do not notify the painter on a tick where nothing in its layer changes. Before `app.notify(painter)` in the clock loop, ask the layer `changes_at(now)`: true for any `Paint` or `Tile` item, and for a `Frames` item whose `head.advance` on a copy (as `fps()` does) picks a different index from the one last shown. Frames stay up an even number of refreshes because they still land on display-rate ticks, and it needs no vsync data, so it works on all three systems. Force a draw at least every 100 ms so a very slow clip never crosses `PAUSE_GAP` (200 ms) and reads as paused.
- Proof plan: deterministic test on `Timeline` and a fake head: a coarse 30 fps clip at 120 ticks gives exactly 30 changes a second, evenly spaced at 4 ticks, and a fine clip changes every tick. Then the demo's sticker with a forced coarse flag: frames/s 107 → about 30 with the same picture sequence.
- Scope: `src/ui/sticker_player/view.rs`, `src/ui/anim_layer.rs`, `src/ui/frame_clock.rs`, `src/ui/app_slice.rs`. Medium: must not reintroduce the #1207 stutter, so check the tap effect on a 120 Hz display.


## media-4: Opening a video sticker blocks a sticker worker, and every clip on that worker stops getting frames
- Severity: P2 measurable waste
- Kind: stall
- Platforms: Linux and Windows through the shim. macOS with a system `ffmpeg`, since releases ship neither the shim nor the tools (`scripts/build-ffmpeg.sh:12`). Without one, WebM clips fail and show their still, which is the known open gap.
- Where: `src/sticker_playback/player/worker.rs:83` runs `Origin::Decoded`'s decode-all on the worker thread. `engine.rs:458-467` `next_job` always takes a pending open before any frame. Clips are pinned to a worker (`engine.rs:206-214`), which ThorVG needs only for Lottie canvases.
- Trigger: open the emoji or sticker panel on a video pack, or scroll a chat of video stickers, while anything animates on screen.
- Effect and size: measured in `sweep-webm-stickers.md`: 77 ms for a 512 px 3 s sticker at box 224, 28 ms at box 56. At 448 px on a 2x screen, likely twice that or more (guess). The `ffmpeg` fallback measured 280 to 470 ms per clip. During each open, every Lottie sticker and emoji on that worker gets no frames, so at 120 fps one open costs 10 to 50 held frames for a half or a third of what plays. Queued opens run back to back: 40 emoji at 28 ms over 3 workers is about 370 ms with no frames at all (inferred). Each open also holds up to 58 MB of raw frames (72 × 800 KB at 448 px) at once on that worker (inferred).
- Why it is real: `MAX_WANTED` and the store help drawing, not opening. `docs/decisions/sticker-player.md` says late frames come from clip opening, with a Lottie open of a few ms in mind. A WebM open is one to two orders larger. `Kind::Stored` holds no canvas, so nothing pins this work to a worker.
- What tdesktop does: video stickers and emoji stream through `Media::Clip`, frame by frame, with no decode-all step.
- Fix sketch: run `Origin::Decoded` on a separate one-thread opener. On completion it installs the store and the worker registers a `Kind::Stored` source. In `next_job`, let frames win when both are ready. Longer term, stream WebM through the shim so the raw frames never pile up.
- Proof plan: engine test with `workers = 1`: clip A ready and wanting frames, clip B's `Origin::Decoded` blocked on a channel the test holds. Assert A's frame arrives while B is open, then release B. Deterministic on `Engine::idle`, no sleeps. `QUILL_TRACE_STICKERS` late count with a video pack before and after.
- Scope: `src/sticker_playback/player/{engine,worker}.rs`, tests. Medium risk: thread ownership of `Source`.


## media-5: One slow draw demotes a sticker to its file rate for the rest of the clip's life
- Severity: P2 measurable waste
- Kind: cpu
- Platforms: any display over 61 Hz: mainly macOS 120 Hz, also Windows and Linux high-rate screens.
- Where: `src/sticker_playback/player/engine.rs:68-77` `note_draw`. The first sample seeds `draw_cost` (`if self.draw_cost.is_zero() { draw }`), it is tested at once, and `self.coarse |=` never clears. The threshold is `COARSE_SHARE / grid` (`engine.rs:93`): 3.33 ms on a 120 grid. Wall time is measured in `worker.rs:145-154`. The waste follows in `view.rs:105-124`: a coarse clip still returns `tick_fps(display_high)`.
- Trigger: frame 0 is the cold draw, and the doc says opening and drawing it "takes a few ms" (`sticker-player.md`). A clip that opens while the machine is busy, or a worker preempted for about 10 ms in the first loop, trips it: from a 0.8 ms average, one 14 ms sample lifts the smoothed cost past 3.33 ms.
- Effect and size: this answers the unexplained 3 of about 24 runs. The bench p90 at 448 px is 1.46 ms, so normal draws never demote; only the cold or preempted sample does, which is why it looks random. The cost measured on #1252: 107 window frames/s for 30 pictures/s. A rough size from the same runs, different scenes: 28% CPU with a sticker ticking at 120, against 10 to 13% for a clip plus emoji. The sticker also loses its fine grid, half its motion, until the clip closes.
- Why it is real: the test (`a_clip_too_slow_for_the_fine_grid_drops_to_its_files_rate`) checks `too_slow_for_grid` with fixed costs and never `note_draw`. #1207 tuned this for the 672 px tap effect (median 2.6 ms, p90 6.0 ms), which trips within a few frames anyway.
- Fix sketch: skip frame 0, require about 8 samples before testing, and compare a slower average (1/8 weight), so a real effect still drops inside its first 100 ms and a spike cannot. Separately, a coarse clip does not need 120 ticks: tick at twice its file rate, capped at the display. A 30 fps file then ticks 60, each picture two ticks, and a 60 fps file keeps the 120 that #1207 measured as even.
- Proof plan: unit test on `note_draw`: 6 ms then 0.8 ms × 200 stays fine, one 14 ms spike stays fine, a run of 2.6 to 6 ms samples goes coarse within 16 samples. Demo: add the coarse clip count to the `QUILL_TRACE_STICKERS` line and read it across 24 launches of `ready-sticker-playback`, before and after.
- Scope: `src/sticker_playback/player/engine.rs`, `src/ui/sticker_player/view.rs`, tests. Low risk. Re-run the tap effect trace from #1207.


## media-6: On the content grid a play head can sit exactly on a frame boundary, so paint jitter skips a frame and holds the next for 67 ms
- Severity: P2 measurable waste
- Kind: stall
- Platforms: macOS at 120 Hz. The grid needs display link times; without them phases drift as before.
- Where: `src/sticker_playback/player/head.rs:18` `GRID_SLACK = 0.25` and `:169` `floor(pos × grid_fps + GRID_SLACK)`. `view.rs:58` passes `player.now()`, the wall time of the paint, not the vsync the frame targets. Reached from `head.start(now)` (`sticker_player/mod.rs:503`), which sets the phase at whatever paint first found the frame.
- Trigger: 30 fps emoji beside a clip on the grid at 120 Hz.
- Effect and size: for a 30 fps clip the slack is 8.33 ms, exactly one 120 Hz vsync. Paints beat every 4 vsyncs. A head started 1 vsync after that beat has `pos × 30 + 0.25 = k` exactly, at every paint, for its whole life. A paint a little late shows `k`, a little early shows `k − 1`. Late after early is a skip; early after late repeats a frame, and each pair is a 67 ms hold. That is the measured signature: "4 -> 11 per about 945 frames, clumped by run; each a 67 ms hold". One head in four starts on this phase. Before #1252 the clock wandered, so a head crossed the boundary now and then. On the grid it stays there. Model of the floor alone (`/private/tmp/claude-501/-Users-idan-Developer-Projects-quill/1a25b32a-d1f0-4d10-8b2a-5682db8c8588/scratchpad/media/head_phase.py`, 3000 frames, ±1 ms paint jitter): offsets 0, 2, 3 vsyncs skip 0%, offset 1 skips 17.6% on average. Real runs are lower because the start paint's own lateness moves the phase off the exact line.
- Why it is real: `frame_grid_tests.rs:629-630` uses the same formula, but from one fixed `t0` with no paint jitter, so it never lands on the line. The doc's "1-10 -> 0 skips" comes from that sim.
- Fix sketch: make the slack half a display frame at most: `min(0.25 frame, half the vsync period)`. At 30 fps on 120 Hz that is 0.125, which puts every vsync phase 4.17 ms from the boundary. 60 fps is unchanged. At 60 Hz nothing changes, because half a vsync is 8.33 ms. In the model this takes offset 1 to 0% for jitter up to ±2 ms. A later step is feeding heads the frame's target vsync in place of `Instant::now()`.
- Proof plan: extend the `frame_grid_tests.rs` sim with four head offsets and seeded paint jitter of ±1.5 ms, through the real `head::pick`: assert every step is 1. Fails today at offset 1. Then the existing `a11y` emoji pace recipe across 12 runs.
- Scope: `src/sticker_playback/player/head.rs` (`pick` needs the display period, or a slack passed in), `frame_grid_tests.rs`. Low risk: keep `jittered_paints_hold_each_frame_for_the_same_number_of_refreshes` green.


## media-10: A clip off the grid delays the emoji's tick, so emoji beside a 24/25 fps clip hold 50 ms in place of 42
- Severity: P3 minor
- Kind: stall
- Platforms: macOS at 120 Hz.
- Where: `src/ui/frame_clock.rs:441-460` `later_tick`, using `frame_grid::content_due` (`frame_grid.rs:186`) from `LayerDraws`' last redraw.
- Trigger: 30 fps emoji in a layer with a 24 or 25 fps clip.
- Effect and size: measured on #1252: 36 to 45 of about 235 emoji holds at 50 ms against 5 to 11 before, hold sd 8-10 to 11-12 ms. Traced by hand: pictures at vsyncs 5, 10, 15, 20, ticks on 8, 14, 18, 24. A picture at 10 redraws the layer without showing a new emoji frame, the tick due at 12 moves to 14, and that frame holds 6 vsyncs.
- Why it is real: `later_tick` assumes a redraw showed the content's next frame. That holds only for a clip on the grid. `shared` says something else plays, not that it plays on the grid.
- Fix sketch: note a layer redraw in `LayerDraws` only when the clip's `Cadence::even()` holds, so an off-grid clip never puts a tick off. The emoji then tick every 4 vsyncs and share frames only by chance.
- Proof plan: `frame_grid_tests.rs`: 30 fps emoji with a 24 fps clip on one layer, assert every emoji hold is 4 vsyncs and window frames stay at about 48.
- Scope: `src/ui/frame_clock.rs`, `frame_grid.rs`, tests. Low risk, same harness as media-6.

