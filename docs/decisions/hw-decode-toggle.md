# Hardware decoding toggle (`parity:data-hw-decode`)

tdesktop has "Hardware accelerated video decoding" in Settings > Advanced >
Performance (`settings_advanced.cpp` `BuildPerformanceSection`,
`Core::Settings::hardwareAcceleratedVideo`, on by default on macOS and off
elsewhere, `core_settings.h`). The media viewer
and its picture in picture pass it as `hwAllowed` when a clip opens
(`media_view_overlay_widget.cpp`, `media_view_pip.cpp`); inline loops,
stickers and stories always decode in software. `MakeCodecPointer` then sets
`get_format = GetHwFormat` (`ffmpeg_utility.cpp`), which tries a platform
list (Windows D3D11, DXVA2, CUDA; macOS VideoToolbox; Linux VAAPI, VDPAU,
CUDA), creates the device, and otherwise returns the last software format.
Hardware pictures are copied back with `av_hwframe_transfer_data`
(`media_streaming_utility.cpp` `TransferFrame`). Players already open keep
their decoder.

## What Quill does

- `scripts/build-ffmpeg.sh` builds VideoToolbox on macOS and D3D11VA plus
  DXVA2 on Windows. The Windows hwaccels are the `*_d3d11va2` ones: they
  take a device context (`AV_PIX_FMT_D3D11`, as tdesktop uses), while the
  plain `*_d3d11va` ones need an app-supplied `hwaccel_context`.
- `quillvideo.c` gets `QvOptions.hw` and `QvInfo.hw` (ABI 3). With hardware
  allowed, the video decoder gets the first device in tdesktop's order that
  the codec supports, and a `get_format` that picks it when offered and the
  last software format otherwise. Hardware pictures are copied back (NV12
  for 8-bit), so the rest of the pipeline is unchanged.
- tdesktop's `GetHwFormat` also tries CUDA on Windows and Linux. Quill
  leaves it out because the FFmpeg build is LGPL with `--disable-autodetect`
  and has no CUDA SDK to build against.
- Failure handling: if a device can't be created or the codec has no
  hardware path, the decoder is software from the start. If a hardware
  decoder later fails (receive error, a refused packet, a failed transfer),
  the shim reopens the decoder in software once, seeks back, and drops what
  it already returned, so no picture is lost or repeated. `QvInfo.hw` drops
  to 0 then, and also when the decoder hands out software pictures on its
  own. If the software decoder won't open, the hardware one stays installed
  with `QvInfo.hw` still 1 and the error ends the clip. `QUILL_TRACE_VIDEO=1`
  prints `VD,opened,<codec>,hardware|software` and `VD,fell back,...`.
- The one-time software fallback goes beyond tdesktop, which fails the video
  when a hardware picture can't be copied back
  (`media_streaming_video_track.cpp:512-523`, `fail(Error::InvalidData)`).
- `quill::advanced::hardware_video` stores the choice in
  `hardware_video.json` under the app root, like `update_channel`. It is on
  by default on macOS, off elsewhere, like tdesktop; Windows D3D11VA/DXVA2 is
  unverified on a real GPU, so it stays opt-in until it is.
  `backend_ffmpeg.rs` reads it for `Purpose::Viewer` and
  `ViewerLoop` only (tdesktop's scope), when a clip opens.
- The Advanced page shows a Performance heading with the switch, above the
  Experimental settings entry (tdesktop order). The strings are tdesktop's
  `lng_settings_performance` and `lng_settings_enable_hwaccel`. Quill has no
  settings search, so tdesktop's keywords have no place to go.
- `quill --video-probe` decodes with hardware allowed and prints
  `hardware=true|false`.

## Platform scope

- macOS: the switch affects the FFmpeg path only (`QUILL_VIDEO_BACKEND=ffmpeg`).
  The default AVPlayer path always decodes in hardware.
- Windows: D3D11VA, then DXVA2. FFmpeg loads `d3d11.dll`, `dxgi.dll`,
  `d3d9.dll` and `dxva2.dll` at run time; DXVA2 also imports `ole32` and
  `user32`, which are system DLLs (`check-bundle-pe.ps1` allows them).
- Linux: software only for now. FFmpeg links libva and libvdpau as `NEEDED`
  entries of libavutil/libavcodec, so the package would fail to load on a
  system without them. `check-bundle-elf.sh` would have to allow them as
  system libraries, and they are not on every install. tdesktop avoids this
  by dlopening them through generated trampolines
  (`_libva_so_tramp_resolve_all`). A Linux follow-up needs the same: either
  trampolines or a libva-free FFmpeg configure with a dlopen shim. The switch
  is shown on Linux like tdesktop shows it; there it changes nothing yet.

## Verification

On this Mac, against `scripts/build-ffmpeg.sh` output with VideoToolbox:

- `video_decode::tests::hardware`: with hardware allowed the H.264 fixture
  decodes on VideoToolbox (`QvInfo.hw` stays 1 to the end) and returns the
  same 10 pictures and sound as software. The BGRA pictures are
  byte-identical (H.264 decoding is bit-exact), and with NV12 asked every
  picture comes out as NV12. A stand-in hardware decoder whose third
  picture fails (`QV_HW_FAIL_THIRD_PICTURE`) hands over to software with the
  same pictures; a mutant that resumes from the start fails that test with
  repeated pictures. A second stand-in whose software reopen also fails
  (`QV_HW_FAIL_THIRD_PICTURE_AND_REOPEN`) keeps the hardware decoder and its
  flag, and a seek then plays the whole clip on it. Both stand-ins ship in
  the shim, but the app can't reach them, since outside tests the Rust side
  only sends `QV_HW_OFF` or `QV_HW_ALLOWED`.
- The app on the `ready-video-controls` demo with
  `QUILL_DEMO_REAL_PLAYER=1 QUILL_VIDEO_BACKEND=ffmpeg QUILL_TRACE_VIDEO=1`
  plays the 12 s demo clip and logs `VD,opened,h264,hardware`; with
  `hardware_video.json` set to off it logs `VD,opened,h264,software` and
  plays the same.

Only a GPU machine can check: D3D11VA and DXVA2 on Windows (device creation,
transfer, and the CI runner's behavior without a GPU, where the fallback is
expected to run), HEVC and VP9 through VideoToolbox (the fixtures are H.264
and a tiny VP9 that VideoToolbox may decline), and 10-bit clips (P010
transfer, then a BGRA conversion).
