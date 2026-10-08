# Voice and round-video recording on every platform

Branch `codex/capture-cross-platform`.

## References (tdesktop, read-only)

- `media/audio/media_audio_capture.cpp`: 48 kHz mono capture, Opus at
  32 kbit/s, first 400 ms muted then 300 ms faded in
  (`kCaptureSkipDuration`, `kCaptureFadeInDuration`), tail faded out,
  one peak per 10 ms (`waveformEach`) squeezed into the 5-bit waveform.
  Quill already had `collect_waveform` / `encode_waveform_5bit`; the new
  encoder now matches the capture side too (skip, fades, 32 kbit/s).
- Round video: tdesktop captures the camera through its own webrtc
  stack. Encoding video in-process is out of scope here, so round videos
  stay on ffmpeg.

## Decision: voice notes are in-process, no external binary

| Piece | Choice | Why |
|---|---|---|
| Microphone | `cpal` 0.17 (CoreAudio / WASAPI / ALSA) | Already in the tree through `rodio` (call sounds, and #443's playback), so no new system library on any platform. |
| Opus encoder | `rusty-opus` 1.0 (pure Rust) | libopus would need a system package or vendored C build on Linux and Windows. `rusty-opus` is conformance-tested against libopus, has zero dependencies and no FFI. `opus-rs` was the alternative; `rusty-opus` documents bit-exact RFC 8251 vectors and libopus interop in both directions. |
| Ogg muxer | `ogg` 0.9 (pure Rust) | Same crate #443 adds for demuxing voice notes. |

Decoding stays with #443 (`opus-decoder` + `ogg`); the tests here decode
with `rusty-opus` only because #443 is not merged yet. Either decoder
reads the file (libopus through ffprobe does too, see below). The three
crates are optional and enabled by the `ui` feature. The headless core
(`--no-default-features`) has no capture and returns a plain error.

Nothing changed for packaging: no new shared library, so
`scripts/linux-package.sh`, `check-bundle-elf.sh`, the Windows package
script and `docs/build.md` need no changes (ALSA was already a build
dependency for `cpal`).

## Pipeline

`voice_input.rs`: the default input device's own format is down-mixed to
mono and resampled to 48 kHz (linear when upsampling, area average when
downsampling) inside the cpal callback, then sent over a channel.
`voice_opus.rs`: a worker thread applies the tdesktop skip/fade, measures
10 ms peaks (the same `peak / 256` levels `collect_waveform` expects),
encodes 20 ms Opus frames and writes OpusHead (pre-skip 312, the libopus
48 kHz lookahead) / OpusTags / audio pages, flushing a page every second.
The last packet carries `pre-skip + recorded samples` as its granule
position, so the duration is exact; the encoder delay is flushed with
trailing silent frames. `VoiceCapture` keeps its public API (start,
`sample_bar`, `failure`, `finish`, `discard`), so the hold-to-record, lock,
cancel and live waveform UI is untouched. `VoiceDraft.duration_secs` now
comes from the encoded sample count instead of the wall clock.

A permission refusal that delivers silence instead of an error ends the
recording after 3 s with a "nothing was recorded" message.

## Round video

Still ffmpeg, now with per-platform devices:

- **Linux:** `/dev/video*` nodes are probed with `VIDIOC_QUERYCAP`
  (`V4L2_CAP_VIDEO_CAPTURE`/`_MPLANE` in `device_caps`), so metadata and IR
  nodes are skipped and the first real capture node is used (no camera
  picker exists in Settings). Audio uses ffmpeg's `pulse` input (also
  served by PipeWire) when this ffmpeg has it, otherwise `alsa`.
- **Windows:** DirectShow. `ffmpeg -list_devices true -f dshow -i dummy`
  is parsed (new `"Name" (video)` and old section-header layouts, alternative
  names skipped) and the first camera and microphone become
  `-i video=…:audio=…`. The capture is spawned with a piped stdin and
  without `-nostdin`, so `stop_capture`'s `q` finalizes the MP4
  (`media_tools::spawn_capture`, also `CREATE_NO_WINDOW`).
  `stop_capture` now waits up to 8 s for a graceful exit, then kills.
- **macOS:** unchanged (AVFoundation defaults).
- **Missing ffmpeg:** a message that names the install command for the
  platform (`brew install ffmpeg`, `winget install ffmpeg` or ffmpeg.exe
  next to Quill.exe, the distro package). No package bundles ffmpeg.

## Permissions

| | Before | After |
|---|---|---|
| macOS | AVCaptureDevice check and prompt | unchanged |
| Windows | "not supported" | consent store (`HKCU\…\CapabilityAccessManager\ConsentStore\{microphone,webcam}` and `\NonPackaged`, read with `reg query`) is checked before recording; `Deny` shows "turn on Let desktop apps access your microphone in Settings › Privacy & security"; a DirectShow open failure is mapped to the same message when the store says denied |
| Linux | n/a | the cpal / ffmpeg error text is shown |

## Platform matrix

| | macOS | Linux | Windows |
|---|---|---|---|
| Voice, before | ffmpeg avfoundation | ffmpeg pulse | unsupported |
| Voice, after | in-process cpal + Opus | in-process cpal (ALSA, PulseAudio/PipeWire through its plugin) + Opus | in-process cpal (WASAPI) + Opus |
| Round video, before | ffmpeg avfoundation | ffmpeg v4l2 `/dev/video0` + pulse | unsupported |
| Round video, after | unchanged | first V4L2 capture node + pulse or alsa | ffmpeg dshow first camera + mic |

## Verified where

- macOS (this machine): unit tests encode a sine through the encoder and
  decode it back (duration to the sample, level within 1.5 dB, tone by
  zero crossings, muted lead-in, faded tail, odd chunk and frame sizes),
  `ffprobe` (libopus) reads the file as mono 48 kHz Opus with the right
  duration, resampler tests for 16/44.1/48/96 kHz, DirectShow list parsing,
  V4L2 capability logic, ALSA/Pulse choice, registry parsing. The gate
  passes. No real microphone or camera was opened.
- Windows: `cargo check --target x86_64-pc-windows-msvc --no-default-features`
  and the `windows-build` CI job (UI compile). Live microphone, camera,
  dshow and `q` stop are untested.
- Linux: `linux-fmt-clippy-test` / `linux-package` CI. Live capture
  untested (no devices in CI).

## Known limits

- The first 400 ms of every voice note are muted, as in tdesktop.
- DirectShow device choice is the first listed; there is no picker.
- cpal records in the device's default format; a device that only offers
  an unsupported sample format reports an error.
- The README line for `media-voice-record` still says "ffmpeg OGG
  capture"; README is generated, so the wording updates on the next
  reconcile.
