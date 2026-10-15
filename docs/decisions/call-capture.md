# Call capture: Quill's own microphone, noise suppression

Slice 1 of 4 of `quill-tools/designs/call-media-capture.md`. Closes
`parity:calls-noise-suppression`. Screen capture (the chooser, pause, the
macOS, Windows and Linux backends) comes in slices 2 to 4 and plugs into the
same module.

## What tdesktop does

- The group call settings box (`calls_group_settings.cpp:380-391`, opened
  from the call menu's "Settings", `calls_group_menu.cpp:599`) has a switch
  "Enable Noise Suppression" (`lng_group_call_noise_suppression`) under the
  microphone test meter. It is saved in `Core::Settings::
  groupCallNoiseSuppression` and applied to the running call at once
  (`GroupCall::setNoiseSuppression` → tgcalls
  `setIsNoiseSuppressionEnabled`, `calls_group_call.cpp:2283`), which runs
  RNNoise on the microphone.
- 1:1 calls always have noise suppression on and no switch for it
  (`calls_call.cpp:1140`, `.enableNS = true` in the tgcalls config).
- The switch exists only in that box. Settings > Calls has the devices and
  the microphone test (`settings_calls.cpp`, `AudioInputTester`, the raw
  input level) but no noise suppression row.

## What Quill did before

ntgcalls opened the microphone itself (`NTG_MEDIA_SOURCE_DEVICE`) and its
outgoing audio channel has WebRTC's noise suppression, echo cancellation and
gain control all off, with no switch in the C API
(`codex-call-audio-levels.md`). Calls had no noise suppression at all, and
the level tap opened the microphone a second time to measure your level.

## What changed

- **`src/calls/capture/`** (new). `mod.rs`: `MicCapture` (start on a named
  device into a sink, stop), `AudioFrame` / `VideoFrame`, `CaptureEvent`,
  `CaptureError`, `MicControls` (noise suppression and mute as atomics, read
  per frame) and `LevelCell` (the peak-since-read atomic the level tap had).
  `denoise.rs`: `MicProcessor` turns chunks of any size into exactly 480
  samples (10 ms at 48 kHz, one ntgcalls external frame), runs
  `nnnoiseless::DenoiseState` (RNNoise) when the switch is on, sends silence
  while muted, and measures the level in tgcalls' windows. `feeder.rs`:
  `FrameSink` (bounded, drops the oldest frame, never blocks the audio
  callback; 8 audio / 4 video frames), `CaptureClock` (Unix ms read once,
  then the monotonic clock; never goes backwards) and `CaptureFeeder` (one
  thread per call, validates frame sizes, logs engine errors at most once a
  second). `mic_cpal.rs`: the microphone through cpal, by name from the
  engine's device list, at 48 kHz when the device offers it (otherwise the
  existing voice-note resampler), down-mixed to mono.
- **Engine** (`src/calls/engine/external.rs`,
  `src/calls/engine/ntgcalls/external.rs`, `mock/external.rs`):
  `CallEngine::external_frame_sender(target)` returns a `Send` sender bound
  to a call once its sources are issued; `send_external_frame` is the
  one-shot form. The ntgcalls sender calls `ntg_send_external_frame` behind
  an `RwLock` gate: the engine attaches the instance when it creates it and
  detaches it, waiting for in-flight sends, before destroying it, so a
  feeder can never reach a freed instance. The microphone description in
  `issue_group_sources` and the 1:1 `set_media_sources` is now
  `NTG_MEDIA_SOURCE_EXTERNAL`, 48000 Hz, 1 channel, with no device JSON.
  Hook lines in `ntgcalls/mod.rs`: the module line, the gate field and its
  init, attach in `ensure_instance`, detach in `Drop`, and the two
  microphone descriptions.
- **`QUILL_CALL_CAPTURE=native`** keeps ntgcalls' device source (and the
  level tap's own device open) for debugging. Builds without the `ui`
  feature have no cpal and always use it.
- **Driver** (`src/connect/call_capture.rs`): `pump_call_capture` runs
  before the level pump. It opens the chosen microphone while a 1:1 call has
  a transport (connecting, connected or reconnecting) or while you are
  joined to a group call whose transport is up, follows mute (silence
  frames, same stream), reopens on a different microphone or call, and
  after a failed open or a lost device shows the error in the call window
  and retries after 2 s. A 1:1 call always denoises, whatever the switch
  says, as in tdesktop; a group call follows the switch, and
  `apply_call_noise_suppression` applies it to the running stream at once.
- **Level** (`connect/call_audio.rs`, `calls/level_tap.rs`): your level in
  a group call now comes from the call's own stream (`LevelCell`), so the
  second device open is gone. `LevelTap` is rebuilt on the capture path for
  the Settings > Calls microphone test (and native mode) and measures the
  raw input, like tdesktop's `AudioInputTester`.
- **Setting**: `CallPrefs::call_noise_suppression` in `call_prefs.json`,
  on by default (tdesktop shows it on), and files written before it existed
  read as on.
- **UI**: the voice chat window's menu gains "Settings"
  (`lng_group_call_settings`), which opens a Settings box
  (`lng_group_call_settings_title`, "Done" closes, Escape closes) with the
  switch row (kit `Switch`, `lng_group_call_noise_suppression`). As in
  tdesktop, that box is the only place the switch lives; Settings > Calls
  has no noise row.
- `nnnoiseless` 0.5.2 (BSD-3-Clause, allowed by `deny.toml`), without its
  default features (no command-line tool). `THIRD_PARTY.md` lists it;
  `THIRD_PARTY_LICENSES.md` is regenerated with
  `scripts/third-party-licenses.sh`.

## Deviations

- 1:1 calls get their always-on noise suppression from RNNoise in Quill
  rather than from WebRTC's processing, which ntgcalls keeps off. The
  behavior matches tdesktop (on, no switch); the algorithm differs.
- The voice chat window's Settings box holds only the noise suppression
  switch. tdesktop's box also has devices, the microphone test and
  push-to-talk; Quill has those in the call window's device menus and in
  Settings > Calls. Moving them into the box is its own gap.
- `MicCapture` is not `Send`: cpal's stream is not `Send` on every
  platform, and the capture lives on the driver thread like the level tap
  did. Frames still leave on cpal's thread and the feeder thread.
- The design names keys `Q_CALLS_*`; Quill's language packs use tdesktop's
  own key when it has one, so this slice uses `lng_group_call_noise_
  suppression`, `lng_group_call_settings` and `lng_group_call_settings_title`.
  The screen-sharing keys come with slice 2.
- The `ScreenCapture` trait is not added yet; `VideoFrame`, the video sink
  size and the feeder's I420 path are in place for it.

## Cost

- One microphone open per call instead of two (the level tap's second open
  is gone).
- Per 10 ms frame: one 480-sample RNNoise pass on cpal's audio thread
  (RNNoise is about 1 % of one core at 48 kHz), one 960-byte frame through
  the sink. Queues: at most 8 audio frames (7.7 KB). `DenoiseState` is
  allocated once per open and again only when the switch turns back on.
- Measured in the test build: the loopback harness pushes 100 microphone and
  100 320×240 I420 frames through `ntg_send_external_frame` in 50 ms
  including setup; the denoise unit test runs 200 frames through RNNoise.
  No release-build benchmark was run (rules: no release builds unless
  asked).

## Verified

- Unit tests: 10 ms framing from arbitrary chunk sizes, timestamps, i16
  round trip and clamping, silence through RNNoise, RNNoise lowering steady
  noise by more than half, muted frames silent with level 0, level from
  processed frames; the sink drops the oldest frame and a closed sink wakes
  its reader; the clock is anchored and monotonic; little-endian PCM; the
  feeder sends with rising timestamps, rejects wrong audio and I420 sizes,
  forwards nothing while paused, counts and keeps engine errors; frame-size
  rules; `QUILL_CALL_CAPTURE` parsing; the External and native microphone
  descriptions; the setting's default and old-file load.
- Driver tests (mock engine, fake microphone): a group call's frames reach
  the group's external microphone only once the transport is up; mute keeps
  the stream and sends silence; leaving closes it; the switch applies to
  the running stream without a reopen; a lost device is reported and not
  reopened within 2 s; a refused microphone shows its error and leaving
  clears it; your level comes from the call stream; a 1:1 call opens the
  chosen microphone by name, denoises with the switch off, reopens on a new
  pick and stops on a failed transport.
- GPUI UI tests: the switch is on by default and saves both ways; the call
  window's Settings box opens from its action, carries the switch, and
  Escape closes it.
- Loopback harness (`loopback_external_frames_are_accepted`, `--ignored`,
  `QUILL_NTGCALLS_LIB` pointing at an ntgcalls 3.0.3 library): a local
  instance with external microphone and screen sources accepts 100 frames
  of each; a wrong size is refused before ntgcalls; after the engine is
  dropped the sender fails with `Unavailable` instead of crashing. No call
  is connected and nothing leaves the process. (The library in the main
  checkout's `vendor/ntgcalls` is older than the current loader and does
  not load; the run used the 3.0.3 library from the installed app.)

## Not verified

- A real call: never started on the owner's account. Whether the remote
  side hears the external microphone, and how RNNoise sounds in a noisy
  room, need the owner's manual check (design "Manual").
- cpal capture on Windows and Linux, and device matching by name there
  (ntgcalls' and cpal's names for the same device are expected to match;
  an unmatched name falls back to the system default).
