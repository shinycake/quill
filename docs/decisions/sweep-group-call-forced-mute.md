# Group-call force-mute follow-ups

Follow-up to `sweep-group-call-mute-truth` (re-review notes).

- The pump makes the engine follow `is_muted_self || ActiveGroupCall::is_force_muted()`. The admin hold is read off the own participant row (muted for all and not `can_unmute_self`), with no stored flag, so a force-mute that lands while a local toggle is in flight (the reducer holds `is_muted_self`) still mutes the engine. The capture gate and tap gate use the same helper.
- Raised hand: tdesktop sets `ForceMuted` or `RaisedHand` from the same server state (`is_muted && !can_self_unmute`, split only by the hand rating) and refuses unmute in both. The code already covers it through `can_unmute_self`; the `NotAllowed` doc now says so (doc fixed, no extra check).
- `ToggleGroupCallParticipantMute` never answered froze own-row settling. It is now dropped by the periodic sweep after `PENDING_REQUEST_TTL` (`RequestPurpose::is_swept_without_error`, separate from `is_sweepable` so a failed toggle still surfaces as a user-action error).

Tests (`src/connect/tests/group_calls/transport.rs`): `group_call_force_mute_during_an_in_flight_toggle_mutes_the_engine`, `group_call_unanswered_mute_toggle_is_swept_and_settling_resumes`. No UI change.
