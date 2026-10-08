Quill for Windows (x86_64)

Unzip this folder anywhere and run quill.exe. Keep the DLLs next to it:

  tdjson.dll                         Telegram Database Library (TDLib)
  libssl-3-x64.dll, libcrypto-3-x64.dll, z.dll   OpenSSL / zlib used by TDLib
  ntgcalls.dll                       call media engine (LGPLv3 sidecar, loaded at runtime)
  rlottie.dll                        animated stickers
  quillvideo.dll, av*.dll, sw*.dll   video playback (FFmpeg, LGPL-2.1+; licenses\ffmpeg)
  vcruntime140*.dll, msvcp140*.dll   Microsoft Visual C++ runtime (app-local copy)

Needs Windows 10 or newer. Nothing is installed or written outside your user
profile. Account data lives in the per-user application data directory,
never next to the executable.

Licenses: Quill is MIT (LICENSE). THIRD_PARTY.md lists every bundled
component and its license. The license texts are in licenses\, and the Rust
crate licenses are in THIRD_PARTY_LICENSES.md. The Visual C++ runtime DLLs are
Microsoft files redistributed under Microsoft's terms (licenses\msvc-runtime.txt).
Source: https://github.com/shinycake/quill

Quill is an independent, unofficial Telegram client. It is not affiliated with,
endorsed by, or sponsored by Telegram. It is early, experimental software and
comes with no warranty (see LICENSE).
