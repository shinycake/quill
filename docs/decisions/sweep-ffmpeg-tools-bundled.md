# Bundled ffmpeg and ffprobe, and capture devices that follow the call settings

Audit finding P4 (platform sweep).

## The problem

Quill runs the `ffmpeg` and `ffprobe` command-line tools in 17 places: probing and thumbnailing
videos before they are sent, the video editor's cut/turn/crop/scale encode, photo downscale,
round-video recording, the profile-photo camera preview, video stickers and video avatars. The
Windows zip and the Linux tarball shipped only FFmpeg's libraries (behind `quillvideo`, for
playback). Windows users almost never have `ffmpeg.exe` on `PATH`, so on a stock install all of
that fell back to a message telling them to `winget install ffmpeg`. Telegram Desktop links FFmpeg
and has no such prerequisite.

The capture path was also fragile. On Windows it passed `video=<first camera>:audio=<first mic>`
as one DirectShow input, so a camera without a microphone, or a virtual camera listed first,
failed the whole recording. On Linux the microphone was always `-i default`.

## What tdesktop does

It links FFmpeg (`lib_ffmpeg`, the same release Quill pins, n9) into the app and encodes with the
libraries directly. Its camera and microphone come from the device ids in the settings
(`Core::Settings::cameraDeviceId` and the call input device), which is what Quill's
`capture_choice` mirrors.
Quill keeps ffmpeg as a separate process (recording and encoding are cancellable, and a crash in
a codec cannot take the window with it), so the tools have to ship.

## What changed

### Packaging

- `scripts/build-ffmpeg.sh` now also builds `ffmpeg` and `ffprobe` (programs are on, `ffplay`
  off) in the same configure as the shared libraries, so there is one build, one cache entry and
  the programs share the DLLs/`.so` files. LGPL configuration unchanged: no `--enable-gpl`,
  `--enable-nonfree` or `--enable-version3`, `--disable-autodetect`, a fixed component list.
  Added components: still-image decoders (JPEG, PNG, WebP, BMP, TIFF, APNG), older phone and
  camera video formats (MPEG-1/2, H.263, WMV, VC-1, ProRes), JPEG/PNG/GIF/MPEG-4/AAC/PCM
  encoders, MP4/MOV/image/raw muxers, the filters Quill's command lines use, the `pipe`
  protocol, and the `lavfi` input (test sources). Device inputs per platform: DirectShow
  (Windows), V4L2 + ALSA + PulseAudio (Linux). zlib is enabled for PNG.
- There is no H.264 encoder in an LGPL build (libx264 is GPL). Windows gets Media Foundation's
  `h264_mf` (the operating system's, `--enable-mediafoundation`, which FFmpeg 9 builds only with `--enable-d3d11va`; `d3d11.dll` and `dxgi.dll` are system DLLs). Linux has none and no VP9
  encoder either; see "Encoders" below.
- `scripts/windows-package.ps1` puts `ffmpeg.exe`, `ffprobe.exe` and the two new DLLs
  (`avfilter-N`, `avdevice-N`) next to `quill.exe`; `check-bundle-pe.ps1` requires them and the
  existing import check covers them. `scripts/linux-package.sh` installs `ffmpeg` and `ffprobe`
  in the package root (RUNPATH `$ORIGIN/lib`) plus `libavfilter` and `libavdevice`;
  `check-bundle-elf.sh` allows `libpulse.so.0` (the one new system library, used to open
  microphones). The macOS bundle is unchanged: the script is only run on macOS for development,
  `Quill.app` plays through AVFoundation and gets ffmpeg from Homebrew as before. Bundling there
  would need nested-binary signing and notarisation, which this change does not take on.
- Cache keys already hash `scripts/build-ffmpeg.sh` and `native/quillvideo/*`, so editing the
  script rebuilds once per OS and nothing else changes.
- Size: see the PR description for measured deltas.

### Resolution (`src/media_tools.rs`)

`media_tools::command("ffmpeg")` already looked next to the executable first. Now:

- A bundled copy only counts if `<tool> -version` exits 0 within five seconds. A Linux machine
  without `libpulse.so.0` or a Windows N edition without `mfplat.dll` then falls through to
  `PATH` and the package-manager folders instead of failing every call.
- `candidates(name)` lists every working copy in order (bundled, `PATH`, Homebrew/MacPorts);
  `command`/`is_installed` still use the first. Missing tool: the bare name, and the message
  from `ffmpeg_missing_message` (now says the tool ships in Quill's folder).

### Encoders (`src/video_encoder.rs`)

The command lines name `libx264` (editor, round video, video avatar) and `libvpx-vp9` (video
stickers). Because the shipped LGPL ffmpeg has neither, the encode picks the first working ffmpeg
that has an encoder (`libx264`, then VideoToolbox, then Media Foundation) and rewrites
`-c:v libx264 [-preset ..] [-crf ..]` for it (a bitrate of about 3 bits per pixel per second
where the encoder has no constant-quality mode; a command line's own `-b:v` stays). So:

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| photo downscale, probes, thumbnails, camera preview, frame extraction | bundled | bundled | Homebrew |
| editor / round video / video avatar (H.264) | bundled `h264_mf` | system ffmpeg with libx264 | Homebrew |
| video stickers (VP9) | system ffmpeg with libvpx | system ffmpeg with libvpx | Homebrew |

On Linux sending a video unedited, photo resizing and the camera preview work from the tarball
alone; editing, round videos and video stickers still need the distribution's ffmpeg, and say so.
Shipping a software H.264 encoder there needs OpenH264 built from source (BSD, no patent grant
from Cisco's binaries) or a hardware path; that is a licensing decision for the owner, not made
here.

### Capture devices (`src/media_capture.rs`, split out of `media_tools.rs`)

Pure functions build the input arguments per platform (`avfoundation_input`, `v4l2_input`,
`dshow_input`, `choose_dshow`), so they are unit tested without hardware.

- Windows: camera and microphone are two `-f dshow` inputs (`video=NAME`, `audio=NAME`), each the
  call-settings pick or the first device. A camera without a microphone opens on its own.
- Linux: the camera is the picked `/dev/video*` node (matched by path, file name or the V4L2 card
  name the call engine lists, as before). The microphone is the PulseAudio source (also served by
  PipeWire) whose description or name matches the picked microphone, found with
  `ffmpeg -sources pulse`; otherwise `default`. ALSA stays on `default`.
- `camera_input` builds the camera alone for the profile-photo preview, replacing the
  string-munging `video_only_input`; a machine without a microphone can now use that preview.
- Defaults are unchanged when nothing is picked or the pick is unplugged.

## How it was verified

- Unit tests: resolution order (bundled before `PATH`, a bundled copy that cannot start is
  skipped, repeats collapse, missing tool resolves to its bare name and is not installed), the
  per-OS missing-tool message, `-version` self-test (exit code, absent file), argument
  construction for every capture backend including the Windows separate inputs, device choice
  with a virtual camera first and with no microphone, the PulseAudio listing parser and pick,
  the encoder picker and the `libx264` rewrite.
- `scripts/check-ffmpeg-tools.sh` runs the shipped tools the way Quill does (downscale JPEG and
  PNG, probe, MPEG-4/AAC clip, the camera pipe, cut/turn/scale) and fails on GPL/nonfree
  configuration, `libx264`, or a missing expected encoder. It ran locally against the macOS
  development build and runs in CI from the extracted Windows zip and Linux tarball, where a
  missing binary or DLL fails the job.
- Not verifiable here: a real camera and microphone on Windows or Linux, `h264_mf` encoding on a
  real machine (CI runners have no Media Foundation H.264 encoder to exercise), and the Windows
  N edition behaviour.
