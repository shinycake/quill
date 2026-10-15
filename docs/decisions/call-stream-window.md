# Group call streams, PR 2: the live stream window

Design: `quill-tools/designs/group-call-streams.md`, build plan item 2.
Branch `call-stream-window`, on top of PR 1 (`call-streams.md`). Parity
`calls-rtmp-viewing` is not declared yet; see "Not like tdesktop" below.

## What tdesktop does

`calls/group/calls_group_panel.cpp` with `_call->rtmp()` forces
`PanelMode::Wide`:

- Window 720 × 580, at least 240 × 160, titled with the peer's name.
  The top line is the call title (or the peer name), " • " and
  `lng_group_call_rtmp_viewers` with at least 1 viewer.
- The viewport fills the window: 9 px around and 24 px on top in a normal
  window, the whole window when full screen or maximized (`_rtmpFull`),
  where the corners are square and the background black. The picture is
  drawn without cropping; before the first frame the peer's photo is
  blurred with radius 15 (`Viewport::RendererSW::validateUserpicFrame`).
- Controls (`updateButtonsGeometry`, RTMP branch): the settings button
  or, for managers, the "…" menu; the round button, which for RTMP
  toggles full screen (`toggleFullScreen`); Leave. They sit on a
  `groupCallBg` backdrop at 80 %, radius 12, 108 px from the bottom.
- When they show (`setupControlsBackgroundNarrow`, `trackControls`,
  `setupFullScreenBehavior`): in a normal window while the mouse is over
  the viewport or the controls. Full screen, any mouse move shows them
  and restarts a 5 s timer (`kHideControlsTimeout`); leaving the
  controls hides them. Both slide over 150 ms. Full screen the title
  slides with them on a backdrop rounded at the bottom.
- Esc leaves full screen; a double-click on the title (the window's
  maximize request) toggles it. Space stays push-to-talk.
- Nobody broadcasting (`setupEmptyRtmp`): a rounded label (12 px padding,
  text at most 380 px wide) a third of the way down, with
  `lng_group_call_no_stream` for viewers and `lng_group_call_no_stream_admin`
  when `rtmpInfo().url` is known. That URL comes from the join answer's
  `rtmp_stream_url`.
- Pin on top shows "Live stream is pinned on top." and the unpin text.
- The panel closes with the call.
- tdesktop has no unjoined state: the panel opens with the join.
- `FillMenu` for an RTMP call (managers): edit title
  (`lng_group_call_edit_title_channel`), start or stop recording,
  Settings, screen sharing only when `videoIsWorking`, and End live
  stream (`lng_group_call_end_channel`). Viewers get the Settings button.
- `SettingsBox` for an RTMP call: Speakers, Share invite link when the
  channel has a username, for managers the Server URL and Stream Key rows
  with Copy (the key hidden behind a warning) and a "…" with Revoke, a
  volume slider with mute, End live stream for managers, Done.
- `Panel::endCall`: a viewer leaves at once; a manager gets the
  `LeaveBox` (leave, with "also end the live stream").
- `refreshTitleGeometry`: the title line is centered on the window inside
  the room left by the macOS traffic lights (70 px) and the pin button;
  when short, the longer of title and viewer count gives way first, then
  both equally.

## What changed

- `src/ui/calls/stream_tile.rs`: `StreamTile`, the stream picture with
  its placeholder, drawn with `img(...).object_fit(...)`. While the stream
  is live it asks the frame clock for its next redraw at the stream's
  30 fps interval, so no timer of its own runs. It reports its width when
  it changes; the window passes that to
  `ConnectDriver::report_stream_tile_width` (quality bucket, PR 1).
  `blurred_photo` blurs the chat photo off the UI thread.
- `src/ui/calls/stream_picture.rs`: the picture's images. The driver keeps
  the newest frame as `Arc<VideoFrame>`; when its sequence changes the
  window converts it (one copy plus the red/blue swap) on the background
  executor, one conversion at a time. The window keeps the image it shows
  and the one before it; the one from two frames back is dropped from
  every window's atlas at once with `App::drop_image`, without waiting for
  the main window's sweep.
- `src/ui/calls/stream_controls.rs`: the show/hide rules as a pure state
  machine with the 5 s timer and the 150 ms slide. The bar stays mounted
  while hidden (slid below the window), so its hover state never goes
  stale.
- `src/ui/calls/stream_title.rs`: tdesktop's title geometry.
- `src/ui/group_call_stream_layout.rs`: the layout, `StreamFacts` (what
  one draw needs, read without copying the call), the join intent and the
  Join state. `src/ui/group_call_stream_boxes.rs`: the "…" menu, Settings,
  Speakers, Leave, Revoke and key-warning boxes.
  `src/ui/group_call_window.rs`: the window; it is titled with the chat's
  name and follows a rename. The 24 px band over the picture moves the
  window (a caption area on Windows, a press-and-move on Linux as the kit
  title bar does, the transparent title bar on macOS); a double-click
  toggles full screen.
- Joining follows what you asked for. The chat's video chat button, a
  stream link (`CallLinkStep::Offer`) and starting a stream now
  (`createVideoChat` without a date) record a `StreamJoinIntent`; once
  that call is tracked as an RTMP stream the window joins it, once. Any
  other unjoined stream (for example the `updateGroupCall` with
  `is_joined: false` right after Leave) shows a Join button instead.
  Closing the window or leaving drops the intent; asking again opens the
  window again. Leave, then reopen, joins again.
- An ended stream: the window leaves full screen, closes, and the call is
  left whenever it was joined (also when the window was already closed).
- Audio: `GroupCallMedia` carries the chosen speaker. The group call's
  playback is issued with `ntg_set_stream_sources(PLAYBACK)` on that
  device, and a device change while a group call is joined re-issues it
  (`select_call_devices` now covers group calls). "Mute for me" sets the
  playback description to none, which ntgcalls answers by dropping its
  audio writer (stream_manager.cpp, 3.0.3); unmuting restores it.
- The main window's call bar shows no microphone for a stream viewer and
  counts viewers.
- Push-to-talk does nothing for a stream viewer.
- `connect/group_calls/stream.rs`: the join answer's `rtmp_stream_url`
  and `rtmp_stream_key` fill `ActiveGroupCall::rtmp_url` /
  `rtmp_stream_key`, as tdesktop's `rtmpInfo()`, and
  `latest_stream_frame_seq` lets the window skip a frame it already has.
- Strings: tdesktop's keys for every label above, plus two Quill keys
  (the Unsupported label and a join that could not be sent).
- Screenshot demos `ready-rtmp-connecting`, `ready-rtmp-live` and
  `ready-rtmp-no-stream` (demo channel 13 with a photo and a still
  picture; nothing joins).

## Memory: stream pictures with the main window idle

Each stream picture is a full RGBA image (3.5 MiB at 1280 × 720) at up to
30 a second. Before, a replaced image went to `image_budget::retire_all`,
which only the main window's draw sweeps. With the main window minimized
or in the background, nothing swept.

Measured with a GPUI test that feeds 200 frames at 1280 × 720 and draws
only the stream window:

| | images still held | memory |
|---|---|---|
| Before | 199 retired, waiting for the sweep | 699 MiB |
| After | 2 (shown and previous) | 7 MiB |

`stream_pictures_stay_bounded_with_the_main_window_idle` keeps the check
(at most 2 images alive after 200 frames). Each frame also used to be
copied twice on the UI thread (the driver's clone under its lock, then the
conversion); now the driver hands out an `Arc` and the one copy happens on
the background executor.

## Not like tdesktop

- No volume slider: ntgcalls 3.0.3 has no playback volume. "Mute for me"
  is there (tdesktop's slider at 0).
- Screen sharing is not in the managers' menu: it needs `videoIsWorking`,
  which an RTMP call that Quill joins as a viewer never has.
- An unjoined window with a Join button (and the reason after a failed
  join) is Quill-only; tdesktop never shows the panel unjoined.
- The cursor is not hidden in full screen: GPUI has no API for it.
- The letterbox is plain `groupCallMembersBg` (black full screen), as
  tdesktop's software renderer draws it. The OpenGL renderer fills it
  with the blurred frame.
- No placeholder when the chat has no photo file yet (tdesktop draws the
  letter avatar).
- A scheduled RTMP stream keeps the scheduled panel. Live stories have
  their own viewer (PR 3).
- An MTProto stream (a large non-RTMP call) keeps the chat panel; its
  pictures go to the stream slot and are not drawn yet (PR 3).
- Unsupported (ntgcalls connect error -103) is Quill-only: the label
  says "Live stream playback is not available in this build."
- Design deviation: the design put the picture in `video_player`. It is
  drawn like the group tiles instead (no `video_player` change).

Parity `calls-rtmp-viewing` is not declared: besides the volume slider,
the cursor and the letter-avatar placeholder still differ.

## How it was verified

- Unit tests: `stream_controls`, `stream_tile`, `stream_picture` (only
  two images stay, a stale conversion is handed back, BGRA order),
  `stream_title` (centering, shrinking, pushing inside the rect), the
  RTMP URL from the join answer, the frame sequence, and the chosen
  speaker playing the stream and following a change
  (`connect/tests/group_calls/stream.rs`).
- GPUI UI tests (`group_call_stream_tests.rs`, `demo-capture`): the
  stream layout and its insets; the label a third of the way down at
  `min(W - 48, 380) + 24`; the controls following the mouse; the admin
  text; Ended closing the window; leave, then reopen, joins again (and
  the unjoined call right after Leave is not joined); the Join button
  joining after a failure; Settings opening and Esc closing it; the
  bounded pictures above.
- Screenshots of the three demos on macOS (in the PR, not committed).
- Not run: a real stream (no call is joined from this work), Linux and
  Windows. The drag band uses `start_window_move` (Linux) and
  `WindowControlArea::Drag` (Windows), as the kit title bar does.
