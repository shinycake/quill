Quill for Windows (x86_64)

Unzip this folder anywhere and run quill.exe. Keep the DLLs next to it:

  tdjson.dll                         Telegram Database Library (TDLib)
  libssl-3-x64.dll, libcrypto-3-x64.dll, zlib1.dll   OpenSSL / zlib used by TDLib
  ntgcalls.dll                       call media engine (LGPLv3 sidecar, loaded at runtime)
  rlottie.dll                        animated stickers
  vcruntime140*.dll, msvcp140*.dll   Microsoft Visual C++ runtime (app-local copy)

Needs Windows 10 or newer. Nothing is installed or written outside your user
profile. Account data lives in the per-user application data directory,
never next to the executable.

Licenses: see LICENSE and THIRD_PARTY.md. Source: https://github.com/shinycake/quill
