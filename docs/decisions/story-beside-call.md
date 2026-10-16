# Watch a live story beside any call

Parity item `stories-live-beside-call`. Design: `quill-tools/designs/live-gaps.md`, PR B.

## What tdesktop does

Opening a live story makes a `VideoStream` with its own `Calls::GroupCall`.
`Calls::Instance::registerVideoStream` adds it to `_streams`, which
`handleGroupCallUpdate` feeds next to the current call. A stream is never
`_currentCall` or `_currentGroupCall`. So `confirmLeaveCurrent` only runs when
you join a video chat or a stream from a chat, and an incoming call is declined
as busy only for a real call (`inCall()` / `inGroupCall()`). The story's audio
plays mixed with the call's audio.

## What Quill does

- `CallsState.story_stream` is a second tracker, separate from
  `active_group_call`. It only receives. `group_call(id)`,
  `group_call_mut(id)` and `group_call_ids()` find a call in either slot.
- `accept_group_call_update` sends the update for the watched story's call to
  `story_stream`. Every other live-story update is dropped, so a live story can
  never become the current call. The participant, message and verification
  updates find their slot by id.
- The engine opens the story's call with `GroupCallRole::Viewer`. A viewer
  opens no microphone or camera source, and `ntg_mute` is never called for it.
  The tdesktop stream is receive-only in the same way.
- The playback mute is per call (`group_playback_muted: HashSet<i32>`). The
  story's sound toggle mutes only the story, and the stream window's toggle
  mutes only its own call. Nothing ducks.
- When you leave the story, the driver tears down that call's transport and
  sends `leaveGroupCall` for it. It does not touch `left_group_call`, the
  current call, or that call's transport.
- The transport pump, the auto-rejoin and the stream feeder each go through
  both slots.
- The one-call confirm (`current_call_for_create`) reads only `active_call`
  and `active_group_call`, and the incoming-call prompt reads only
  `active_call`, so a story that is open changes neither of them.
- The leave card is removed, along with `leave_other_call_for_live_story`, the
  `live-story-leave-other` Esc layer, the `leave_other` and `retry_join`
  state, `chrome::attention` and `qll_media_live_leave_and_watch`. If the story
  ends before its call shows up, the viewer still closes
  (`check_story_ended_before_call`).
- The live PiP for a story reads the story's slot for its viewer count, its
  mute state and its end.

## Limits

- ntgcalls keys each connection by chat id, and a 1:1 call is keyed by the
  peer's user id. When the story's poster chat matches the chat of the current
  video chat, or matches the 1:1 peer's id, `ntg_create_call` fails for the
  story. The driver isolates that failure to the story, and the current call
  keeps running. This has not been tried on a live account.
- No one has heard two real streams mixing through WebRTC's audio device
  module.
