# Video player: live streams, live stories and video chat tiles

PR 7 of the video player series (`quill-tools/designs/video-player.md`,
"Live build plan" item 7). Live pictures now play through the same
player as files: a channel's live stream, a live story and every video
chat track. The engine hands I420 planes through a one-slot feed, and
the player draws the newest frame on the next display frame.

## Before

- The engine converted every group call frame to RGBA on its own thread,
  drawn or not (`i420_to_rgba`, about 5 ms per 1080p frame).
- The stream window and the live story each converted the newest RGBA
  frame again, into BGRA (`StreamPicture`), and polled for new frames
  every 33 ms (`StreamTile`, `request_video_frame` on the whole panel).
- Video chat tiles redrew only by accident, on the 30 Hz mic tick. They
  converted on the UI thread and kept a cache per tile
  (`cached_group_video_image`, `cached_paused_image`).

## After

- **Engine.** `native.rs` copies ntgcalls' planes once and pushes a
  `LiveFrame` into the feed for its `LiveKey` (stream by chat, track by
  call, user and screen, or your camera). There is no conversion on the
  engine thread. Only 1:1 calls keep RGBA, until PR 9.
- **Feeds.** `quill::calls::live_video`: `LiveSource` (engine side,
  `Send`) and `LiveSink` (UI side) share one slot: the newest frame, the
  sender's state, the end, and an armed waker. The driver keeps one feed
  per key (`connect/group_calls/live_feeds.rs`). A track's feed is made
  on first use while its call is tracked; all feeds of a call close when
  it is left.
- **Player.** `VideoPlayer::live(sink, Purpose::Live { tile })`
  (`video_player/backend_live.rs`).
  - A waker task arms the sink. When a frame lands it converts large
    frames off the UI thread, then notifies the one view the element
    drew in.
  - The transport is live: Opening, then Playing on the first frame;
    Buffering after 1.5 s without a frame (the picture stays); Paused
    while the sender paused; Ended and Failed when the feed ends.
  - There is no position, duration or rate; play, pause, seek, rate and
    frame steps do nothing.
- **Pictures** (`video_player/live_picture.rs`):
  - macOS fills a pooled IOSurface NV12 pixel buffer
    (`CVPixelBufferPool`, recreated on a size change) and draws it
    through `surface()`. Small frames convert during the draw; frames
    above 800 px convert on the background executor.
  - Windows and Linux convert to one BGRA `RenderImage`, always on the
    background executor. Windows NV12 comes with PR 9.
  - Rotation is applied in the plane copy (`rotate_planes`).
- **Element.**
  - `video_frame(&player)` gains `.paused_overlay(bool)`, which draws the
    feed's 90 px blurred last frame while the sender is paused, and
    `.on_width(report)`, which keeps `report_stream_tile_width` working.
  - Live players never hibernate. A live tile painted off screen for two
    wakes stops redrawing and stops asking the frame clock; it looks
    again once a second.
- **Stream window.**
  - The picture is its own view (`calls/stream_view.rs`). It is a cached
    sibling under the cached panel, in a new window root
    (`GroupCallRoot`), so a frame redraws the picture and the panel
    replays.
  - Before the first frame it shows the chat's photo blurred, or the
    chat's initials when it has no photo (an open item from #852).
- **Live story.** The player is drawn behind the story chrome. The
  backdrop is `player.paused_picture()`.
- **Tiles and stage.** There is one player per feed, kept in
  `group_call.players`. A player goes when its call or stream goes. Pin,
  name and shadow are unchanged.
- **Removed:** `stream_picture.rs`, `stream_tile.rs`,
  `cached_paused_image`, `cached_group_video_image` and the tile image
  maps.
- **Demos.** The stream, live story and tiles fixtures send their
  pictures at 30 fps through a `LiveSource` (`calls/demo_feed.rs`).

## Measurements

Numbers are from an Apple M-series Mac. Conversion figures are the median
of 61 runs in release (`measure_1080p_frame_costs`, an ignored test in
`src/calls/live_video/convert_tests.rs`). Memory and redraw figures come
from the GPUI tests and a run of the demo binary.

**CPU per 1080p30 frame**

| Step | Before | After |
|---|---|---|
| Engine thread | 4,987 µs (copy + I420→RGBA) | 64 µs (copy only) |
| Conversion for the picture | 873 µs (clone + RGBA→BGRA, background) | macOS: 62 µs (I420→NV12, background); Windows/Linux: 4,215 µs (I420→BGRA, background) |
| Frames converted | every frame received, on the engine thread | only frames the UI takes (the newest when it draws) |

**CPU per 640 × 360 tile frame**

| Step | Before | After |
|---|---|---|
| Engine thread | 539 µs | copy only |
| UI thread | 91 µs | macOS: 5 µs (NV12 during the draw); Windows/Linux: 0, with 467 µs on the background executor |

The paused picture (90 px, blurred) costs 118 µs from a 1080p
frame. A paused tile makes it once. The live story's backdrop makes it
again for each new frame while the picture is shown whole.

**Bytes written per 1080p frame**

| | Before | After |
|---|---|---|
| Engine | 3.1 MB copy + 8.3 MB RGBA | 3.1 MB copy |
| Picture | 8.3 MB clone, swapped to BGRA, uploaded to the atlas | macOS: 3.1 MB NV12 into a pooled IOSurface (no atlas); Windows/Linux: 8.3 MB BGRA, uploaded to the atlas |

**Slice redraws, stream window, 30 frames at 30 fps**
(`a_stream_frame_redraws_only_the_picture`, `trace_slice_render`
counts, display frames that draw dirty views only)

| View | Before | After |
|---|---|---|
| Panel (title, controls, labels) | 30: the tile was inside the panel and every frame redrew the panel | 0 |
| Picture | (inside the panel) | 30 |

The "before" count comes from reading the code: `StreamTile` asked for
frames for the view it was drawn in, which was the panel. It was not
counted, because the trace hook did not exist then.

**Images, 200 frames at 1280 × 720 with the main window idle**
(`stream_pictures_stay_bounded_with_the_main_window_idle`)

- Before this series: 199 images alive (699 MiB).
- After, macOS: 0 images made, 0 alive. Every frame is a pooled
  pixel buffer.
- After, Windows/Linux: at most 2 alive. This is asserted, but on this
  Mac it was only checked through `QUILL_LIVE_BGRA=1` in the run below.

**Resident memory, stream window demo, 60 s at a 30 fps feed**
(`ready-rtmp-live`, release build, `ps -o rss=` every 5 s, main window
idle)

The picture is the showcase photo (1100 × 739), sent as a new frame
every 33 ms. The trace (`QUILL_TRACE_TICKS=1`) showed 30 to 31 picture
redraws a second and no other slice in either run.

| t | NV12 surfaces (macOS) RSS | CPU | BGRA images (`QUILL_LIVE_BGRA=1`) RSS | CPU |
|---|---|---|---|---|
| 0 s | 148.8 MB | 4.0 % | 152.7 MB | 10.2 % |
| 5 s | 148.8 MB | 4.0 % | 152.6 MB | 10.7 % |
| 10 s | 137.8 MB | 4.1 % | 152.7 MB | 10.9 % |
| 15 s | 137.8 MB | 3.6 % | 152.7 MB | 10.9 % |
| 20 s | 121.2 MB | 4.1 % | 149.2 MB | 11.0 % |
| 25 s | 121.2 MB | 4.2 % | 126.4 MB | 11.3 % |
| 30 s | 121.5 MB | 3.5 % | 133.1 MB | 10.3 % |
| 35 s | 121.5 MB | 3.5 % | 133.1 MB | 10.1 % |
| 40 s | 121.5 MB | 3.5 % | 133.1 MB | 10.8 % |
| 45 s | 121.5 MB | 3.5 % | 133.2 MB | 10.7 % |
| 50 s | 121.5 MB | 3.5 % | 135.7 MB | 10.0 % |
| 55 s | 111.6 MB | 3.8 % | 135.7 MB | 10.0 % |
| 60 s | 111.7 MB | 3.2 % | 135.7 MB | 9.6 % |

Both runs are flat or falling. At 30 frames a second, a leak of one
1100 × 739 BGRA image per frame would grow by about 98 MB a second. The
BGRA run did not grow, so the images are freed while the main window
idles. The sampling script is a `ps` loop over the demo binary run with
`QUILL_DEMO_LINGER_MS=75000`.

## Deviations from the design

1. **Color range.** The pool is
   `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`, not VideoRange with
   BT.709. GPUI's Metal surface shader draws only that format, with
   BT.601 full-range math. That is also what the engine's `i420_to_rgba`
   did, so colors match today's pixel for pixel. The BT.709/VideoRange
   choice would need a shader change in `gpui-pre-macos`.
2. **`next_due` is a stall check, not a frame prediction.** Frames come
   through the waker on the first display frame after they arrive, so
   the element asks the frame clock only for the stall check: the last
   arrival plus the stall time, then every 250 ms while Buffering. The
   stall time is the larger of 1.5 s and four median arrival gaps, so a
   screen share of a still window (one frame a second) does not read
   Buffering. A stale stall-check ask can cause one extra redraw per
   1.5 s.
3. **Images leave the atlas through `App::drop_image` in the waker**, not
   through `image_budget::retire_all`. The player keeps the shown image
   and the one before it, which the last painted frame of the picture's
   view may still use. Older ones are dropped from every window as soon
   as the next frame lands. `retire_all` gets only what is left when the
   player itself is dropped.
   - `retire_all` frees images only when `image_budget::sweep` runs, and
     only `QuillApp::render` runs it. The stream window draws in its own
     window while the main window idles. That pattern measured 199 live
     images (699 MiB) before.
   - A fix that calls `sweep` from slices and the anim layer (in
     progress on main) still would not cover the stream window's root.
4. **The feed types live in the library** (`quill::calls::live_video`),
   not in `video_player/`, because the engine and the driver push into
   them.
5. **The off-screen policy counts wakes, not pulls.** A tile is off
   screen after two wakes at least 100 ms after its last redraw without
   a paint in view. The 100 ms grace stops frames that arrive faster than
   the window draws from counting. An off-screen tile is looked at once a
   second: a window coming back from behind another replays its cached
   views without painting, so `note_shown` alone would never bring the
   tile back.
6. **Background threshold.**
   - macOS converts frames above 800 px (`INLINE_MAX_EDGE`, as the
     brief says) off the UI thread, not above 512 px. NV12 for a
     640 × 360 tile takes 5 µs.
   - Windows and Linux convert every frame off the UI thread. BGRA takes
     467 µs for a 640 × 360 tile, more than the old UI-side
     swap (91 µs).
7. **The paused overlay draws only the blurred picture.** The tiles keep
   drawing the shade, the pause icon and "Video is paused" themselves
   (`group_call_paused::paused_overlay`), so the visuals stay as they
   are.
8. **The live story backdrop is the 90 px paused picture**
   (`paused_picture()`, blur radius 15), not the 40 px, blur 6 crop of
   the box (`frame_backdrop`). The design says the backdrop comes from
   `paused_picture()`; it reads slightly softer.
9. **The demos send today's pictures**, not a gradient: the showcase
   photo for the stream and the live story, and the made-up desktops for
   the tiles. They are sent at 30 fps through a `LiveSource`, so the
   screenshots stay as they were. The paused screen tile sends one frame
   and stops, as a paused sender does. Tests use `gradient_frame`.
10. **The screen-share preview** of your own share goes through
    `rgba_to_i420` into your own track's feed, because the capture
    produces RGBA.
11. **`set_volume` does nothing on a live player.** Stream volume is PR 8.
12. **The design's 120 Hz display** in the slice test is not simulated:
    the test draws one display frame per feed frame.

## tdesktop

- tdesktop has no picture-in-picture for live streams or video chats.
  Quill adds one in PR 9.
- The paused tile (`kPausedVideoSize` 90, `kBlurRadius` 15, shadow alpha
  80) and the blurred photo before the first frame
  (`Viewport::RendererSW::validateUserpicFrame`, radius 15) are kept.
- The stall rule has no tdesktop equivalent: tdesktop draws whatever
  frame it has.

## Not verified

- Nothing ran in a real call or a real stream; it ran only with demo
  feeds and tests. Rotation from a phone camera, real 1080p stream
  pacing and the engine's plane copy under load are untested end to end.
- Windows was not built. The Windows cross-check from this Mac stops at
  `ring`'s build script. Windows shares the BGRA path with Linux, which
  passes cross-target clippy (`x86_64-unknown-linux-gnu`, stub C tools).
  Neither ran.
- The Windows/Linux memory behavior was measured on macOS by forcing
  BGRA (`QUILL_LIVE_BGRA=1`), not on those platforms.
- GPU cost (atlas uploads, surface draws) was not measured. Only CPU
  conversion time was.
- PR 6 (Windows NV12 surfaces) has not merged. When it does, Windows
  live pictures can move to it in PR 9.
