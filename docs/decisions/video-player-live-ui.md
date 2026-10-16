# Video player: the live controller and the live states

Item 8 of the video player design. PR 7 (`video-player-live.md`) gave
live pictures one player and a state machine (`PlayState` from
`observe_live`). This PR gives them one controller and one set of state
overlays, and moves the stream window, the live story and the video
chat tiles onto them.

## Before

- The stream window drew its own bar (menu, full screen, Leave), its
  own LIVE-less title and its own no-stream label. The fade lived in
  `calls/stream_controls.rs`. The cursor stayed visible in full screen
  (`call-stream-window.md`: "GPUI has no API for it").
- A stalled, paused, ended or failed stream showed nothing special: the
  last frame stayed, or the blurred photo.
- The live story drew its own LIVE badge and sound button.
- A video chat tile drew its own paused shadow (`group_call_paused.rs`),
  and a stream with no frame yet had no tile at all.
- The stream window had no keys besides Esc.

## After

- `VideoController::live(row_id, LiveFacts, LiveActions)`
  (`video_player/controls/live.rs`) draws the live controller in two
  layouts:
  - Bar (the stream window): the host's leading slot (the menu), a
    mute disc, a volume slider only when the host passes one, PiP only
    when the host passes a handler, the round full-screen button, and
    the host's trailing slot (Leave). It sits on the 68 px controls
    backdrop the stream window already had.
  - Header (the live story): the LIVE badge with the viewer count,
    then the mute button.
  - There is no scrubber, time label, speed control, frame step or seek
    key in either layout.
- The LIVE pill is `groupCallMessageBadge`: 11 px tall, radius 5, 8 px
  semibold uppercase on the kit's `danger` token. "{n} viewers"
  (`lng_group_call_rtmp_viewers`) follows it and is hidden at 0. The
  stream window's title shows the pill before the count. The live
  story's header and the comments' admin badge use the same pill.
- Keys (`live_key_action`): Space is taken and does nothing, M mutes, F
  toggles full screen and Esc leaves full screen. Outside full screen,
  Esc keeps its old job. Ctrl, Alt, Cmd and Fn chords pass on.
- The live states come from `LiveOverlayClock` (pure, clock injected)
  and are drawn by `PictureOverlay` through new `PictureState`
  variants:

  | State | When | Drawn |
  | --- | --- | --- |
  | Opening | no player yet, or `PlayState::Opening` | the host's placeholder (blurred photo or initials), the spinner after 500 ms |
  | Stalled | `PlayState::Buffering` (1.5 s without a frame) | the last picture at 60 % with the spinner; "Connecting…" once 5 s have passed since the last frame |
  | Paused | `PlayState::Paused` | tdesktop's 80/255 shadow and the pause icon over the blurred 90 px picture; "Video is paused" when wide |
  | Idle | not joined, nobody broadcasting, unsupported | the placeholder with no spinner (the host draws its label) |
  | Ended | `PlayState::Ended` | the picture dimmed, "Live stream ended" and Leave |
  | Failed | `PlayState::Failed`, or the joined call's `transport_error` | the viewer's failed card with the reason and Retry (rejoin) |

  Failed beats Ended, Ended beats Idle, and Idle beats the player's
  state.
- The no-stream label (`lng_group_call_no_stream` / `_admin` on the
  controls backdrop) moved from the stream layout to
  `controls::stream_label_box`. Its text and geometry are unchanged.
- The fade moved to `video_player/controls/fade.rs` as `ControlsFade`:
  hidden after 5 s idle in full screen, a 150 ms slide, and the mouse
  shows it again. The fade now hides the cursor when the controls hide
  in full screen (tdesktop `setCursorShown(!_rtmpFull || shown)`). The
  fade and its hide timer read the GPUI executor's clock, so tests can
  advance it. Files in the media viewer do not use it and are
  unchanged.
- Consumers:
  - The stream window uses the bar layout and the overlays. The
    join flow, the Join label, the title band, the settings box and the
    viewers are unchanged, apart from the pill in the title.
  - The live story uses the header layout and the overlays over its
    picture. Retry rejoins through the story's join path, and Leave
    closes the viewer.
  - Video chat tiles keep their own chrome (name, pin, full screen).
    They now take the Paused overlay from the shared code, and a stream
    with no frame yet shows the avatar with the Opening spinner after
    500 ms instead of no tile. `group_call_paused.rs` is gone.
  - The full-screen stage gets a volume slider (0 to 200 %) for the
    pinned participant, but not for you (`group_call_stage_volume.rs`).

## The volume finding

The question was whether Quill can set the volume of the stream it
plays.

- ntgcalls v3.0.3 has no such call. Its C API exports 77 `ntg_*`
  symbols (`vendor/ntgcalls/include/ntgcalls.h` and the shipped
  library's symbol table), and none of them is a volume or gain
  setting.
- Playback goes to ntgcalls' own device writer
  (`NTG_MEDIA_SOURCE_DEVICE` in `src/calls/engine/native.rs` and the
  group playback description in `src/calls/engine/ntgcalls/mod.rs`).
  Quill never sees the samples.
- WebRTC's audio device module has `SetSpeakerVolume`, but that sets the
  OS device volume, and ntgcalls does not expose it.
- The only mixer-side path is to switch the playback writer to
  `NTG_MEDIA_SOURCE_EXTERNAL`, take the PCM from
  `ntg_on_frames_callback`, and play it through Quill's own output with
  a gain. That is a new real-time audio path with its own clock drift,
  device selection and echo-cancellation reference in video chats. It
  cannot be checked without a real stream, so it is out of scope here.

Decisions:

- No stream volume slider: mute stays, through
  `set_group_call_playback_muted`. `LiveFacts.volume` is in place, so
  the slider appears as soon as an engine can set the level.
  `parity:calls-rtmp-volume` stays open.
- Video chats: the stage slider sends
  `setGroupCallParticipantVolumeLevel` on release, which is the same
  request as the participant menu's volume steps. TDLib applies it for
  everyone only when you manage the call; otherwise the server keeps it
  for you. Quill's own playback does not follow it either, for the same
  reason: ntgcalls mixes the call itself and has no gain per source.

## PiP

The stream window has no PiP of its own. The video chat's mini window
is not a picture window, so `pip` is `None` and the button is hidden. P
is not mapped. PR 9 wires it.

## The cursor

GPUI's `Platform` has `hide_cursor_until_mouse_moves` on macOS
(`NSCursor setHiddenUntilMouseMoves`), Windows (`SetCursor(None)` while
hovered), X11 and Wayland. `App` does not expose it. Quill already wraps
the platform in `ZoomPlatform` for the interface zoom, so `ZoomPlatform`
keeps a weak handle and `interface_zoom::hide_cursor_until_mouse_moves`
calls through it. The statement in `call-stream-window.md` that "GPUI has
no API for it" no longer holds. The cursor comes back on the next mouse
move, which also shows the controls, matching tdesktop.

## tdesktop

- `calls/group/calls_group_panel.cpp`:
  - `kHideControlsTimeout` (88): the 5 s idle timeout.
  - `updateWideControlsVisibility` (1300): `setCursorShown(!_rtmpFull || shown)`.
  - `setupFullScreenBehavior` (1501): a mouse move shows the controls
    and restarts the timer.
  - `setupEmptyRtmp` (1983): the no-stream labels.
  - `trackControl` (2197): entering the controls shows them, and the
    timer still runs.
- `calls/calls.style` `groupCallMessageBadge` (1808): the pill's metrics.
- `calls/group/calls_group_viewport_raster.cpp`: the paused tile's
  shadow and the blurred 90 px frame.
- `calls/group/calls_volume_item.h` `MenuVolumeItem`: the 0 to 200 %
  participant volume.

## Deviations from the design

- The LIVE pill in the stream title: tdesktop's RTMP title has no
  badge. The design asks for one, and it reuses the live story's pill.
  The title keeps tdesktop's count of at least 1 viewer, while the
  header hides the count at 0 as designed.
- The pill's colour: it uses the kit's `danger` token, as designed,
  not tdesktop's `attentionButtonFg`. In the dark theme that token is a
  lighter red.
- M and F keys: tdesktop's RTMP panel has no M or F keys. The design
  asks for them. Esc and the full-screen button behave as before.
- No stream volume slider: see "The volume finding".
- Ended overlay: the stream window and the live story still close
  when the call ends, as tdesktop does, so the Ended plate shows only
  between the feed ending and the call ending.
- Failed overlay: the driver never produces `LiveEnd::Failed`, so
  the stream window also treats the joined call's `transport_error` as
  a failure. Retry runs the reconnect path when the call asks for it,
  and Join otherwise.
- "Connecting…" timing: it shows 3.5 s into Buffering, which is 5 s
  after the last frame under the default stall rule (1.5 s). A feed
  whose stall rule is four median gaps, such as a slow screen share,
  reaches it later.
- The live story has no full screen, so the fade and F do not apply
  there. It keeps its own keys: Space types in the comment field, and M
  is not mapped because the field takes letters.
- Tiles take only the Paused and Opening states. A stalled tile
  keeps its last frame undimmed, as tdesktop's viewport does. A tile
  for a stream with no frame yet is new: before this PR, such a stream
  had no tile until its first frame.
- Capture switch: `demo_ui.group_stage` was never set. The RTMP
  demos now set it from `QUILL_DEMO_FULL=1`, and the stream window
  draws its full-screen layout when it is set, because a capture cannot
  go full screen.

## Tests

- Pure:
  - the state-to-overlay mapping and its delays on an injected clock
    (`live_overlay.rs`);
  - the live key map and the pill width (`live.rs`);
  - the fade machine, including the cursor following the controls only
    in full screen (`fade.rs`);
  - the stage volume's level mapping.
- GPUI (`group_call_stream_tests.rs`, `group_call_tiles_tests.rs`,
  `story_viewer/live_tests.rs`):
  - no scrubber, play button, speed control or PiP, plus the pill in
    the title, mute and full screen on the bar;
  - Space, M, F and Esc;
  - the stall overlay 1.5 s after the last frame with the picture kept,
    and "Connecting…" 5 s after it;
  - the paused overlay coming and going with the feed's state;
  - full screen hiding the controls after 5 s, sliding them out and
    hiding the cursor once;
  - the no-stream text with no spinner;
  - a paused tile with the shared overlay;
  - the stage volume shown for the other participant and not for you;
  - the live story header as the live controller.

## Not verified

- No real stream, video chat or live story was played. Every state was
  driven by demo feeds and test clocks.
- The cursor hiding was checked through a counter on the test platform.
  It was not watched on a real screen on macOS, Windows or Linux.
- The stage slider's request was not sent to Telegram. The demo
  reports the level instead.
- Only macOS builds were run. Linux and Windows were not built here.
- Captures (light and dark, not committed): the stream window normal
  and in its full-screen layout, the paused tile and the live story.

## Parity

`parity:calls-rtmp-window-polish`: the cursor now hides with the
controls in full screen, and the letter avatar for a chat with no photo
was already in place (`stream_view.rs` `placeholder`).
`parity:calls-rtmp-volume` stays open.
