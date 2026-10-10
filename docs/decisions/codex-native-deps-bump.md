# Native dependency bump: ntgcalls v3.0.2 and FFmpeg n9.0.2

Closes the deps-watch issues for ntgcalls (#554) and FFmpeg (#553), following
`docs/dependency-updates.md`.

## What tdesktop does

Telegram Desktop builds its own FFmpeg and tgcalls from pinned sources in `Telegram/build/prepare/`, and
moves them by hand. Quill's pins are the equivalent, so no behavior is copied.

## ntgcalls v3.0.0 to v3.0.2

- `scripts/vendor-ntgcalls.sh`: new version and the SHA256 of the Linux x86_64, macOS arm64 and Windows
  x86_64 assets, taken from the release's asset digests. The macOS download was checked by running the script.
- Upstream changes between the tags are small: a new participants request type and callback argument,
  plus edits in group and conference call handling, audio/video receivers and gzip in `wrtc` (judged from the
  file list of the compare view; the release notes only link the build). `version.properties` is identical, so the statically linked
  components (FFmpeg 9.0.1, GLib 2.89.4, WebRTC 152) did not change and `licenses/ntgcalls-components.txt`
  only needed the version string.
- C header diff against v3.0.0: `ntg_participants_request` (SSRC list), `ntg_participants_request_free`, and
  `ntg_request_participants_callback_cb` now receives that struct by value. `crates/ntgcalls-sys` is updated
  to match (struct, callback type, free function and its loader entry). Quill never registers that callback,
  so no call-flow code changes.
- Verified: `cargo test -p ntgcalls-sys` (dlopen smoke tests load the new dylib and resolve the symbols),
  `cargo test --test native_calls`, and the gate.

## FFmpeg n8.1.3 to n9.0.2

- Quill has no Rust FFmpeg binding. It builds FFmpeg with `scripts/build-ffmpeg.sh` and talks to it through
  its own C shim `native/quillvideo/quillvideo.c`, so the question was whether the shim compiles against
  FFmpeg 9. It does, with `-Wall -Wextra -Werror`, and no source change: the shim already uses the
  `AVChannelLayout` API and `codecpar`, and none of the removed fields.
- `TAG` and `COMMIT` changed (the commit is the tag's commit, not the annotated tag object). Configure flags,
  decoder, demuxer and parser lists are unchanged: LGPL, no `--enable-gpl` or `--enable-nonfree`,
  `--disable-autodetect`. `THIRD_PARTY.md` has the new tag and commit.
- Shared library majors moved (libavcodec 63, libavformat 63, libavutil 61, libswscale 10, libswresample 7).
  The Linux and Windows packaging scripts glob on `.so.*` and `-\d+.dll`, so they needed no edit. The
  `windows-ffmpeg` cache key hashes the build script and shim, so it refreshes on its own.
- Verified on macOS (a development build; Quill.app itself keeps AVPlayer): the script built FFmpeg and the
  shim, and the 24 `video_decode` tests passed, including the ones that decode the real H.264/AAC, rotated MP4
  and VP9 WebM fixtures and the seek and player tests.
- Not verified here: the Linux and Windows (MSYS2) builds of n9.0.2. CI runs them.
