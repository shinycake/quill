# Streaming sticker player

Animated stickers, custom emoji, dice, send effects, Premium sticker
effects, the emoji tap effect and live story Stars bursts all play through
one streaming player. It draws each frame shortly before it is shown, at the
pixel size of the box it is painted in. It replaces the decode-all caches,
which decoded every frame of a sticker up front at one of a few fixed sizes
(Sticker, Emoji and Dice classes, plus mirrored copies) and scaled the
result on screen.

## Design

The engine lives in `src/sticker_playback/player`, in the core crate, and
has no GPUI types. The UI half is `src/ui/sticker_player`.

- A clip is one file drawn one way: `ClipKey` holds the file,
  the edge in pixels, the skin tone, the tint, the mirroring, and the
  display rate class. The edge is the painted box times the window's
  scale factor, rounded up to a multiple of 8 (16 to 1024 px). A 112 pt
  emoji on a 2x screen is drawn at 224 px, and the 336 pt tap effect at
  672 px.
- A pool of 2 worker threads (3 on machines with 8 or more
  cores) opens clips and draws frames. ThorVG's software renderer keeps its
  memory pool in a `thread_local` (`tvgSwMemPool.cpp`), so a canvas is only
  ever drawn on the thread that created it. Each worker owns the clips it
  opened, and new clips go to the least loaded worker.
- On a display faster than 60 Hz a Lottie clip gets a grid
  `step` times finer than the file's own rate (`rate_step`: 60 fps files get
  step 2 at 120 Hz, 30 fps files get step 4, never above 120 frames a
  second). ThorVG draws the in-between frames with fractional
  `set_frame`. A view that may not use the fine rate reads every
  `step`-th grid frame, which is the file's own frames. The rate class is
  part of the key (`grid_hz`: 60 up to 61 Hz, else the display's rate up to
  120), so a window that moves to a faster display opens the clip again on
  the finer grid. Until that clip is ready, the old clip's frame stays up.
  WebM stickers are decoded whole (the bundled decoder, else `ffmpeg`), keep
  their own rate, and play from the store.
- Each view names a head (`HeadId`), so two views of one
  clip share frames but not position. Modes are loop, play once and rest on
  the first or last frame, a number of loops, and a fixed position (the
  Stars burst). A head starts when the view's first frame is on screen and
  only advances while the view paints. A gap of more than 200 ms pauses it,
  so a sticker scrolled away resumes where it was. A lone emoji plays once,
  rests on frame 0 and replays when tapped. Its rest frame is prefetched
  with the last frames of the play, so it never shows late.
- Each head asks for the next 3 frames (`AHEAD`). If the frame a head reached is not ready, the last frame
  stays up and the paint counts as late. Nothing is drawn for a view that
  does not paint (off screen, a closed panel, a hidden window).
- Drawn frames are kept LZ4-compressed per frame
  (`lz4_flex`), so the second and later loops unpack instead of drawing.
  One byte budget of 96 MB covers every clip's store. Frames are evicted
  least recently used first, and a clip painted in the last 500 ms keeps
  its frames. Clips nobody painted for 5 s close and free their ThorVG
  canvas. (Since `sticker-clip-memory.md`: clips close 5 s after no window
  shows them, between frames, also when nothing paints; a clip's frames
  are packed into 256 KiB blocks.)
- Views request ticks from the existing frame clock, at the
  rate their clip plays. The frame clock now allows up to 120 ticks a
  second (`MAX_TICK_FPS`). The fine grid is used only when the display is
  faster than 60 Hz, the window is active, and power saving does not cover
  animations. Custom emoji in text are capped at 60 frames a second and
  always use the file's own grid.
- Workers wake the UI only when a clip opens or fails and for a clip's
  first frame. Later frames are picked up when views paint on the next
  tick.

The decode-all path (`decode_tgs`, `decode_webm`, the size classes, the
mirrored-frame copies and the per-cache LRUs) is gone. No caller kept it.

## Frame store format

tdesktop stores each frame XORed with the previous one, then LZ4-compressed.
Both were measured on the same frames:

| Edge | Plain LZ4 per frame | XOR with previous, then LZ4 |
| --- | --- | --- |
| 224 px | 10.5% of raw | 11.9% of raw |
| 448 px | 6.0% of raw | 7.6% of raw |

Plain LZ4 is smaller here. Sticker frames are mostly transparent, and
those runs already compress well, while XOR turns moving antialiased edges
into noise. With plain LZ4 any frame unpacks on its own, without the frame
before it, which a head that jumps (restart, the Stars burst, a pause)
needs.

## Measurements

All on an Apple M1 Pro (10 cores, so 3 workers), macOS, 2026-10-27.
Treat them as relative: before and after on the same machine.

### Engine, per frame

Method: `cargo test --release --no-default-features --lib
sticker_store_bench -- --ignored` with `QUILL_STICKER_BENCH_DIR` pointing at
41 real cached TGS stickers. One thread draws every grid frame of every
file, packs it, and unpacks it again.

| Edge | Grid | Frames | Draw median / p90 | Pack median | Unpack median / p90 | Stored |
| --- | --- | --- | --- | --- | --- | --- |
| 112 px | 60 Hz | 6039 | 0.19 ms | | 0.03 ms | 18.0% of raw |
| 224 px | 60 Hz | 6039 | 0.35 / 0.63 ms | 0.11 ms | 0.11 / 0.15 ms | 127 MB of 1212 MB (10.5%), 20.5 KB a frame |
| 448 px | 60 Hz | 6039 | 0.80 / 1.46 ms | 0.28 ms | 0.42 / 0.58 ms | 289 MB of 4848 MB (6.0%), 46.8 KB a frame |
| 448 px | 120 Hz | 12282 | 0.81 ms | | 0.42 ms | 596 MB (6.0%) |

A 120 Hz grid costs the same per frame and twice the frames. At 448 px a
worker draws a frame in under 1.5 ms at p90, well inside the 8.3 ms a
120 Hz frame allows.

### In the app

Method: release builds with `--features demo-capture`, the
`ready-sticker-playback` demo in a 1200×800 window on the built-in 120 Hz
display, with `QUILL_DEMO_STICKERS=40` (40 more animated stickers in the
panel). A scripted pointer hovers 20 panel cells for 400 ms each, so each
hovered sticker animates. `footprint -p` reads `phys_footprint` 11 s and
15 s after launch. Before is main at 30fa9e5 (decode-all caches), after is
this branch. Runs alternated, three each. The script and logs are in the
sticker-player scratch directory (`measure.sh`).

| Sticker in the demo | Before | After |
| --- | --- | --- |
| The demo fixture (30 fps, 2 s, a flat shape) | 188 to 192 MB | 130 to 144 MB |
| A real sticker (60 fps, 3 s, from a pack) in place of the fixture | 225 to 229 MB | 172 to 177 MB at 11 s, 159 to 160 MB at 15 s |

After plays on the 120 Hz grid (the frame clock ticked at 120 fps,
`QUILL_TRACE_TICKS=1`); before ticked at 30 fps. With the
real sticker, `QUILL_TRACE_STICKERS=1` showed over each 15 s run:

- drawing 0.63 to 0.87 ms a frame, packing about 0.05 ms, unpacking 0.20
  to 0.27 ms,
- 45 to 49 late paints out of about 4800 (1%), all while cells were being
  hovered and their clips opened (the count stops once the pointer rests),
- the store peaking at 10.6 MB, nothing evicted.

The old path showed a still until every frame was decoded, then played
from memory, so it never painted late. A sticker stood still longer before
it moved, and every frame sat unpacked in memory at a fixed size.

### What limits the numbers

- Memory is the process baseline (fonts, GPU atlases, the demo chat)
  plus the frames on screen. The store is small here because only one
  sticker animates at a time. The 96 MB budget is reached only with many
  large clips looping at once, and then the oldest frames go first.
- Late frames come from clip opening: parsing the Lottie JSON and
  drawing frame 0 takes a few ms. A view waiting on its first frame shows
  the still, so a late paint repeats a frame and never shows a blank.
- Unpacking is not always cheaper than drawing. For the demo fixture, drawing took 0.05 ms and unpacking 0.2 ms. A future policy could
  skip the store for clips that draw faster than they unpack.
- Cost per frame grows with the edge squared. The 336 pt tap effect at
  2x is 672 px; scaling the 448 px median gives about 1.8 ms a frame
  (estimated, not measured). It plays once, so every frame is drawn live,
  which still fits a 120 Hz frame on one worker.

### Follow-up: the tap effect stuttered (2026-10-28)

The estimate above was wrong for real effects. The 😂 and 👏 effects from a
real account draw at 672 px in 2.6 ms median and 6.0 ms at p90 (bench above,
2 files, 620 frames), and their busiest stretch is slower than that. At
120 Hz one worker has 8.3 ms a frame, so the effect fell behind about
three quarters of a second in and most later frames came late. Separately,
paints land up to a millisecond either side of each refresh, so mapping
wall time to frames with a plain floor showed some frames for one refresh
and others for three.

Two changes, both traced on the real 😂 effect (`ready-emoji-tap-effect`
with its file swapped in, a frame log per paint):

- Each clip keeps a smoothed draw time. Once it passes 40% of a grid
  frame, the clip plays at its file's own rate for the rest of its life,
  while views still tick at the display's rate.
- A head counts a paint up to a quarter grid frame early as on time.

Before: the second half of the effect late, frames held for 1, 2 or 3
refreshes (35, 72 and 29 times). After: 1 late paint out of 358, and
146 of 150 frames held for exactly 2 refreshes (the rest at the switch).

### Follow-up: one slow draw demoted a fast sticker (2026-11-01)

The smoothed draw time started from the first draw and was tested on
every draw, and a demotion never cleared. The first draw runs cold, and a
worker preempted for 10 ms in the first loop lifts the average past the
3.33 ms a 120 Hz grid allows from a 0.8 ms norm. That explains the 3 of
about 24 runs where a normal sticker played at its file's rate: 107 window
frames a second for 30 pictures.

- The first draw is left out. The average is the plain mean of the next
  draws until there are 8, then a running eighth, and it is tested only
  from the 8th on. The 672 px effect still drops by its 9th draw (75 ms
  at 120 Hz). One 14 ms draw among 0.8 ms ones never does.
- `QUILL_TRACE_STICKERS` prints how many open clips play at their file's
  rate (`coarse`).

### Follow-up: emoji on the content grid skipped frames (2026-11-01)

A quarter grid frame of slack is 8.33 ms for a 30 fps emoji, one whole
refresh at 120 Hz. Emoji paint every fourth refresh there, so a head that
started one refresh after that beat sat exactly on a frame boundary at
every paint for its whole life: a paint a little early showed the frame
before, one a little late the frame due, and each early-late pair was a
skip and a 67 ms hold (4 to 11 per 945 frames, clumped by run). One head
in four starts on that phase.

The slack is now a quarter grid frame or half a refresh, whichever is
less, so every phase sits at least 4.17 ms from a boundary at 120 Hz. 60
fps files and 60 Hz displays keep a quarter frame. A seeded test of four
start phases with paints up to 1.5 ms off shows every frame once; with
the old slack the second phase skips or repeats on about half its paints.
Feeding heads the target vsync in place of the paint's wall time would
remove the jitter itself and is left for later.

### Follow-up: ticks that draw nothing new (2026-11-01)

A clip at its file's rate still ticks at the display's rate, so each frame
stays up for the same number of refreshes. Every tick drew the whole window:
107 window frames a second for 30 pictures on a 120 Hz Mac.

Ticking such a clip at twice its file's rate instead, which saves the wakes
too, gave 86% of frames their 4 refreshes where the display's rate gave 100%,
with 8 frames in 118 held 3 and 8 held 5. The clock runs free at 92% of a
frame, so at that rate a tick and a frame's start beat against each other. A
tick that draws nothing costs almost nothing, so the wakes were not worth it.

The clock now asks an animation layer before it notifies it whether a
draw before the next refresh would change anything: any clock-driven
drawing or live tile does, a sticker only when its head would pick another
frame than the one its view shows (`PlayHead::changes_at`, on a copy of
the head). A layer that would not change keeps ticking without a draw.
Frames still land on ticks, so they keep their even holds, and the check
needs no vsync times, so it works on every system. A view unpainted for
100 ms paints anyway, so a slow clip never reaches the 200 ms pause gap.

In a model of a 30 fps clip at its file's rate ticked at 120 Hz, with each
tick up to a refresh before its paint: 120 draws a second → 38, each of
the 30 frames up for exactly 4 refreshes. The window frame count on a real
120 Hz display still needs a Mac.

### Follow-up: WebM opens held back other clips' frames (2026-11-01)

A WebM sticker decodes every frame when it opens: 77 ms for a 512 px 3 s
sticker at box 224, 28 ms at box 56, 280 to 470 ms through the `ffmpeg`
fallback (`sweep-webm-stickers.md`). That ran on the sticker worker the clip
was pinned to, and a worker took a pending open before any frame, so every
clip on that worker got no frames meanwhile. Queued opens ran back to back.

Clips decoded up front now open on one opener thread of their own
(`opener.rs`). It installs the frames in the clip's store, and the clip's
worker only unpacks them; a decoded clip holds no canvas, so the worker
needs no source for it. Workers still open TGS clips, which keep their
canvas there, but a frame now goes before an open unless 8 frames in a row
already did. A test with one worker holds a WebM decode open and gets
another clip's frame meanwhile; before, the decode ran on the worker.

Each open still holds all its raw frames at once (up to 58 MB at 448 px,
inferred), and opens still queue on the one opener. Streaming WebM frame by
frame through the shim, as tdesktop's `Media::Clip` does, would remove both.

## Verification

`/verify` captures at 2x, before and after: a sticker message in history,
the sticker panel with a hovered cell, and the emoji tap effect
(`ready-emoji-tap-effect`, new in this change). After, history stickers are
sharp with no halo from upscaling, and the tap effect is drawn at
tdesktop's `EmojiEffectSize` (336 pt) instead of five times the emoji
height.

## Risks and follow-ups

- Linux and Windows are covered by CI builds and tests, not by a run on
  real hardware with a high refresh display.
- `refresh_interval` is read from the window that last painted a sticker.
  With two windows on displays of different rates, clips follow the one
  that painted last.
- The display rate is unknown until the first paint, so a clip opened
  during the first render starts on the 60 Hz grid and is opened again on
  the next frame on a faster display.
