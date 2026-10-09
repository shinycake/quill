# Calls polish: push-to-talk, join as, pinned tiles, paused streams

## What tdesktop does
- Push-to-talk (`calls/group/calls_group_call.cpp`, settings_calls.cpp): global shortcut plus a release delay; the mic stays closed until the key is held.
- Join as (`calls_group_common.cpp`, join-as box): `getVideoChatAvailableParticipants`, saves the pick with `setVideoChatDefaultParticipant`.
- Pin (`calls_group_viewport*.cpp`): local UI state; the pinned stream is shown large, and the pin drops if the stream goes away.
- Paused streams: `groupCallParticipantVideoInfo.is_paused` dims the tile with a paused label.

## What changed
- `src/calls/ptt.rs`: pure `PushToTalk` state machine (auto-repeat ignored, release delay, re-press cancels the mute) and `PttConfig` stored in `CallPrefs.push_to_talk` (`call_prefs.json`, serde-defaulted so old files load). Settings > Calls: switch, key capture, delay presets (20/200/500/1000 ms). The voice chat window handles key down/up (focus tracked), mutes on join and on enabling, and releases the mic when the window deactivates.
- `src/calls/tile_pin.rs`: pure `TilePin` (toggle, layout, drop vanished stream). `src/ui/group_call_tiles.rs`: one tile per camera/screen stream, click toggles the pin, pinned tile is large, "Camera paused" / "Screen sharing paused" overlay.
- Join as: `videoChat.default_participant_id` parsed, `getVideoChatAvailableParticipants` fetched once per unjoined chat-bound call, picker row in the window, choice sent as `joinVideoChat.participant_id` (also on rejoin) and saved with `setVideoChatDefaultParticipant`.
- Demo capture: `QUILL_DEMO_CAPTURE_WINDOWS=1` also saves other windows (the voice chat window) as `<path>.window<N>.png`; new demos `ready-group-call-polish`, `ready-group-call-join-as`.

## Skipped / already present
- Global hotkey: no cross-platform hotkey crate in the tree and macOS needs Accessibility permission; push-to-talk works only while the voice chat window is in front (stated in Settings). Linux and Windows get the same in-window behaviour.
- Noise suppression: the ntgcalls binding exposes no such option; skipped.
- Raise hand and per-participant volume / mute for me already existed.
- Pin is not true fullscreen, and "pause my own screen share" is not built, so those parity ids are not claimed.

## Verified
Unit tests (PTT machine, pinning, join-as pick, request builders, messageSenders reducer path), gate OK, demo screenshots of the pinned/paused and join-as states. Not verified: real key events in the window, Settings screenshot, live calls.
