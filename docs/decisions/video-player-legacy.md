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
  The picture is the shared `video_frame` element under the crop canvas,
  laid out in exactly the box the canvas paints the crop over
  (`FittedPlayer`, sharing `picture_fit` with the canvas); the canvas
  paints the still only while the player has no picture. User pause, the
  playhead and the still without a player behave as before. A player at
  the file's end (`Ended`, when the kept range runs to the end) seeks to
  the range's start before it plays again, from the loop and from a
  click. A clip that fails after opening drops its player (the still
  shows) and sets the same note as a failed open.
  No new `Purpose`: the editor wants a muted decode capped at 2048 px, which
  is `Purpose::Inline { max_edge: 2048 }` (what it used before), so
  `Purpose::Editor` was not needed.
- **Premium promo and preview** (`premium_ui/promo.rs`,
  `premium_preview_state.rs`). A muted looping `Purpose::Inline { max_edge:
  720 }` player drawn with `video_frame(..).object_fit(Cover)` and 12 px
  corners (`corner_radii`, the dialog background behind them, as
  `NativeVideo` drew them; the stage under it is rounded the same); the player is
  dropped when the file changes or the preview closes, as before. The
  30 fps `request_animation_tick` for it is gone (the element asks for
  frames itself).
- **History.** `PlaybackUi` lost `playing_video`, `video_frames`,
  `video_frame`, `video_tick`, `video_cache_file`, `pending_video_play`;
  `HistoryRowInputs` and the row builders lost `video_playing` and
  `video_frame`; `video_attachment` and `video_note_attachment` lost the
  `playing` and `frame` parameters. A click on a video's play disc (and the
  album tile's Play button) opens the viewer, which is what the supported
  path already did.
- **Clips on demand** (`inline_players/demand.rs`). With autoplay off or
  data saver on, nothing plays by itself, but a click still plays, as in
  tdesktop. A click (or Space on the focused row) on a round video that
  is not playing inline asks for it with `Demand::Sound`: the file is
  downloaded first if need be, and once its row renders it local the
  inline player opens and plays it once from the start with sound
  (`toggle_sound`, the same call a click on a muted loop makes); when it
  ends the demand is done and, with autoplay off, the still shows again.
  The Play disc of an MP4 GIF asks with `Demand::Loop`: a muted loop on
  the inline player while the row stays on screen. A demand goes with its
  slot (row scrolled away, chat left, viewer opened). The click handler
  and the keyboard path call the same `play_video_note`.
- **GIF files.** The design says `image/gif` documents play through "the GIF
  decoder already used for stickers". There is none: stickers use rlottie
  and the player. The decoder Quill does use is the `image` crate's, which
  GPUI's `img` runs for the viewer. So history GIFs now decode with it in
  `ui/gif_clip.rs`:
  - Only real GIF bytes: the file must start with `GIF87a`/`GIF89a`. A
    video labelled `image/gif` comes back `NotGif` and is handed to the
    inline player on demand (`Demand::Loop`).
  - Bounds, since the file is untrusted: a canvas over 4096 px on a side
    or over 16 Mpx is refused before any frame; the decoder runs under
    `image::Limits` (4096 x 4096, 192 MB of allocations: canvas, one
    frame and one composite at the 16 Mpx cap); reading stops after 3000
    frames or 5 minutes of playing time, at 600 held pictures or 48 MB of
    them. A frame that fails to decode ends the clip with the frames
    before it; only a GIF with no good frame fails.
  - Cancellation: each decode has an `AtomicBool`, checked before every
    frame and set by `stop_animation_playback`. There is one decode per
    GIF (`PlaybackUi::gif_decodes`): playing a GIF whose decode still runs
    clears its flag instead of starting a second one, and a decode that
    already gave up (`Cancelled`) is restarted when it reports back.
  - Size: pictures are decoded at the tile's size in device pixels
    (`InlineTile::decode_edge`, the same as inline videos, capped at
    `INLINE_MAX_EDGE`, 800 px), not a fixed 320 px.
  - A picture equal to the one before is held once and its duration
    extended (a still stretch of N ticks is one `RenderImage`, not N).
  - Drawing: the row hands the clip (`GifPlay`, an `Rc` of it and the
    start time) to `anim_layer::tile`, which picks the picture due at
    paint time and asks the frame clock for the GIF's own cadence (one
    redraw per shortest held picture, at most 50 fps) while the row is
    painted. The history no longer ticks at 30 fps for a GIF; only
    outside an animation layer does the row ask the history for ticks,
    at the same cadence. The badge and the hover Pause disc are drawn in
    the tile, since the layer paints over the row.
  - The clip retires its pictures to the atlas sweeper when it drops.
  MP4 GIFs already play on the player. The `ffmpeg` binary is no longer
  used for GIFs.
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

## Measurements

`QUILL_TRACE_TICKS=1`, the `ready-gif-playback` demo (one playing
`image/gif`, a 2-picture loop at 500 ms each), window active, 1200 x 900,
steady state over 4 one-second windows, debug build on macOS:

| | Conversation slice renders/s | Animation layer renders/s | Frame clock |
|---|---|---|---|
| Before (this PR's first version) | 29 | 58 | history at 30 fps |
| After | 0 | 4 | layer at 2 fps (the GIF's cadence) |

A GIF that plays no longer re-renders the history at all; its tile is
redrawn when its picture can change.

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
  and a 500 ms loop, frame delays (10 ms and less play at 100 ms), fitted
  sizing (down, never up) and decoding at the size asked for, the held
  count and byte bounds, the frame bound, malformed and truncated files
  (no panic; an MP4 is `NotGif`), an oversized canvas refused, a cancel
  flag stopping between frames, and a late bad frame keeping the frames
  before it.
- GPUI tests (`video_player/tests/on_demand.rs`, scripted players): with
  autoplay off a clicked round video that is not downloaded waits, then
  plays from the start with sound once local, and rests on its still
  when it ends; a clicked MP4 GIF loops muted on the inline player and
  never reaches the GIF decoder; a video labelled `image/gif` is handed
  from the decoder to the player.
- Demo captures after the review fixes (not committed): `ready-gifs` and
  `ready-gif-playback` show the playing GIF's first picture with its
  badge, drawn by the layer; `ready-video-note` shows the round video's
  first frame from the inline player.

## Not verified

- The video editor on a real clip (no editor demo exists; the existing
  editor tests still pass, but nothing drives a real player there). Loop and
  pause logic was ported line by line, not run; the `Ended` seek and the
  fitted picture box were not seen on screen either.
- The hover Pause disc over a playing `image/gif`, now drawn by the
  animation layer: the badge shows in the captures, hover can't be
  captured.
- The Premium preview with a real promotion clip.
- Linux and Windows: only the macOS build ran. `gif_clip` and the other
  changes are not macOS-only and CI compiles them on Linux.
- A long real-world GIF's memory use; the bounds are tested, the dedupe
  gain on real files is not measured.

## Deviations

Every place this PR differs from the PR 5 design, and why:

- **No `Purpose::Editor`.** The editor wants a muted decode capped at
  2048 px, which `Purpose::Inline { max_edge: 2048 }` already is (what it
  used before); a new purpose would be the same thing under another name.
- **`extract_frames`, `ViewerFrames` and the `gif-frames` cache root
  stay.** The design listed them for deletion, but webm stickers
  (`sticker_playback::decode_webm`) still extract frames with `ffmpeg`
  (VP9 with alpha needs `libvpx-vp9`, which the player does not use).
  `extract_frames` lost its `start_timestamp` argument, which only video
  playback used.
- **`native_video_rotate.rs` moved instead of being deleted.**
  `VideoPlayer::set_orientation` is built on it; it is now
  `video_player/rotate.rs` with its tests.
- **No ffmpeg-binary fallback where the player is unsupported** (no FFmpeg
  shim). The design kept none, but the old code had one: MP4 GIFs and
  videos now stay still there and the viewer says so; a round video or
  MP4 GIF clicked there opens the viewer. A sponsored video's disc only
  counts the sponsored click (there is no viewer to open).
- **A new GIF decoder** (`gif_clip.rs`) instead of "the GIF decoder already
  used for stickers", which does not exist (stickers use rlottie and the
  player). It is the `image` crate's decoder, which GPUI already ships;
  it holds each repeated picture once (the coordinator's note).
- **A video labelled `image/gif` goes to the player.** The design routed
  by MIME type only; a mislabelled file would have failed in the GIF
  decoder.
- **Clips on demand** (`Demand`) are new: the design assumed autoplay, and
  with autoplay off (or data saver on) a round video's click only
  downloaded it and an MP4 GIF's disc fed an MP4 to the GIF decoder.
- **GIFs drawn by the animation layer** at their own cadence instead of
  the frame clock's 30 fps history tick the first version used (see
  Measurements).
- **The Premium stage is rounded at 12 px** (it was `rounded_lg`) so the
  clip's corners and the stage's agree.
- **`scripts/gate.sh` and the GPUI tests.** `clippy --features ui
  --all-targets` does not compile the `gpui_kit::test` tests without
  `test-support`; the same checks were run with `--features demo-capture`
  as CI does.
