# cpal 0.18.2 and rodio 0.23.0

Refs #552 (the `crates` deps-watch issue lists both as newer majors). They move
together: rodio 0.23.0 is a minimal release whose only change is `cpal = "0.18.2"`.

Branch `cpal-0.18-rodio-0.23`. Sources read: cpal 0.18.2 `CHANGELOG.md` (0.18.0 to
0.18.2 in full), `UPGRADING.md`, `src/error.rs` and the ALSA, CoreAudio (macOS) and
WASAPI backends; cpal 0.17.3's ALSA and WASAPI backends for comparison; rodio 0.23.0
`CHANGELOG.md` and `src/stream.rs`; ntgcalls v3.0.3
`ntgcalls/src/media/devices/win_core_device_module.cpp`.

## What changed in Quill

| File | Change |
| --- | --- |
| `Cargo.toml`, `Cargo.lock` | `cpal = "0.18"`, `rodio = "0.23"`, both still `default-features = false`. New lock entries (`jni` 0.22, `jni-macros`, `simd_cesu8`, `mach2` 0.6) are Android or Apple-only dependencies of cpal. |
| `src/cpal_stream.rs` (new) | Error-callback policy (`StreamErrors`), `downmix_into` for every sample type, ntgcalls device-name matching. Tests in `src/ui/audio/cpal_stream_tests.rs` (app target, see Verification). |
| `src/calls/capture/mic_cpal.rs` | `cpal::Error`, `StreamConfig` by value, I24/F64, error filter, name matching, permission errors by kind. |
| `src/voice_input.rs` | Same for voice notes; `friendly_error` / `is_permission_error` use `ErrorKind::PermissionDenied` first, text second. |
| `src/calls/capture/system_audio_cpal.rs` | Same API changes, I24/F64; the ignored error callback now logs through the filter. Windows only. |
| `src/ui/audio/mod.rs` | `SharedOutput` opens the default device with a quiet error callback and reopens when its stream ended. |

## 1. Error callback: what is fatal

cpal 0.18 has one `Error { kind, message }`. Running streams now also report
events they survive through the error callback. Quill used to treat every callback
as fatal: the call microphone pushed `CaptureEvent::Failed` (the driver then stops
it and reopens after `REOPEN_AFTER`), and a voice note was aborted with
"Microphone stopped: …".

`cpal_stream::severity` decides per kind, from the `ErrorKind` docs and the
backend code:

| Kind | Severity | Why |
| --- | --- | --- |
| `Xrun` | Glitch: debug log, at most one line per 30 s with a count | "potential audio glitch"; ALSA prepares and restarts the PCM, WASAPI and CoreAudio only report it. |
| `DeviceChanged` | Notice: debug log | "The stream remains active and no rebuild is required." Only CoreAudio default output emits it now. |
| `RealtimeDenied` | Notice: debug log | "Audio will still play". Quill does not enable `realtime`, so only JACK/AAudio could send it. |
| `DeviceNotAvailable`, `StreamInvalidated`, `PermissionDenied`, `HostUnavailable` | Fatal | Device gone, rebuild required, or access revoked. |
| `DeviceBusy`, `ResourceExhausted`, `InvalidInput`, `UnsupportedConfig`, `UnsupportedOperation`, `BackendError`, `Other`, any future kind | Fatal | Nothing promises the stream still delivers audio. This keeps the pre-0.18 behavior (every `BackendSpecific` error was fatal), and the call microphone reopens on its own. `ErrorKind` is `#[non_exhaustive]`, so a new kind stays fatal until reviewed. |

Fatal lines are rate-limited too, because ALSA's worker keeps calling the
callback each period on a repeating backend error until the stream is dropped.
Logging goes to Quill's debug log (`advanced::debug_mode::debug_log`, only written
while debug mode is on). The filter lives in the callback closure, so it takes
no lock on the audio thread.

Applied to: the call microphone (a fatal `PermissionDenied` is now reported as
`CaptureError::Permission` with the Settings hint), the voice-note microphone,
WASAPI loopback (which ignored every error and still only logs) and the rodio
output (section 5).

**Was this a live bug on Linux with cpal 0.17?** Yes. cpal 0.17.3's ALSA
`input_stream_worker` calls `error_callback(StreamError::BufferUnderrun)` on
`PollDescriptorsFlow::XRun` and in `process_output` on `EPIPE`, then recovers
and keeps the stream running. One overrun under load therefore failed the call
microphone (stopped, error shown, reopened about a second later) or ended a voice
note. With 0.18 the same now also happens on macOS (CoreAudio overload listener),
Windows (capture discontinuity flag) and PipeWire, which is why the filter had to
ship with the bump.

## 2. I24 and F64 sample formats

`default_input_config` and `default_output_config` now rank formats
`F32 > F64 > integers by width`, and WASAPI reports 24-bit devices as `I24`
(0.17 called them `I32`). All three input sites now match `F32, F64, I16, U16,
I24, I32`. The data callback receives `cpal::I24` (cpal's WASAPI backend already
shifts the 24 bits down out of the 32-bit container), and
`f32: FromSample<I24>` / `FromSample<f64>` come from `dasp_sample`. The
three copies of the down-mix loop became `cpal_stream::downmix_into`, generic
over the sample type; tests cover I24 full scale, F64, and the older formats.
rodio 0.23's output already builds every format, so playback needed nothing.

## 3. Permission errors

`voice_input::friendly_error(&cpal::Error)` and `is_permission_error` check
`ErrorKind::PermissionDenied` first (ALSA maps `EPERM`/`EACCES` to it, CoreAudio
maps `AudioUnitError::Unauthorized`) and keep the text match as the fallback:
WASAPI maps `E_ACCESSDENIED` to `BackendError` with the text "Access is denied.
(0x80070005)", so on Windows the text is still what recognizes it.
`cpal::Error`'s `Display` prints only the message when
there is one, so the kind check also catches denials whose message never says
"permission". `mic_cpal::classify` uses the same check instead of matching the
output string.

## 4. Windows device names

cpal 0.17's WASAPI `description().name()` was `DeviceDesc` ("Microphone"), with
`FriendlyName` only in the extended lines; 0.18 prefers `FriendlyName`
("Microphone (Realtek(R) Audio)") and no longer lists the other.

ntgcalls v3.0.3 on Windows (`WinCoreDeviceModule::get_devices`) lists WebRTC's
`core_audio_utility::GetInputDeviceNames`: index 0 is the console default, index 1
the communications default, then every device. From WebRTC's
`core_audio_utility_win.cc` (fetched by ntgcalls at build time, not in its tree,
so not re-read here) the first two are named `Default - <FriendlyName>` and
`Communication - <FriendlyName>` and the rest by `PKEY_Device_FriendlyName`.

So with 0.17, choosing a specific microphone in a call could never match by name
and always fell back to the system default. With 0.18 the names are the same
property. `find_input` now goes through `cpal_stream::requested_input`: on
Windows `Default - …` means the system default and `Communication - X` is
matched as `X` (cpal has no communications role); the comparison trims and then
ignores case. On macOS and Linux names are taken as they are (ALSA's
`description()` is unchanged between 0.17 and 0.18).

## 5. Output follows the default device

`SharedOutput` caches one rodio sink for the app. What the backends do now:

- **macOS**: a stream built from `default_output_device()` uses the
  `DefaultOutput` AudioUnit, which reroutes on its own when the default changes
  and reports `DeviceChanged` (non-fatal). With no default left it pauses and
  reports `DeviceNotAvailable`.
- **Windows**: the 0.18.0 changelog says default streams reroute, but 0.18.2
  reverted the claim ("Default device changes no longer report `DeviceChanged`,
  which wrongly implied the stream had rerouted automatically"). WASAPI never
  rebinds the `IAudioClient`; it reports `StreamInvalidated` (or
  `DeviceNotAvailable` when no default is left), and a removed device then fails
  the render loop.
- **Linux (ALSA)**: the `default` PCM goes through the PulseAudio or PipeWire
  ALSA plugin on desktop systems, so the sound server moves it.

Quill already opened the *default* device (rodio's `open_default_sink` uses
`default_output_device()`), so macOS follows by itself. For the rest, the sink's
error callback sets a `stale` flag on a fatal kind, and the next `mixer()` call
(the next notification sound, call tone, voice note or video) opens the new
default. The replaced sink is kept until the following replacement, so a track
still playing on a device that is still there (Windows: default switched without
unplugging) is not cut off by an unrelated notification sound.

rodio's `open_default_sink` installs `default_error_callback`, which `eprintln!`s
every event, xruns included. `open_default_output` repeats what
`open_default_sink` does (default device, then any non-null device via
`open_sink_or_fallback`) with `with_error_callback`, and the callback logs
through `StreamErrors`.

## Other 0.18 changes checked

- **Streams start paused on every backend.** All three input sites already call
  `stream.play()`, and rodio's `MixerDeviceSink::open` does too.
- **Default config prefers 48 kHz.** Helps: the call microphone skips the
  resampler on more devices. Voice notes and loopback resample anyway.
- **`default-features = false` stays.** Linux remains ALSA-only. cpal 0.18's new
  `pipewire` and `pulseaudio` hosts would take priority if enabled; `pipewire`
  needs libpipewire headers and bindgen at build time, and either would change
  device names and routing on Linux, which belongs in its own PR. `realtime` /
  `realtime-dbus` stay off (D-Bus build dependency on Linux).
- `StreamInstant`, `DeviceId`, `device.name()`: Quill uses none of them.

## Verification

- Unit tests: `src/ui/audio/cpal_stream_tests.rs` (each `ErrorKind`'s
  severity, only fatal kinds come back from `check_with`, xrun and fatal log
  throttling, I24/F64/I16/U16 down-mix, Windows name rules) and `voice_input`
  (permission by kind and by text). `cpal_stream` needs the `ui` feature and CI
  tests the library without it, so its tests sit in the app target, which CI
  runs with `--features demo-capture --bin quill`. The `voice_input` tests are
  in the library like the ones already there, so CI compiles them (clippy
  `--all-targets`) but does not run them; they were run here with
  `cargo test --features demo-capture --lib voice_input` (3 passed). The old
  `permission_errors_get_the_settings_hint` expected "Microphone" in the Linux
  hint, which quotes the backend instead; it never ran in CI, and now checks
  the prefix every platform shares.
- Gate on Linux, after rebasing onto `main` at `72e5987e`: app name, file
  sizes, interface strings, hotspots, `cargo fmt --check`, both clippy runs
  (`-D warnings`), `cargo test --features demo-capture --bin quill` (799
  passed, 12 of them `cpal_stream_tests`), `test-frame-idle.sh`,
  `test-notification-extras.sh`, `cargo deny check licenses` and
  `third-party-licenses.sh --check` all pass. `cargo test --no-default-features`
  passes except `atomic_write_failure_keeps_old_file`, which fails only because
  the container runs as root (unrelated; passes on CI).
- `Cargo.lock` and `THIRD_PARTY_LICENSES.md` come from `cargo update -p
  cpal@0.17.3 -p rodio` and `scripts/third-party-licenses.sh` on top of `main`;
  nothing else moved.
- Windows and macOS: PR CI compiles neither. `cargo check --target
  x86_64-pc-windows-msvc` of the whole crate stops in `ring`'s build script (no
  MSVC toolchain here). Instead `system_audio_cpal.rs` (Windows only),
  `cpal_stream.rs` and `voice_input.rs` were copied unchanged into a scratch
  crate against cpal 0.18.2 with stubs for the rest of Quill, and `cargo clippy
  -- -D warnings` passes for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`.
  `mic_cpal.rs` and `ui/audio/mod.rs` have no platform `cfg` and are compiled
  by the Linux gate.
- No performance or memory measurement: the audio callbacks do the same work
  as before (the down-mix loop moved into a function; the error filter runs
  only when cpal reports an error).
- Not verified: no audio hardware in the container, so no stream was opened on
  any OS. The checklist below is for the owner.

## Manual test checklist

Turn on debug mode (`--debug`) so the `audio:` lines land in `logs/quill-debug.log`.
"Share System Audio" (cpal loopback) exists on Windows only, so it is listed
there alone.

macOS
- [ ] Call: talk for a minute while loading the machine (build, video); the
      other side keeps hearing you, no "Microphone stopped" in the call window.
      Xruns show as rate-limited `dropout (Xrun)` lines.
- [ ] Call with the default microphone, change the system input in Control
      Center: note what happens (input streams do not reroute on CoreAudio).
- [ ] Voice note: record 30 s; it is not aborted. Deny microphone access in
      System Settings and record: the Settings hint appears.
- [ ] Notification sound: play one through headphones, unplug them, trigger
      another: it plays from the speakers (log: `note (DeviceChanged)`).
- [ ] Voice note playing on headphones, unplug: playback continues on the
      speakers.

Windows
- [ ] Settings > Calls: choose a specific microphone (not "Default - …"), start
      a call: the chosen device is used (it was the system default before).
      Also try the "Communication - …" entry.
- [ ] A 24-bit USB microphone (shows as 24-bit in Sound settings): call and
      voice note both work (I24 path).
- [ ] Notification sound after unplugging headphones: the next sound plays on
      the new default (log: `stopped (StreamInvalidated)` then
      `reopening the default output device`).
- [ ] Switch the default output in the tray while a voice note plays: the voice
      note keeps playing on the old device, the next notification goes to the
      new one.
- [ ] Share screen with "Share System Audio": the remote side hears it, and
      unplugging the output device stops only the system audio.
- [ ] Deny microphone access in Settings > Privacy: the Settings hint appears.

Linux (ALSA, usually through PipeWire or PulseAudio)
- [ ] Call under load: no "Microphone stopped" (this was the 0.17 bug).
- [ ] Voice note: record 30 s while compiling something.
- [ ] Notification sound after unplugging headphones: plays on the speakers.
- [ ] Choose a specific microphone in a call: the call uses it.
