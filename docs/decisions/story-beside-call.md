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
- A story stream that gave up shows Failed with Retry in the viewer
  (`ActiveGroupCall::join_failed`). It gives up when its transport failed, its
  first `joinLiveStory` failed, or its rejoins ran out. Retry leaves the failed
  slot and joins the story again. None of these failures reach the call
  window's error line. The story's leave reuses `LeaveGroupCall`: an error for
  an id that is neither the current call nor `left_group_call` is the story's,
  and it is dropped. A join that is still pending when you leave is
  forgotten, so its late answer cannot land in a new slot for the same
  story.
- Switching accounts leaves the live story first: the viewer and a story PiP
  close, and the story's stream is left. The outgoing account then goes to the
  background, or is closed if it is in a real call.

## Limits

- A live story and a group call in the same chat (a channel's live story and
  its video chat or stream) share the chat's stream feed,
  `LiveKey::Stream { chat_id }`, and the engine's stream mode and quality,
  which are keyed by chat too. Whichever comes first, the call you join wins,
  and the story shows Failed with Retry. Retry fails again for as long as you
  are in the call.
  - If the call comes first, the story's join is not sent
    (`refuse_story_stream_in_member_chat`) and the error lands on its slot.
  - If the story comes first, the driver checks before it creates the video
    chat's connection (`yield_story_stream_to_chat`). It tears down the
    story's transport and marks the slot failed.
  - Feeds keyed by call rather than by chat would be needed to show both.
  - None of this has been tried on a live account.
- With ntgcalls a 1:1 call with the story's poster took the story's key as
  well. tgcalls keeps every call apart, so the story plays beside it.
- No one has heard two real streams mixing through WebRTC's audio device
  module.
