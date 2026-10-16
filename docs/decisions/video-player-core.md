# Video player core: shared player, vsync pacing, viewer on the core

PR 1 of 6 of the video player overhaul. The full design is
`quill-tools/designs/video-player.md` (owner request, 2026-10-14: "video
player needs a serious overhaul, it's ugly and jittery"). This PR builds the
player core, paces it on the display link, and moves the media viewer and
Picture-in-Picture onto it. Controls keep their current look; they now read
the player's transport.

## Problem

Measured on `899b6732` (release build, M1 Pro, 120 Hz panel):

- The viewer pulled the AVPlayer picture on a 60 fps `background_executor`
  timer that runs freely against the display. About one picture in fifteen
  was held for 3 vsyncs and the next source picture was skipped (36 double
  steps in 765 pictures).
- The pull asked for the picture due *now* (`item.currentTime()`), not at
  the vsync the frame is shown on (8 to 16 ms later).
- A `PlaybackClock` mirrored the player every 125 ms, so the elapsed label
  and slider moved in 125 ms steps. Each media tick re-rendered the whole
  overlay.
- PiP drew PNG frames.
- Without AVPlayer, the viewer extracted PNG frames with the `ffmpeg`
  binary.

Decode and render were never the bottleneck: draw took 2 to 3 ms, and
AVPlayer always had the picture for the item time it was asked for. The
jitter came from pacing.

## Approach (from the design)

1. **One player core, `src/ui/video_player/`.**
   - `mod.rs`: the `VideoPlayer` API, a cloneable handle around shared
     state. Watchers are views notified on state changes and at most every
     100 ms while playing.
   - `transport.rs`: the state machine. States are `Opening`, `Ready`,
     `Playing`, `Buffering`, `Paused`, `Ended` and `Failed`. Position is
     extrapolated between pulls; buffered ranges are merged here.
   - `pacing.rs`: present-time prediction and the frame-request policy.
   - Three backends behind one `Backend` trait:
     - `backend_avplayer.rs` (macOS)
     - `backend_ffmpeg.rs` (the bundled shim: Linux, Windows, and macOS
       with `QUILL_VIDEO_BACKEND=ffmpeg`)
     - `backend_still.rs` (screenshot captures)
   - `element.rs`: the `VideoFrame` element.
   - Errors are `PlayerError { kind, detail }`. A failing backend moves the
     transport to `Failed` and holds the last picture. Nothing panics.
2. **The display link paces playback; no timer does.**
   - Drawing the element is the pull. It records the draw start, predicts
     when that frame reaches the screen, and takes the picture due then.
   - After painting, it calls `frame_clock::request_video_frame`, which
     notifies only the view holding the picture. That is either the very
     next display frame (`window.on_next_frame`), or a one-shot timer to
     half an interval before the frame that shows the next picture, then
     that frame.
   - `request_media_tick` stays for its other users.
3. **Only the picture redraws.** The viewer's picture is its own view,
   `media_viewer::layer::ViewerVideoLayer`. It holds a `WeakVideoPlayer`
   and sits above the cached viewer panel slice (`SliceKind::Viewer`). A
   video frame redraws the layer and the app shell, which replays the
   cached slices. The panel (controls, labels) is a watcher and redraws at
   most 10 times a second while playing.
4. **PiP** draws `video_frame` from the same shared player. It no longer
   uses PNG frames.

### AVPlayer backend

- Paced with `itemTimeForMachAbsoluteTime` (the same API as
  `itemTimeForHostTime`, taking Mach ticks). The ticks come straight from
  the display link's output time, so the picture asked for is the one due
  on the vsync the frame is shown on.
- `preferredForwardBufferDuration = 3.0` (tdesktop `kBufferFor`).
- `actionAtItemEnd = .pause`; the transport handles looping (seek 0 and
  play).
- Frame step uses `stepByCount`.
- Buffered ranges come from `loadedTimeRanges`.
- At rates other than 1, `audioTimePitchAlgorithm` is `.timeDomain`.
- Open is asynchronous: the item is created on the main thread, and a
  `cx.spawn` task polls its status every 50 ms until it is ready or has
  failed.
- `next_due` asks the video output whether the item times of the next
  eight vsyncs have a new pixel buffer (`hasNewPixelBufferForItemTime`).
  AVPlayer decodes ahead, so this finds the vsync of the next picture
  exactly.

### FFmpeg backend

- `take_frame(position_at_present)` takes the picture due at the
  predicted present time (`src/video_decode/present.rs`).
- `next_due` is the front picture's pts converted to wall time.
- The picture queue is 4 deep (tdesktop `kFramesCount`).
- Frame step pops the next picture while paused.

## Measurements

Release build with `--features frame-profiler`, M1 Pro, 120 Hz built-in
panel. Run with `QUILL_TRACE_VIDEO=1 QUILL_ASSUME_ACTIVE=1
QUILL_DEMO_LINGER_MS=15000` and `QUILL_FRAME_PROFILE`.

- **1080p60 runs:** `site/media/hero-1920.mp4` was copied over
  `docs/screenshots/fixtures/demo-clip-12s.mp4` for the run and restored
  afterwards. The `ready-video-pip` demo was run as is for the PiP numbers.
  For the viewer-only numbers, a local build had the demo's
  `open_video_pip` call removed; that build was not committed.
- **Round video:** `ready-video-note` (25 fps).
- **Analysis:** `python3 scripts/analyze-video-trace.py trace.log`.
- **Draw times:** the frame profiler's `draw` and `dirty_to_present`
  cover the main window only, so the PiP run has no draw numbers.
- **Before:** the scratch tracing on branch `profile-scratch` against
  `899b6732`, with the same analysis script.

| Scenario | New pictures | Gap p50 / p90 / p99 / max (ms), sd | Gap histogram (120 Hz vsyncs) | In expected bucket | Double steps | Draw p50 / p90 | dirty→present p50 |
|---|---|---|---|---|---|---|---|
| Viewer 1080p60, before | 765 in 13.5 s (56.4 fps) | 16.7 / 24.7 / 25.7 / 39.0, sd 3.3 | 1:21 2:656 3:81 4:5 5:1 | 85.9 % (2) | 36 | 3.05 / 5.89 ms | 8.07 ms |
| Viewer 1080p60, after | 977 in 16.5 s (59.1 fps) | 16.7 / 17.4 / 21.3 / 46.9, sd 1.7 | 1:8 2:958 3:8 4:1 6:1 | 98.1 % (2) | 1 (first 0.3 s) | 0.92 / 1.35 ms | 1.06 ms |
| PiP 1080p60, after | 968 in 16.6 s (58.4 fps) | 16.7 / 18.0 / 18.6 / 35.3, sd 1.3 | 1:4 2:958 3:4 4:1 | 99.2 % (2) | 0 | (main window only) | |
| Round video 25 fps, before | 270 in 11.5 s | 34.6 / 66.9 / 76.1 / 102, sd 14.5 | 4:145 5:55 8:34 9:13 … | 74 % (4/5) | 1 | 1.98 / 3.79 ms | 6.17 ms |
| Round video 25 fps, after | 401 in 16.5 s | 34.0 / 66.6 / 75.1 / 75.8, sd 12.7 | 4:236 5:81 8:45 9:13 … | 79 % (4/5) | 3 | 0.86 / 1.89 ms | 4.90 ms |

What the numbers show:

- **Viewer.** It now shows 60 fps content at 60 fps on an even 2-vsync
  cadence.
  - The only skipped picture is in the first 0.3 s after open (the seek
    used to warm the output).
  - The remaining irregular gaps are 7 pairs of a 3-vsync gap followed by
    a 1-vsync gap: one late frame, then a catch-up, with no picture
    skipped.
  - Draw time fell by two thirds, because a video frame now redraws the
    picture layer and not the overlay.
- **PiP** meets every target.
- **The round video is unchanged.** It still plays through the legacy
  `NativeVideo` on the 30 fps frame clock and moves to the core in PR 3.
  Its run doubles as the no-regression check for inline playback: draw p50
  went from 1.98 to 0.86 ms, the histogram is the same shape, and its
  4/5-vsync share is 79 % against 74 % before.

Targets from the design:

- ≥ 97 % of gaps in the expected buckets: met for the viewer (98.1 %)
  and PiP (99.2 %).
- p99 gap ≤ 1.5 × the content interval (25 ms at 60 fps): met (21.3 and
  18.6 ms).
- Zero double steps on local files: met in steady state. There is one in
  the first 0.3 s of the viewer run, and none in PiP.
- Draw p50 not worse: met (0.92 against 3.05 ms).
- The round video target belongs to PR 3.

Two pacing bugs were found while measuring and fixed before the numbers
above:

- **The display's own interval.** `PresentEstimator` first learned the
  frame interval from the spacing of its own draws. A player that sleeps
  between pictures draws at its own rate, so the estimate locked onto the
  content rate. The vendored macOS display link now also reports the
  refresh period (`gpui_macos::present_timing`), and the estimator uses it
  when the platform has one. Without a platform period, only the closest
  draw spacings count.
- **GPUI's inactive-window cap.** GPUI caps an unfocused window's
  next-frame callbacks at 30 a second (`WindowOptions::inactive_frame_interval`,
  33 ms by default). PiP is unfocused by design and played at 24 fps until
  its window set the interval to `None`. The main window keeps the cap:
  a video in an unfocused main window plays at up to 30 fps.

## Platform notes

**macOS.** Vendored `third_party/gpui-pre-macos`:

- `display_link.rs` keeps the latest output time per display, as an
  `Instant` and in host ticks, together with the refresh period.
- `gpui_macos::present_timing(display_id)` reads both under one registry
  lock, so the hint and the period always come from the same callback.
- The change is listed in `QUILL-CHANGES.md`.
- Surfaces stay `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` with
  hardware decode. Rounded corners over a surface still use the
  corner-mask technique.

**Linux and Windows.**

- Playback uses the FFmpeg shim backend with BGRA `RenderImage`s through
  the atlas, as before.
- Pacing uses the same request policy. The present time is predicted from
  draw starts, because there is no platform hint yet. The Windows vsync
  thread's `signal_at` hint and the NV12 surfaces are later PRs (PR 6).
- When the shim does not load, the viewer now shows an error instead of
  extracting PNG frames with the `ffmpeg` binary. That path is gone; the
  shim ships in every package.
- Without a platform period, a clip that always sleeps between pictures
  can settle on a multiple of the true vsync interval. Its pictures still
  land on real vsyncs, possibly one vsync early.
- This was not measured on Linux or Windows here. This Mac has no Linux
  or Windows C toolchain, so `cargo check --target x86_64-unknown-linux-gnu`
  stops in `wayland-backend`'s build script. CI's Linux jobs compile and
  test the non-macOS `cfg` paths.

**GIF files in the viewer** (`.gif` documents opened full screen) play
through the player only with the FFmpeg backend, because AVPlayer can't
read GIF. On macOS with AVPlayer, the viewer layer shows them with GPUI's
animated `img`.

## Supersedes

- **`codex-native-video.md`, "Decision":**
  - The paragraphs on the viewer pulling the `CVPixelBuffer` while the
    overlay requests animation frames, and on `PlaybackClock` mirroring
    the player, are replaced. The viewer holds a `VideoPlayer`, its
    picture is the `ViewerVideoLayer` paced by `request_video_frame`, and
    the controls read `player.transport()`.
  - "Other platforms keep the ffmpeg path until they get a native engine"
    no longer applies to the viewer.
- **`codex-video-cross-platform.md`, "UI":**
  - The viewer no longer goes through `NativeVideo`; it opens a
    `VideoPlayer`, whose FFmpeg backend replaces `ffmpeg_video.rs` for the
    viewer and PiP.
  - The bullet that keeps the old ffmpeg-binary frame path as the
    viewer's fallback is replaced: there is no fallback, and a viewer
    without the shim shows an error.
  - `NativeVideo` and `ffmpeg_video.rs` remain for inline tiles, round
    videos, GIFs and stories until PR 3 and PR 5. (Both files were
    deleted in PR 5, `video-player-legacy.md`.)
- **`codex-viewer-video-extras.md`, "What changed":**
  - Rotate now applies to any playing viewer video through
    `VideoPlayer::set_orientation`. The player turns each new picture
    (surface or image), so "videos whose frames Quill decodes itself" and
    the cached oriented frame no longer apply.
  - Share-at-time and the quality switch read the transport position.

## Deviations from the design

- **Viewer picture layer: no new `Layer::ViewerVideo` enum value.**
  - The viewer picture is a separate view, `ViewerVideoLayer`, rendered
    above the new cached `SliceKind::Viewer` panel slice.
  - Effect: the same as a slice. A frame redraws the picture view, and
    the cached slices replay.
  - Why not the anim-layer enum: it describes per-tile layers in the
    message list.
- **Screen-capture backend.**
  - Demo captures use `backend_still.rs`. On macOS it is a single
    `AVAssetImageGenerator` still; elsewhere it is the shim's first
    picture. It exists only in `demo-capture` builds (the only ones that
    capture), so `QUILL_DEMO_CAPTURE` never makes a production build block
    on a decode.
  - The design said "the FFmpeg backend's synchronous first frame", but
    the shim is not installed on development Macs, and captures must work
    there.
  - The still is the first picture. Captures cap the wait for it at 2 s,
    so they stay deterministic.
- **The display period comes from the platform.** The design measured the
  interval from draw starts alone. That locks onto the content rate once
  the player sleeps (see Measurements), so macOS reads the period from
  the display link.
- **Request policy uses the measured lead.**
  - The design's rule was "within 1.5 intervals: next frame; otherwise
    sleep until `next_due - interval - 1 ms`".
  - The macOS display link's output time runs about 1.3 frames ahead of
    the callback that starts the draw. With the interval rule, 60 fps
    content landed on a 1/3-vsync pattern (68.8 % in the 2-vsync bucket).
  - The policy now wakes at `next_due - lead - interval / 2`, where `lead`
    is the last predicted present time minus its draw start.
- **`automaticallyWaitsToMinimizeStalling` is `false` for every file.**
  The viewer only plays downloaded files today. The remote-file case comes
  with streaming. With it off, AVPlayer drops its rate to 0 on a stall and
  does not raise it again, so the backend re-issues the rate once the
  buffer keeps up while play is still wanted (`resumes_after_stall`). The
  buffer state (`loadedTimeRanges`, buffer empty, likely to keep up) is
  read at most every 100 ms; position and rate are read every draw.
- **The macOS present hint is per display, not per window.** The vendored
  display link is shared by the windows on one display, so
  `present_timing(display_id)` reports that display's latest output time.
  A window on that display draws for the same vsync, so the result is the
  same as the design's per-window hint.
- **No Windows present hint yet.** The design had the vendored
  `gpui-pre-windows` expose its vsync thread's `signal_at`. That moves to
  PR 6 with the other Windows work; until then Windows uses the draw-start
  estimator like Linux.
- **The backend switch is `QUILL_VIDEO_BACKEND=ffmpeg`** (the existing
  variable), not the design's `QUILL_FFMPEG_VIDEO=1`.
- **API differences.**
  - `PlayerError` has no `Audio` kind, because no backend reports an
    audio failure on its own yet; one is added with the first that does.
  - There is no `toggle()` or `seek_fraction()`; the viewer toggles and
    converts fractions itself, and PR 2's controller decides whether they
    belong in the player.
  - The design's `picture(present_at)` and `next_due()` are folded into
    `pull` (the element's draw) and `frame_demand` (after paint), because
    only the element calls them, and only in that order.
- **No 3 s FFmpeg audio buffer.** The shim's audio queue is unchanged in
  this PR. The 4-picture video queue is in.
- **`ViewerVideoStart::ExtractFrames` stays as a dead variant.** The viewer
  treats it like `PlayNow`. It goes with the rest of the legacy extraction
  code in PR 5. (Removed in PR 5.)
- **Muted loops decode no sound.** GIF-style loops in the viewer open the
  FFmpeg backend with `want_audio = false` (`Purpose::ViewerLoop`).
- **Frame step has no key binding yet.** It is in the player
  (`VideoPlayer::step_frame`: AVPlayer `stepByCount`, FFmpeg pops the next
  picture or seeks back 1.5 frames). The key arrives with the controller in
  PR 2.
- **Copy Frame on an AVPlayer surface** behaves as before this PR. It
  shows the "once the video has loaded" message, because a surface has no
  CPU pixels without a readback (the GPUI effects rule: no production
  readback). FFmpeg pictures copy.
- **PiP inactive frame interval.** The design did not foresee GPUI's
  30 fps inactive cap. PiP sets `inactive_frame_interval: None` (see
  Measurements).

## How verified

- **Unit tests:** `cargo test --features demo-capture video` runs the
  player, pacing, transport, AVPlayer helper and frame-request tests, all
  passing.
  - `transport.rs`: opening, buffering, the end, loops, seeks while paused,
    playing and ended, rate changes, buffered-range merging.
  - `pacing.rs`:
    - 60/120/144 Hz with jitter
    - missed frames
    - fresh and stale hints
    - the platform period winning over a sleeping player
    - the closest-gap rule
    - sleep or next frame for 24/25/30/60 fps on 60 and 120 Hz
    - the lead-based wake
  - `tests.rs`, with a scripted backend:
    - open then play
    - a pull asking for the predicted present time
    - a paused clip that stops asking for frames
    - loop and step
    - watcher cadence
    - drop releases the backend
    - trace columns
    - a stall buffering and resuming
    - a failing backend: `Failed`, the error reported once, the last
      picture kept, no more frames asked for
  - `backend_avplayer.rs`: when a stalled AVPlayer is told to play again.
  - `frame_clock.rs`: a frame request is in flight only for its own view
    and until its deadline. With the test platform, two draws in one frame
    ask once, the frame clears the request, and another view's leftover
    request does not hold a new one back.
- **GPUI UI test:** the viewer opens a clip in `Opening` with the layer
  present, then closes while opening, and the backend is dropped.
- **`src/video_decode/tests.rs`:** three more tests for taking the picture
  due at present time, the next due time, and frame step. All 28 pass.
- **Trace measurements:** the table above, from release builds. The raw
  logs were scratch files and are not kept; rerun the commands under
  Measurements to regenerate them.
- **Captures:** `QUILL_DEMO_CAPTURE` captures of `ready-video`,
  `ready-video-note` and `ready-video-pip`, each run twice, are
  byte-identical (`cmp`). The PNGs were reviewed but not committed. The
  PiP window itself is not in the main-window capture.
- **Not verified here:**
  - a live Telegram account
  - Linux and Windows runs (CI compiles and tests them)
  - an unfocused main window
  - VoiceOver
