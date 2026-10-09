# Third-party notices

Quill is MIT-licensed (`LICENSE`). This file lists the third-party components
that Quill compiles in or ships next to its executable, with their licenses.
The license texts are in `licenses/` and, for Rust crates, in
`THIRD_PARTY_LICENSES.md`. Every package (macOS app, Linux tarball, Windows zip)
includes these files; in the app, Settings → Appearance → About Quill → Open-source licenses
opens this file. How each item was checked is recorded in
`docs/decisions/codex-legal-compliance.md`. This is a maintained inventory, not
legal advice.

## Rust crates (compiled into the `quill` binary)

`THIRD_PARTY_LICENSES.md` holds the license text of every crate in `Cargo.lock`
that is compiled into the shipped binary (default features; macOS arm64, Linux
x86_64/arm64, Windows x86_64). `scripts/third-party-licenses.sh` regenerates it
with cargo-about, and CI's `licenses` job fails when it is stale or when a crate
uses a license that `deny.toml` does not allow.

Notes on specific crates:

| Crate(s) | License | Notes |
|---|---|---|
| gpui-kit, gpui-component, gpui-pre (GPUI) and related crates | Apache-2.0 | crates.io 0.6.1 / gpui-pre 0.3.x |
| gpui-base 0.7.0 | Apache-2.0 | Patched copy in `third_party/gpui-base`; the changes are listed in `third_party/gpui-base/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre-macos 0.3.7 | Apache-2.0 | Patched copy in `third_party/gpui-pre-macos`; the changes are listed in `third_party/gpui-pre-macos/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-kit-assets 0.7.0 | Apache-2.0; bundled Lucide icons ISC, some derived from Feather (MIT) | The icon license is in `licenses/lucide-ISC.txt` (copied from the crate's `LICENSE-LUCIDE`). |
| spellbook, symphonia (via rodio), dwrote, option-ext | MPL-2.0 | Unmodified crates.io releases. MPL-2.0 is file-level copyleft; the source of these files is on crates.io and in each project's repository (links in `THIRD_PARTY_LICENSES.md`). |
| webpki-roots | CDLA-Permissive-2.0 | Mozilla CA certificate data. |
| ICU4X crates (icu_*, zerovec, yoke, ...) | Unicode-3.0 | |
| rusty-opus, ogg, cpal | BSD-3-Clause, BSD-3-Clause, Apache-2.0 | In-process voice-note recording. No libopus. |

No crate in the shipped graph is licensed only under GPL, AGPL or LGPL. Crates
that offer a GPL license only as one option (for example `self_cell`,
"Apache-2.0 OR GPL-2.0-only") are used under the permissive option.

## Native libraries shipped with the packages

| Component | License | Shipped in | Notes |
|---|---|---|---|
| TDLib 1.8.68 (`libtdjson`, `tdjson.dll`) | BSL-1.0 (`licenses/tdlib-BSL-1.0.txt`) | all | Pinned upstream commit with one local patch, `native/patches/tdlib-quill-takeout-contacts.patch` (scoped contacts and message-range takeout requests). Built by `scripts/build-tdlib.sh` / `scripts/build-tdlib-windows.ps1`. The schema is vendored in `schema/td_api.tl`. |
| OpenSSL 3 (`libssl`, `libcrypto`) | Apache-2.0 (`licenses/openssl-Apache-2.0.txt`) | all | Needed by TDLib. macOS: shared libraries copied from the build machine's Homebrew `openssl@3`; Linux: shared libraries from the build host's distribution packages; Windows: vcpkg `openssl:x64-windows-static`, linked statically into `tdjson.dll`. Unmodified. |
| zlib | Zlib (`licenses/zlib.txt`) | Windows | vcpkg `zlib:x64-windows-static`, linked statically into `tdjson.dll`. macOS and Linux use the system zlib. |
| FFmpeg n8.1.3 (libavcodec, libavformat, libavutil, libswscale, libswresample) | LGPL-2.1-or-later (`licenses/ffmpeg/`) | Linux, Windows | Unmodified, commit 1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7. Built by `scripts/build-ffmpeg.sh` as shared libraries with decoders and demuxers only: no `--enable-gpl`, no `--enable-nonfree`, no external libraries (`--disable-autodetect`). `licenses/ffmpeg/FFMPEG-SOURCE.txt` records the source and the exact configure line. Quill loads FFmpeg through its own MIT shim `quillvideo` (`native/quillvideo`), so the FFmpeg libraries can be replaced. On Windows the FFmpeg DLLs statically include the MinGW-w64 runtime: libgcc (GCC Runtime Library Exception) and winpthreads (MIT and BSD-3-Clause, `licenses/winpthreads.txt`). macOS uses AVFoundation and ships no FFmpeg. Patent note: H.264/HEVC decoding may be patent-encumbered in some countries, as in every FFmpeg-based player. |
| ntgcalls v3.0.0 (call media engine) | LGPL-3.0 (`licenses/ntgcalls-LGPL-3.0.txt`, `licenses/GPL-3.0.txt`) | all | Official prebuilt shared library from https://github.com/pytgcalls/ntgcalls, unmodified and SHA256-pinned by `scripts/vendor-ntgcalls.sh`. Not linked into the Quill binary: the `ntgcalls-sys` crate loads it at runtime, so it can be replaced. It statically contains WebRTC, FFmpeg, Opus, OpenH264, Boost and, on Linux, GLib and X11/Mesa libraries; see `licenses/ntgcalls-components.txt`. Source: the v3.0.0 tag at the URL above. |
| rlottie (animated stickers) | MIT, plus FreeType, Pixman, Skia, stb and RapidJSON notices (`licenses/rlottie/`) | all | Unmodified Samsung/rlottie commit ea06d2f29ba01b8d06c00a838d107f5e484ae59b, built by `scripts/build-rlottie.sh`. Source: https://github.com/Samsung/rlottie |
| Microsoft Visual C++ runtime (static) | Microsoft Visual Studio license terms (`licenses/msvc-runtime.txt`) | Windows | Linked statically (`/MT`, Rust `+crt-static`) into `quill.exe`, `tdjson.dll` and `rlottie.dll`; no `vcruntime140*.dll` / `msvcp140*.dll` ship. Not covered by Quill's MIT license. |
| quill-qr-scanner (macOS helper) | MIT | macOS | Quill's own code, `native/qr-scanner.m`. |
| quillvideo shim | MIT | Linux, Windows | Quill's own code, `native/quillvideo`. |

Every GitHub release also carries the corresponding source of the LGPL
components above as `quill-source-*` archives (FFmpeg as Quill builds it,
ntgcalls at its tag, and the FFmpeg and GLib releases ntgcalls links), with
the exact commits and a written offer in `LGPL-SOURCES.txt`
(`scripts/release-lgpl-sources.sh`).

## Data and assets

| Item | License | Notes |
|---|---|---|
| Emoji list `assets/emoji/emoji-17.tsv` (compiled in) | Unicode-3.0 (`assets/emoji/LICENSE.txt`, shipped as `licenses/unicode-emoji/LICENSE.txt`) | Derived from Unicode 17.0 `emoji-test.txt`. |
| English spellcheck word list `assets/spellcheck/en.txt` (compiled in) | CC BY-SA 4.0 (`licenses/spellcheck-wordlist.txt`) | Adapted from FrequencyWords by Hermit Dave (`content/2018/en/en_50k.txt`). The adapted list is also CC BY-SA 4.0; this applies to the word list only. |
| App and tray icons (`assets/icons/`) | Quill project | Quill's own design, a "Q" with a quill feather (`docs/decisions/codex-app-icon.md`). It is not the Telegram logo. |
| Call tones | Quill project | Synthesized in code (`src/ui/call_tones.rs`); no audio files and none of Telegram's GPL sounds. |
| Fonts | none bundled | Quill uses the operating system's fonts. |

## Source not used

No source was copied from Paper Plane, Coop or Mezon (GPL). ZapFast was not
forked.
