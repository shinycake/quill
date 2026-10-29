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
  feature have no cpal and always use it. Removed on 2026-10-29 with ntgcalls: the
  tgcalls library opens no microphone (docs/decisions/tgcalls-engine.md).
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


# Slice 2: macOS screen capture, the source chooser, pause

Slice 2 of 4. Closes `parity:calls-screen-source-chooser` and
`parity:calls-screen-share-pause` on macOS. Windows and Linux keep ntgcalls'
own desktop capture and the direct share button until slices 3 and 4 add
their backends.

## What tdesktop does

- **Chooser** (`calls/group/ui/desktop_capture_choose_source.cpp`):
  - It is a window of its own that stays on top, with an empty title. It is
    sized for 2.5 rows of a 3-column grid. Sources are 235 × 165 with skips
    of 2 / 10 and margins of 12 / 8 / 12 / 6. The picture sits inside a
    7 / 7 / 7 / 33 padding, with a semibold white label 7 px from the
    bottom. The background is `groupCallMembersBg` and the title strip is
    `groupCallMembersBgOver`.
  - It lists screens first ("Screen {index}", `lng_group_call_screen_title`),
    then windows by title, or "Window N" for untitled windows (a literal in
    tdesktop).
  - Each source gets one picture. A 1 fps capture runs until the first
    frame, then is dropped (`clearHelper`), so the pictures are not live.
  - A click selects a source (`groupCallMuted1` background; hover
    `groupCallMembersBgOver`). There is no double-click, no tabs and no
    Escape handling.
  - When you are not sharing, the first screen starts selected. When you
    are, the shared source starts selected. The bottom bar shows Cancel and
    Share Screen, or Stop Sharing (`groupCallMemberMutedIcon` text) when the
    selection is the shared source. Buttons are 34 px, uppercase, 19 px from
    the edges.
  - "Share System Audio" shows only where loopback audio exists, which is
    Windows only.
  - With no sources the chooser does not open. Share Screen and Stop Sharing
    close it.
- **Access** (`calls_group_common.cpp:49`, `ScreenSharingPrivacyRequestBox`):
  without screen recording access a box replaces the chooser. It shows
  `lng_group_call_mac_screencast_access`, a blank line, then
  `lng_group_call_mac_recording` with "Screen Recording" in bold, plus
  "Open Settings" and Cancel. The same box is used in group and 1:1 calls.
- **Entry points:** the voice chat's share button and the 1:1 call's
  screencast button always open the chooser, sharing or not
  (`calls_group_panel.cpp:1589`, `calls_panel.cpp:413`). The voice chat
  menu's "Stop Sharing" stops directly (`calls_group_menu.cpp:587`).
- **Pause:** when the captured window is hidden, a group call sends
  `toggleGroupCallScreenSharingIsPaused` (`setOnPause`,
  `SendUpdateType::ScreenPaused`). 1:1 calls do not send it ("Paused not
  supported right now", `calls_call.cpp:578`).
- **Paused tile** (`calls_group_viewport_raster.cpp:105-280`): the last
  frame is scaled into 90 × 90 and blurred with radius 15
  (`BlurLargeImage`), under a black shadow of alpha 80 with the
  `groupCallPaused` icon. "Video is paused" (`lng_group_call_video_paused`)
  shows only in wide mode. Camera and screen tiles use the same text.

## What changed

- **`src/calls/capture/screen.rs`** (new): `ScreenBackend` (access, privacy
  settings, sources, one thumbnail, start) and `ScreenCapture`. It also has
  tdesktop's labels (`source_labels`), `fit_capture_size`, `nv12_to_i420`,
  the I420 → RGBA preview and `PreviewSlot` (your own tile's picture,
  480 px wide, at most 4 per second). `native_backend()` returns `None` off
  macOS.
- **`src/calls/capture/screen_mac.rs`** (new, macOS + `ui`) uses
  ScreenCaptureKit through `objc2-screen-capture-kit` 0.3.2:
  - Sources come from `SCShareableContent`: displays, then on-screen layer-0
    windows of other apps that are at least 100 × 100.
  - Thumbnails come from `SCScreenshotManager`.
  - Capture is an `SCStream` delivering NV12 (`420v`) at up to 1920 × 1080
    and 15 fps on a dispatch queue. Each frame is converted to I420 into
    the sink, with the preview offered alongside.
  - Access is checked with `CGPreflightScreenCaptureAccess`.
    `CGRequestScreenCaptureAccess` and the Privacy & Security
    "Screen Recording" pane handle "Open Settings".
  - A shared window is polled every 500 ms (`CGWindowListCopyWindowInfo`).
    Off screen pauses, back on screen resumes, gone ends the share. A
    stream stopped by the system ends it too.
- **Engine** (kept small): `external_screen_capture_enabled()` (macOS
  build, `QUILL_CALL_CAPTURE` not `native`).
  `capture_screen_description` sends an `NTG_MEDIA_SOURCE_EXTERNAL` screen
  description (1920 × 1080 @ 15; each frame carries its own size) in place
  of the desktop one, in both `issue_presentation_sources` (group) and
  `set_media_sources` (1:1).
- **Request:** `toggle_group_call_screen_sharing_is_paused`, sent under the
  existing `CallsPurpose::ToggleGroupCallVideo` purpose.
- **Driver** (`src/connect/call_screen.rs`, new):
  - It holds the chooser's pick and opens the capture once the call's
    screen stream is set up: a joined group call that is sharing, or a 1:1
    call that is sharing with a transport.
  - It feeds frames through the feeder into `external_frame_sender`.
  - It follows pause edges both ways: the feeder stops forwarding, and
    group calls send the request once per edge and set
    `ActiveGroupCall::screen_share_paused`.
  - A closed window or a failed capture ends the share and keeps the
    error.
  - It publishes your preview into your own screen tile's frame slot.
  - It switches source when the pick changes.
  - Stopping no longer needs a source: the group gate moved after the stop
    branch.
- **UI:**
  - `screen_chooser.rs` holds the chooser window and the privacy box over
    the call window, with Escape closing the box as it closes every box.
  - `group_call_paused.rs` held the paused tile, using the blurred last
    frame (`wallpaper_raster::blur`). The tiles now take the paused
    overlay from `video_player::controls` (`video-player-live-ui.md`).
  - `screen_share_state.rs` holds the state.
  - The voice chat menu's Share Screen and the 1:1 share button open the
    chooser when there is a backend; otherwise they share directly as
    before.
- **Strings:** tdesktop keys `lng_group_call_screen_share_start`,
  `lng_group_call_screen_title`, `lng_group_call_video_paused` and
  `lng_group_call_mac_settings`. Quill keys are used for the two privacy
  lines (tdesktop's say "Telegram Desktop") and for "Window {index}".
- **Demo** `ready-call-screen-share`: a voice chat where your shared window
  is hidden (paused tile) and the chooser is open over four made-up
  sources. `QUILL_DEMO_SCREEN_DENIED=1` shows the privacy box instead.
- Unit tests no longer open the real microphone. `CallCapture::enabled()`
  and `CallScreen::enabled()` turn on in `cfg(test)` only with an injected
  capture. Before this, driver tests without one opened the default input
  through cpal and could hang in CoreAudio for minutes.

## Deviations

- The brief asked for tabs, double-click to share, live 1 s thumbnails and
  Escape in the chooser. tdesktop has none of them, so none were built.
  The chooser has one grid, click selects, one picture per source, and
  Cancel closes it.
- The brief asked for a permission empty state inside the chooser. tdesktop
  shows a separate box over the call window instead, so that is what Quill
  does.
- The privacy box text says Quill (the app-name rule). Its buttons are the
  kit's.
- Wide mode: tdesktop shows "Video is paused" only when the voice chat
  window is wide (600 px and more). Quill's window has no wide layout, so
  the text shows on the full-screen stage and the strip tiles show the
  icon.
- tdesktop asks "{user} is sharing their screen…" before you share over
  someone else's screen (`lng_group_call_sure_screencast`). That is not in
  this slice.
- The chooser window's picture size comes from the backend's thumbnail
  (twice the 221 × 125 picture area). tdesktop asks tgcalls for 235 × 165
  at the device pixel ratio.

## Cost

Measured in the debug test build (`cargo test`, unoptimised) on this Mac,
for a 1920 × 1080 frame:

| Step | Per frame | At its rate |
|---|---|---|
| NV12 → I420 | 5.2 ms | 15 fps: about 78 ms of one core per second |
| I420 → 480 px RGBA preview | 7.5 ms | 4 per second: about 30 ms per second |

- Memory: one 1080p I420 frame is 3.1 MB. The sink holds at most 4 (12.4
  MB at worst) and the preview slot one 480 × 270 RGBA picture (0.5 MB).
  Each chooser thumbnail is at most 442 × 250 RGBA (0.44 MB), taken once.
  The paused tile keeps one 90 × 90 picture per paused stream.
- With no share running, nothing captures. The pause poll runs only while a
  window is shared (one `CGWindowListCopyWindowInfo` call every 500 ms).
- No release build was measured (rules: no release builds unless asked).
  An optimised build should be several times faster.

## Verified

- Unit tests:
  - labels, fit sizes, NV12 split and its rejects, preview colours and
    throttling;
  - the external or desktop screen description, and the request JSON;
  - the paused picture fitting 90 px and keeping its shape;
  - the chooser's first selection (the first screen, or the shared source,
    or nothing when that source is gone) and the Stop Sharing rule;
  - the privacy text's bold run.
- Driver tests (mock engine, fake capture):
  - the chosen window feeds the group presentation;
  - a hidden window pauses once per edge, both ways, with the requests;
  - a closed window ends the share;
  - a refused capture ends it with its error;
  - starting needs a pick and stopping does not;
  - your shared screen shows in your tile;
  - a 1:1 call shares without pause requests.
- GPUI tests:
  - Share Screen opens the chooser with the four labels, every picture
    loaded and the first screen selected, and shares the pick and closes;
  - the chooser of a running share selects it, Cancel changes nothing and
    Stop Sharing stops;
  - without access the box shows only over the voice chat window and
    Escape closes it;
  - without a backend the share stays direct.
- Demo captures (in-process, scratch only): the chooser window, the paused
  tile, and the privacy box.

## Not verified

- ScreenCaptureKit on a real screen: this session has no screen recording
  permission and must not start a real call. Listing, thumbnails, the
  stream, the window poll and the remote side seeing the frames need the
  owner's manual check on macOS 14+.
- A real voice chat receiving `toggleGroupCallScreenSharingIsPaused` and
  other members seeing the paused tile.

# Slice 3: Windows screen capture and system audio

Slice 3 of 4. Brings the chooser, pause and Quill's own screen capture
(slice 2) to Windows, and adds tdesktop's "Share System Audio". Linux keeps
ntgcalls' desktop capture until slice 4.

**Only compile-checked.** No Windows machine was available. All Windows
code (`screen_win.rs`, `system_audio_cpal.rs` and the `cfg(windows)`
paths) was type-checked and linted with `cargo check` and `cargo clippy
--target x86_64-pc-windows-msvc --features ui` from macOS, with stub C
tools standing in for the MSVC ones. It has never run. The BGRA
conversion, the frame pacer, the system audio queue and its mixing, the
driver and the chooser are platform independent and were tested on
macOS.

## What tdesktop does

- Windows lists screens and windows through tgcalls' WebRTC capturer:
  visible, unowned, titled top-level windows that are not tool windows
  and not cloaked (other virtual desktops, suspended Store apps).
- **System audio** (`calls_group_call.cpp`, `calls_call.cpp`):
  - The chooser shows "Share System Audio"
    (`lng_group_call_screen_share_audio`) only when
    `Webrtc::LoopbackAudioCaptureSupported()`, which is Windows. It is in
    both the voice chat and the 1:1 chooser.
  - The box starts as the running share has it
    (`chooseSourceActiveWithAudio`). Stop Sharing shows only when the
    selection is the shared source *and* the box matches
    (`updateButtonsVisibility`); otherwise Share Screen restarts the share
    with the new choice.
  - Group calls send it on the screen instance through
    `LoopbackAudioDeviceModuleCreator` (`calls_group_call.cpp:3174`),
    muted unless the share has audio (`:938`). 1:1 calls capture it with
    `Webrtc::CreateSystemAudioCapture` and hand the samples to
    `addExternalAudioSamples` (`calls_call.cpp:1522`), which mixes them
    into the call's own audio.

## What changed

- **`src/calls/capture/screen_win.rs`** (new, Windows + `ui`) uses
  Windows.Graphics.Capture:
  - It needs Windows 10 1903 (UniversalApiContract 8) and
    `GraphicsCaptureSession::IsSupported()`. Older systems get
    `CaptureError::Unsupported` with "Screen sharing needs Windows 10
    version 1903 or later." (`qll_ops_screen_unsupported`), which the
    chooser shows as the status note.
  - Screens come from `EnumDisplayMonitors`, the primary first. Windows
    come from `EnumWindows` with tdesktop's filters, plus: not minimised,
    at least 100 × 100 (as on macOS), and not Quill's own.
  - `IGraphicsCaptureItemInterop` makes the item for a monitor or window.
    A free-threaded `Direct3D11CaptureFramePool` (BGRA8, one buffer)
    delivers frames on a system thread. Each due frame (the pacer keeps
    15 fps) is copied to a staging texture, mapped and converted to I420
    at the call's size. A resized window remakes the pool. The cursor is
    captured. The yellow border is turned off where Windows allows it
    (errors ignored).
  - The item's `Closed` event ends the share. A shared window is polled
    every 500 ms: minimised pauses, restored resumes, gone ends.
  - Thumbnails come from a one-frame session per source, with a 2 s
    timeout.
  - Access is always granted; Windows has no screen recording permission.
- **`src/calls/capture/screen_bgra.rs`** (new, every platform): BGRA →
  I420 (nearest scaling, BT.601 video range, libyuv's integer form, 2 × 2
  chroma average), BGRA → RGBA thumbnails, and `FramePacer`.
- **System audio:**
  - `system_audio.rs` (new) holds `SystemAudioMix`, a 200 ms queue of 48
    kHz mono samples.
  - `system_audio_cpal.rs` (new, Windows) records it with WASAPI
    loopback: cpal 0.17 opens an input stream on the default output
    device. Samples are down-mixed and resampled to 48 kHz.
  - The microphone path (`denoise.rs`) adds it to each frame after noise
    suppression and after the level window. A muted microphone drops it,
    so unmuting does not replay old sound.
  - The driver (`call_screen.rs`) starts the loopback while a share
    picked with the box runs. It hands the queue to the call microphone
    (`CallCapture::set_system_audio`, kept for microphones opened later)
    and stops both when the share stops or the box is cleared. If the
    loopback fails, the screen keeps going and the error is kept.
- **Chooser:** a "Share System Audio" box at the bottom left where loopback
  exists, starting as the running share has it. Stop Sharing follows
  tdesktop's rule above. The demo shows the box with
  `QUILL_DEMO_SCREEN_AUDIO=1`.
- **Turned on for Windows:** `native_backend()`,
  `external_screen_capture_enabled()` and the 1:1 call's share button.
- **`windows` crate features** added: `Foundation_Metadata`, `Graphics`,
  `Graphics_Capture`, `Graphics_DirectX`, `Graphics_DirectX_Direct3D11`,
  `Win32_Graphics_Direct3D`, `Win32_Graphics_Direct3D11`,
  `Win32_Graphics_Dxgi`, `Win32_Graphics_Dxgi_Common`, `Win32_Graphics_Gdi`,
  `Win32_Graphics_Dwm`, `Win32_System_WinRT`,
  `Win32_System_WinRT_Direct3D11` and
  `Win32_System_WinRT_Graphics_Capture`. No new crates.
- **Windows build fix:** the dock menu's Quit item (`os_menus`) used a
  language key without its imports and broke the Windows build since
  a8e52046. That fix is its own commit.

## Deviations

- **System audio joins after noise suppression, not before** (the design
  said before). RNNoise treats music as noise and would remove most of
  it. tgcalls' `addExternalAudioSamples` also mixes after its processing.
- **Group calls carry system audio in your microphone audio, not on the
  presentation.** ntgcalls routes the microphone device only to the main
  connection, so there is no audio channel on the presentation to use.
  Members hear it as part of your voice (as in tdesktop 1:1 calls), it
  is silent while you are muted, and it does not light your speaking
  indicator.
- System audio needs Quill's own microphone capture. Under
  `QUILL_CALL_CAPTURE=native` neither the chooser nor the box is used.
- Thumbnails are one frame per source, as in slice 2 and tdesktop. The
  design said one per second.

## Cost

BGRA → I420 for a 1920 × 1080 frame, measured on this Mac (the Windows
build has the same Rust loop):

| Build | First version | Final | Notes |
|---|---|---|---|
| Release (`cargo test --release`) | 8.3 ms | 3.3 ms | 15 fps: about 50 ms of one core per second (5 %) |
| Debug (`cargo test`) | 377 ms | 152 ms | debug builds cannot keep 15 fps |
| Release, 2560 × 1440 → 1080p | 8.4 ms | 3.2 ms | scaling costs nothing extra |
| Release, 442 × 250 thumbnail | 0.23 ms | 0.23 ms | once per source |

The final version reads each 2 × 2 block once for both luma and chroma,
and works out the source column offsets once per frame.

- Memory: the staging texture is one BGRA frame of the source size (8.3
  MB at 1080p), in addition to slice 2's figures. The system audio queue
  holds at most 9,600 floats (38 KB).
- The GPU copy and map were not measured; they need Windows.
- With no share running nothing captures. The pause poll runs only while
  a window is shared.

## Verified

- Windows: `cargo check` and `cargo clippy` for
  `x86_64-pc-windows-msvc --features ui`, and `cargo check --lib --tests`,
  with no new warnings. This is a compile check only.
- Unit tests (macOS): BGRA colours (white, black, red, blue after
  scaling), refusals, thumbnails, the pacer turning 60 Hz into 15 fps; the
  mix queue (order, limit, clean samples); system audio added after
  denoise, not counted in the level and dropped while muted.
- Driver tests (fake loopback and microphone): the audio reaches the
  running microphone and stops with the share or the box; a microphone
  opened later still gets it; a failed loopback keeps the screen and the
  error; without loopback the box is ignored.
- GPUI test: no box without loopback; with it, the box starts as the
  share has it, and clearing it on the shared screen shares again instead
  of stopping.
- Demo capture (scratch only): the chooser with the box ticked and Stop
  Sharing.

## Not verified

- Everything that needs Windows: listing, thumbnails, capture, resizing,
  the pause poll, the `Closed` event, the 1903 check, the border switch
  and WASAPI loopback. The owner should check these on Windows 10 1903+
  and Windows 11 before the slice is trusted, and a Windows 10 1809
  machine for the Unsupported note.
- A real call hearing the system audio.

# Slice 4: Linux screen capture

Slice 4 of 4. Brings Quill's own screen capture, the chooser and pause to
Linux, the way tdesktop's tgcalls capturers do it.

**Only compile-checked.** No Linux desktop was available. All Linux code
(`screen_linux.rs`, `pipewire_dl.rs`, `screen_x11.rs`) was type-checked
and linted with `cargo check` and `cargo clippy --all-targets --features
demo-capture --target x86_64-unknown-linux-gnu` from macOS, with stub C
tools and pkg-config. It has never run. The SPA POD writer and parser,
the PipeWire buffer conversion and the YUV conversions are platform
independent and unit-tested on macOS.

## What tdesktop does

- On Wayland no app may read the screen, so tgcalls opens the
  `org.freedesktop.portal.ScreenCast` portal and the desktop shows its own
  picker. tdesktop then skips its chooser (`uniqueDesktopCaptureSource`):
  the share button starts sharing, or stops a running share.
- On X11 tgcalls lists screens and windows itself and tdesktop shows its
  chooser as on the other systems.
- System audio sharing is Windows only.

## What changed

- **`src/calls/capture/screen_linux.rs`**: `LinuxScreen` picks the portal
  when `WAYLAND_DISPLAY` is set (as GPUI does) and X11 otherwise.
  - Portal: `ashpd` creates a session, asks for one monitor or window
    (the types the portal offers), with the cursor embedded where
    supported, starts it and opens the PipeWire remote. The share ends
    when the portal session closes (the desktop's own "Stop sharing", the
    source going away) or the picker is cancelled. A missing portal or
    PipeWire gives "Screen sharing needs xdg-desktop-portal and PipeWire."
    (`qll_ops_screen_portal_missing`).
  - PipeWire pausing the stream after frames have flowed pauses the share
    (`toggleGroupCallScreenSharingIsPaused`); streaming again resumes it.
- **`pipewire_dl.rs`**: libpipewire-0.3 is loaded at run time with
  `libloading`, so building needs no PipeWire headers and Quill still
  starts where PipeWire is missing. One stream per share on its own
  `pw_thread_loop`, shared-memory buffers mapped by PipeWire. Callbacks
  never unwind into C (`catch_unwind`), and teardown runs in reverse
  order under the loop lock.
- **`spa_pod.rs`**: writes the format offer (raw video, I420 preferred,
  NV12 and BGRx/RGBx accepted, any size, up to 15 fps) and parses the
  negotiated format. No DMA-BUF modifiers are offered.
- **`pipewire_frames.rs`, `screen_yuv.rs`**: the negotiated buffer to the
  call's I420 at the fitted size.
- **`screen_x11.rs`** (best effort, as the design says): RandR monitors
  (primary first) and `_NET_CLIENT_LIST` windows without docks, desktops,
  hidden windows, Quill's own and anything under 100 × 100. Pictures come
  from `GetImage` on the root area or the window's Composite pixmap. A
  shared window is checked every 500 ms: unmapped or
  `_NET_WM_STATE_HIDDEN` pauses, back resumes, gone ends. Only 24/32-bit
  little-endian TrueColor displays are read; others get Unsupported.
- The chooser skips itself where the OS has a picker
  (`ScreenBackend::system_picker`). `external_screen_capture_enabled()`
  and `can_share_screen` include Linux.
- Dependencies: `ashpd` 0.13 (screencast, async-io; MIT), and the
  `composite` and `randr` features of the existing `x11rb`.

## Verified

- `cargo check` and `cargo clippy -D warnings` for
  `x86_64-unknown-linux-gnu` with `demo-capture` and all targets: clean.
- macOS gate (core and UI tests).
- Unit tests: the POD offer and the parse of a negotiated format, I420,
  NV12 and BGRx/RGBx buffers to I420 (strides, scaling, RGB order), the
  X11 source id round trip.
- GPUI test: with a system-picker backend the share button shares at
  once without a chooser and stops a running share.

## Not verified

- Everything that needs a Linux desktop: the portal on GNOME and KDE,
  the PipeWire stream and its formats, pause from the stream state, the
  session-closed end, and X11 listing, thumbnails, capture and the window
  poll. The owner should check these on a Wayland and an X11 session.
