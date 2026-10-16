# Video player: live PiP, the 1:1 call through live players, leave and watch

Item 9 of the video player design. PR 4 (`video-pip.md`) gave files a
Picture-in-Picture window, PR 7 (`video-player-live.md`) gave live
pictures one player, and PR 8 (`video-player-live-ui.md`) gave them one
controller. This PR puts live pictures in the PiP window, moves the 1:1
call's pictures onto live players, and replaces the "leave your call?"
modal for a live story with a card in the story frame.

tdesktop has no live PiP. Its stream panel and its stories have no PiP
button, so the live PiP is Quill's own. It follows the file PiP's
window and only changes its controls.

## Before

- The stream window's PiP slot and the live story header had no
  button.
- A 1:1 video call converted every remote and local frame to RGBA on
  the main thread (`i420_to_rgba`, `rotate_rgba`). It built a new
  `RenderImage` for each frame and drew both pictures in the main
  window's call card. Another copy went to the call window. The call
  tick notified the whole app every 100 ms while video was on.
- The local preview was not mirrored. tdesktop mirrors it.
- A live story opened while another call held the call slot showed a
  modal confirm (`LiveStoryLeaveOther`).

## After

### Live PiP

- `PipOpen.live: Option<LivePip>` switches `VideoPip` to live mode
  (`controls/pip/live.rs`). It shows the LIVE pill with the viewer
  count on a dark backing in the bottom-left corner, always visible so
  the window reads as live. Close, enlarge and mute fade with the
  pointer, like the file PiP's controls.
- There is no playback line, volume line, play button or time. The
  live layout (`ControlsLayout::live`) puts mute in the volume side's
  top corner. The areas where the lines were are picture, so dragging
  there moves the window. Space does nothing; M mutes.
- Mute goes through the call (`toggle_stream_mute`), so the stream
  window's mute and the PiP's always agree.
- The window closes when the player reports Failed or Ended.
- `calls/live_pip.rs` owns the link (`LivePipLink`): the source (a
  stream's call or a live story), the account and the window to return
  to.
  - **Open from the stream window.** The window's PiP button moves the
    stream's player into the PiP and closes the stream window at once,
    out of full screen first. It marks the window closed by the user,
    so the window sync keeps the player (`live_pip_keeps_stream`)
    instead of clearing it. The call stays joined.
  - **Open from a live story.** The header's PiP button moves the
    story's player into the PiP and closes the viewer. The story stays
    watched: `close_story_viewer` does not leave it while the PiP shows
    it, and `sync_live_story` keeps it as its target.
  - **Enlarge.** For a stream, enlarge reopens the stream window. For a
    story, it reopens the viewer on that story. Either way the same
    player goes back to its host and nothing restarts.
  - **Close.** Closing a stream's PiP keeps the call; the next window
    sync drops the player. Closing a story's PiP leaves the story.
  - **One sound at a time.** Opening a live PiP closes a file PiP and
    the reverse. `sound_starts` (a voice message, a story) closes both.
  - **Automatic close.** `check_live_pip` closes the window on lock, on
    logout or an account switch, when the call ends or is left, and
    when the story stops being watched.
  - **Hidden window.** Minimising the window or hiding it behind
    another space stops drawing through the live core's off-screen
    policy. The player stays and the sound goes on (keep_alive).

### 1:1 call pictures

- 1:1 calls get `LiveKey::Call { call_id, local, screen }`. The native
  driver routes 1:1 frames to live feeds like group tiles. `VideoFrame`,
  `i420_to_rgba`, `rotate_rgba`, the frame callbacks and every
  `VideoFrame.rgba` reader are gone.
- `calls/call_video.rs` adds `CallPicture { Remote, Local }`, and
  `call_picture_player` gives each picture a live player that follows
  its feed. The remote picture prefers an active screen share over the
  camera.
- The call window's root (`CallWindowRoot`) stacks four layers: the
  ground and backdrop, the remote picture, the local preview, and the
  panel. Each layer is a cached view, so a frame redraws only its
  picture's view.
- The local preview is mirrored. The mirror is applied in the plane
  copy (`mirror_planes`) and on the paused raster (`mirror_raster`).
  The preview is images-only (`set_live_images_only`), so it can round
  its own corners: rounding a native surface would need a solid corner
  color over video.
- The screen-share preview path (`rgba_to_i420`) is unchanged.
- The dead `call_active_card` keeps its avatar. Its ten RGBA-stage
  status strings had no other reader and are removed.

### Live story while in another call

- When joining is refused because another call holds the slot, the
  story frame shows a card in place of the picture. The card asks
  tdesktop's `confirmLeaveCurrent` question
  (`lng_call_leave_to_other_sure_channel` and the like) and offers
  Cancel and "Leave call and watch" (`qll_media_live_leave_and_watch`).
- Confirm leaves the other call and joins once the slot is free (the
  existing `retry_join`). Cancel clears the question, and the Join
  button asks again.
- The card blocks presses from reaching the backdrop, which would close
  the viewer, and the media, which would pause it.
- Its buttons use the viewer's always-dark chrome
  (`chrome::ghost`, `chrome::attention`). The kit's danger button
  follows the app theme, which gave light-theme colors on the dark
  card.

## Deviations from the design, with reasons

- **A story PiP's close leaves the story.** Nothing else would show a
  joined live story, so keeping it would play sound from nowhere. A
  stream's call does stay, as the design says.
- **The old leave dialog was a modal confirm**, not the bare refusal
  the design assumed. The card replaces it, and its question keeps
  tdesktop's words.
- **The story PiP does not run the story tick.** Comments, Stars and
  effects pause while it floats; the PiP shows the picture only.
  Enlarging brings them back.
- **The PiP's player does not follow a feed change.** A rejoin gives a
  new feed; the PiP closes when its feed ends rather than swapping
  players under the window.
- **The badge is always visible**, while other controls fade, so a
  still frame is never mistaken for a paused file.
- **The 1:1 preview** sits at right 12 and bottom 99, measures
  160 × 110 with radius 10 and a shadow, and is drawn under the panel
  so the device menus and the privacy overlay stay on top. The panel's
  bottom shade darkens its lowest ~25 px slightly.
- **The group self tile is not mirrored.** That is out of scope here
  and is a one-line `set_live_mirrored` follow-up.
- **README `parity:calls-camera-preview`** still describes the old card's
  "Starting camera…" and "Camera off" labels. Those labels lived in the
  dead card. The call window shows no preview until the first frame,
  as tdesktop does. A feature PR does not edit the README, so the
  wording is left for the pipeline.

## Measurements

| What | Before | After | How |
| --- | --- | --- | --- |
| App notifies from the call tick during a 1:1 video call (they redraw the main window and the call window) | 10/s (`call_tick_interval`: 100 ms while video was on) | 1/s (the clock's tick; frames wake only their picture views) | Timer intervals in `calls/mod.rs`, before and after; not profiled |
| Main-window work per 1:1 frame | I420→RGBA conversion, rotation and a new `RenderImage` per frame, for both pictures | None: frames go to the live sinks and redraw only the picture views | Code path; the RGBA path is deleted |
| Copies of each 1:1 frame | RGBA in the call card plus a copy for the call window | One plane buffer per feed, uploaded by the live core | Code path |

No wall-clock or memory profile was taken. There is no real call or
stream on this machine, and the demo feeds are synthetic.

## Verified

- Pure tests:
  - `mirror_planes` reverses every row of every plane and leaves a
    malformed frame alone; `mirror_raster` reverses pixels.
  - `ControlsLayout::live` puts mute in the corner and removes the
    lines in both corner orders.
  - `call_picture_key` covers screen over camera, the camera off and
    states other than Ready.
- GPUI tests (`#[cfg(test)]` over `#[cfg(feature = "demo-capture")]`,
  settled frames, no sleeps):
  - `video_player/tests/live_pip.rs`:
    - the stream PiP shows the badge and mute with no scrubber or
      times, the stream window closes, the call stays joined and the
      player is the same;
    - mute goes through the call;
    - enlarge reopens the stream window with the same player;
    - closing keeps the call and drops the player;
    - the PiP closes on the call's end and on lock;
    - one sound at a time with a file PiP, in both directions and on
      `sound_starts`;
    - the live story PiP keeps the story, enlarges back to the viewer,
      and closing it leaves the story.
  - `story_viewer/live_tests.rs`: the leave card replaces the picture,
    Cancel keeps the viewer and leaves nothing, and "Leave call and
    watch" leaves and retries the join.
  - `calls/call_video_tests.rs`: the local preview draws through a live
    player and is mirrored, turning the camera off drops it, and the
    players go with the call.
  - `group_call_stream_tests.rs`: the stream bar now has the PiP
    button.
- Captures, not committed: the live PiP window over the demo stream
  (`ready-rtmp-pip`, `QUILL_DEMO_PIP_CONTROLS=1`), the leave card
  (`ready-live-story-leave`) and the 1:1 call window with its mirrored
  preview (`ready-video-call`).

## Not verified

- No real 1:1 call, video chat or RTMP stream ran. Live frames came from
  demo feeds and hand-fed sinks, and the native driver's routing of 1:1
  frames is covered by connect tests only.
- PiP placement, snapping and keep-alive on Windows and Linux: the live
  mode reuses PR 4's window code unchanged, which was checked there.
- Windows NV12 for live frames is out of scope.
