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

- `set_group_call_self_mute` (driver) is fail-closed for mute and engine-first
  for unmute. Muting always sets `is_muted_self` (the capture gates in
  `call_capture.rs` and `call_audio.rs` then stop audio on their own), tells
  Telegram, and reports `SelfMuteError::Engine` if the engine refused; the
  pump retries the engine. Unmuting calls the engine first and an error leaves
  you muted. Unmuting is refused at the source with `SelfMuteError::NotAllowed`
  while `can_unmute_self` is false (admin hold or raised hand).
- `Session::accept_group_call_participant_update` settles `is_muted_self` from
  your own participant row, but ignores it while a
  `ToggleGroupCallParticipantMute` request is in flight, so a stale update
  cannot reopen the microphone right after a local mute.
- The pump reconciles the engine with `is_muted_self` (`group_mute_state`) and
  records a retry failure in `transport_error` once instead of every tick.
- The voice chat window's own member row derives its icon and status from
  `call.is_muted_self` (`member_look`), with the admin hold and raised hand
  outranking it; new string `qll_av_muted_status` ("muted").
- A failure to tell Telegram after the engine changed shows its own note
  (`qll_av_microphone_changed_others_not_told`) instead of "Couldn't change the
  microphone." Chosen over a pump retry because the local state is already
  truthful and the next participant update settles the server side.
- Stale doc comments fixed. Mock engine: `fail_group_mute()`.
- Capture harness (`QUILL_DEMO_CAPTURE_WINDOWS`): a second window that is not
  on screen gets no display-link frames, so `render_to_image` returned its
  first frame ("You are Live" next to a muted bar). The harness now draws each
  extra window before capturing it. This was the cause of the earlier stale
  screenshot; the app itself repaints both windows (both observe `QuillApp`).

## Verification

- Driver tests (`src/connect/tests/group_calls/transport.rs`):
  `group_call_unmute_failure_leaves_you_muted`,
  `group_call_mute_failure_fails_closed_and_the_pump_retries`,
  `group_call_stale_own_update_does_not_undo_a_local_mute`,
  `group_call_unmute_is_refused_while_force_muted`,
  `group_call_server_mute_settles_the_button_and_the_engine`,
  `group_call_push_to_talk_drives_the_engine_first` (a failed release now
  stays muted), `group_call_self_mute_reaches_the_microphone_and_telegram`.
- UI test: `own_row_follows_the_call_mute_flag` (group_call_panel.rs).
- Captures of `ready-call-screen-share` after the call bar mic click and an 800 ms
  pause, light and dark, main window and voice chat window: bar, big button
  ("Unmute") and own row ("muted", crossed mic) agree; unmuted shows
  "You are Live" and "listening".
- `gate.sh`.
