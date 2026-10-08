# Third-party notices

This file tracks licenses of dependencies Quill links or vendors. It is not a completed legal audit.

| Component | License | Notes |
|---|---|---|
| Quill | MIT | `LICENSE` |
| GPUI Kit, GPUI (`gpui-pre*`), gpui-base, gpui-component, gpui-kit-assets | Apache-2.0 | crates.io 0.6.1 / gpui-pre 0.3.x family |
| Lucide icons (via gpui-kit-assets) | ISC (Lucide) | bundled only when the `assets` feature is on |
| TDLib | Boost Software License 1.0 | schema vendored in `schema/td_api.tl`; native library optional |
| ntgcalls (media engine) | GNU Lesser General Public License v3.0 (LGPL-3.0) | **Not linked into the Quill binary.** `pytgcalls/ntgcalls` v3.0.0 (released 2026-09-25) prebuilt shared libraries `libntgcalls.so` (Linux x86_64) and `libntgcalls.dylib` (macOS ARM64), procured unmodified by `scripts/vendor-ntgcalls.sh` (SHA256-pinned, git-ignored, never committed) and loaded at runtime via `dlopen` from the `ntgcalls-sys` crate. The macOS sidecar can also be copied unmodified into the app bundle’s Frameworks directory; Windows assets remain unsupported by the procurement script. Source: https://github.com/pytgcalls/ntgcalls — the complete corresponding LGPL source for the unmodified library is available from upstream at that URL (release tag v3.0.0). |
| rlottie (runtime) | MIT plus bundled FreeType, Pixman, Skia, stb and RapidJSON notices | Unmodified Samsung/rlottie at commit ea06d2f29ba01b8d06c00a838d107f5e484ae59b; built with scripts/build-rlottie.sh. Source: https://github.com/Samsung/rlottie. Bundled macOS packages include COPYING and every upstream licenses/ notice. |
| FFmpeg (libavcodec, libavformat, libavutil, libswscale, libswresample; Linux and Windows packages) | LGPL-2.1-or-later | **Dynamically linked, not part of the Quill binary.** Unmodified FFmpeg n8.1.3 (commit 1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7, the release Telegram Desktop pins) built by `scripts/build-ffmpeg.sh` as shared libraries with decoders/demuxers only: no `--enable-gpl`, no `--enable-nonfree`, no external libraries (`--disable-autodetect`). Quill loads it at runtime through its own MIT shim `quillvideo` (`native/quillvideo`). The packages ship FFmpeg's license texts plus the exact source pointer and configure line in `licenses/ffmpeg/`; users may replace the FFmpeg libraries with their own compatible build. Source: https://github.com/FFmpeg/FFmpeg/tree/n8.1.3. Patent note: H.264/HEVC decoding may be patent-encumbered in some jurisdictions, as for every FFmpeg-based player (Telegram Desktop included). macOS keeps AVFoundation and does not ship FFmpeg. |
| rusty-opus (pure-Rust Opus encoder/decoder), ogg (Ogg muxer), cpal | BSD-3-Clause / BSD-3-Clause / Apache-2.0 | Unmodified crates.io, statically linked, used for in-process voice-note recording (`src/voice_opus.rs`, `src/voice_input.rs`). No libopus. |
| spellbook (Hunspell-compatible spell checker, Linux) | MPL-2.0 | Unmodified crates.io 0.4.x, statically linked; file-level copyleft only. Dictionaries are read from the system at runtime and not bundled. |
| Rust crates in `Cargo.lock` | various OSI | inspect `cargo license` before a public release |

No source was copied from Paper Plane, Coop, or Mezon (GPL). ZapFast was not forked.

The native TDLib build applies `native/patches/tdlib-quill-takeout-contacts.patch` to the pinned upstream commit. This Quill extension adds scoped contacts and message-range takeout requests; the official schema snapshot and upstream pin stay intact. Build from source with `scripts/build-tdlib.sh`; `scripts/tdlib-takeout-smoke.py` checks the compiled extension offline. This is a locally modified TDLib build, not an upstream API addition.
