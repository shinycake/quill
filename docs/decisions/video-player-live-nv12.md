# Video player, PR 10: live frames as NV12 surfaces on Windows

## Problem

PR 6 (#859) moved file video on Windows to NV12 surfaces. Live video
(group call tiles, screen shares, RTMP streams, live stories) still went
the old way:

1. The engine hands over an I420 frame.
2. `i420_to_bgra` converts it on the CPU.
3. The renderer uploads the 4-byte-a-pixel image into the sprite atlas.

At 1080p that costs about 4.4 ms of CPU and 8.3 MB of upload per frame,
per tile. The DirectX renderer could already draw NV12, so both the
conversion and most of the upload could go.

## What changed

### Plane packing (`src/calls/live_video/convert.rs`)

- `i420_to_nv12_packed` writes a frame into one tight buffer of the kind
  `SurfaceFrame::new(.., SurfaceFormat::Nv12, ..)` accepts: the luma
  rows, then Cb and Cr interleaved. Rotation and mirroring have already
  been applied when it runs, so the frame is upright, as for BGRA.
- `nv12_packed_size` cuts an odd side down to the even size below it.
  NV12 needs even sides, and odd live frames are common: a shared window
  can be any size, and the demo photo is 1100 x 739. Such a frame loses
  its last column or row (1100 x 738) and stays on NV12.
- `nv12_packed_len` gives the exact byte length with checked arithmetic.
  The packer refuses a buffer of any other length.

### The live picture (`src/ui/video_player/live_surface.rs`, new)

- `LiveNv12` lives on the live track. It holds:
  - a pool of up to 4 buffers of the last size. A buffer is filled again
    only when no frame holds it (`Arc::get_mut`), so a frame's pixels
    never change after it is made. The renderer relies on that.
  - the pause after a refusal. It is `NV12_RETRY` (two seconds), the
    file player's constant.
  - the NV12 frame on screen, so the backend can read its draw state.
- `fill` packs a frame into a pool buffer and tags it with the live
  colour (`LIVE_COLOR`, BT.601 full range).
- `fallback_image` converts a refused frame on the CPU (`nv12_to_bgra`)
  into an image of the same picture.

### Conversion and the backend (`live_picture.rs`, `backend_live.rs`)

- `Target::Nv12(buffer)` and `Prepared::Nv12` sit next to the BGRA and
  macOS pixel-buffer targets. On Windows the target is NV12 except in
  these cases:
  - the track is in its pause
  - the frame is smaller than 2 x 2
  - BGRA is forced
- Conversion still runs where it did before. It happens during the draw
  when both sides are at most `INLINE_MAX_EDGE` (800), and on the
  background executor above that. The single slot that keeps only the
  newest frame has not changed.
- On Windows the picture is `Picture::Surface(Arc<SurfaceFrame>)`. The
  same surface element as file video paints it. macOS keeps its
  `CVPixelBufferPool`. Linux joined Windows later
  (`video-nv12-linux.md`).
- `SURFACES_HELD` is 1 on Windows. Holding the frame on screen is
  enough, because a pool buffer is reused only when no frame holds it.
- A refusal is handled as in the file backend:
  - While the frame on screen is pending, `next_due` returns `None`. The
    element then looks again on the next display frame. A paused or
    ended tile keeps asking too, through `unsettled()`.
  - When the renderer refuses the frame, `Backend::replace_refused`
    (`live_nv12.rs`) shows the same picture as an image and starts the
    two-second pause. Frames during the pause convert to BGRA, and NV12
    is tried again after it.
  - The tile is never empty. The refused frame is replaced by its own
    image in the same pull.
- `QUILL_LIVE_BGRA=1` forces BGRA on Windows too, in demo-capture
  builds. `bgra_only()` now always returns true on Linux.

### Trace lines

Live players now print `VF` lines under `QUILL_TRACE_VIDEO=1`. Before,
only file players did, so there was no way to check this PR on Windows.
Each line has the file player's columns:

- the format (`nv12` or `bgra`)
- the size
- the upload bytes
- the conversion time, measured either during the draw or on the
  background executor

`scripts/analyze-video-trace.py` reads them the same way it reads file
video lines.

### A fix along the way

When a surface replaced an image, `show()` called
`shown_image.take().or(previous_image.take())`. That dropped one image
without removing it from the sprite atlas, on every switch from image to
surface. On macOS it happened whenever pixel buffers took over from
images, for example after a pool failure. Both images are now retired.

### Not touched

The FFmpeg shim and the vendored GPUI trees were not changed. Everything
this PR needed (`SurfaceFrame`, `SurfaceDrawState`, `with_yuv`) was
already there after PR 6.

## Measurements

These cover the plane maths only. They ran on an M1 Pro and are medians
of 61 runs:

```
cargo test --release --lib measure_windows_live_nv12 -- --ignored --nocapture
```

| Frame | Before: I420 to BGRA | After: to NV12, new buffer | After: pooled buffer | Upload before / after |
|---|---|---|---|---|
| 1920 x 1080 | 4441 µs | 96 µs | 76 µs | 8 294 400 / 3 110 400 B |
| 1280 x 720 | 1984 µs | 43 µs | 33 µs | 3 686 400 / 1 382 400 B |
| 640 x 360 | 496 µs | 10 µs | 7 µs | 921 600 / 345 600 B |

- The CPU work per frame drops about 50 times. NV12 needs only a copy
  and an interleave, with no colour maths.
- The upload drops to 37.5 % of the BGRA bytes.
- During a refusal pause each frame costs what it did before (BGRA).
  On a refusal, one picture is converted back from NV12 on the CPU. That
  costs 7.7 ms at 1080p (`nv12_to_bgra`) and happens at most once every
  two seconds.

All of these are CPU numbers taken on macOS. The Windows GPU side was
not measured.

## The image budget

Nothing is left behind when a feed ends, a PiP window closes or the
player is dropped. The checks:

- **Feed end.** The waker calls `release_buffers(false)`, which empties
  the NV12 pool and keeps the last frame on screen. The GPUI test
  `a_refused_nv12_frame_shows_as_an_image_and_nv12_comes_back` ends the
  feed, then checks that the pool is empty and the frame still shows.
- **PiP close or the tile leaving.** `release_live` calls
  `release_buffers(true)`, which also drops the frame on screen. The same
  GPUI test then checks that at most one image is still alive.
- **Player drop.** Dropping the player drops the core and with it the
  track. `nv12_buffers_are_reused_and_go_with_the_feed_and_the_player`
  keeps weak references to every buffer and checks that all of them are
  dead after the drop. It also checks that 30 frames use at most 3
  distinct buffers.
- **GPU textures.** The renderer frees an NV12 slot that has not been
  drawn for 2 frames (PR 6), so a closed tile leaves no textures behind.

## How verified

On macOS, `cargo test --features demo-capture --bin quill video_player`
and the core tests pass. The new tests cover:

- the plane maths, on every platform:
  - the packed layout and the exact length
  - even sides, and odd frames losing their last column and row
  - a turned and mirrored frame, checked against its planes
- the live colour against `i420_to_bgra`. It is at most 1 of 255 apart
  at 360p, 720p and 1080p. BT.709 limited range would be more than 10
  apart.
- the pool:
  - buffer reuse
  - a shared or wrong-sized buffer that is not filled
  - the pause after a refusal
  - a feed end that keeps its picture
- the backend:
  - NV12 during the pull up to the inline edge, and on the background
    executor above it
  - the VF stats
  - a thin frame staying BGRA
  - `next_due` while a frame is pending, refused and drawn
- GPUI (`#[cfg(test)]` over `#[cfg(feature = "demo-capture")]`, no
  sleeps), in one test:
  1. A refused frame shows as its image.
  2. Images follow for the pause.
  3. NV12 comes back.
  4. The feed ends.
  5. The player is released.

The tests force NV12 on every platform. Off Windows, an NV12 picture is
shown as its CPU image, so the same code path also runs on macOS and
Linux.

`cargo check --target x86_64-pc-windows-msvc --features demo-capture
--tests` and `cargo clippy` for that target pass on macOS. The build
scripts that compile C ran with stub `llvm-rc`, `clang-cl` and
`llvm-lib` on `PATH`. The remaining warnings are in files this PR does
not touch.

## Not verified

- Anything on a Windows GPU: no picture on screen, no upload trace, no
  draw times, no fallback run and no device-loss run. The Windows CI
  runs no UI tests. The steps are below.
- The colour on screen. The surface uses the same rows the CPU tests
  check, but nobody has looked at it.
- How fast a refusal is noticed on a slow feed. The element looks again
  on the next display frame. However, the frame clock does not start a
  new request while a stall-check timer is pending. On a feed slower
  than the display, the refusal may only be seen at the next frame or
  stall check. Until then the tile goes on showing the refused frame in
  whatever state the renderer left it.

## Windows steps

1. Build both trees in release with the profiler:
   `cargo build --release --features frame-profiler,demo-capture`. Do it
   once on `main` (before) and once on this branch (after).
2. Run each build on a live demo:

   ```
   set QUILL_TRACE_VIDEO=1
   set QUILL_ASSUME_ACTIVE=1
   set QUILL_DEMO_LINGER_MS=15000
   set QUILL_FRAME_PROFILE=profile.txt
   quill.exe --screenshot-demo ready-rtmp-live <out dir> 2> trace.log
   ```

   Repeat with `ready-live-story`, `ready-video-call` and
   `ready-call-screen-share`. The demos push a photo at 30 fps.
3. Run `python scripts\analyze-video-trace.py trace.log 60`, using the
   panel's refresh rate. Compare:
   - `VF`: `main` prints no live lines. This branch should print `nv12`
     lines. For the RTMP photo, expect `nv12,1100,738,1217700`. As BGRA,
     the same frame was 3 251 600 bytes.
   - `VU`: before, `atlas` uploads of about 3 MB, 30 times a second.
     After, `nv12` uploads of about 1.2 MB and no large atlas uploads.
   - `profile.txt`: `draw` and `dirty_to_present`, p50 and p90.
4. Fallback check: apply a one-line local patch that makes
   `YuvSurfaces::new` in `yuv_surfaces.rs` return an error, then run the
   RTMP demo again. The tile must keep its picture. `VF` must say `bgra`
   for two seconds at a time, with an `nv12` line between.
5. Run again with `set QUILL_LIVE_BGRA=1`. `VF` should say only `bgra`.
6. Device-loss check: with a live tile playing, switch the GPU driver or
   run `dxcap -forcetdr`. Playback must continue without a crash.

## Deviations

- **The colour is BT.601 full range at every size.** The brief asked
  for BT.709 at 720 lines and above, to match macOS. The live path never
  used that rule, though. `i420_to_bgra` and the macOS pool both use
  BT.601 full range, and the engine sends no colour metadata with a
  frame. A BT.709 surface would have changed the colour of every HD tile
  on Windows, and of the CPU fallback image next to it. The tests show
  the NV12 surface within 1 of 255 of the BGRA path. Whether to use the
  stream's real matrix is a separate question for all three platforms
  (tdesktop assumes BT.601 limited range).
- **Odd frames lose a column or a row.** The file player keeps odd
  frames on BGRA. For live video that would have kept the demo feeds and
  many screen shares on the CPU path, so the frame is cut to even sides
  instead.
- **Small frames still convert during the draw, large ones in the
  background**, as the brief asked. At 0.1 ms for 1080p, NV12 could
  convert during the draw at any size. That is left for later, so the
  threshold stays the same on every platform.
- **Live players print `VF` lines.** They didn't before. The line has
  the file player's columns.
- **A pre-existing atlas leak in `show()` was fixed** (see above). It
  turned up while writing the image-budget tests.
- **`NV12_RETRY` and `nv12_allowed` are now `pub(super)`** in
  `backend_ffmpeg.rs`, so the live backend shares the file player's
  pause.

## Parity

This closes no README parity item, so it has no parity fragment.
