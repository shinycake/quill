# Calls live: full-screen pinned stream and incoming call notification

## What tdesktop does
- Full screen (`calls/group/calls_group_panel.cpp`, `calls_group_viewport*.cpp`): the call window can go full screen. The pinned stream then fills the window on a black background with square corners, and Esc leaves full screen.
- Incoming call (`calls/calls_instance.cpp`): when a call rings and Telegram is not in front, the system shows a notification with the caller's name. The call can be answered or declined from it.
- Also in the panel: a "pin on top" window option, a speaking indicator for your own mic, and the peer's low-battery and muted-microphone hints.

## What changed
- Pinned tile, full screen. The pinned tile has a full screen button. It toggles the voice chat window's full screen state, and while full screen with a pinned stream the window shows only that stream, fitted without cropping on black, with the name, an unpin button and an exit button. Esc also exits. If the stream goes away the window shows the member list again. Same frame path as the windowed tiles (`latest_group_video_frame`), so it works on Linux and Windows too. Pure logic in `src/calls/tile_pin.rs` (`stage_tile`, `stage_active`, `exits_fullscreen`), UI in `src/ui/group_call_tiles.rs` and `src/ui/group_call_panel.rs`.
- Incoming call notification. `src/notify_call.rs` decides (pure, tested) whether a ringing call gets a notification: only when no Quill window is active and once per call. A locked app shows "Quill" and no buttons, so answering cannot skip the passcode. macOS and Windows use the GPUI system notification with Accept and Decline buttons, and the response comes back through the existing `on_system_notification_response` callback using a `:call:<id>` tag that cannot collide with chat tags. Linux uses `notify-send --wait` with `--action` buttons through the same backend selection as message notifications. Picks are matched against the currently ringing call, so a late click on an old notification does nothing. Accept calls `acceptCall` and Decline calls `discardCall`, both already used by the call window. The toast is withdrawn when the call stops ringing (macOS and Windows).
- Demo capture: `ready-group-call-stage` (the stage layout; capture cannot go full screen, so a demo flag forces it).

## Skipped
- Video tiles live (`calls-video-tiles`): needs a real group call with the ntgcalls runtime. Not claimed.
- Speaking detection: the ntgcalls binding exposes no local audio level, so there is nothing to apply a threshold to. TDLib's `setGroupCallParticipantIsSpeaking` is ready to use once the engine reports a level.
- Peer indicators: the engine has a muted flag in its media state but it is not wired to the UI, and there is no battery signal in the binding or TDLib API. Not claimed.
- Window options: GPUI has `WindowKind::Floating` only at window creation and only honored on macOS, with no runtime always-on-top on Linux or Windows. Not claimed.

## Verified
Unit tests for stage selection, Esc handling, the notification decision (active window, once per call, locked), tag round trip and separation from chat tags, action ids, and `notify-send` output and argument safety. Gate OK. Demo screenshots of the windowed pinned tile (with the new button) and the stage were viewed. Not verified: a real full screen toggle on any OS, the notification on screen on any OS (system notifications cannot be captured in-process), Accept and Decline round trips against a real ring.
