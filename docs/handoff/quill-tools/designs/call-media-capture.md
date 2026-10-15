# Call media capture: noise suppression, screen source chooser, screen pause

Parity items `calls-noise-suppression`, `calls-screen-source-chooser`,
`calls-screen-share-pause`. Build-ready design, Fable architect pass
2026-10-15. Supersedes the 2026-10-15 triage note. Format follows
`designs/video-player.md`.

## Summary of the approach

1. **Quill owns capture; ntgcalls only encodes.** ntgcalls' media
   description accepts `NTG_MEDIA_SOURCE_EXTERNAL` for any device, and
   `ntg_send_external_frame(handle, chat_id, device, data, len,
   frame_data)` (`vendor/ntgcalls/include/ntgcalls.h:515`) pushes frames
   into that stream. Verified from ntgcalls source (master,
   `stream_manager.cpp:250, 495-504`): an external reader is registered
   per device when the description's source is External, and
   `send_external_frame` throws `InvalidParams` if the stream was not set
   up that way, so the description must be issued before the first frame.
   Formats: audio frames are PCM s16le at the description's
   `sample_rate`/`channel_count`, one frame = 10 ms
   (`audio_streamer.cpp:22`: `frame_size / (2 * channel_count)`
   samples); video frames are **I420** at `frame_data.width × height`
   (`video_streamer.cpp` → `I420ImageData(width, height, sample, size)`),
   falling back to the description's width/height when `frame_data`
   carries 0.
2. **Three capture backends behind two traits** in a new
   `src/calls/capture/` module: `MicCapture` (cpal, already a dependency
   and already used by `src/calls/level_tap.rs` / `src/voice_input.rs`)
   and `ScreenCapture` (ScreenCaptureKit on macOS, Windows.Graphics.Capture
   on Windows, xdg-desktop-portal ScreenCast + PipeWire on Linux). Each
   produces frames on its own thread and reports pause/resume and fatal
   errors through a small event channel.
3. **Denoise in the microphone path**: `nnnoiseless` 0.5.2 (BSD-3-Clause,
   allowed by `deny.toml`, recorded in
   `docs/decisions/codex-call-audio-levels.md`) runs on the capture
   thread, 48 kHz, 480-sample frames = exactly one 10 ms external frame.
   One switch "Enable Noise Suppression" (tdesktop
   `lng_group_call_noise_suppression`) in the call settings, default on,
   persisted, applied live. The level tap reads from the same stream, so
   the second device open from the audio-levels decision goes away.
4. **Screen chooser** is a kit dialog over the backend's source list with
   live thumbnails refreshed once a second, tdesktop's copy
   (`lng_group_call_screen_share_start` "Share Screen",
   `lng_group_call_screen_share_stop` "Stop Sharing",
   `lng_group_call_screen_title` "Screen {index}",
   `lng_group_call_screen_share_audio` "Share System Audio"). On Linux
   Wayland the portal shows its own picker; the dialog is skipped there.
5. **Pause** follows tdesktop (`calls_group_call.cpp:725-734`,
   `setOnPause`): when the shared window is minimised or hidden the
   backend reports paused, Quill stops pushing frames and calls
   `toggleGroupCallScreenSharingIsPaused(is_paused = true)`
   (`schema/td_api.tl:14700`); on resume, frames and `false`. Other
   members already render `groupCallParticipantVideoInfo.is_paused`
   (`:7498`) as the paused tile ("Video is paused",
   `lng_group_call_video_paused`); Quill's tiles must honour it too.

## What exists today (Quill)

- Engine (`src/calls/engine/`): `CallEngine` trait (`api.rs:110-180`):
  `set_group_camera`, `start_screen_share` (`ntg_init_presentation`),
  `connect_screen_share` (`ntg_connect(…, is_presentation = true)` then
  `issue_presentation_sources`), `stop_screen_share`,
  `presentation_active`, `select_devices`, `media_devices`.
  `ntgcalls/mod.rs:236` `issue_group_sources` (mic =
  `NTG_MEDIA_SOURCE_DEVICE` default device, camera 640×480@30);
  `:317` `screen_video_description` = `NTG_MEDIA_SOURCE_DESKTOP`
  1920×1080@15 with the opaque desktop input; `:370-420` the 1:1 path
  builds the same descriptions (screen replaces camera; ntgcalls rejects
  both in capture mode).
- Driver (`src/connect/group_calls/controls.rs:14`
  `toggle_group_call_screen_share`; `mod.rs:190-230` pending answer →
  `connect_screen_share`; `join.rs:352-365` cleanup on leave).
- UI: `src/ui/group_calls.rs:288` `toggle_group_call_screen_share`,
  device menus in `src/ui/call_panel_devices.rs`, Settings > Calls in
  `src/ui/call_audio_ui.rs` and `call_device_prefs_ui.rs` (microphone
  test meter from the level tap).
- Level tap: `src/calls/level_tap.rs` opens the default input through
  cpal mono 48 kHz on its own thread and publishes a peak level atomic;
  `src/connect/call_audio.rs::pump_call_audio` opens it while joined and
  unmuted. `voice_input::open_default` is the cpal pattern (f32 mono
  conversion, permission error wording).
- Push-to-talk already has per-platform global hooks
  (`src/calls/ptt_global_{mac,win,x11}.rs`), the pattern for per-OS
  backends with a shared trait.

## What tdesktop does (reference)

- `calls_group_settings.cpp:380-391`: the switch, stored in
  `Core::Settings::groupCallNoiseSuppression` (default false in
  `core_settings.h:1118`, UI default shown on), applied with
  `GroupCall::setNoiseSuppression` → tgcalls
  `setIsNoiseSuppressionEnabled` (`calls_group_call.cpp:2283`). 1:1 calls
  keep WebRTC's processing on.
- Chooser: `Webrtc::DesktopCaptureChooseSource` (lib_webrtc, not in the
  reference checkout): a box titled "Share Screen"/"Share Window" with a
  grid of thumbnails, selected source highlighted, "Share System Audio"
  checkbox on Windows, "Share Screen" primary button. `Panel::
  chooseShareScreenSource` (`calls_group_panel.cpp:1589`) opens it, and
  `screenSharingPrivacyRequest` (`:1583`) shows the macOS privacy hint
  when screen recording permission is missing.
- Pause: `_screenCapture->setOnPause` → `_screenState = Paused`; the
  server flag is sent with the participant's video params; receivers
  show the paused tile with the last frame blurred and
  `lng_group_call_video_paused`.

## What the user sees

**Noise suppression.** Settings > Calls gains a toggle row "Enable Noise
Suppression" under the microphone test (kit `Switch`, default on). The
group call window's settings sheet shows the same row. Flipping it
takes effect immediately in a running call. The microphone test meter
shows the denoised level when the switch is on, so the user can hear the
difference by watching the meter in a noisy room.

**Share screen.** The share button in the call window opens the chooser:
a 560×480 kit dialog, title "Share Screen", two tabs "Screens" and
"Windows" (tabs hidden when only one kind exists), a grid of 3 per row,
each card 160×100 thumbnail with `radius_md`, 1 px `border` and a
`primary` 2 px ring when selected, caption `text_sm` ("Screen 1",
window title with the app name muted). Thumbnails refresh once a
second. Footer: on Windows a "Share System Audio" checkbox (left),
"Cancel" and primary "Share Screen" (right, disabled until a source is
selected). Double-click shares at once. Esc cancels. Missing screen
recording permission (macOS) shows the chooser empty with the privacy
hint and an "Open Settings" button (same wording path as
`voice_input::friendly`). Linux Wayland: the share button triggers the
portal picker directly; X11 gets the Quill dialog.

**Paused share.** Minimising or hiding the shared window switches your
own tile to the paused state (last frame blurred, "Video is paused") and
the share button keeps its "stop" state; restoring the window resumes
within a second. Other members see the same paused tile.

## Capture module (`src/calls/capture/`, new)

```
pub trait MicCapture: Send {
    fn start(&mut self, device: Option<&str>, sink: FrameSink<AudioFrame>) -> Result<(), CaptureError>;
    fn stop(&mut self);
}
pub trait ScreenCapture: Send {
    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError>;      // list + fresh thumbnails
    fn start(&mut self, source: &SourceId, opts: ScreenOptions, sink: FrameSink<VideoFrame>) -> Result<(), CaptureError>;
    fn stop(&mut self);
}
pub struct ScreenSource { id: SourceId, kind: Screen | Window, title: String, app: Option<String>, thumbnail: Option<Arc<RgbaImage>>, size: (u32, u32) }
pub struct ScreenOptions { max_width: u32 /* 1920 */, fps: u8 /* 15 */, system_audio: bool }
pub enum CaptureEvent { Paused, Resumed, Failed(CaptureError), Ended /* source closed */ }
pub struct AudioFrame { pcm: Vec<i16> /* 480 samples mono */, captured_at: Instant }
pub struct VideoFrame { i420: Vec<u8>, width: u16, height: u16, captured_at: Instant }
```

`FrameSink` is a bounded channel (4 frames for video, 8 for audio) plus
the event channel; a full channel drops the oldest frame (never blocks
a capture callback). A `CaptureFeeder` thread per joined call drains the
sinks and calls `CallEngine::send_external_frame(chat_id, device, &bytes,
frame_data)` with `absolute_capture_timestamp_ms` from `captured_at`
mapped to Unix ms once at start (monotonic offset), `rotation = 0`, and
the frame's width/height.

### Microphone (`mic_cpal.rs`)

cpal input stream on the chosen device (by name, matching the entry from
`media_devices`; `None` = default), requested config 48 kHz mono i16,
converting from whatever the device gives (existing `voice_input`
conversion; resample with a linear resampler only when the device cannot
do 48 kHz, as ntgcalls' own device module does). Chunks are accumulated
into exactly 480-sample frames. When noise suppression is on, each frame
goes through `nnnoiseless::DenoiseState::process_frame` (f32 in, f32
out, scaled back to i16). The frame is then handed to the sink and its
peak goes to the level tap's atomic, so `LevelTap` becomes a consumer of
this stream rather than a second device open (keep the standalone tap
for the Settings meter when no call is running). Muting keeps the
stream open and sends silence frames (ntgcalls `ntg_mute` still applies;
silence avoids the "External source not initialized" throw when
unmuting).

Engine change: `issue_group_sources` and the 1:1 `issue_sources` build
the microphone description with `media_source =
NTG_MEDIA_SOURCE_EXTERNAL, sample_rate = 48000, channel_count = 1`
when the capture module is active (always, once this ships; the Device
source stays as a fallback behind `QUILL_CALL_CAPTURE=native` for
debugging). A new `CallEngine::send_external_frame` wraps the FFI call;
results other than `NTG_OK` are logged at most once per second.

### Screen (`screen_mac.rs`, `screen_win.rs`, `screen_linux.rs`)

- **macOS**: `objc2-screen-capture-kit` 0.3.2 (same objc2 generation as
  the vendored AVFoundation bindings). `SCShareableContent
  getShareableContentWithCompletionHandler` lists displays and on-screen
  windows (skip windows under 100×100 and Quill's own call window);
  thumbnails via `SCScreenshotManager` (macOS 14+) or a one-frame
  `SCStream` (13). `SCStream` with `SCStreamConfiguration` pixel format
  `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange` (NV12) at the
  configured size and `minimumFrameInterval = 1/15`; the output callback
  converts NV12 → I420 (split the interleaved UV plane; a 20-line loop,
  SIMD not needed at 1080p15) into the sink. Pause: a 500 ms timer on the
  capture thread checks the window's `onScreen` / minimised state through
  `SCShareableContent` for window sources (display sources never pause);
  `SCStreamDelegate didStopWithError` → `Ended`. Permission: a failed
  `getShareableContent` with the TCC error → `CaptureError::Permission`
  with the "System Settings > Privacy & Security > Screen Recording"
  hint. `CGPreflightScreenCaptureAccess` for the pre-check.
- **Windows**: `windows` crate features `Graphics_Capture`,
  `Graphics_DirectX_Direct3D11`, `Win32_Graphics_Direct3D11`,
  `Win32_System_WinRT_Graphics_Capture` (add to the existing `windows =
  "0.62"` line). Sources: `EnumDisplayMonitors` + `EnumWindows` (visible,
  non-tool, titled, not cloaked via `DwmGetWindowAttribute DWMWA_CLOAKED`);
  `GraphicsCaptureItem` from `IGraphicsCaptureItemInterop`
  `CreateForMonitor` / `CreateForWindow`; `Direct3D11CaptureFramePool`
  free-threaded, BGRA8, 1 buffer; frames are copied to a staging texture,
  mapped, converted BGRA → I420 on the capture thread (plain Rust loop;
  1080p15 ≈ 3 % of a core). `IsIconic(hwnd)` polled at 500 ms → Pause;
  `Closed` event → Ended. Thumbnails: one captured frame per source every
  second while the chooser is open, downscaled. "Share System Audio":
  WASAPI loopback on the default render device through cpal's
  `loopback` host feature when available, mixed into the microphone frame
  before denoise; off by default. Windows 10 1903+ required; older →
  `CaptureError::Unsupported`.
- **Linux**: `ashpd` 0.13 `ScreenCast` portal (`create_session`,
  `select_sources` with `SourceType::MONITOR | WINDOW`, `start`) → PipeWire
  node id → `pipewire` 0.10 stream in `SPA_VIDEO_FORMAT_I420` preferred,
  `NV12`/`BGRx` accepted and converted. The portal shows the system
  picker, so the Quill chooser is not shown on Wayland; on X11 without a
  portal, `xcb` listing of top-level windows (`_NET_CLIENT_LIST`) and
  `XGetImage` thumbnails, capture through the existing XComposite path or
  the portal if present. Pause: PipeWire stream `paused` state /
  `_NET_WM_STATE_HIDDEN` poll. The cross-platform rule applies: Wayland
  through the portal must work, X11 is best effort.

### Pause plumbing

`CaptureEvent::Paused`/`Resumed` → driver `screen_share_paused: bool` on
the tracked call → `toggleGroupCallScreenSharingIsPaused` (request
builder + purpose) and the feeder stops/starts forwarding frames. The
engine keeps the presentation transport up during a pause. Receiving
side: `groupCallParticipantVideoInfo.is_paused` is parsed into the
participant's screen/camera state (check `src/state/call_types.rs`; add
if missing) and `group_call_tiles.rs` paints the paused tile: last frame
blurred (`blur` once, cached), centred `text_sm` "Video is paused".

### Never-crash

Capture callbacks run on OS threads: they only push into bounded
channels and never touch GPUI. All FFI/COM/ObjC results are checked; a
failure stops the backend and emits `Failed`. Frame sizes are validated
(`len == w*h*3/2`) before the FFI call. `nnnoiseless` processes a fixed
480-sample slice, never an index into a shorter buffer. Device
disconnect mid-call → `Ended` → the driver stops sharing (or re-opens the
default microphone after 2 s for audio) and shows the existing error
toast; nothing panics.

## Settings and strings

`settings.call_noise_suppression: bool` (default true) beside the
existing call device prefs. Keys: `Q_CALLS_NOISE_SUPPRESSION` "Enable
Noise Suppression", `Q_CALLS_SHARE_SCREEN` "Share Screen",
`Q_CALLS_STOP_SHARING` "Stop Sharing", `Q_CALLS_SCREEN_INDEX` "Screen
{index}", `Q_CALLS_SHARE_SYSTEM_AUDIO` "Share System Audio",
`Q_CALLS_SCREENS` "Screens", `Q_CALLS_WINDOWS` "Windows",
`Q_CALLS_VIDEO_PAUSED` "Video is paused",
`Q_CALLS_SCREEN_PERMISSION` "Quill needs screen recording permission.
Allow it in System Settings > Privacy & Security > Screen Recording.",
`Q_CALLS_SCREEN_UNSUPPORTED` "Screen sharing needs Windows 10 version
1903 or later."

## Memory and performance

- Audio: 480 × 2 bytes per 10 ms; RNNoise costs ≈ 1 % of a core.
- Video: one I420 frame 1080p = 3.1 MB, at most 4 queued = 12.4 MB;
  conversion on the capture thread, nothing on the UI thread.
- Thumbnails: ≤ 24 sources × 160×100 RGBA = 1.5 MB, dropped when the
  chooser closes.
- No frame clock involvement; the chooser repaints on its 1 s timer only
  while open.

## Tests

- `nnnoiseless` wrapper: 480-sample framing from arbitrary chunk sizes,
  pass-through when off, silence stays silence, i16 round trip.
- Feeder: drop-oldest on a full channel, timestamp mapping monotonic,
  frame size validation rejects bad lengths, pause stops forwarding and
  sends exactly one `toggleGroupCallScreenSharingIsPaused` per edge.
- Engine: descriptions use External + 48000/1 when capture is active;
  the Device fallback under `QUILL_CALL_CAPTURE=native`.
- Reducer: `is_paused` on participant video info → tile state.
- Chooser: selection, double-click share, disabled button, tab hiding;
  demo fixture with four fake sources and thumbnails for screenshots;
  paused tile fixture.
- Loopback harness (dev-only test, `--ignored`): create a local ntgcalls
  instance, issue an External microphone, push 100 frames, assert no
  error from `ntg_send_external_frame`; same for a 320×240 I420 screen.
- Manual (owner): macOS first. Share a window, minimise it, confirm the
  paused tile on a second device; toggle noise suppression in a noisy
  room and watch the meter. Windows and Linux on the owner's machines
  when available; the decision doc records what was and was not
  verified.

## Build plan (Opus 5.5, medium; one PR at a time, in order)

1. **Capture module + external microphone + noise suppression**:
   `src/calls/capture/{mod,feeder,mic_cpal,denoise}.rs`,
   `CallEngine::send_external_frame`, External mic descriptions, the
   setting + both switches, level tap fed from the stream, tests, the
   loopback harness. Decision doc `docs/decisions/call-capture.md`.
   Closes `calls-noise-suppression`.
2. **Screen capture macOS + chooser + pause**: `screen_mac.rs`, the
   chooser dialog, External screen description in
   `issue_presentation_sources` and the 1:1 path, pause plumbing both
   ways, paused tile, fixtures, screenshots. Closes
   `calls-screen-source-chooser` and `calls-screen-share-pause` on macOS.
3. **Windows backend** (`screen_win.rs`, system audio), Windows CI build.
4. **Linux backend** (`screen_linux.rs`, portal + PipeWire, X11
   listing), Linux build with the new deps gated behind the `calls`
   feature.

Each PR: worktree on the SSD target, rules.txt, gate.sh, independent
reviewer, queue.sh, cleanup, THIRD_PARTY_LICENSES.md regenerated for the
new crates. Real calls are never started on the owner's account by the
builder; the loopback harness and fixtures are the floor.

## Deviations and open points

- WebRTC's AEC/AGC stay off (they are off in the prebuilt ntgcalls
  already); RNNoise is the only processing. If echo turns out to matter
  on speakers, a later slice can add `webrtc-audio-processing` behind
  the same trait.
- The microphone chosen in the call window now drives the capture
  (fixes the "default device only" note in the audio-levels decision).
- 1:1 calls get noise suppression from the same switch (tdesktop
  hard-codes it on there; one switch is simpler).
- System audio sharing only on Windows (as tdesktop); macOS would need
  SCStream audio + a mixer, deferred.
- If `objc2-screen-capture-kit` lacks a needed binding, use the
  `objc2` `extern_class!`/`msg_send!` escape hatch in-tree rather than
  vendoring; note it in the decision doc.
