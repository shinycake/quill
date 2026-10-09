# Inline video on Linux and Windows: sized, capped and bounded

## What was actually missing

`codex-cross-platform-gaps.md` listed inline video tiles as "an empty
placeholder" off macOS. That was wrong. #452 (`codex-video-cross-platform.md`)
already plays inline clips on Linux and Windows:

- `native_video::NativeVideo` has two backends. macOS uses AVPlayer, whose
  `AVPlayerItemVideoOutput` hands back a `CVPixelBuffer` (bi-planar 4:2:0)
  that GPUI's `surface` draws. Linux and Windows use `ffmpeg_video::FfmpegVideo`
  over `quill::video_decode::Player`: the bundled FFmpeg 8.1.3 behind the
  `quillvideo` shim, one decode thread per clip, swscale to BGRA, and every
  new picture becomes a `RenderImage`. The replaced picture is retired
  through `image_budget`.
- `inline_video::InlineVideos` runs the same logic on every platform: muted
  loops, click-to-sound on round videos, the seek ring, players dropped two
  renders after their row stops rendering, and muted loops paused while the
  window is inactive or minimized (`set_window_active`).
- `message_media::inline_surface` and `round_inline_surface` take the image
  branch (`VideoPicture::clips`): images round their own corners, or clip to
  a circle for round videos. The `cfg(not(macos))` empty `div` under it
  can't be reached.
- The animation layer (`anim_layer::tile`) draws only visible tiles, and
  only visible tiles ask the frame clock for ticks (`Layer::demand` skips
  items outside their content mask). An offscreen or overdraw tile has
  nothing pulling its pictures, so its decoder fills its 3-picture queue
  and sleeps.

The packages ship the shim on both platforms (`linux-package.sh`,
`windows-package.ps1`, `windows-ffmpeg` job). `native_video::supported()`
falls back to stills only when the shim is missing, as in a plain
`cargo run` without `scripts/build-ffmpeg.sh`.

I corrected the audit row. The rest of this slice fixes what the FFmpeg path
still did worse than it should. On macOS it decodes in hardware, but on
Linux and Windows every inline tile costs software decoding, a BGRA
conversion and an atlas upload each frame.

## Telegram Desktop's rules (`history_view_gif.cpp`, `data_auto_download.cpp`)

- Autoplay GIFs, videos and round videos muted, each gated by its autoplay
  setting (`ShouldAutoPlay`). Quill does this already (`media_prefs.autoplay_gifs`,
  `autoplay_videos`, data saver, no secret or spoiler media).
- `kMaxInlineArea = 1920 * 1080`: a clip whose frame area is larger doesn't
  play inline (`ValidFrameSize` in `streamingReady` / `ChooseInlineQuality`).
  Quill was missing this. tdesktop decodes with FFmpeg on every OS, so the rule
  is about software decoding cost.
- `repaintStreamedContent` stops repainting while
  `elementAnimationsPaused()` (window not active or covered), except for an
  active round video with sound. Quill matches this through
  `set_window_active` and `sounding()`.
- Elements that leave the visible area drop their stream
  (`unloadHeavyPart`). Quill does the same by dropping players whose rows
  stop rendering.
- The default autoplay size limit is 50 MB per type (`kDefaultAutoPlaySize`).
  Quill prefetches clips of 20 MB or less for autoplay and plays any clip
  that is already downloaded. I left this as it is (see "Not in this slice").

## Changes

All four apply only where pictures are decoded in this process:
`native_video::decodes_in_process()`, which is FFmpeg on Linux and Windows,
or macOS with `QUILL_VIDEO_BACKEND=ffmpeg`. AVPlayer ignores the size hint and
holds no BGRA buffers of ours, so macOS behaves as before.

1. **Decode at the tile's size** (`InlineTile::decode_edge`). Before, every
   inline clip decoded at up to 720 px on its longer side, whatever the tile
   and display. Now the longer side is the size at which the picture covers
   its tile (`media_frame`, or the 220 pt round-video circle) in device
   pixels. The window's scale factor reaches `InlineVideos` through
   `frame_start`. Before the first frame it assumes 2x. The size is
   clamped to 16..800 px; 800 is the 400 pt tallest tile at 2x, which the
   old 720 cap blurred. Clips of unknown proportions use the cap. The
   decoder never upscales.
2. **No inline playback above 1080p in software** (`InlineTile::within_inline_area`,
   tdesktop's `kMaxInlineArea`). This is checked against Telegram's
   width/height metadata before a player opens. The shim reports only the
   scaled output size, so the real stream size can't be checked after
   opening without an ABI change. These clips show their still, and the
   viewer plays them.
3. **A memory budget across inline players** (`admits`,
   `INLINE_DECODE_BUDGET = 64 MiB`). Each player is charged its decoded size
   × (3 queued + 1 shown) × 4 bytes. If a new player would push the total
   over the budget, that tile keeps its still until a running clip stops.
   The first player is always admitted. At 1x, 57 players at 720p-GIF tile
   size fit, and 14 at 2x, so the budget is a hard ceiling that normal use
   doesn't reach. Before this change nothing bounded the total.
4. **Idle silent decoders stop polling** (`video_decode::idle_wait`). An
   idle decode thread used to wake every 40 ms to re-check its queue. That
   poll is only needed with sound: the audio thread drains the sound queue
   outside the state lock, so its wake can race the check. Without sound,
   every change happens under the state lock and calls `notify_all`: a
   picture taken, a seek, or the player dropped. Muted inline loops,
   whether paused, behind another window or offscreen, now wake once a
   second as a safety net. Viewer and sounding clips keep the 40 ms poll.

I didn't use `ui/lru.rs` here. The data is a set of live players, which is
already bounded by visibility and the budget, not a cache of recomputable
values. Remembering refusals isn't needed either: the area check is a
multiply on metadata at render time.

## Measurements

Hardware: M1 Pro, release build. Decoder: pinned FFmpeg 8.1.3 with the
`quillvideo` shim (`scripts/build-ffmpeg.sh`), the same libraries the
Linux and Windows packages ship. Clips: `testsrc2` H.264 High, 30 fps, 6 s
(180 frames). Method: an ad hoc benchmark through
`video_decode::ffi::FfiDemuxer` with `OpenOptions::inline(edge)`, best of 3.
The benchmark isn't committed.

Decode + scale + BGRA per frame, and the size of one picture:

| Clip | edge 720 (before) | edge 440 (2x round) | edge 360 (1x GIF tile) | edge 220 (1x round) |
| --- | --- | --- | --- | --- |
| 1280x720 GIF/video | 2.81 ms, 720x405, 1.17 MB | 2.38 ms, 0.44 MB | 2.24 ms, 360x203, 0.29 MB | 1.99 ms, 0.11 MB |
| 640x640 round video | 1.55 ms, 640x640, 1.64 MB | 1.47 ms, 440x440, 0.77 MB | 1.25 ms, 0.52 MB | 1.05 ms, 220x220, 0.19 MB |

Effect per playing tile at 30 fps (picture memory = 3 queued + 1 shown):

| Tile | Before | After |
| --- | --- | --- |
| 720p GIF, 1x display | 4.7 MB held, 35 MB/s converted and uploaded, 84 ms CPU/s | 1.2 MB, 8.8 MB/s, 67 ms CPU/s |
| 720p GIF, 2x display | same as before (edge 720) | same |
| Round video, 1x | 6.6 MB, 49 MB/s, 47 ms CPU/s | 0.8 MB, 5.8 MB/s, 32 ms CPU/s |
| Round video, 2x | 6.6 MB, 49 MB/s, 47 ms CPU/s | 3.1 MB, 23 MB/s, 44 ms CPU/s |

Most Linux and Windows desktops run at 1x or 1.25x. There a round video
needs 8x less picture memory and upload bandwidth and about a third less
decode CPU. GIF tiles drop by 4x and about a fifth.

Idle decode threads, 32 paused silent players for 10 s
(`getrusage` user + system):

| | CPU over 10 s |
| --- | --- |
| Before (40 ms poll) | 82.2 ms |
| After (1 s safety net) | 2.4 ms |

## Verification

- Unit tests (`ui::inline_video::tests`): decode edge per tile and scale
  (1x, 1.5x, 2x, unknown scale, very wide, unknown proportions, the cap),
  no upscaling, the 1080p area rule, and the budget (first player always
  admitted, 1x and 2x headroom, AVPlayer players cost nothing).
  `video_decode::tests::a_silent_idle_player_sleeps_long_but_wakes_when_a_picture_is_taken`
  checks that a silent player refills its queue right after a picture is
  taken, not after the 1 s wait. The 23 existing decoder tests pass
  against the pinned FFmpeg build (`QUILL_VIDEO_LIB=…`).
- Demo captures (`--screenshot-demo ready-video-note`) on macOS with
  `QUILL_VIDEO_BACKEND=ffmpeg`, using a detailed 640x640 test clip in place
  of the solid-colour fixture: the round video plays through the FFmpeg
  image path, clipped to a circle and sharp at 2x (decoded at 440 px). The
  default AVPlayer capture is unchanged.
- Windows: `cargo check --target x86_64-pc-windows-msvc --no-default-features
  --lib` passes, which covers `video_decode`. The UI crate can't be checked
  for Windows on this Mac (the GPUI build script needs `llvm-rc`). By
  reasoning, the UI changes are platform-neutral Rust in code that already
  builds for Windows. The one new `cfg` is in
  `native_video::decodes_in_process()`: the non-macOS branch is a literal
  `true`, and `ffmpeg_on_macos` is only referenced in the macOS branch.
  No new FFI or dependencies. The `windows-build` CI job compiles it.
- Linux: `cargo check --target x86_64-unknown-linux-gnu --no-default-features
  --lib` passes, and gate runs the core and UI tests on the host. The
  `linux-package` CI job builds the Linux UI.
- Not verified: real playback on a Linux or Windows desktop (frame pacing,
  atlas upload cost on wgpu/DirectX). CI runners have no display.

## Not in this slice

- The real stream size after opening (to apply `kMaxInlineArea` when
  Telegram's metadata is missing or wrong) needs the shim to report
  pre-scale dimensions, which is a `QV_ABI_VERSION` bump.
- If a clip's real proportions differ from its metadata (for example a
  non-square "round" video), it is decoded for the metadata's shape and
  can look soft. Telegram's round videos are square.
- A player keeps the size it opened with if the window moves to a display
  with a different scale. The next time the row's player is recreated
  (scroll away and back, or reopen the chat), it picks up the new scale.
- tdesktop's per-type autoplay size limit (50 MB default, user-adjustable)
  is a data policy, not a decoding cost. Quill's prefetch cap stays at 20 MB
  and downloaded clips of any size autoplay, on every platform, as before.
- No YUV texture path off macOS (GPUI has none), so pictures still go
  through BGRA and the sprite atlas. Hardware decoding (VA-API, D3D11VA)
  remains a follow-up from #452.
