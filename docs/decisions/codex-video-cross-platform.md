# In-process video on Linux and Windows (bundled FFmpeg)

On macOS, inline autoplay (muted GIF/video loops, round video messages) and
the media viewer play through AVPlayer and draw its `CVPixelBuffer`s with
GPUI's `surface`. On Linux and Windows the inline tiles were stills and the
viewer shelled out to an `ffmpeg` binary on `PATH` to extract PNG frames
(slow, low frame rate, missing on most Windows machines). This slice gives
Linux and Windows the same players, in process, through a bundled FFmpeg —
the decoder Telegram Desktop uses on every platform
(`media/streaming/media_streaming_*.cpp`, `ffmpeg/ffmpeg_utility.cpp`).
macOS keeps AVPlayer (hardware decode, already verified).

## Options

| | (a) bundled FFmpeg (chosen) | (b) pure-Rust / small C decoders | (c) GStreamer (Linux) + Media Foundation (Windows) |
| --- | --- | --- | --- |
| Codecs | H.264 (all profiles, B-frames), HEVC, VP8, VP9, MPEG-4; AAC, MP3, Opus, Vorbis, FLAC, PCM; MP4/MOV, Matroska/WebM, GIF, Ogg | `openh264` decodes Constrained Baseline well, Main/High support is partial; no pure-Rust HEVC or VP9; AV1 via dav1d/rav1d; demux via `mp4`/`mp4parse`, no WebM video demuxer of note | whatever plugins/extensions the user has: H.264 on Linux needs `gst-libav` or `openh264` (often missing on Fedora/minimal installs); HEVC on Windows is a paid Store extension |
| Robustness on Telegram files | what tdesktop ships; handles edit lists, rotation, odd timestamps | patchwork of crates, each with its own edge cases | depends on distro/plugin versions; two backends to keep consistent |
| Licensing | LGPL-2.1+, dynamic linking, no GPL parts (see below) | BSD/MIT; OpenH264 built from source is not covered by Cisco's patent licence (only their prebuilt binary is) | system components, nothing shipped |
| Package size | +~8 MB uncompressed (decoders/demuxers only) | small | none |
| Build/CI | one script on all three hosts, ~1 min on macOS arm64, cached by script hash | cargo only, but several C/Rust decoders | two native APIs (GObject, COM) to bind and test |
| Pixel path | swscale → BGRA in the decoder thread | per-crate YUV → RGB conversion to write | GStreamer `videoconvert` / MF video processor |

(a) is the only option that plays every Telegram clip on both platforms
with one code path and no user-installed components, which is why tdesktop
made the same choice. The cost is a native build in CI (cached) and a few MB.

### How it is linked: a runtime-loaded shim

Quill does not bind FFmpeg's structs from Rust. `native/quillvideo/` is a
~500-line C shim (MIT) compiled against the exact FFmpeg headers it ships
with, exporting six functions over plain structs (`quillvideo.h`,
`QV_ABI_VERSION` checked at load): open a local file, info, next
picture-or-sound, seek, close. Quill loads it with `libloading` exactly like
`librlottie`/`libtdjson`: next to the binary, in `lib/`, from
`QUILL_VIDEO_LIB`, or from a dev build under `vendor/ffmpeg/prefix`. So:

- the Rust build needs no FFmpeg headers, bindgen or pkg-config — the
  `windows-build` job and every developer machine compile unchanged;
- FFmpeg ABI changes can't silently corrupt memory (the shim is rebuilt
  with the libraries it ships with);
- without the shim (a plain `cargo run` on Linux) the app works as before:
  stills inline and the ffmpeg-binary viewer path (`native_video::supported()`).

The shim also restricts input to the `file` protocol, decodes only the best
video and audio streams (others `AVDISCARD_ALL`), and is built with
`CONFIG_SAFE_BITSTREAM_READER`.

## FFmpeg build (`scripts/build-ffmpeg.sh`)

- FFmpeg **n8.1.3** (commit pinned and verified after clone) — the release
  tdesktop pins in `Telegram/build/prepare/prepare.py`.
- `--enable-shared --disable-static --disable-programs --disable-network
  --disable-autodetect --disable-avdevice --disable-avfilter
  --disable-everything` plus decoders `h264 hevc vp8 vp9 mpeg4 gif aac
  aac_latm mp3 mp3float opus vorbis flac alac pcm_*`, demuxers `mov
  matroska gif ogg mp3 aac flac wav`, their parsers, `file` protocol,
  swscale, swresample. `configure` reports **"License: LGPL version 2.1 or
  later"** and no external libraries.
- One script for Linux (gcc, nasm), Windows (MSYS2 UCRT64 MinGW-w64 gcc,
  nasm; `--enable-w32threads --disable-pthreads -static-libgcc` so the DLLs
  import only Windows system DLLs — `check-bundle-pe.ps1` enforces it) and
  macOS (development only).
- Output `vendor/ffmpeg/prefix` (git-ignored, like rlottie), including
  `share/quillvideo/` with `COPYING.LGPLv2.1`, FFmpeg's `LICENSE.md` and
  `FFMPEG-SOURCE.txt` (tag, commit, source URLs, the exact configure line).

### LGPL compliance

No `--enable-gpl`/`--enable-nonfree`, no external codecs; FFmpeg stays in its
own shared libraries (`libavcodec.so.62` …, `avcodec-62.dll` …) that the user
can replace; the packages carry the licence text, source location and build
configuration in `licenses/ffmpeg/`; `THIRD_PARTY.md` has the entry. The
shim is MIT and Quill source. Patents: H.264/HEVC decoding may be
patent-encumbered in some jurisdictions — the same posture as every
FFmpeg-based player including Telegram Desktop; noted in `THIRD_PARTY.md`.

## Packaging and CI

- `scripts/linux-package.sh`: `lib/libquillvideo.so` + the five FFmpeg
  libraries under their sonames (RUNPATH `$ORIGIN`, stripped),
  `licenses/ffmpeg/`. `check-bundle-elf.sh` validates them unchanged;
  `check-bundle-dlopen.py` also loads `libquillvideo.so` and asserts all five
  FFmpeg libraries map from the package.
- `scripts/windows-package.ps1`: `quillvideo.dll` + five FFmpeg DLLs +
  `licenses\ffmpeg`. `check-bundle-pe.ps1` requires `quillvideo.dll`;
  `check-bundle-load.ps1` loads it, checks its exports and that the FFmpeg
  DLLs map from the package.
- `ci.yml`: `linux-package` installs `nasm`, restores/builds/saves
  `vendor/ffmpeg/prefix` (key: hash of the build script and shim), runs the
  decoder tests against it (`QUILL_VIDEO_LIB=… cargo test --lib
  video_decode`), and the package smoke runs `quill --video-probe` on the
  H.264+AAC and VP9 fixtures from the packaged binary. New `windows-ffmpeg`
  job (MSYS2 UCRT64, cached the same way) uploads the DLLs; `windows-package`
  needs it and also runs `--video-probe` from the extracted zip. No new
  required checks.
- `quill --video-probe <file>` decodes a whole file with the installed
  decoder and prints codec, size, picture and sample counts.

## Playback pipeline (`src/video_decode/`, core crate)

- One `Player` per clip, one `quill-video` thread per player. The file is
  opened on that thread, so opening a GIF tile never blocks a frame.
- Bounded queues: the thread decodes while the picture queue
  (`OpenOptions::video_frames`: 3 inline, 4 in the viewer) has room or the
  sound buffer is below 0.5 s, and never past 3 s of sound
  (`wants_more`); otherwise it sleeps. A paused clip costs nothing. Inline
  clips are scaled in the decoder to fit 720 px and decoded with one thread
  and no sound; the viewer caps pictures at 1920 px.
- Pictures come out as upright, tightly packed BGRA (swscale with the
  stream's BT.601/709/2020 matrix and range; display-matrix rotation applied
  in Rust, `rotate_bgra`).
- Clock: with sound the audio output is the master — the `AudioTap` (pulled
  by the UI's rodio source) records the presentation time of the next sample
  it plays; what is heard is that minus `AUDIO_OUTPUT_LATENCY` (70 ms:
  stretcher block and read-ahead plus device period). Pictures pace on a
  smooth wall clock (`Clock`) that is pulled to the sound whenever they
  drift more than `SYNC_TOLERANCE` (40 ms, inside the ±45 ms lip-sync
  threshold), because the sound is pulled in ~20 ms bursts. Without sound
  (muted inline loops, no output device) the wall clock alone; it holds
  after a seek/open until the first picture is shown.
- `take_due` shows the newest due picture and drops late ones.
- Seek: the decoder seeks to the keyframe before the target and drops
  pictures and samples before it (in the shim, before conversion), stale
  queued items are discarded by epoch, and the sound restarts in place.

## UI (`src/ui/`)

- `native_video.rs`: `NativeVideo` keeps its API with two backends,
  AVPlayer (macOS) and `ffmpeg_video::FfmpegVideo` (Linux, Windows; macOS dev
  with `QUILL_VIDEO_BACKEND=ffmpeg`). `frame()` returns a `VideoPicture`:
  `Surface(CVPixelBuffer)` or `Image(Arc<RenderImage>)`. GPUI only has the
  `surface` primitive (YUV upload) on macOS — `SurfaceSource` has no other
  variant and the wgpu/DirectX renderers draw `PaintSurface` nowhere — so
  Linux/Windows upload BGRA through the sprite atlas.
- `ffmpeg_video.rs`: each new picture becomes a `RenderImage` (the decoded
  buffer moves in, no copy); the replaced one goes to
  `image_budget::retire_all`, which drops it from the atlas once no cached
  slice can replay it, so a playing clip holds a picture or two on the GPU.
  The soundtrack plays through `audio::StreamSound` (the app's rodio output
  shared via `share_output_with_video`, wrapped in the pitch-preserving
  `Tempo` stretcher so 0.5–2× works). A muted inline loop opened without
  sound reopens with it on the first `set_volume(>0)` (round video click).
- Images clip themselves, so inline tiles round their corners and round
  video messages become circles without the backdrop-coloured masks the
  macOS surface needs (`VideoPicture::clips`).
- `inline_video.rs` lost its `cfg(target_os = "macos")` stubs: muted loops,
  the round video sound toggle, the seek ring and the inactive-window gate
  (`set_window_active` pauses muted loops) run on every platform. The
  animation layer (`anim_layer::tile`) draws them unchanged.
- The media viewer's native path (seek, J/L, speed, volume/mute, loop, end
  handling) now runs on Linux/Windows when the shim loads; the old
  ffmpeg-binary frame path remains the fallback when it doesn't.

## Verified

| What | macOS (this Mac) | Linux | Windows |
| --- | --- | --- | --- |
| Shim + pinned FFmpeg 8.1.3 build script ("License: LGPL version 2.1 or later") | built, 58 s (arm64) | CI `linux-package`, ~1 m 45 s cold | CI `windows-ffmpeg`, ~7 min cold (MSYS2 UCRT64) |
| Decoder tests on fixtures (H.264 High + AAC, VP9 WebM, rotation, seek, scaling, end-to-end player) | 23/23 against 8.1.3 and Homebrew 9.0.2 | 23/23 in CI against the built libraries | — (probe below) |
| Pipeline tests (queue bounds, seek, sound clock, latency/sync math, end, detach) | pass | pass (required `linux-fmt-clippy-test`) | pass (`windows-build` core tests) |
| UI build with the FFmpeg backend | `cargo build --features ui`, macOS UI build workflow | `linux-package` release build | `windows-build` release build |
| Package checks (deps, loader, licences) | n/a | `check-bundle-elf.sh` (12 ELF files), `check-bundle-dlopen.py` (FFmpeg maps from the package) | `check-bundle-pe.ps1` (16 PE files, system imports only), `check-bundle-load.ps1` |
| `quill --video-probe` from the package | built binary | `video=h264 96x64 frames=10 audio=aac samples=48128`, `video=vp9 64x48 frames=10` | same output from the extracted zip |
| UI: inline GIF loop, viewer playback with transport, round video loop and click-to-sound (seek ring, countdown) | demo captures with `QUILL_VIDEO_BACKEND=ffmpeg`; colours within ±3/255 of AVPlayer | — | — |
| AVPlayer path (default on macOS) still plays | round video demo capture; code moved verbatim into `native_video::avplayer` | — | — |

Package size: Linux tarball artifact 80.2 → 83.1 MB (+2.9 MB compressed),
Windows zip artifact 43.2 → 47.3 MB (+4.2 MB). The FFmpeg libraries are
~7 MB uncompressed per platform.

The first Windows run caught `avutil-60.dll` importing
`libwinpthread-1.dll` (MinGW backs `clock_gettime`/`nanosleep` with it);
`-static` in FFmpeg's link flags fixed it, and the PE checker now proves
the DLLs import only UCRT/KERNEL32/bcrypt.

Not verified: real-time behaviour on actual Linux/Windows desktops (frame
pacing under the wgpu/DirectX atlas upload, WASAPI/ALSA latency against the
70 ms constant) — CI runners have no display or audio device.

## Not in this slice

- WebM video stickers (VP9 with alpha) still go through the `ffmpeg` binary
  with `libvpx-vp9` (`sticker_playback::decode_webm_sized`). FFmpeg's native
  VP9 decoder drops the alpha plane; tdesktop links libvpx for it. The shim
  already prefers `libvpx-vp9` for `alpha_mode` streams, so adding libvpx
  (BSD) to the build and a sticker frame API is the follow-up.
- AV1: FFmpeg's native AV1 decoder needs a hardware accelerator; software
  AV1 needs libdav1d (BSD). Telegram's main `video` file is H.264 (AV1/HEVC
  arrive as `alternative_videos`, which Quill doesn't pick), so it was left
  out; adding dav1d to `build-ffmpeg.sh` is the follow-up.
- Hardware decoding (VA-API, D3D11VA) as tdesktop does; software decoding
  is fine at Telegram's resolutions but costs CPU on 1080p+.
- GPUI has no YUV texture path off macOS, so pictures are converted to BGRA
  on the decode thread and re-uploaded per frame.
- Recording, thumbnails and probing (`src/video.rs`) still use the ffmpeg
  binary.
