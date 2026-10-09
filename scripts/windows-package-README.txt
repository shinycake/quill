Quill for Windows (x86_64)

Unzip this folder anywhere and run quill.exe. Keep the DLLs next to it:

  tdjson.dll                         Telegram Database Library (TDLib; OpenSSL and zlib built in)
  ntgcalls.dll                       call media engine (LGPLv3 sidecar, loaded at runtime)
  rlottie.dll                        animated stickers
  quillvideo.dll, av*.dll, sw*.dll   video playback (FFmpeg, LGPL-2.1+; licenses\ffmpeg)

Needs Windows 10 or newer. Nothing is installed or written outside your user
profile. Account data lives in the per-user application data directory,
never next to the executable.

Licenses: Quill is MIT (LICENSE). THIRD_PARTY.md lists every bundled
component and its license. The license texts are in licenses\, and the Rust
crate licenses are in THIRD_PARTY_LICENSES.md. No Visual C++ Redistributable
is needed: the C runtime is linked into quill.exe and the DLLs.
Source: https://github.com/shinycake/quill

Quill is an independent, unofficial Telegram client. It is not affiliated with,
endorsed by, or sponsored by Telegram. It is early, experimental software and
comes with no warranty (see LICENSE).
