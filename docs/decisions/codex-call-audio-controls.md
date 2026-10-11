# Call audio controls: push-to-talk delay, noise suppression, echo cancellation

## What tdesktop does
- Push-to-talk (`calls/group/calls_group_settings.cpp`, `calls_group_call.cpp`): a switch, a shortcut recorder, and a "Release delay: N ms / N.NN s" slider with 201 positions (`DelayByIndex(i) = i * 10`, so 0 to 2 s). Shortcuts are global (`base::GlobalShortcuts`) where the platform allows it; macOS asks for Input Monitoring first. `GroupCall::pushToTalk` opens the microphone on press and mutes after the delay on release; a press inside the delay cancels the mute.
- Noise suppression: one "Enable Noise Suppression" switch in the group call settings box (`groupCallNoiseSuppression`, default on), applied live with `setIsNoiseSuppressionEnabled` and at start through `initialEnableNoiseSuppression`.
- Echo cancellation: no setting anywhere. 1:1 calls hard-code `enableAEC = false, enableNS = true, enableAGC = true` in the `tgcalls::Config`; group calls expose only the noise suppression switch.

## What was already in Quill
Push-to-talk existed: the pure state machine with release delay (`src/calls/ptt.rs`), the system-wide hook on macOS, Windows and X11 with the Wayland in-window fallback (`src/calls/ptt_global*.rs`, `codex-ptt-global.md`), the shortcut recorder and the switch in Settings > Calls. The delay was a four-preset radio group (20, 200, 500, 1000 ms).

## What changed
- The release delay is now tdesktop's slider: 0 to 2000 ms in 10 ms steps, label "Release delay: 200 ms" or "1.25 s" (`delay_by_index`, `index_for_delay`, `delay_label` in `src/calls/ptt.rs`). The UI is `src/ui/ptt_delay.rs`; the label follows the drag and the setting is saved on release. Old prefs files still load (values are clamped; the upper bound grew from 1000 to 2000 ms).
- `parity:calls-push-to-talk` goes in the fragment: shortcut recording, global shortcut where the OS allows it, release delay slider, persistence.

## Not done, and why
The call engine is the unmodified prebuilt ntgcalls v3.0.2 (`scripts/vendor-ntgcalls.sh`, SHA-256 pinned). Its C API (`vendor/ntgcalls/include/ntgcalls.h`, 76 exported `ntg_*` symbols, checked with `nm`) has no audio-processing setting: no echo canceller, noise suppression or gain switch, and no call-time options struct. The library contains WebRTC's `AudioProcessing` (the strings are there) but configures it internally. So neither of these can be switched from Quill:
- `parity:calls-noise-suppression` (tdesktop's group call switch)
- `parity:calls-audio-fx` (echo cancellation and noise suppression toggles; tdesktop has no echo cancellation setting at all)

A switch that stores a preference and changes nothing would mislead, so no UI or dead preference was added. Ways to unblock: a patched ntgcalls build that exposes the `AudioProcessing` config (needs a custom build pipeline in place of the pinned prebuilt), or an upstream request to pytgcalls/ntgcalls. Denoising on Quill's side (capture the microphone, denoise, feed it as an external source) would need ntgcalls to accept external audio frames; `ntg_send_external_frame` exists but is only used for video here, and it cannot be tried without a live call.

## Verified
Unit tests for the slider mapping, label text, clamping and a 2 s release delay in the state machine (`src/calls/ptt.rs`). `gate.sh`: GATE OK. Not verified: dragging the slider in a window (no screenshot taken), real key events on any OS, live calls (none were started).
