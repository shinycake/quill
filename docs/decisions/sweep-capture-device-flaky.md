# Capture follows the picked camera and microphone; deterministic spell-engine test

## Capture devices (sweep item 23, second half)
tdesktop records round videos and shows the profile-photo camera on the device chosen in Settings > Calls.
Quill's ffmpeg capture (`media_tools::capture_input`) ignored that pick: Windows took the first DirectShow camera
and microphone (an OBS virtual camera listed first won), Linux the first `/dev/video*`, macOS `default:default`.

What changed
- New `src/capture_choice.rs`: the pure selection helpers and the `CaptureChoice` value (camera and microphone names).
  There is no stored copy of the pick. `ConnectDriver::capture_choice()` derives it on demand from the driver's own
  `selected_camera` / `selected_devices.0` and `call_devices_cache`, so it cannot disagree with the call menu, resets
  with the driver (account switch) and is per window. `QuillApp::capture_choice()` reads it from the live driver and the
  three UI callers (round video, story reply video, profile-photo camera) pass it into
  `media_tools::capture_input(&choice)`. A pick that is gone falls back to the first device (Windows, Linux) or
  `default` (macOS); no devices is an error as before.
- Windows: DirectShow camera and microphone chosen by name. macOS: the AVFoundation list is parsed for indices (only
  when something was picked). Linux: the V4L2 node is chosen by path, file name, or the card name in
  `/sys/class/video4linux/videoN/name`, because the call engine lists cameras by name, not `/dev/videoN`. Whether the
  engine's Linux names equal the card name could not be checked here (ntgcalls is a prebuilt library); a name that
  does not match falls back to the first node as before.
- Linux microphone stays on `default`: PulseAudio source names do not match the call engine's names. The in-call mic
  already honors the pick through cpal.
- Capture starts on the UI thread (`begin_video_note_capture`, `begin_story_video_capture`), so the ffmpeg device
  lists (AVFoundation, DirectShow) now run with a 3 s limit (`stderr_within`) and are killed on timeout; the camera
  stream already starts on a worker.

Unverified: no Windows or Linux hardware here; selection logic is unit-tested on fixtures, the Windows and Linux ffmpeg
invocations are not run.
The pick is the in-call menu selection for this run of Quill (there is no persisted device setting).

## Flaky `slow_engine_build_does_not_block_and_swaps_in`
It asserted `elapsed < 250ms` and failed under load. Now the spell engine builder is an `Arc<dyn Fn>` and each test
owns a `Gate` (condvar plus finished flag) its build waits on; no process statics are shared between tests. The test
checks the build had not finished when the call returned and the old checker was still in place, then opens the gate and
checks the swap. The other test uses an instant builder. No wall-clock assertion remains.

## Tests
- `connect::tests::calls::media::call_menu_pick_reaches_the_capture_input`: driver selection to capture choice to the
  AVFoundation argument, including clearing the pick and an unplugged device.
- `capture_choice::tests::v4l2_matches_the_card_name_the_engine_lists`, `v4l2_matches_path_or_file_name`.
- `media_tools::tests::stderr_within_returns_output_and_gives_up_on_a_stuck_command`.
- `ui::spell_engine_tests` (per-test gate).
