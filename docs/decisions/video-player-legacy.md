# Video player, PR 5: the legacy playback code goes

## Problem

PR 1 to 4 moved the viewer, inline tiles, round videos, MP4 GIFs, stories
and Picture-in-Picture onto `src/ui/video_player/`. Four consumers and a
pile of fallback code still played video another way:

- `native_video.rs` and `ffmpeg_video.rs` (the old `NativeVideo`), used by
  the video editor, the Premium promo and the Premium feature preview.
- A second path in the history that extracted PNG frames with the `ffmpeg`
  binary and cycled them on a timer: `toggle_video_playback`,
  `spawn_video_tick` (a 400 ms timer), the `playing_video`/`video_frames`
  fields, the `video_frame` input of every history row, and the same for
  `image/gif` files (`spawn_animation_tick`, `animation_*` fields).
- `src/video/mod.rs` extraction code and its cache roots.

tdesktop has one player for all of these (`Media::Streaming`); Quill had
three.

## What changed

- **Video editor** (`video_editor.rs`, `video_editor_tools.rs`,
  `video_editor_timeline.rs`). `editor.player` is `Option<VideoPlayer>`,
  no `RefCell`. The range loop reads `transport().position`, seeks and
  pauses go through the player, rotate and flip call `set_orientation`.
  The picture is the shared `video_frame` element under the crop canvas
  (same aspect and fit as the canvas computes); the canvas paints the still
  only while the player has no picture. User pause, the playhead and the
  still without a player behave as before. A clip that fails after opening
  now sets the same note as a failed open.
  No new `Purpose`: the editor wants a muted decode capped at 2048 px, which
  is `Purpose::Inline { max_edge: 2048 }` (what it used before), so
  `Purpose::Editor` was not needed.
- **Premium promo and preview** (`premium_ui/promo.rs`,
  `premium_preview_state.rs`). A muted looping `Purpose::Inline { max_edge:
  720 }` player drawn with `video_frame(..).object_fit(Cover)`; the player is
  dropped when the file changes or the preview closes, as before. The
  30 fps `request_animation_tick` for it is gone (the element asks for
  frames itself).
- **History.** `PlaybackUi` lost `playing_video`, `video_frames`,
  `video_frame`, `video_tick`, `video_cache_file`, `pending_video_play`;
  `HistoryRowInputs` and the row builders lost `video_playing` and
  `video_frame`; `video_attachment` and `video_note_attachment` lost the
  `playing` and `frame` parameters. A click on a video's play disc (and the
  album tile's Play button) opens the viewer, which is what the supported
  path already did. A round video that is not playing inline yet asks for
  its download; the inline player starts it once local.
- **GIF files.** The design says `image/gif` documents play through "the GIF
  decoder already used for stickers". There is none: stickers use rlottie
  and the player. The decoder Quill does use is the `image` crate's, which
  GPUI's `img` runs for the viewer. So history GIFs now decode with it in
  `ui/gif_clip.rs`: pictures are scaled to 320 px, a picture equal to the
  one before is held once and its duration extended (a still stretch of N
  ticks is one `RenderImage`, not N), decoding stops at 600 held pictures or
  48 MB, and the frame is chosen from the elapsed time, redrawn through the
  frame clock (`request_animation_tick`, 30 fps) instead of a repeating
  timer. Unit tests cover the dedupe (builder and a real encoded GIF),
  looping and the bounds. MP4 GIFs already play on the player. The `ffmpeg`
  binary is no longer used for GIFs.
- **`inline_playback.rs`** keeps the GIF methods (`maybe_autoplay_gif`,
  `toggle_animation_playback`, `stop_animation_playback`,
  `resume_pending_gif`) and the voice recording tick. The `apply_ready_*`
  fixtures moved to `demos/video_fixtures.rs`.
- **`src/video/mod.rs`**: the viewer and row playback extraction
  (`playback_frames`, `viewer_playback_frames[_cancelable]`, the `VIEWER_*`
  constants, the `video-frames` and `viewer-frames` cache roots and their
  discard functions) is deleted. Kept: the probes, the video note capture,
  `round_video_size`, `is_playable_video`, and `extract_frames` with
  `ViewerFrames` and the cancelable child wait, because webm stickers
  (`sticker_playback::decode_webm`) still extract frames with `ffmpeg`.
  `animation.rs` is now only the `gif-frames` cache root (used by the webm
  sticker cache); the two GIF extraction functions and `is_playable_clip`
  are gone, and the display allowlist lost the video and viewer roots.
- **`ViewerVideoStart::ExtractFrames`** (the dead variant PR 1 left) and the
  `frames_ready` argument are removed.
- **Deleted:** `native_video.rs`, `ffmpeg_video.rs`. `native_video_rotate.rs`
  is not deleted: `VideoPlayer::set_orientation` is built on it, so it moved
  to `video_player/rotate.rs` with its tests.
- **Demos.** `ready-video` now plays `demo-clip-12s.mp4` (it used a GIF file
  posing as a video); `ready-video`, `ready-video-note`, `ready-video-send`,
  `ready-video-note-send` and `ready-gifs` just seed messages and the
  inline player or GIF clip takes over. Under `QUILL_DEMO_CAPTURE` the
  player's still backend shows the first frame and the GIF shows its first
  picture.

The diff against main is about 1,960 lines deleted and 730 added.

## What the superseded decision docs are replaced by

| Doc and paragraph | Now |
|---|---|
| `codex-native-video.md` Decision: `NativeVideo`, the AVPlayer wrapper and the frame pull | `video_player::backend_avplayer` and `VideoPlayer`; the file is gone |
| `codex-inline-video.md` and `codex-round-video-autoplay.md`: inline players and round video autoplay | PR 3 (`video-player-inline.md`) for the tiles; the 400 ms tick and ffmpeg-frame fallback they describe for unsupported platforms no longer exist |
| `codex-video-cross-platform.md` UI section: `ffmpeg_video.rs`, `QUILL_VIDEO_BACKEND` read by `NativeVideo` | `video_player::backend_ffmpeg` and its own `preferred_on_macos`; the pipeline section (`src/video_decode/`) stands |
| `codex-viewer-video-extras.md` Rotate on frames Quill decodes itself | `VideoPlayer::set_orientation` (PR 1) and `video_player/rotate.rs` |
| `video-player-core.md`, "NativeVideo and ffmpeg_video.rs remain ... until PR 3 and PR 5", and "ExtractFrames stays as a dead variant" | Both are deleted (this PR) |
| `video-player-inline.md`, "remain for the video editor, premium promo and premium preview until PR 5", and "PR 5 deletes native_video.rs" | Done |

## How verified

- Core and UI test suites pass (4,624 core, 870 UI), clippy
  `-D warnings` on the core build and the `demo-capture` build, `cargo fmt`,
  `scripts/check-lang.py`, `check-file-size.sh`, `check-hotspots.sh`.
- Demo captures of `ready-video`, `ready-video-note` and `ready-gifs` in
  light and dark (not committed): the video tile and the round video show
  the first frame of the real clip from the inline player; the GIF panel and
  the playing GIF show their first picture.
- `gif_clip` tests: a repeated picture is held once with its whole
  duration, a decoded GIF with 3 equal and 2 equal frames holds 2 pictures
  and a 500 ms loop, the held pictures are bounded.

## Not verified

- The video editor on a real clip (no editor demo exists; the existing
  editor tests still pass, but nothing drives a real player there). Loop and
  pause logic was ported line by line, not run.
- The Premium preview with a real promotion clip.
- Linux and Windows: only the macOS build ran. `gif_clip` and the other
  changes are not macOS-only and CI compiles them on Linux.
- A long real-world GIF's memory use; the bounds are tested, the dedupe
  gain on real files is not measured.

## Deviations

- No `Purpose::Editor` (above).
- `extract_frames`, `ViewerFrames` and the `gif-frames` cache root stay,
  for webm stickers; the brief listed them for deletion.
- `native_video_rotate.rs` moved into the core instead of being deleted.
- On a platform where the player is unsupported (no FFmpeg shim), MP4 GIFs
  and videos no longer fall back to ffmpeg-binary frames: they stay still
  and the viewer says so. A sponsored video's disc only counts the
  sponsored click (no viewer to open).
- The GIF decoder is new code on the `image` crate, because none existed
  for stickers; it follows the coordinator's note to hold each picture once.
- `scripts/gate.sh` runs `clippy --features ui --all-targets`, which does
  not compile the `gpui_kit::test` tests without `test-support`; the same
  checks were run with `--features demo-capture` as CI does.
