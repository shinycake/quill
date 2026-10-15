# Video player: shared core, vsync pacing, kit controls

Architecture pass for the video player thread (Idan, 2026-10-14: "video
player needs a serious overhaul, it's ugly and jittery. I want you to design
a state of the art beautiful and modern player with the highest level of
craft, robust, performant and stable. then replace it throughout the app").
Copy this file to `docs/decisions/video-player.md` on the first build branch
and fill in "How verified" per PR.

Checked against Quill `899b6732`, gpui-kit 0.7.1 (gpui-component-0.7.1
`slider.rs`, `progress/`, `spinner.rs`, `kbd.rs`, `popover.rs`; gpui-base
0.7.1 `slider.rs`), the vendored `third_party/gpui-pre` (0.3.8 + Quill
patches: cross-platform `PaintSurface` payload `SurfaceFrame`),
`third_party/gpui-pre-macos` (display link with idle stop),
`third_party/gpui-pre-windows` (vsync thread, `FrameGate`, `hdr_surfaces.rs`),
`third_party/gpui-pre-linux` (Wayland frame callbacks, X11 idle frames;
renderer is the registry `gpui-pre-wgpu` 0.3.8, not vendored), tdesktop
`Telegram/SourceFiles/media/streaming/*`, `media/view/media_view_playback_controls.cpp`,
`media/view/media_view_pip.cpp`, `media/view/media_view.style`,
`history/view/media/history_view_video_message_seek.cpp`.

## Measurements (before)

Release build, M1 Pro, 120 Hz display, `QUILL_TRACE_VIDEO=1` scratch
tracing in `AvPlayerVideo::frame()` (local branch `profile-scratch`, never
pushed) plus `QUILL_FRAME_PROFILE`. Analysis script and raw logs:
`/Volumes/Idan1TB/quill-work/scratch/video-profile/`.

| Scenario | Pulls | New frames shown | New-frame gap p50 / p90 / p99 / max | Gap histogram (120 Hz ticks) | GPUI draw p50 |
|---|---|---|---|---|---|
| Round video inline, 25 fps clip, AVPlayer, 30 fps frame-clock tick | 516 in 11.5 s (gap p50 32 ms) | 270 = 23.5 fps | 34.6 / 66.9 / 76.1 / 102 ms, sd 14.5 | 4:145 5:55 8:34 9:13 | 1.98 ms |
| Viewer, 1080p60 clip, AVPlayer, 60 fps media tick | 821 in 13.6 s (gap p50 16.7 ms) | 765 = 56.4 fps | 16.7 / 24.7 / 25.7 / 39 ms, sd 3.3 | 1:21 2:656 3:81 4:5 | 3.05 ms |

What the numbers say:

- Decode and render are not the problem. Draw is 2 to 3 ms, dirty-to-present
  6 to 8 ms, and AVPlayer never fails to have a frame for the item time it is
  asked for.
- The jitter is pacing. The frame clock is a `background_executor().timer`
  at `1/fps`, free-running against the display link. A 30 fps timer pulling
  25 fps content lands 4, 5, 8 or 9 vsyncs apart: every sixth source frame is
  shown late or skipped (the "8" and "9" buckets are a dropped frame followed
  by a double hold). At 60 fps the timer is close to the content rate but
  still phase-drifts, so about one frame in fifteen is held for 3 vsyncs and
  the next source frame is skipped (36 double item-time steps in 765).
- The pull asks for the frame due at `item.currentTime()` (now), not at the
  moment the frame will actually be on screen (next vsync, 8 to 16 ms later),
  which shifts every frame half a period late on average and adds to the
  drop/double pattern.
- The viewer also mirrors the player into a `PlaybackClock` every 125 ms and
  re-renders the whole overlay on every media tick; the elapsed label and
  slider move in 125 ms steps.

## Summary of the approach

1. **One player core, `src/ui/video_player/`**, replacing the `NativeVideo`
   facade, the viewer's `PlaybackClock` mirror, the PiP PNG frames, and the
   legacy ffmpeg-binary PNG extraction (`src/video/mod.rs`,
   `src/ui/inline_playback.rs`, `src/ui/playback_state.rs`
   `playing_video/playing_animation`, viewer `extract_viewer_frames`). Every
   surface that plays video (viewer, inline tiles, round messages, GIF
   panel, stories, PiP, video editor, premium promo) holds a `VideoPlayer`
   and renders a `VideoFrame` element from it. The core owns: backend
   (AVPlayer on macOS, FFmpeg shim elsewhere), transport state machine,
   clock, buffering state, seeking, frame pacing, and errors.
2. **Present on the display link, not a timer.** A new `request_video_frame`
   in `frame_clock.rs` schedules `window.on_next_frame(notify slice)`, so the
   next pull happens at the start of the next real frame (CVDisplayLink on
   macOS, the vsync thread on Windows, `wl_callback` on Wayland, the idle
   frame timer on X11). Each pull asks the backend for the picture due at the
   *predicted present time* of that frame (next vsync), and the backend
   returns a new picture only when one is due; otherwise the last one is
   repainted. Content at 24/25/30 fps on a 120 Hz panel then lands on an
   exact 5/5/4-vsync cadence (24 fps: alternating 5 and 5, with the 3:2
   pattern only on 60 Hz panels, which is the best any player can do), and
   60 fps content lands on 2.
3. **Only the video region redraws.** Inline tiles already paint through
   `anim_layer::tile`; the viewer's picture and the PiP picture move into
   their own animation-layer slices (`Layer::ViewerVideo`,
   `Layer::StoryVideo`), so a 120 Hz pull re-renders a surface element and
   nothing else. Controls, labels and the scrubber are in the overlay slice
   and redraw at most 10 times a second (time labels change once a second,
   the scrubber thumb interpolates from the clock at paint time).
4. **Kit controls, tdesktop geometry.** The viewer controller is rebuilt on
   kit components (`Button` ghost icon, kit `Slider` for volume and speed,
   `Popover`, `Tooltip` + `Kbd`, `Spinner`, `ProgressCircle`) with a
   `VideoScrubber` composed from the kit's own base slider parts
   (`gpui_kit::base::slider::{BaseSlider, SliderTrack, SliderIndicator,
   SliderThumb}`, the same parts `Slider` is built from) because the scrubber
   needs a buffered-range layer, a hover time preview and a hover-revealed
   thumb that the packaged `Slider` has no builder for. No hand-rolled hit
   testing or drag math. Sizes follow tdesktop's `media_view.style`
   (controller 480 wide, 3 px track, 12 px thumb, 40 px play button, 32 px
   side buttons, 75 px volume) and colors come from kit tokens on a dark
   overlay that is identical in both themes, as tdesktop's media view is.
5. **Picture-in-Picture plays the real player** in a frameless floating GPUI
   window with tdesktop's PiP chrome (close/enlarge top corners, volume,
   2 px/4 px bottom bar, edge snapping, aspect-locked resize), on all three
   platforms (always-on-top where the window system has it).
6. **Windows and Linux keep the FFmpeg shim** and get the same pacing. A
   follow-up PR adds `SurfaceFormat::Nv12` to the vendored
   `SurfaceFrame` payload and a YUV pipeline to the DirectX renderer (the
   `hdr_surfaces.rs` pattern), so the decoder stops converting to BGRA and
   the atlas stops re-uploading 8 MB a frame; Linux (registry wgpu renderer,
   not vendored) stays on BGRA `RenderImage` with vsync pacing, documented.

## What the user sees

- Smooth motion everywhere video plays: no periodic stutter on round videos
  and GIFs in the chat, no micro-judder in the viewer at 60 fps, pans and
  credits scroll evenly. Elapsed time and the scrubber move continuously,
  not in 125 ms steps.
- The viewer controller: a single 480 px rounded panel at the bottom with
  the scrubber on top (buffered range in a lighter tint, hover shows a time
  bubble at the pointer, the thumb appears on hover and grows while
  dragging), elapsed on the left and remaining on the right in 12 px
  semibold, and under it play/pause (40 px, centered), the volume toggle and
  75 px slider on the left, speed, quality, PiP and fullscreen on the right.
  The panel fades in over 200 ms and out over 600 ms after 1.1 s of rest
  (tdesktop `mediaviewWaitHide`, `mediaviewShowDuration`,
  `mediaviewHideDuration`), never while the pointer rests on it or a drag
  is in progress.
- Buffering shows a kit `Spinner` in the center after 500 ms (tdesktop
  `kWaitingShowDelay`), never a flash on a short stall. Downloading shows a
  `ProgressCircle` with the percentage and a cancel on click, the picture
  behind it is the thumbnail. An unplayable file shows the thumbnail, a
  short message ("This video can't be played") and a "Download" or "Open
  with…" button; the viewer never closes on its own.
- Keyboard in the viewer (tdesktop `OverlayWidget::handleKeyPress`):
  Space/K pause, J/L long seek, Left/Right short seek in fullscreen, 0–9
  jump to tenths in fullscreen, `,`/`.` frame step when paused, Alt+Enter or
  Ctrl/Cmd+Enter fullscreen, Esc leaves fullscreen then the viewer. Holding
  Space for 200 ms plays at 2x until release (tdesktop
  `kSpeedBoostDelay`), with the speed badge showing "2x" while held. Each
  button's tooltip carries its key as a kit `Kbd`.
- Round videos keep the seek ring and the sound toggle; GIFs and inline
  videos keep the mute badge and the duration pill. They just stop
  stuttering, and a tile scrolled out of view stops decoding within a frame.
- PiP: a floating 320 px window (tdesktop `kPipDefaultSize` logic, minimum
  120) that snaps to screen edges, resizes from any edge keeping the aspect,
  shows close and "back to viewer" in the top corners and a thin playback
  bar at the bottom that grows from 2 to 4 px on hover; clicking the picture
  pauses. Works on macOS, Windows and Linux X11; on Wayland it is a normal
  floating window the compositor may not keep on top (documented).
- The same core drives stories (vertical 9:16, muted by default until the
  story is tapped, as today), the video editor preview (scrubbing frames),
  and the Premium promo clips.

## The player core (`src/ui/video_player/`)

Files, each under 1,000 lines: `mod.rs` (public API, `VideoPlayer`),
`transport.rs` (state machine + clock, pure, unit-tested), `pacing.rs`
(present-time prediction, pure, unit-tested), `backend_avplayer.rs`
(macOS), `backend_ffmpeg.rs` (all platforms, used on macOS only for the
demo-capture path and when `QUILL_FFMPEG_VIDEO=1`), `element.rs`
(`VideoFrame` element: surface or image, object fit, corner masks),
`controls/` (the viewer controller, scrubber, speed popover, PiP chrome).

### `VideoPlayer` API (what consumers use)

```
VideoPlayer::open(path, Purpose, cx) -> VideoPlayer         // never blocks the UI thread
  Purpose::Inline { max_edge } | Purpose::Viewer | Purpose::Story | Purpose::Editor
player.play() / pause() / toggle() / seek(secs) / seek_fraction(f)
player.step_frame(±1)                                        // paused only
player.set_volume(f32) / set_rate(f64) / set_looping(bool) / set_orientation(turns, mirrored)
player.transport() -> Transport { state, position, duration, buffered: Vec<Range<f64>>, rate, volume, has_sound }
player.picture(present_at: Instant) -> Option<Picture>       // the frame due at present_at
player.next_due() -> Option<Instant>                         // when the next picture becomes due, if known
player.take_error() -> Option<PlayerError>
```

`state` is `Opening | Ready | Playing | Buffering | Paused | Ended |
Failed`. `Opening` runs the backend open on the background executor
(FFmpeg probing can take tens of milliseconds on a cold file; AVPlayer's
`AVPlayerItem` status is observed until `readyToPlay`). `Buffering` is
entered from `Playing` when the backend reports no picture due and no
audio ahead (FFmpeg: video queue empty and not at EOF; AVPlayer:
`isPlaybackBufferEmpty` true and `isPlaybackLikelyToKeepUp` false, polled
on the pull, no KVO), and left when playback resumes. `Ended` holds the
last frame; looping players never enter it.

Position is read from the backend on every pull and cached in the
transport together with the pull's wall time; between pulls the UI
extrapolates `position + (now - pulled_at) * rate` while `Playing`, which
is what makes the scrubber continuous without a timer. Duration comes
from the backend once ready, falling back to the message's
`duration_secs` while opening.

### Frame pacing (`pacing.rs`)

`PresentEstimator` keeps the last N (8) frame start times handed to it by
the `VideoFrame` element (the `Instant` when GPUI began the draw that
contains the element) and the platform's present-time hint when one
exists:

- macOS: the vendored display link already converts `CVTimeStamp` host
  time to `Instant` (`display_link.rs` `frame_request_instant`). The
  patch records the callback's `output_time` (the vsync this draw targets)
  on the window state and exposes it as
  `gpui_macos::next_present_time(&Window) -> Option<(Instant, u64 host_time)>`.
  The AVPlayer backend passes the host time straight into
  `AVPlayerItemVideoOutput::itemTimeForHostTime`, then
  `hasNewPixelBufferForItemTime` / `copyPixelBufferForItemTime` for that
  item time. That is the API's intended use and removes the half-frame
  lateness.
- Windows: the vsync thread's `signal_at` reaches the window in
  `RequestFrameOptions`; the next present is `signal_at + interval`. Not
  exposed to elements today; the patch stores it on the window and exposes
  `gpui_windows::next_present_time`. Fallback: estimator.
- Linux: Wayland `wl_callback` done time (`u32` ms) and X11 idle-frame
  timer have no reliable present stamp; the estimator alone is used, with
  the interval measured from draw starts (clamped to 1/240..1/20 s, median
  of the last 8).

`present_at = platform hint, else last_draw_start + measured_interval`.
Pure functions, tested: steady 120 Hz input gives 8.33 ms; a 60 Hz panel
with one missed frame still predicts the next vsync; a stale hint (older
than two intervals) is ignored.

Request policy (in `frame_clock.rs`, replacing `request_media_tick` for
video): `request_video_frame(slice, next_due)`.

- `next_due` within 1.5 intervals or unknown: `window.on_next_frame` →
  notify the slice. The pull happens at the start of the next draw.
- `next_due` further away (30 fps content on 120 Hz; a paused player with a
  pending seek): a one-shot `background_executor().timer(next_due -
  interval - 1 ms)` followed by `on_next_frame`. Halves wasted redraws for
  low-rate content; the final alignment is still the display link.
- Window inactive: muted loops stop asking (today's `tick_wanted`); a
  player with sound keeps the timer path so audio stays in sync when the
  window comes back. Occluded or minimized windows draw no frames; audio
  continues through its own output and the first draw after un-occlusion
  resyncs (AVPlayer does this itself; FFmpeg `take_due` drops the stale
  pictures).
- An element that was not painted for two consecutive pulls (tile scrolled
  away, viewer closed) stops requesting; the player pauses inline loops
  after 250 ms off-screen and releases their decode budget after 2 s
  (today's `InlineVideos` policy, kept).

### Backends

**AVPlayer (macOS).** Kept: hardware decode, HDR-ready, `AVPlayerItemVideoOutput`
with `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` painted through
`surface()`. Changes: `itemTimeForHostTime` pacing (above);
`preferredForwardBufferDuration = 3.0` (tdesktop `kBufferFor`);
`automaticallyWaitsToMinimizeStalling = true` for remote/partial files,
false for fully local ones; `audioTimePitchAlgorithm = .timeDomain` for
rates ≠ 1 (voice stays intelligible at 1.5–2x, tdesktop uses the same
class of algorithm through its own resampler); `actionAtItemEnd = .pause`
with the loop handled by the transport (`seek(0)` + `play`, no
`AVPlayerLooper`, so inline loops and the viewer share one code path).
`step_frame` maps to `AVPlayerItem::stepByCount`. Buffered ranges from
`loadedTimeRanges`. Open becomes asynchronous: the item is created on the
main thread (AVFoundation requirement) but `Ready` waits for the status
observer, delivered through a `cx.spawn` poll at 50 ms until ready or
failed, never a blocking wait.

**FFmpeg shim (Windows, Linux, demo capture).** Kept: the `quillvideo` C
shim over bundled FFmpeg, the decode thread, `Clock`, audio-master sync
(`AUDIO_OUTPUT_LATENCY`, `SYNC_TOLERANCE`), `take_due`. Changes:
`take_frame` takes a `position_at_present` argument computed from the
clock at the predicted present time, not at pull time; `next_due` returns
the front picture's pts converted to wall time so the request policy can
sleep until just before it; the picture queue grows from 3 to 4 frames
(tdesktop `kFramesCount`) and the decoder keeps `kBufferFor`-style 3 s of
audio; `step_frame(+1)` pops the next queued picture, `step_frame(-1)`
seeks to `pts - 1.5/fps` and takes the first picture; `Buffering` is
reported from the queue state. The BGRA `RenderImage` upload stays in
PR 1; PR 6 adds NV12 surfaces for DirectX (below).

Both backends report errors as `PlayerError { kind: Open | Decode |
Audio | Unsupported, detail: String }` and never panic; a backend that
fails mid-stream moves the transport to `Failed` holding the last picture.

### `VideoFrame` element (`element.rs`)

`video_frame(&player, present_hint).object_fit(Contain|Cover).corner_radius(px)`.
Paints `Picture::Surface(CVPixelBuffer)` through `surface()` on macOS and
`Picture::Image(Arc<RenderImage>)` through `img()` elsewhere, and in
`paint` records the draw start into the player's estimator and calls
`request_video_frame` when the transport is `Playing` or `Buffering`
(so the request lives with the element that needs it, not with each
consumer). Corner radii: surfaces have no radius support in GPUI; the
element keeps today's `corner_mask` technique (four corner quads in the
backdrop color) for surfaces and uses real `rounded_*` for images, both
behind the one builder, so consumers do not know the difference. The PR
that vendors a radius into `PaintSurface` is listed under open points.

## Controls (`controls/`)

The controller is a `RenderOnce` `VideoController` taking a
`&Transport`, the `Entity<SliderState>`s it owns through a
`VideoControlsState` kept on `ViewerUi`, and callbacks into `QuillApp`
(the existing `toggle_viewer_video`, `seek_viewer_to`, `set_viewer_rate`,
`toggle_playback_mute`, `open_video_pip`, `viewer_toggle_fullscreen`).

Geometry from `media_view.style` (`mediaviewController*`,
`mediaviewPlayback*`, `mediaviewVolume*`), expressed with `px()` constants
in one `metrics` module and kit tokens for everything else:

| Part | tdesktop | Quill |
|---|---|---|
| Panel | 480 × 72, radius 9, `mediaviewSaveMsgBg` (black 0.75) | `w(px(480.))`, `h(px(72.))`, `rounded(cx.theme().radius_lg)`, `bg(overlay_panel())` = `black().opacity(0.75)` through the viewer's token helper |
| Scrubber | `mediaviewPlayback` top 49, width 3, seek 12, left/right 4 | `VideoScrubber`: track 3 px, hover 5 px, thumb 12 px (hover reveal 120 ms, `SliderThumb` with `opacity` on hover/drag), buffered layer `fg.opacity(0.3)` under the fill `accent()`; time bubble on hover via kit `Tooltip` anchored to the pointer fraction |
| Time labels | `mediaviewPlayProgressLabel` 12 px semibold, left/right 8 | `text_xs().font_semibold()` `fg.opacity(0.9)` (AA on the panel), elapsed left, `-remaining` right |
| Play | 40 px, centered, top 2 | kit `Button` ghost icon `size(px(40.))`, Lucide `Play`/`Pause`, tooltip "Pause" + `Kbd` Space |
| Volume | toggle 32 at left 6 top 6; slider width 75 | kit `Button` ghost 32 px + kit `Slider` `w(px(75.))`, `bg(accent())`, mute toggles the icon tier (`VolumeX` / `Volume1` / `Volume2` by level) |
| Right group | speed, quality, PiP, fullscreen, 32 px, right 8, skip 4 | kit `Button` ghost 32 px each, `gap(px(4.))`; speed shows the rate as text ("1.5x") in a `Badge`-style pill when ≠ 1 |
| Speed menu | `mediaviewSpeedMenu`: 0.5 … 2.5 slider + presets | existing `speed_dial` `Popover` restyled: kit `Slider` 0.5–2.5 step 0.1, preset rows 0.5 / 0.75 / 1 / 1.25 / 1.5 / 1.75 / 2 as kit menu items with a check on the current |
| Controls hide | wait 1100, show 200, hide 600 | existing `VIEWER_WAIT_HIDE_MS` 1100, `VIEWER_SHOW_MS` 200; `VIEWER_FADE_MS` becomes 600 for the hide direction (today 150 both ways, a deviation from tdesktop) |
| Waiting | `kWaitingShowDelay` 500 ms, radial | kit `Spinner` `size_8` after 500 ms, centered over the picture |
| Download | radial with cancel | kit `ProgressCircle` 48 px with the percentage label; click cancels |

Light and dark themes: the viewer backdrop and controller are dark in
both, as in tdesktop; only `accent()` and `text_on_fill()` follow the
theme. Focus rings use `cx.theme().ring` (kit default) so keyboard users
see where they are. Every button has `accessibility_label` and a tooltip.

Round video (`inline_video.rs`): the seek ring, its 220/150/150 ms
easings and the sound toggle stay as they are; only the frame source
changes to `VideoFrame`. Inline tiles: mute pill, duration pill and the
download `ProgressCircle` stay.

PiP chrome (`controls/pip.rs`), from `media_view_pip.cpp`: borderless
floating window (`titlebar: None`, `WindowKind::Floating`, always-on-top
through `window_control::set_always_on_top` where it returns true),
default width 320 (tdesktop `kPipDefaultSize` ×, clamped to a quarter of
the screen), minimum 120, 16 px edge snapping, 10 px resize margins on
all edges keeping the aspect, 20 px inset; close (top-right) and enlarge
(top-left) as 32 px kit ghost buttons that fade with the controls; volume
toggle bottom-left; a 2 px bottom bar growing to 4 px on hover with the
buffered tint and the `accent()` fill; click on the picture toggles
pause; drag anywhere else moves. Opening PiP closes the viewer and hands
the `VideoPlayer` over (no reopen, no seek); "enlarge" hands it back and
reopens the viewer on the same message.

## Platform notes

### macOS (first)

Vendored `third_party/gpui-pre-macos`: `display_link.rs` records the
callback's `output_time.hostTime` and its `Instant` next to the existing
signal; `window.rs` keeps the latest pair per window;
`gpui_macos::next_present_time(window)` reads it. About 40 lines, noted in
`QUILL-CHANGES.md`. Nothing else in GPUI changes on macOS.

Hardware decode and the surface path are unchanged. HDR video (10-bit
HLG/PQ) is not in this design; the HDR viewer design leaves the pipeline
slot open, and the AVPlayer backend will hand such buffers through
unchanged once the renderer accepts them.

### Windows

PR 1 ships the FFmpeg backend with vsync pacing and BGRA images. The
vendored `gpui-pre-windows` gets `next_present_time` from the vsync
thread's `signal_at` (stored on the window in `on_request_frame`).

PR 6 (NV12 surfaces): `third_party/gpui-pre` `SurfaceFormat::Nv12`
(`SurfaceFrame::new` validates `w*h*3/2` bytes; `bytes_per_pixel` becomes
`byte_len(w, h)`), the FFmpeg shim gains `quillvideo_set_output(NV12)` so
`sws_scale` is skipped when the decoder output is already `yuv420p`/`nv12`
(the common case for H.264/HEVC/VP9/AV1), and `directx_renderer` gets a
`yuv_surface` pipeline next to `hdr_surfaces.rs`: two textures
(`R8_UNORM` luma, `R8G8_UNORM` chroma) updated with `UpdateSubresource`
per new frame, BT.601/709 matrix picked from the stream's colorspace,
full/limited range flag. Expected effect at 1080p60: upload 3 MB/frame
instead of 8 MB and no CPU color conversion (today's `sws_scale` is
roughly 4 ms per 1080p frame on one core). Measured before/after with the
same trace lines on a Windows machine before the PR is opened.

Hardware decode (D3D11VA) through the shim is a separate follow-up; not
needed for smoothness at 1080p60, needed for 4K.

### Linux

Wayland: `pending_frame_callback` already gates draws on the compositor's
frame callback, so `on_next_frame` is vsync-aligned; the estimator supplies
the present time. X11: the idle-frames timer (`codex-idle-frames-x11-windows`)
runs at the refresh rate while something requests frames; same estimator.
Pictures stay BGRA `RenderImage` through the atlas because the renderer is
the registry `gpui-pre-wgpu` and the HDR work did not vendor it. If
measurements on Linux show the atlas upload as the bottleneck (unlikely
below 1080p60), the follow-up is to vendor `gpui-pre-wgpu` and add the
same NV12 pipeline in WGSL; documented fallback until then. Always-on-top
for PiP: X11 through `_NET_WM_STATE_ABOVE` (already in `window_control`),
Wayland none.

## Migration plan (what replaces what)

| Today | After |
|---|---|
| `src/ui/native_video.rs` `NativeVideo` + `ffmpeg_video.rs` | `video_player::{VideoPlayer, backend_avplayer, backend_ffmpeg}`; the old files are deleted in PR 5 once no consumer imports them |
| Viewer: `video.rs` `play_native_viewer_video`, `playback.rs` 125 ms `spawn_viewer_tick`, `PlaybackClock` mirror, `video_frames`/`video_fps`/`frame_cache_file`/`extracting`/`extract_*` fields, `extract_viewer_frames` PNG fallback | `ViewerUi.player: Option<VideoPlayer>` + `VideoControlsState`; `Layer::ViewerVideo` slice paints `video_frame`; controls read `player.transport()`; no tick task; demo capture uses the FFmpeg backend's synchronous first frame |
| `overlay.rs` `request_animation_tick(30)` / `request_media_tick(60|10)` for the video | removed; `VideoFrame::paint` requests frames itself |
| `overlay_controls.rs` `viewer_video_controls` (640 px panel, kit `Slider` seek) | `controls::VideoController` (480 px, `VideoScrubber`) |
| `video_pip.rs` (PNG frames, macOS only, Play/Return buttons) | `controls::pip` on the shared player, all platforms |
| `inline_video.rs` `InlineTile` frame pull on the 30 fps tick | `InlineTile.player: VideoPlayer`, `video_frame` inside `anim_layer::tile`, same budget and prefetch rules |
| `story_more.rs` `sync_story_video` + `request_media_tick(30)` | `VideoPlayer` with `Purpose::Story`, `Layer::StoryVideo` |
| `inline_grid.rs` GIF panel | unchanged API (`inline_videos`), new pacing for free |
| `video_editor_tools.rs`, `premium_ui/promo.rs`, `premium_preview_state.rs` | `VideoPlayer` with `Purpose::Editor` / `Purpose::Inline` |
| `src/video/mod.rs` (ffmpeg binary PNG extraction), `inline_playback.rs`, `playback_state.rs` `playing_video/playing_animation`, `animation.rs` frame cycling for video | deleted; `image/gif` documents play through the GIF decoder already used for stickers, videos through `VideoPlayer`; demos `ready-video`, `ready-video-note`, `ready-video-pip` move to the player with `QUILL_DEMO_CAPTURE` first-frame freeze |
| `frame_clock.rs` `request_media_tick` | kept for sound-only users (voice waveform); video uses `request_video_frame` |

Decision docs superseded in part: `codex-native-video`, `codex-inline-video`,
`codex-video-cross-platform`, `codex-parity-macos-pip`, `codex-viewer-controls`,
`codex-viewer-video-extras`. Each build PR's decision doc names which
paragraphs of those it replaces.

## Memory and performance

- One decoded picture per player on the UI side (the surface or image the
  element paints) plus the backend's 4-picture ring. `INLINE_DECODE_BUDGET`
  (64 MiB) and `MAX_INLINE_AREA` keep governing inline tiles; the viewer's
  `OpenOptions::viewer` cap (3840 × 2160) stays.
- At 120 Hz a playing 30 fps clip costs about 60 redraws a second of one
  slice (the sleep-until-due policy skips the other 60), each a single
  surface primitive. The frame profiler is run before and after each PR
  (`QUILL_FRAME_PROFILE`) and the trace lines from the scratch branch
  become a permanent `QUILL_TRACE_VIDEO` feature in the core (CSV to
  stderr, off by default) so the histogram above can be regenerated.
- Targets, same scenarios as the table above: new-frame gap histogram with
  ≥ 97 % of frames in the two expected buckets (4/5 ticks for 25 fps, 2 for
  60 fps), p99 gap ≤ 1.5 × the content interval, zero double item-time
  steps on local files, draw p50 unchanged or lower, no extra redraw of the
  conversation while a tile plays (slice trace).

## Never-crash and safety

- No `unwrap`/`expect`/indexing in the core; backends return `Result` and
  the transport moves to `Failed`. The AVPlayer backend's `objc2` calls are
  on the main thread only; the status poll is a `cx.spawn` task that checks
  `WeakEntity` upgrades. The shim exposes only C functions Rust calls (no Rust
  callbacks), so no unwind can cross into C; the decode thread's loop
  stays panic-free by construction (`panic = "deny"` lint).
- `RefCell` borrows in `InlineVideos` and `VideoControlsState` are scoped
  to one statement; nothing inside `anim_layer::tile` or the controller's
  closures reads state the caller holds (the #747 rule).
- The vendored patches replace nothing that asserts; the one existing
  `assert_eq!` on the surface pixel format in `metal_renderer.rs` was
  already scheduled for removal by the HDR design and is confirmed gone in
  PR 1's review.
- Deterministic shutdown: dropping a `VideoPlayer` stops its decode thread
  and output within one tick; the PiP window drop returns the player or
  drops it, never leaving an orphan thread. Tests cover drop while
  `Opening`.

## Tests

- `transport.rs`: state transitions (open → ready → playing → buffering →
  playing → ended; loop; seek while paused/playing/ended; rate change keeps
  position; failure holds last position), extrapolated position at rate
  1 / 1.5 / 0.5, buffered-range merge.
- `pacing.rs`: present estimation at 60/120/144 Hz with jitter, missed
  frames, stale hints, the sleep-until-due decision for 24/25/30/60 fps on
  60 and 120 Hz; `take_due` against predicted present time (no late pick,
  newest due wins).
- Scrubber math (`controls/scrubber.rs`): pointer fraction → seconds,
  buffered layer geometry, hover time string, drag preview vs. release.
- Key handling: extended `viewer_key_action` table (frame step, Space hold
  boost timing as a pure `BoostState`), PiP keys (Esc closes, Space
  toggles).
- GPUI UI tests: the controller renders both states (playing/paused,
  muted/level, boosted) in light and dark; PiP window opens, snaps, and
  hands back; inline tile pauses off-screen; viewer closes while
  `Opening` without a crash.
- Backend integration (ignored by default, run by the builder on the
  Mac): open the two fixtures, play 2 s, assert ≥ 95 % of expected frames
  and monotonic positions, seek accuracy ≤ 1 frame, frame step ±1.
- Visual: `QUILL_DEMO_CAPTURE` of `ready-video-pip` (controller), the
  speed popover, PiP, buffering and failed states, in both themes; before
  and after screenshots in each PR.

## Build plan (Opus 5.5, medium; one PR at a time, in order)

1. **Core + macOS pacing + viewer.** `src/ui/video_player/` (API,
   transport, pacing, AVPlayer backend, FFmpeg backend behind the same
   trait, `VideoFrame` element, `request_video_frame`), the display-link
   present-time patch in `gpui-pre-macos`, the viewer moved onto the core
   with `Layer::ViewerVideo`, the legacy PNG viewer path removed, the
   `QUILL_TRACE_VIDEO` feature. Controls stay the current ones, re-pointed
   at the transport. Before/after histograms in the decision doc. Target
   ≤ 3,500 lines.
2. **Controller and scrubber.** `controls::VideoController`,
   `VideoScrubber`, speed popover restyle, keyboard additions (frame step,
   Space boost), fade timings, waiting/download/failed states. UI tests
   and captures in both themes.
3. **Inline tiles, round videos, GIF panel, stories** on the core; delete
   `native_video.rs`/`ffmpeg_video.rs` consumers there; off-screen policy
   tests; slice-trace proof that only the tile redraws.
4. **PiP** on the shared player, tdesktop chrome, all platforms; delete
   `video_pip.rs`.
5. **Legacy removal.** `src/video/mod.rs`, `inline_playback.rs`,
   `playback_state.rs` video paths, `animation.rs` video frame cycling,
   demos re-based on the player; editor and promo consumers; delete
   `native_video.rs` and `ffmpeg_video.rs`.
6. **Windows NV12 surfaces** (vendored core format, shim output mode,
   DirectX pipeline), measured on Windows. Linux stays documented.

Each PR: worktree, `ssd-target.sh`, kit skills loaded, rules.txt, gate in
the foreground, rebase before push, decision doc, parity fragment only
for README ids fully done, independent reviewer, merged through queue.sh,
cleanup of target and scratch. Testing against the live account happens
only in Saved Messages.

## Deviations and open points

- Scrub preview: tdesktop seeks only on release and previews the time
  label. The AVPlayer backend can afford coarse seeks (tolerance 0.5 s) at
  10 Hz during the drag, so macOS previews the picture while dragging; the
  FFmpeg backend previews the label only. One constant,
  `SCRUB_PREVIEW_HZ`, gates it per backend; the decision doc states the
  difference.
- Corner radius on surfaces stays the corner-mask technique until
  `PaintSurface` gains radii in the vendored core (a small patch to
  `scene.rs`, the metal and DirectX surface shaders); proposed as a
  follow-up after PR 6, not blocking.
- Hardware decode on Windows/Linux and 10-bit HDR video are follow-ups.
- Wayland PiP cannot be kept on top; documented.
- 60 Hz panels showing 24 fps content keep a 3:2 cadence; that is the
  display's limit, and the estimator makes it a steady 3:2 rather than a
  random one.
