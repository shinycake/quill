# Group-call mute button tells the truth

## Problem (reactive-state audit, finding 3)

`set_group_call_self_mute` flipped the tracked `is_muted_self` flag first and
then ignored the results of `engine.set_group_muted` and the
`toggleGroupCallParticipantIsMuted` request. A failing engine left the button
showing "muted" while the microphone was open, and an admin mute from the server
never reached `is_muted_self` or the engine. Push-to-talk used the same path.
The 1:1 call path (`set_call_muted`) already did it right: engine first.

## What tdesktop does

tdesktop's `GroupCall::setMuted` applies the mute locally and tells the
instance (`updateInstanceMuteState`); the participant update for yourself then
decides the state (`MuteState::ForceMuted` when an admin muted you, with the
instance muted to match). Quill's difference is intentional: it never shows a
mute the engine did not apply, and the server's word on your own participant
wins.

## What changed

- `set_group_call_self_mute` (driver) calls the engine first. A failure returns
  `SelfMuteError::Engine`, leaves `is_muted_self` as it was, and sends nothing to
  Telegram. After the engine succeeds the flag flips and the request goes out; a
  failed send returns `SelfMuteError::Send` (the button is truthful, other
  participants may see the old icon).
- The UI (button and push-to-talk) shows the existing status-note path with a
  new string, "Couldn't change the microphone." (`lang/en/av.strings`).
- `Session::accept_group_call_participant_update` settles `is_muted_self` from
  the own participant's `is_muted_for_all_users` once the call is joined.
- The group-call transport pump reconciles the engine with `is_muted_self`
  (new `group_mute_state`, the same pattern the camera already used), so a mute
  imposed by the server mutes the engine, and a failure is retried on the next
  update and surfaced as `transport_error`. Capture was already gated on
  `is_muted_self`, so a muted button never feeds the microphone.
- Mock engine: `fail_group_mute()` for tests.

Camera and screen share: the camera already follows the TDLib flag through the
pump (`group_camera_state`), and screen share starts engine-first with rollback
(`start_group_call_screen_share`). Neither flips UI state ahead of the engine,
so they are unchanged.

## Verification

- Driver tests (`src/connect/tests/group_calls/transport.rs`): engine failure
  keeps the state and sends nothing; a server mute settles the flag and mutes
  the engine, an unmute opens it again; push-to-talk press/release go through
  the engine first and a failed release keeps the mic open.
- Visual capture of the voice chat panel, muted and unmuted, light and dark.
- `gate.sh`.
