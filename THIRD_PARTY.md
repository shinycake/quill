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
| gpui-kit, gpui-component, gpui-pre (GPUI) and related crates | Apache-2.0 | crates.io 0.8.1 / gpui-pre 0.3.10 |
| gpui-base 0.7.3 | Apache-2.0 | Patched copy in `third_party/gpui-base`; the changes are listed in `third_party/gpui-base/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre 0.3.10 | Apache-2.0 | Patched copy in `third_party/gpui-pre`; the changes are listed in `third_party/gpui-pre/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre-apple 0.3.10 | Apache-2.0 | Patched copy in `third_party/gpui-pre-apple`; the changes are listed in `third_party/gpui-pre-apple/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre-macos 0.3.10 | Apache-2.0 | Patched copy in `third_party/gpui-pre-macos`; the changes are listed in `third_party/gpui-pre-macos/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre-linux 0.3.10 | Apache-2.0 | Patched copy in `third_party/gpui-pre-linux`; the changes are listed in `third_party/gpui-pre-linux/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre-wgpu 0.3.10 | Apache-2.0 | Patched copy in `third_party/gpui-pre-wgpu`; the changes are listed in `third_party/gpui-pre-wgpu/QUILL-CHANGES.md` and marked in each changed file. |
| gpui-pre-windows 0.3.10 | Apache-2.0 | Patched copy in `third_party/gpui-pre-windows`; the changes are listed in `third_party/gpui-pre-windows/QUILL-CHANGES.md` and marked in each changed file. |
| opus-decoder 0.1.1 | MIT OR Apache-2.0 | Patched copy in `third_party/opus-decoder`; the changes are listed in `third_party/opus-decoder/QUILL-CHANGES.md` and marked in each changed file. The crates.io package has no license file; `LICENSE-MIT` and `LICENSE-APACHE` there are the upstream repository's, at the 0.1.1 release commit. |
| blocking 1.7.0 | Apache-2.0 OR MIT | Patched copy in `third_party/blocking`; the change is listed in `third_party/blocking/QUILL-CHANGES.md` and marked `QUILL`. |
| gpui-kit-assets 0.8.1 | Apache-2.0; bundled Lucide icons ISC, some derived from Feather (MIT) | The icon license is in `licenses/lucide-ISC.txt` (copied from the crate's `LICENSE-LUCIDE`). |
| spellbook, symphonia (via rodio), dwrote, option-ext | MPL-2.0 | Unmodified crates.io releases. MPL-2.0 is file-level copyleft; the source of these files is on crates.io and in each project's repository (links in `THIRD_PARTY_LICENSES.md`). |
| webpki-roots | CDLA-Permissive-2.0 | Mozilla CA certificate data. |
| ICU4X crates (icu_*, zerovec, yoke, ...) | Unicode-3.0 | |
| rusty-opus, ogg, cpal | BSD-3-Clause, BSD-3-Clause, Apache-2.0 | In-process voice-note recording. No libopus. |
| ciborium, p256, aes, cbc, hmac, hkdf, sha2 (RustCrypto); core-foundation-sys (macOS) | Apache-2.0 (ciborium); MIT OR Apache-2.0 (the rest) | Passkeys on a USB security key (CTAP2 over HID, `src/webauthn/`). No OpenSSL, no libfido2. |
| nnnoiseless | BSD-3-Clause | Call noise suppression (an RNNoise port) on the microphone frames Quill hands to the call library. |

No crate in the shipped graph is licensed only under GPL, AGPL or LGPL. Crates
that offer a GPL license only as one option (for example `self_cell`,
"Apache-2.0 OR GPL-2.0-only") are used under the permissive option.

## Native libraries shipped with the packages

| Component | License | Shipped in | Notes |
|---|---|---|---|
| TDLib 1.8.69 (`libtdjson`, `tdjson.dll`) | BSL-1.0 (`licenses/tdlib-BSL-1.0.txt`) | all | Pinned upstream commit with one local patch, `native/patches/tdlib-quill-takeout-contacts.patch` (scoped contacts and message-range takeout requests). Built by `scripts/build-tdlib.sh` / `scripts/build-tdlib-windows.ps1`. The schema is vendored in `schema/td_api.tl`. |
| OpenSSL 3 (`libssl`, `libcrypto`) | Apache-2.0 (`licenses/openssl-Apache-2.0.txt`) | all | Needed by TDLib and the call library. macOS: shared libraries copied from the build machine's Homebrew `openssl@3`; Linux: shared libraries from the build host's distribution packages; Windows: vcpkg `openssl:x64-windows-static`, linked statically into `tdjson.dll` and `quill_calls.dll`. Unmodified. |
| zlib | Zlib (`licenses/zlib.txt`) | Windows | vcpkg `zlib:x64-windows-static`, linked statically into `tdjson.dll` and `quill_calls.dll`; MSYS2's static zlib, linked into the FFmpeg DLLs and programs. macOS and Linux use the system zlib. |
| FFmpeg n9.0.2 (libavcodec, libavformat, libavutil, libavfilter, libavdevice, libswscale, libswresample, and the `ffmpeg` and `ffprobe` programs) | LGPL-2.1-or-later (`licenses/ffmpeg/`) | all (macOS: only the libraries the call library links) | Unmodified, commit 946fcce07b6dcd0331c8cc609192aeff5e1924f8. Built by `scripts/build-ffmpeg.sh` as shared libraries plus the `ffmpeg` and `ffprobe` programs, with a fixed list of codecs, formats and filters: no `--enable-gpl`, no `--enable-nonfree`, no x264 or other GPL library, and only zlib (Windows, statically), PulseAudio and ALSA (Linux, system libraries, for microphones) and Media Foundation (Windows, the system's H.264 encoder) as external code (`--disable-autodetect`). The programs are the ones Quill runs for the video editor, photo downscale, round videos and the camera preview; `docs/decisions/sweep-ffmpeg-tools-bundled.md` has the details. `licenses/ffmpeg/FFMPEG-SOURCE.txt` records the source and the exact configure line. Quill loads FFmpeg through its own MIT shim `quillvideo` (`native/quillvideo`), so the FFmpeg libraries can be replaced. On Windows the FFmpeg DLLs statically include the MinGW-w64 runtime: libgcc (GCC Runtime Library Exception) and winpthreads (MIT and BSD-3-Clause, `licenses/winpthreads.txt`). macOS plays video with AVFoundation; its app ships only the FFmpeg libraries the call library links, from the same build, in `Contents/Frameworks`. Patent note: H.264/HEVC decoding may be patent-encumbered in some countries, as in every FFmpeg-based player. |
| libquill_calls (calls): tgcalls 1c236c09, with tg_owt 07fb35bc, Opus v1.5.2, libvpx v1.14.1, OpenH264 v2.6.0, mozjpeg v4.1.5 and rnnoise d8ea2b0 | tgcalls: LGPL-3.0 (`licenses/GPL-3.0.txt` and `licenses/quill-calls/licenses/tgcalls/`); the other parts: BSD, MIT, Apache-2.0 and IJG/zlib-style licenses (`licenses/quill-calls/licenses/`) | Linux, macOS, Windows | Telegram's call library, the one Telegram Desktop ships, built from source by `scripts/build-tgcalls.sh` at the commits Telegram Desktop pins, with Quill's C API (`native/quill-calls`, MIT) and Quill's patches (`native/quill-calls/patches`). Not linked into the Quill binary: `crates/quill-calls-sys` loads it at runtime, so it can be replaced. It links FFmpeg dynamically and OpenSSL dynamically on Linux and macOS (the copies above); on Windows OpenSSL 3 and zlib from vcpkg are linked in statically. `licenses/quill-calls/SOURCES.txt` in each package names the exact source of every part. |
| ThorVG v1.1.3 (animated stickers), with RapidJSON | MIT (`licenses/thorvg/` in the packages, staged from `third_party/thorvg/LICENSE` and `third_party/thorvg/src/loaders/lottie/rapidjson/LICENSE`) | all | Compiled into the Quill binary by `build.rs` from `third_party/thorvg`: the subset `scripts/vendor-thorvg.sh` copies from the v1.1.3 tag (commit 589bc4120f2097fb1b135e552c2ef39f6581612b), with the changes in `third_party/thorvg/QUILL-CHANGES.md`. Source: https://github.com/thorvg/thorvg |
| Microsoft Visual C++ runtime (static) | Microsoft Visual Studio license terms (`licenses/msvc-runtime.txt`) | Windows | Linked statically (`/MT`, Rust `+crt-static`) into `quill.exe` (ThorVG included) and `tdjson.dll`; no `vcruntime140*.dll` / `msvcp140*.dll` ship. Not covered by Quill's MIT license. |
| quill-qr-scanner (macOS helper) | MIT | macOS | Quill's own code, `native/qr-scanner.m`. |
| quillvideo shim | MIT | Linux, Windows | Quill's own code, `native/quillvideo`. |

Not shipped: on Linux (and on macOS when ImageIO can't read a file), Quill
opens HEIC and AVIF pictures with the system's libheif (LGPL-3.0) if it is
installed, loading it at run time (`src/ui/image_decode/libheif.rs`). Quill
neither links nor distributes it. Windows uses the system's WIC codecs.

Every GitHub release also carries the corresponding source of the LGPL
components above as `quill-source-*` archives (FFmpeg as Quill builds it and
tgcalls at the commit libquill_calls is built from), with
the exact commits and a written offer in `LGPL-SOURCES.txt`
(`scripts/release-lgpl-sources.sh`).

## Data and assets

| Item | License | Notes |
|---|---|---|
| Emoji list `assets/emoji/emoji-17.tsv` (compiled in) | Unicode-3.0 (`assets/emoji/LICENSE.txt`, shipped as `licenses/unicode-emoji/LICENSE.txt`) | Derived from Unicode 17.0 `emoji-test.txt`. |
| English spellcheck word list `assets/spellcheck/en.txt` (compiled in) | CC BY-SA 4.0 (`licenses/spellcheck-wordlist.txt`) | Adapted from FrequencyWords by Hermit Dave (`content/2018/en/en_50k.txt`). The adapted list is also CC BY-SA 4.0; this applies to the word list only. |
| Emoji set previews `assets/emoji/sets/emoji-set-noto-preview.png` (compiled in) and the downloadable Noto pack | Apache-2.0 (Noto Emoji, googlefonts/noto-emoji); flags from region-flags, public domain | Four faces per set for Settings → "Choose emoji set". The full pack is not shipped; it is downloaded on request from the `emoji-sets-v1` release and built by `quill-tools/emoji-sets` (`docs/decisions/emoji-set-style.md`). The pack's `LICENSE.txt` has the full Apache-2.0 text, the region-flags notice and what Quill changed. |
| Emoji set preview `assets/emoji/sets/emoji-set-twemoji-preview.png` (compiled in) and the downloadable Twemoji pack | CC-BY 4.0 (Twemoji by Twitter, Inc. and contributors, jdecked/twemoji) | Attributed in the "Choose emoji set" box and in the pack's `LICENSE.txt`, which has the full CC-BY 4.0 text and says the art was resized and packed into sprite sheets. Downloaded on request like the Noto pack. |
| Built-in US keyboard map `third_party/gpui-pre-linux/src/linux/wayland/default_keymap.xkb` (compiled in, Linux) | xkeyboard-config's MIT/X11-style notices (`licenses/xkeyboard-config-COPYING.txt`, the 2.41 `COPYING`) | Compiled by libxkbcommon from xkeyboard-config 2.41 (https://gitlab.freedesktop.org/xkeyboard-config/xkeyboard-config). Used on Wayland only when the compositor sends no usable keymap and the system has none. |
| App and tray icons (`assets/icons/`) | Quill project | Quill's own design, a "Q" with a quill feather (`docs/decisions/codex-app-icon.md`). It is not the Telegram logo. |
| Call tones | Quill project | Synthesized in code (`src/ui/call_tones.rs`); no audio files and none of Telegram's GPL sounds. |
| Fonts | none bundled | Quill uses the operating system's fonts. |

## Source not used

No source was copied from Paper Plane, Coop or Mezon (GPL). ZapFast was not
forked.
