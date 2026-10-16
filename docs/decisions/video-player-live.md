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
  per key (`connect/group_calls/live_feeds.rs`).
  - A track's feed is made when the UI asks for it, and only while the
    tracked call has that video on (`call_has_video`). The engine hook
    fills feeds that exist and makes none.
  - When TDLib reports a camera, a screen or your own camera off, its
    feed ends and goes (`sync_call_feeds`, after every reducer update).
    Turned on again, the video gets a new, empty feed, so no frame from
    before shows. All of a call's feeds close when it is left.
- **Player.** `VideoPlayer::live(sink, Purpose::Live { tile })`
  (`video_player/backend_live.rs`).
  - A waker task arms the sink. When a frame lands, the off-screen
    policy decides first (below). Only then does it convert a large
    frame off the UI thread and notify the one view the element drew in.
  - Each converted picture carries its frame's sequence number. A
    picture older than the one shown is dropped, so a large frame still
    converting when a newer small one was drawn never replaces it.
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
  - Live players never hibernate. A live picture painted off screen, or
    not painted at all for 500 ms, for two wakes stops redrawing,
    converting and asking the frame clock. Its frame waits in the feed.
    A tile looks again once a second, a stream or a story four times a
    second. Painted in sight again, it wakes its waker at once and draws
    the waiting frame.
- **Stream window.**
  - The picture is its own view (`calls/stream_view.rs`). It is a cached
    sibling under the cached panel, in a new window root
    (`GroupCallRoot`), so a frame redraws the picture and the panel
    replays.
  - Before the first frame it shows the chat's photo blurred, or the
    chat's initials when it has no photo (an open item from #852).
- **Live story.** The player is drawn behind the story chrome. The
  backdrop is `player.paused_picture()`, made again at most every
  500 ms.
- **Tiles and stage.** There is one player per feed, kept in
  `group_call.players`. A player goes when its call or stream goes. Pin,
  name and shadow are unchanged.
- **Releasing.** Whoever drops a live player calls
  `VideoPlayer::release_live(cx)` first: the stream window's `clear`, a
  stream or story player replaced for a new feed, the live story's
  stop, and both tile prunes. Its images leave every window's atlas at
  once instead of at the main window's next sweep.
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
again for a new frame at most every 500 ms (it was every frame before
the review), so at most 236 µs a second.

**Frames converted by a hidden tile, 30 frames**
(`a_hidden_tile_converts_nothing_and_catches_up_in_sight`, a tile at
y = 5000 in a window that clips, drawn after every frame)

| Path | Before the review | After |
|---|---|---|
| BGRA (`QUILL_LIVE_BGRA=1`, the Windows/Linux path) | 30 | 1 |
| NV12 (macOS) | 30 | 1 |

The "before" column is the same test with the waker put back in its old
order (convert, then decide) and without the quiet check on the pull.
On the BGRA path the old waker converted every frame on the background
executor whether or not anything drew it. On macOS small frames convert
only when the view draws. The test draws after every frame, which is
what a call panel redrawn for other reasons does, so 30 there is the
worst case. The one conversion after the fix is the first frame,
before the tile counted as unseen. Scrolled back into sight, the tile
converts the frame waiting in the feed at once (one more), then every
frame again.

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
- After, Windows/Linux: at most 3 alive (the shown image, the one
  before it, and the one the last draw retired, which the next wake
  drops), and at most 1 after `release_live` (the player's own current
  picture). This is asserted, but on this Mac it was checked only
  through `QUILL_LIVE_BGRA=1` (this test and the run below). The first
  version of this doc said 2. The test asserted it only on macOS, where
  no images are made, so it never checked the BGRA count.

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

**The same run after the review**, rebased on main with the image-budget
fix (#870, main at `1e412823`), release build. The BGRA run had
`QUILL_TRACE_TICKS=1`: 29 to 31 picture redraws a second, and after the
first second no other slice.

| t | NV12 surfaces (macOS) RSS | CPU | BGRA images (`QUILL_LIVE_BGRA=1`) RSS | CPU |
|---|---|---|---|---|
| 0 s | 151.0 MB | 3.2 % | 153.1 MB | 10.4 % |
| 5 s | 153.3 MB | 5.8 % | 154.2 MB | 11.3 % |
| 10 s | 153.4 MB | 2.4 % | 154.2 MB | 10.5 % |
| 15 s | 153.4 MB | 2.8 % | 154.3 MB | 9.9 % |
| 20 s | 153.4 MB | 2.9 % | 154.2 MB | 10.8 % |
| 25 s | 153.5 MB | 3.0 % | 154.2 MB | 10.5 % |
| 30 s | 153.5 MB | 3.3 % | 154.2 MB | 10.5 % |
| 35 s | 153.5 MB | 3.2 % | 154.2 MB | 10.7 % |
| 40 s | 147.9 MB | 3.4 % | 154.2 MB | 9.5 % |
| 45 s | 147.9 MB | 3.6 % | 154.2 MB | 10.7 % |
| 50 s | 147.9 MB | 3.4 % | 154.2 MB | 10.0 % |
| 55 s | 147.9 MB | 3.7 % | 154.2 MB | 11.0 % |
| 60 s | 147.8 MB | 3.5 % | 151.9 MB | 10.9 % |

Both stay flat: NV12 moves within 6 MB, BGRA within 2.4 MB over the
minute (KB figures from `ps`, rounded). The first run fell further
because macOS trimmed the process early; neither run grows.

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
   - The leak fix that landed on main (#870,
     `docs/decisions/retired-images-sidebar-bursts.md`) changes what
     `sweep` may drop, not where it runs. `QuillApp::render` runs on
     every frame the main window draws, but an idle main window draws
     none, so it still would not free the stream window's images. This
     branch is rebased on it; nothing here depends on it.
   - When the owner drops the player, `release_live` drops the rest
     (shown, previous, paused pictures) the same way. `retire_all`
     from `LiveCore::drop` is only the fallback for a player dropped
     some other way.
4. **The feed types live in the library** (`quill::calls::live_video`),
   not in `video_player/`, because the engine and the driver push into
   them.
5. **The off-screen policy counts wakes, and runs before the
   conversion.**
   - At each wake the policy reads two counts the element keeps: paints
     in sight (`note_shown`) and all paints (`note_live_paint`).
     - A paint in sight since the last wake resets it.
     - A paint out of sight counts the wake as unseen.
     - No paint at all counts only once 500 ms have passed without one.
       This covers a minimized window. A slow window that draws a few
       times a second keeps its pictures (`a_slow_window_keeps_its_pictures`).
   - After two unseen wakes the frame is neither converted nor redrawn.
     A redraw for another reason converts nothing either: the pull
     checks the same quiet flag. The frame stays in the feed.
   - A tile looks again once a second, a stream or a story every
     250 ms. A window coming back from behind another replays its
     cached views without painting, so the paint counts alone would
     never bring the picture back.
   - A paint in sight while quiet wakes the waker at once
     (`LiveSink::poke`), which draws the waiting frame.
   - The first version of this PR ran the policy after the conversion,
     so a hidden tile on the BGRA path still converted every frame
     (measured above). It also used a 100 ms grace after the last
     redraw instead of the paint counts, which starved a visible tile
     in a window drawing at under 10 fps.
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
13. **A frame before the UI asks for its feed is dropped.** Feeds are
    made on demand, so the first frame of a camera just turned on can
    arrive before the call window asks for it. The next frame (33 ms
    later at 30 fps, up to a second for a still screen share) is drawn.
14. **macOS holds the last three pixel buffers shown**, not one. The
    player keeps them so the pool does not refill a buffer that a frame
    still in flight may read (see "Not verified").
15. **Each live player is drawn in one view.** The core keeps a single
    view to notify, not a set. Tiles and the stage are drawn only in the
    call window's panel (`group_call_panel.rs`), the stream picture
    only in `StreamPictureView`, and the live story only in the story
    viewer. A later view of the same player (the window opened again)
    takes over from the old one, which is gone by then.

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
- **Pool reuse while the GPU reads.** It is not known whether GPUI's
  Metal path keeps a `CVPixelBuffer` from going back to its pool until
  the GPU is done with it.
  - `draw_surfaces` (`third_party/gpui-pre-apple/src/metal_renderer.rs`,
    about lines 1206 to 1365) makes a `CVMetalTexture` per plane per
    frame and drops it as soon as the draw is encoded.
  - The command buffer is a plain `new_command_buffer()` (about line
    744), so Metal retains the `MTLTexture` objects until it completes
    (the comment near line 1842 relies on the same rule for buffers).
  - Apple's guidance is to keep the `CVMetalTexture` itself alive until
    the command buffer completes. Whether a retained `MTLTexture` alone
    keeps the IOSurface from being refilled by the pool is not
    documented, and it was not tested.
  - The player holds the last three buffers shown (the layer keeps up
    to three drawables, about line 192), so the pool can refill a buffer
    only after three newer frames were shown. A torn or wrong-frame tile
    under GPU load would point here. The fix would be a completed
    handler in the renderer that holds the textures.
- **The 800 px quality switch.** A sender moving between a large size
  (converted in the background) and a small one (converted during the
  draw) is covered by the ordering rule and by unit and GPUI tests
  (`an_older_picture_never_replaces_a_newer_one`,
  `a_size_change_mid_stream_redraws_at_the_new_size`). No real sender
  switching qualities was seen, and the cost of making a new pixel-buffer
  pool on each switch was not measured. A failed pool is retried after
  a second, not on every frame.
- The off-screen thresholds (two wakes, 500 ms without a paint, the
  250 ms and 1 s looks) were not tuned on a slow machine or with a
  real minimized window.
