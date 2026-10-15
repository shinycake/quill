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

## What changed

- `src/ui/calls/stream_tile.rs`: `StreamTile`, the stream picture with
  its placeholder. The owner turns the newest `VideoFrame` into a
  `RenderImage` once per frame sequence and passes it in; the tile draws
  it with `img(...).object_fit(...)`. While the stream plays it asks the
  frame clock for its next redraw at the stream's 30 fps interval
  (`request_video_frame`, `keep_alive`, `plays_sound`), so no timer of its
  own runs. It reports its width on paint when it changes; the window
  passes that to `ConnectDriver::report_stream_tile_width`, which keeps
  the quality bucket (PR 1). `blurred_photo` blurs the chat photo with
  the wallpaper raster's `BlurLargeImage` weights, off the UI thread.
  PR 3 reuses the tile with `Cover`.
- `src/ui/calls/stream_controls.rs`: the show/hide rules above as a pure
  state machine with the 5 s timer and the 150 ms linear slide.
- `src/ui/group_call_stream_layout.rs`: the layout. `src/ui/group_call_window.rs`:
  the window code moved out of `group_call_panel.rs`, which now only
  draws the chat panel. The window picks the layout per render, sizes
  itself for it, and reopens when a call switches layout. A stream that
  ends closes the window and leaves the call. An unjoined RTMP stream is
  joined once when its window opens: tdesktop joins straight from the
  chat's bar into the stream window.
- Push-to-talk does nothing for a stream viewer.
- `connect/group_calls/stream.rs`: the join answer's `rtmp_stream_url`
  and `rtmp_stream_key` fill `ActiveGroupCall::rtmp_url` / `rtmp_stream_key`,
  as tdesktop's `rtmpInfo()`, and `latest_stream_frame_seq` lets the
  window skip converting a picture it already has.
- Strings: tdesktop's `lng_group_call_no_stream`, `_no_stream_admin`,
  `_rtmp_viewers`, `_pinned_on_top`, `_unpinned_on_top`, and one Quill
  key for the Unsupported label.
- Screenshot demos `ready-rtmp-connecting`, `ready-rtmp-live` and
  `ready-rtmp-no-stream` (demo channel 13 with a photo and a still
  picture; nothing joins).

## Not like tdesktop

- No settings button. tdesktop's RTMP settings box has the speaker
  device, the share link, the RTMP rows, a volume slider and End for
  managers. ntgcalls plays group audio on the default device only and has
  no volume API, so the box would be half empty. Managers get the
  existing "…" menu. This is why parity is not declared.
- The cursor is not hidden in full screen: GPUI has no API for it.
- The letterbox is plain `groupCallMembersBg` (black full screen), as
  tdesktop's software renderer draws it. The OpenGL renderer fills it
  with the blurred frame.
- The picture's own corners are rounded with the tile's radius; tdesktop
  rounds only the tile.
- No placeholder when the chat has no photo file yet (tdesktop draws the
  letter avatar).
- A scheduled RTMP stream keeps the scheduled panel. Live stories have
  their own viewer (PR 3).
- An MTProto stream (a large non-RTMP call) keeps the chat panel; its
  pictures go to the stream slot and are not drawn yet (PR 3).
- Unsupported (ntgcalls connect error -103) is Quill-only: the label
  says "Live stream playback is not available in this build." in the
  same style.
- Design deviation: the design put the picture in `video_player`. It is
  drawn like the group tiles instead (no `video_player` change), with
  frame-clock redraws.

## How it was verified

- Unit tests: `stream_controls` (normal window, the 5 s timer, an
  interrupted slide), `stream_tile` (the frame ask, the blurred photo),
  the RTMP URL from the join answer and the frame sequence
  (`connect/tests/group_calls/stream.rs`).
- GPUI UI tests (`group_call_stream_tests.rs`, `demo-capture`): the
  stream opens the window in the stream layout with the 9/24 px insets;
  the label sits a third of the way down within 404 px; the controls
  follow the mouse; the admin text replaces the viewer text when the URL
  is known; Ended closes the window.
- Screenshots of the three demos on macOS (in the PR, not committed).
- Not run: a real stream (no call is joined from this work), Linux and
  Windows. The code uses only GPUI calls that exist on all three
  (`toggle_fullscreen`, `zoom_window`, `is_maximized`).
