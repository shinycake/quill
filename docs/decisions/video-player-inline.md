# Video player inline: tiles, round videos, GIF panel and stories on the core

PR 3 of 6 of the video player overhaul (`quill-tools/designs/video-player.md`,
Migration plan and build plan item 3). PR 1 (`video-player-core.md`) built
the shared `VideoPlayer` and moved the viewer onto it. PR 2
(`video-player-controls.md`) rebuilt the viewer controls. This PR moves every
other playing clip in a chat onto the same player: inline video and GIF
tiles, round videos (video notes), the GIF panel of inline bots, and video
stories.

## Problem

Measured on `ff87c0ba` (main plus the new demo below; release build with
`frame-profiler`, M1 Pro, 120 Hz panel, `QUILL_ASSUME_ACTIVE=1`, 16.5 s per
run):

- Inline tiles and round videos played on the old `NativeVideo` (AVPlayer,
  or FFmpeg through `ffmpeg_video.rs`). Each frame came from a 30 fps
  animation tick on the conversation's animation layer, free-running against
  the display and the clip.
- A 25 fps round video sampled at 30 Hz shows some pictures for one tick and
  others for two. Its gaps were 4 vsyncs (33 ms) most of the time and 8
  vsyncs (67 ms) every few pictures, instead of an even 40 ms.
- A 30 fps clip sampled by a 30 Hz tick that drifts against it skipped
  source pictures: 20 double steps in 467 pictures.
- Stories pulled `NativeVideo` frames from a `request_media_tick(30)`.

## What changed

- **`src/ui/inline_players.rs` (new).** `InlineVideos` holds one
  `VideoPlayer` (`Purpose::Inline { max_edge }`, muted, looping) per clip on
  screen, keyed by `(chat, message)`. It replaces the `NativeVideo` table in
  `inline_video.rs`. It keeps the old rules:
  - the 64 MiB decoded-memory budget (`admits`, `INLINE_DECODE_BUDGET`)
  - tile-sized decoding and the 1080p cap (`InlineTile`, in-process
    decoders only)
  - muted loops pause behind another app; a clip playing with sound keeps
    going
  - a round video click plays it once from the start with sound (muting any
    other), and at the end it goes back to the muted loop
  - the seek ring with its 220/150/150 ms easings

  The state is one `RefCell` with `&self` methods. Every borrow is one short
  scope, and player calls (each one reaches the backend) happen after the
  borrow ends.
- **Off-screen policy.** Before, a player was only dropped once its row
  stopped rendering. A row that rendered but was clipped (scrolled under
  the header, in the list's overdraw) kept decoding.
  - Every pass, `begin_render` notes how often each player has been painted
    inside the visible content mask (`VideoPlayer::shown_count`, counted by
    the `video_frame` pump). A deferred sweep after the paint compares the
    two (`off_screen_step`):
    - a player painted this pass: shown (played)
    - not painted: paused at once (`Hide`), so no more decoding and no frame
      requests
    - hidden for 2 s (`RELEASE_AFTER`): released by a timer (`Release`)
  - Only the sweep clears the hold. A row that keeps rendering while its
    picture is outside the view (the list's first row scrolled half away)
    stays paused with no play/pause churn; a paused player still paints its
    last picture, so scrolling back shows it and the next sweep resumes it.
  - A history clip opens held and plays at the sweep after its first
    picture was painted in view. A row rendered only in the list's overdraw
    never plays. After a release, the next pass that still renders the row
    opens a new held player (one decoder open per release, while passes
    happen).
  - The GIF panel's players (`PANEL_CHAT`) are exempt: the panel renders in
    its own slice. They live by the panel's own passes
    (`begin_panel_render`, at the top of `inline_results_panel`: cells that
    leave the panel stop) and stop when the panel closes (`clear_panel`,
    checked each root render). History passes and `frame_start` leave them
    alone.
  - Play and pause decisions use the player's play intent
    (`VideoPlayer::wants_play`), so an opening player is played or paused
    too (window activation included); a round video going back to its loop
    plays only when its slot wants to (on screen, window active).
  - A viewer or story viewer over the chat clears the players each root
    render (`app_render`). The conversation slice can replay from its cache
    while a viewer is open without running its prepaint, so the animation
    layer could otherwise keep drawing (and pulling) tiles behind the
    viewer; with the players gone their tiles have nothing to draw.
  - tdesktop does the same in two steps: a streamed clip decodes only ahead
    of frames it shows, and `HistoryInner::visibleAreaUpdated` unloads heavy
    parts far from the view (`Session::unloadHeavyViewParts`).
- **Tiles draw `video_frame`.** `message_media/surfaces.rs` builds the tile
  as `video_frame(&player)` with `object_fit(Cover)` inside
  `anim_layer::tile(.., fps 0, ..)`. Fps 0 means the layer no longer ticks
  for it: the frame's paint pump asks for the next frame itself
  (`request_video_frame`), timed by the clip.
  - While the layer builds, the current view is the `AnimationLayer`, so
    only the layer redraws for a new picture. The conversation slice stays
    cached.
  - The row's own copy of a layered tile, painted under the layer, neither
    asks for frames nor wakes watchers (`anim_layer::painting_under_layer`).
  - The still (thumbnail) shows under the live tile until the first picture.
  - Corner masks for AVPlayer surfaces moved to
    `VideoFrame::corner_radii_over` (four colors). A round video is the same
    mask with four radii of half the diameter. The mute badge, duration pill
    and download progress are unchanged.
- **Round videos** are the same tiles in a circle. The sound toggle and the
  seek ring work as before. A sounding clip watches the slice that shows it,
  so its end (back to the loop) redraws the row.
- **GIF panel** (`inline_grid.rs`): `inline_clip_picture` keeps its
  signature but returns the panel cell's `VideoPlayer`. The cell draws the
  still with `video_frame` over it. The `request_animation_tick(30)` is
  gone.
- **Stories** (`story_more.rs`): `stories.player` is a
  `VideoPlayer` (`Purpose::Story`). Opening, failure (`take_error` or
  `Failed`), the wait for the download, pause, mute, segment progress and the
  end read the player and its `transport()`. `story_video_element` draws the
  still under `video_frame(&player)` with `Contain`. The
  `request_media_tick(30)` is gone, and with it `request_media_tick` itself.
- **Core additions** (`video_player/`):
  - `Purpose::Inline { max_edge }` and `Purpose::Story` (FFmpeg: inline and
    viewer open options)
  - `supported()` and `decodes_in_process()`
  - the `shown_count` counter
  - `wants_play()`
  - pacing: two draw starts less than 2 ms apart count as one frame
    (`SAME_FRAME`). A tile drawn twice in one frame (layer copy and slice)
    no longer halves the learned interval on Linux and Windows. GPUI exposes
    no frame counter to tell draws apart by identity; 2 ms is under half a
    240 Hz frame, so a late frame followed by an early one still counts as
    two.
  - transport: a looping player starts over 50 ms before the end
    (`LOOP_LEAD`), as the old inline player did, instead of waiting for the
    backend to stop there. Only the backend's own duration counts, never the
    message's whole-second label. It sends one loop (an `at_end` before the
    seek lands sends no second one) and keeps the position at the start
    until the seek lands.
- **Removed:** the `NativeVideo` / `ffmpeg_video` consumers in
  `inline_video.rs`, `message_media/*`, `inline_grid.rs`, `round_seek.rs`
  and `story_more.rs`, and the methods that became dead
  (`native_video::{decodes_in_process, clips, duration_secs}`,
  `ffmpeg_video::duration_secs`, `circle_mask`). The editor, premium promo
  and premium preview still use `NativeVideo`; both files go in PR 5.
- **Demo:** `ready-video-inline`, a chat with the 12 s inline video tile
  (message 204) and the round video (611), used for the measurements and
  captures.

## Measurements

Release build with `frame-profiler`, M1 Pro, 120 Hz, `QUILL_TRACE_VIDEO=1
QUILL_TRACE_TICKS=1 QUILL_ASSUME_ACTIVE=1`, 16.5 s per run, AVPlayer backend.
"Before" is `ff87c0ba`; "after" is this branch. Gaps are between new
pictures on screen. Release binaries were deleted after the runs.

`ready-video-inline`, video tile (30 fps clip):

| | Before | After |
|---|---|---|
| New pictures | 467 (28.3 fps) | 491 (29.8 fps) |
| Gap p50 / p90 / p99 / max | 33.3 / 41.7 / 43.3 / 66.7 ms | 33.3 / 34.0 / 41.2 / 42.0 ms |
| Gap sd | 4.1 ms | 1.8 ms |
| Gaps in vsyncs | {3: 3, 4: 384, 5: 76, 8: 3} | {1: 1, 3: 5, 4: 478, 5: 6} |
| Skipped source pictures | 20 | 0 |

`ready-video-inline`, round video (25 fps clip):

| | Before | After |
|---|---|---|
| New pictures | 392 (23.7 fps) | 419 (25.4 fps) |
| Gap p50 / p90 / p99 / max | 33.5 / 66.7 / 100.0 / 108.2 ms | 41.5 / 42.3 / 43.5 / 66.7 ms |
| Gap sd | 15.0 ms | 5.7 ms |
| Gaps in vsyncs | {4: 257, 5: 61, 8: 53, 9: 12, 12: 4, ...} | {4: 96, 5: 299, 2: 6, 3: 12, ...} |
| Skipped source pictures | 3 | 0 |
| Loop seam (median of 16) | 74.9 ms | 31.8 ms |

The 25 fps clip now lands on its 4.8-vsync cadence (alternating 5 and 4
vsyncs), as the design asks. The few 2- and 3-vsync gaps are AVPlayer
handing over a picture late and the next on time.

Whole window, same run:

| | Before | After |
|---|---|---|
| Draw p50 / p90 | 0.95 / 1.59 ms | 0.96 / 1.18 ms |
| Dirty to present p50 / p90 | 6.19 / 9.55 ms | 1.23 / 1.49 ms |
| Frames drawn | 466 | 783 |
| Conversation slice renders, after the first second | 0/s | 0/s |
| 30 fps layer tick (`tick_animation_layer`) | running | none |

More frames are drawn because the two clips (30 and 25 fps) each get a frame
when their picture is due, instead of both sharing one 30 Hz tick. Each of
those frames renders only the uncached views: both animation layers (about
96 layer renders/s, two per frame) and the shell root. The cached
conversation slice is not rendered after the first second, where it renders
16 times while the chat opens. The slice-trace "animation layer" key is new
in this PR, so there is no before figure for it.

`ready-video-note` (a round video alone: a 1 s, 25 fps clip, about 16 loops
per run; this demo also runs the legacy `spawn_video_tick`, so the
conversation slice renders 2 to 3 times a second in both runs):

| | Before | After |
|---|---|---|
| New pictures | 395 (23.9 fps) | 399 (24.2 fps) |
| Gap p50 / p90 | 34.0 / 66.7 ms | 41.6 / 42.4 ms |
| Gap sd | 13.7 ms | 10.9 ms |
| Skipped source pictures | 4 | 0 |
| Loop seam median / max | 66.7 / 117.3 ms | 66.9 / 135.0 ms |
| Dirty to present p50 | 6.22 ms | 1.23 ms |

Without `LOOP_LEAD`, the core's loop (wait for the backend to stop, then
seek) made the seam of this 1 s clip 133 ms median. With it the seam is back
to where the old player was. Three of the 16 loops still take 133 ms
(16 vsyncs): AVPlayer's seek to 0 sometimes takes that long. Gapless
looping (`AVPlayerLooper`) is the follow-up below.

`ready-gifs` (real `.gif` files on the legacy frame-extraction path, which
this PR does not touch): conversation slice 23 to 24 renders/s before and
after.

## Supersedes

- **`codex-inline-video.md`:**
  - The first bullet: inline clips are `VideoPlayer`s in
    `inline_players.rs`, drawn by `video_frame` (surface or image), not
    `NativeVideo` with a "~30 fps redraw tick". Players are also paused when
    not painted and released 2 s later, not only dropped after their row
    stops rendering.
  - "Real `image/gif` files keep the old frame-extraction path" still
    holds.
- **`codex-inline-video-cross-platform.md`, "Changes":**
  - Item 1 to 3 (tile-sized decode, the 1080p cap, the 64 MiB budget) are
    kept and now live in `inline_players.rs` / `inline_video.rs` on top of
    `VideoPlayer`; the gate is `video_player::decodes_in_process()`
    instead of `native_video::decodes_in_process()`.
  - Item 4 (idle silent decoders) still applies: the FFmpeg backend opens
    inline players with `OpenOptions::inline`, muted.
- **`codex-native-video.md`, "Not yet":** "Native inline playback for GIFs
  and round videos" is done, on the shared player.
- **`video-player-core.md`, "Supersedes":** "`NativeVideo` and
  `ffmpeg_video.rs` remain for inline tiles, round videos, GIFs and stories
  until PR 3 and PR 5" now reads: they remain for the video editor, premium
  promo and premium preview until PR 5.

## Deviations from the design

- **Stories have no `Layer::StoryVideo`.** The story overlay is built in
  `QuillApp`'s root render, not in a cached slice. A separate view for the
  picture would still be re-rendered with the root on each frame. So the
  story picture's frames redraw the root, as the old media tick did; the
  difference is that frames come when the clip has a new picture (25/30 fps)
  instead of a free 30 Hz tick. Moving the story viewer into a cached slice
  (like the media viewer's `SliceKind::Viewer`) would get the design's
  behaviour; it is left as a follow-up.
- **Stories play with sound.** The design says "muted by default until the
  story is tapped, as today". Today's code (`stories.muted`, default
  `false`) plays them with sound and the mute button silences them. This
  PR keeps the current behaviour.
- **A GIF panel cell with no player** keeps its still; panel players are not
  paused off screen, since the panel draws only visible rows.
- **Story pause** checks the play intent instead of pausing on every
  100 ms tick: a player already paused is left alone, and one paused while
  it opens stays paused once open.

## How verified

- **Unit tests** (`cargo test --features demo-capture`):
  - `inline_players.rs`: the off-screen steps (shown, hidden at once,
    staying hidden, released after 2 s); history and panel passes each reap
    only their own players; the decoded-memory budget, and the queue the
    in-process decoder keeps counting toward it.
  - `pacing.rs`: two draws in one frame count once; jittered 240 Hz frames
    (gaps down to 2.97 ms) are not merged.
  - `transport.rs`: a looping player starts over just before the end, once
    until the seek lands; an `at_end` after an early loop sends no second
    loop and the position stays at the start; the message's duration never
    loops early; a clip that does not loop plays to the end.
- **GPUI UI tests** (`video_player/tests/inline.rs`), on the
  `ready-video-inline` chat with scripted backends (the queue,
  `NEXT_BACKEND`, is now a `VecDeque`, and a test that queues one counts
  clips as playable without the FFmpeg shim, for Linux CI):
  - Both players open but wait while the window is inactive. Once it is
    active, both play muted (`volume 0`, `play 1`). Their frame requests go
    to the conversation's animation layer and never to the conversation
    slice. Neither is hidden while painted. Switching to another chat drops
    both players and releases their backends.
  - Scroll the round video's picture out of view (in 10 px wheel steps)
    while its row still renders: it is held (paused, `wants_play` false);
    four more history passes make no play or pause call; advancing the
    executor clock by `RELEASE_AFTER` releases its backend, and a later pass
    reopens it held, not playing; scrolling back plays it.
  - A story player paused (`apply_story_native_pause`) while its clip is
    still opening does not start once the backend is ready.
- **Captures** of `ready-video-inline` in light and dark (in-process
  `QUILL_DEMO_CAPTURE`, not committed): the video tile with its first
  picture, rounded corners and the `0:12` mute pill; the round video
  clipped to a circle with its `0:08` pill; the pending outgoing round
  video with its play disc.
- **Measurements** above (macOS, AVPlayer). They were taken before the
  review fixes (held opening, panel lifetime, the 2 ms same-frame threshold,
  the loop guards) and were not repeated; those change no steady-state
  playback path on screen.

Not verified:

- Linux and Windows (FFmpeg backend) were not run; Linux compiles only in
  CI. The off-screen pause was not measured on FFmpeg (where it saves
  decoding CPU; on AVPlayer it saves frame requests).
- The GIF panel and the story viewer were not captured or measured: there
  is no demo with a playing panel clip or a video story. The story still is
  always the poster: a video story's `display_file_ids` hold only its
  thumbnail. Both compile on the
  same calls as the tiles, and stories keep their progress, pause and mute
  code paths.
- The live account was not used.

## Follow-ups

- Gapless looping for short AVPlayer clips (`AVQueuePlayer` +
  `AVPlayerLooper`), to remove the remaining 133 ms seams.
- The story viewer in a cached slice, so a story frame redraws only its
  picture.
- PR 5 deletes `native_video.rs` and `ffmpeg_video.rs` with their last
  consumers.

No README parity item is fully completed by this PR, so there is no parity
fragment.
