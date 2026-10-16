# Capture follows the picked camera and microphone; deterministic spell-engine test

## Capture devices (sweep item 23, second half)
tdesktop records round videos and shows the profile-photo camera on the device chosen in Settings > Calls.
Quill's ffmpeg capture (`media_tools::capture_input`) ignored that pick: Windows took the first DirectShow camera
and microphone (an OBS virtual camera listed first won), Linux the first `/dev/video*`, macOS `default:default`.

What changed
- New `src/capture_choice.rs`: the camera and microphone names picked in the call device menu
  (`select_call_camera`, `select_call_devices` publish them), plus pure selection helpers.
  A pick that is gone falls back to the first device (Windows, Linux) or `default` (macOS); no devices is an error as before.
- Windows: DirectShow camera and microphone chosen by name. Linux: V4L2 node chosen by path or file name.
  macOS: the AVFoundation device list is parsed for indices (only when something was picked).
- Linux microphone stays on `default`: PulseAudio source names do not match the call engine's names, and a wrong
  `-i` would break the recording. The in-call mic already honors the pick through cpal (`find_input`, same fallback).

Unverified: no Windows or macOS hardware here; selection logic is unit-tested, the ffmpeg invocations are not run.
The pick is the in-call menu selection for this run of Quill (there is no persisted device setting).

## Flaky `slow_engine_build_does_not_block_and_swaps_in`
It asserted `elapsed < 250ms` and failed under load. Now the build blocks on a condvar gate the test opens:
the test checks the build had not finished when the call returned and the old checker was still in place, then opens
the gate and checks the swap. The other test uses an instant builder. No wall-clock assertion remains.

## Verification run (macOS, real ffmpeg)
`cargo test --lib macos_capture_input -- --nocapture`: with a saved camera pick of "Capture screen 0" (a real
AVFoundation device on this machine), `capture_input` opened input `3:default`; with an unplugged pick it opened
`default:default`. Windows and Linux rely on the unit tests in `capture_choice` (CI). No Settings UI changed, so no screenshots.
