# Group call incoming video

## What tdesktop does
tdesktop (tgcalls group instance) always subscribes to participants' video
endpoints and renders them. There is no switch that hides incoming video.

## Bug
`issue_group_sources` (src/calls/engine/ntgcalls/mod.rs) passed NULL playback
camera and screen descriptions for normal RTC video chats and external
writers only for stream modes. In ntgcalls v3.0.3:

- `ntgcalls/src/media/stream_manager.cpp:95-96`, `optimize_sources`, calls
  `enable_video_incoming(writers_.contains(Camera) || external_writers_.contains(Camera), ...)`
  (same for Screen), so incoming video stays off without an external writer.
- `stream_manager.cpp:657-668`, `setup_video_playback_callbacks`, forwards
  frames to the frames callback only when `external_writers_` holds the
  device; `handle_playback_config` (line 599) is the only place that adds it,
  and a NULL description goes through `handle_no_description` which erases it.
- `CallInterface::set_stream_sources` (call_interface.cpp:45-47) re-runs
  `optimize_sources` on the main connection after a playback change, and
  `GroupCall::set_stream_sources` (group_call.cpp:221) does so for the
  screen-share presentation connection, so setting the writers late works.

`native.rs` `frames_trampoline` routes group `PLAYBACK+CAMERA/SCREEN` frames by
ssrc, the same path the 1:1 call and the stream branch already use.

## Change
Playback sources now always pass external camera and screen descriptions
(`external_playback_video`). Capture, mute and stream-mode logic are unchanged.

## Verified
Source reading of ntgcalls v3.0.3 as above; unit test for the descriptions;
gate. Not verified: a live group call (no real call was joined).
