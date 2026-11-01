# Build

Pinned toolchain: **Rust 1.99.1** (`rust-toolchain.toml`; MSRV 1.92 for `oo7` via GPUI Kit).  
UI: **gpui-kit 0.8.1**.  
TDLib schema/runtime: **1.8.69** at `4c6377aa7386c39217878a9a56d581922ad37ca4`.

## Developer (synthetic UI, no Telegram login)

```bash
rustup show   # should pick 1.99.1 from rust-toolchain.toml
cargo test --no-default-features
cargo run --features ui
```

The window is a GPUI Kit `Root` wrapping a mixed-height synthetic chat and composer when credentials are absent. With credentials **and** tdjson it opens a live TDLib client. After `authorizationStateReady` it pages `loadChats` for `chatListMain` (updates: `updateNewChat`, `updateChatPosition`, `updateChatLastMessage`, … — **not** `getChats`). Selecting a chat sends `getChatHistory`; the composer sends `sendMessage` (`topic_id` null) for supported cloud chats — text, or a locally attached photo/document (`inputMessagePhoto` / `inputMessageDocument`). Photo and document messages parse `messagePhoto` / `messageDocument` and download via `downloadFile` (`updateFile` for progress). Phone, verification code, and 2FA password fields submit the matching TDLib requests. Before Ready the sidebar does not pretend an inbox exists.

Headless (no GPUI) live-connect check:

```bash
cargo run --no-default-features -- --connect-smoke
```

See `docs/credentials.md`. Requires `QUILL_TDJSON_PATH` and owner credentials. Prints one redacted line (`SMOKE_OK wait-phone` / `SMOKE_BLOCKED …`).

## System dependencies (Linux)

`cargo build --features ui` on Linux additionally needs the GTK3 development
libraries — the `tray-icon` crate's AppIndicator backend links GTK3
unconditionally on Linux (no feature flag drops it):

```bash
sudo apt-get install libgtk-3-dev libasound2-dev
```

`libasound2-dev` (ALSA) is for call sounds and voice-note recording: `rodio`/`cpal` link `alsa-sys`.

Without it the build fails in the `gdk-pixbuf-sys` build script
(`gdk-3.0.pc` missing). Not needed on macOS (tray-icon uses Cocoa there).
The `linux-fmt-clippy-test` job does not build `--features ui`; the separate
`linux-package` job does (see below), with the full apt list in
`.github/workflows/ci.yml`.

## Tests (Linux CI)

```bash
cargo fmt --all -- --check
cargo clippy --no-default-features --all-targets -- -D warnings
cargo test --no-default-features
```

## Release profile

```bash
cargo build --features ui --release
```

On macOS, `bash scripts/macos-package-smoke.sh` copies the binary into `dist/Quill.app`. Nested `libtdjson` is included only when `QUILL_TDJSON_PATH` points at a locally built library (see `docs/native-bundle.md`); its OpenSSL dylibs are bundled automatically and `scripts/check-bundle-macho.sh` fails the step if anything references a non-system absolute path. `QUILL_BIN=<path>` skips the cargo build and packages an existing binary. With `vendor/quill-calls/lib/libquill_calls.dylib` (`scripts/build-tgcalls.sh`) the script also ships the call library in `Contents/Frameworks`, with the FFmpeg dylibs (`vendor/ffmpeg/prefix`) and OpenSSL (`OPENSSL_ROOT_DIR`) it links.

## Linux package

`bash scripts/linux-package.sh` (after `cargo build --release --features ui`, `scripts/build-tdlib.sh`, `scripts/build-ffmpeg.sh`, `scripts/build-tgcalls.sh`) writes `dist/linux/quill-linux-<arch>/` and `quill-linux-<arch>-bundle.tar.gz` (`x86_64` or `aarch64`, the build machine's CPU): `quill` plus `lib/` (tdjson, the call library libquill_calls, FFmpeg, bundled OpenSSL 3), RUNPATH `$ORIGIN/lib`, a `.desktop` file, icon, `install.sh` and a README. `scripts/check-bundle-elf.sh <dir>` fails if any ELF needs a library that is neither bundled nor an allowed system library. Needs `patchelf`. The CI `linux-package` job runs all of this with the libquill_calls the `quill-calls` workflow built earlier in the run, and uploads the tarball. Details: `docs/decisions/codex-linux-package.md`.

## Windows package

Needs Visual Studio 2022 Build Tools (C++ workload, run from a developer prompt so `dumpbin`/`cmake` are on `PATH`), Rust 1.99.1 (`x86_64-pc-windows-msvc`), and vcpkg. In order:

```powershell
cargo build --release --features ui
pwsh scripts/build-tdlib-windows.ps1     # vcpkg openssl/zlib/gperf + tdjson.dll -> native/prefix/bin
pwsh scripts/windows-package.ps1         # -> dist/windows/quill-windows-x86_64{,.zip}
```

The package is a flat directory: `quill.exe` plus the DLLs beside it. Everything links the VC++ runtime statically (`.cargo/config.toml` sets `+crt-static`; TDLib and the ThorVG sticker renderer build with `/MT`, OpenSSL and zlib are linked into `tdjson.dll`), so no `vcruntime`/`msvcp` DLL ships. `scripts/check-bundle-pe.ps1 <dir>` fails if a DLL imports anything that is neither bundled nor a Windows system DLL. `quill_calls.dll` (the call library, docs/decisions/tgcalls-engine.md) is built by `scripts/build-tgcalls.sh` from MSYS2 bash with the MSVC environment inherited, links OpenSSL, zlib and the C runtime statically and imports the FFmpeg DLLs shipped beside it; the script takes it from `vendor/quill-calls/lib/` and fails without it. For a developer run without packaging, set `QUILL_TDJSON_PATH` and `QUILL_CALLS_LIB` to the DLL paths. The CI jobs `windows-build`, `windows-native` and `windows-package` run all of this (not required checks). Details: `docs/decisions/codex-windows-package.md`.

## Troubleshooting

- **Linux (X11) or Windows: the window stops updating until the mouse moves,
  or touchpad scrolling starts late.** Quill's vendored GPUI backends stop
  per-frame wakeups while a window is idle
  (`docs/decisions/codex-idle-frames-x11-windows.md`). Start Quill with
  `QUILL_IDLE_FRAMES=0` to turn that off (upstream behavior: a frame every
  vblank) and report the case. Wayland and macOS do not read it.

## Video diagnostics

- `QUILL_TRACE_VIDEO=1` prints one CSV line to stderr per picture pull of
  every playing video:
  `VT,<player_id>,<wall_us>,<item_ms>,<new 0/1>,<display_ms>,<rate>`.
  `wall_us` counts from the first traced pull, `item_ms` is the clip time
  asked for (the moment the frame will be on screen), `new` says whether a
  new picture came, `display_ms` is that picture's own time (`-1000` when
  none) and `rate` is the playback rate (`0` while not playing). The gaps
  between `new=1` lines, in display vsyncs, show the pacing; the analysis
  script and how to run a measurement are in
  `docs/decisions/video-player-core.md`.
  On the FFmpeg backend and on live players (streams, live stories, call
  tiles), a pull that took a new picture also prints
  `VF,<player_id>,<format>,<width>,<height>,<bytes>,<convert_us>`:
  `format` is `nv12` (a GPU surface: Windows, and macOS live pictures) or
  `bgra` (an image), `bytes` is what the renderer uploads for it and
  `convert_us` the CPU time spent turning the decoder's picture (or the
  live frame's I420 planes) into it. On Windows the
  renderer adds `VU,nv12,<bytes>,<upload_us>` for each NV12 surface
  upload and `VU,atlas,<bytes>,<upload_us>` for image atlas uploads of
  64 KiB or more (video pictures, not glyphs). The analysis script
  summarises both; the NV12 before/after run is in
  `docs/decisions/video-player-nv12.md`.
- `QUILL_VIDEO_BACKEND=ffmpeg` (macOS, development) plays through the
  bundled FFmpeg shim instead of AVPlayer, the path Linux and Windows use.
- `QUILL_LIVE_BGRA=1` (demo-capture builds, macOS and Windows) shows live
  pictures as BGRA images instead of NV12 surfaces, the path Linux uses
  and the one a refused surface falls back to
  (`docs/decisions/video-player-live-nv12.md`).

## Notification diagnostics

- `QUILL_TRACE_TOASTS=1` prints `toast window opened in <ms> ms` to stderr
  for each of Quill's own notification windows: how long opening it held
  the UI thread. On Windows that includes building the window's renderer.

## Native TDLib (optional)

Ordinary `cargo test` / `cargo run` do **not** fetch or execute tdjson. To build the pinned runtime:

1. Install TDLib's documented build deps (C++17 compiler, CMake, gperf, OpenSSL, zlib).
2. `bash scripts/build-tdlib.sh`
3. `export QUILL_TDJSON_PATH=$PWD/native/prefix/lib/libtdjson.dylib` (or `.so`)

The loader searches, in order: `QUILL_TDJSON_PATH`, then paths relative to the executable (`lib/`, `Frameworks`, `Contents/Frameworks`, …). It does **not** search Homebrew prefixes.

## Live connect

Provide **your own** `api_id` / `api_hash` from https://my.telegram.org (never commit them) and a local tdjson build:

- macOS Keychain (or Linux `FileSecretStore` under the account app-data dir, mode 0600) holds the per-account database encryption key; `MemorySecretStore` is tests-only
- `setTdlibParameters` uses the pinned signature with `use_secret_chats=false`
- First request after `td_create_client_id` is `getAuthorizationState` so updates start
- Phone submit sends `setAuthenticationPhoneNumber`; WaitCode / WaitPassword UI send `checkAuthenticationCode` / `checkAuthenticationPassword`
- After `authorizationStateReady`, Quill pages `loadChats` (`chatListMain`) until TDLib returns 404. Chat rows come from `updateNewChat` / `updateChatPosition` / `updateChatLastMessage` (schema: do **not** use `getChats` to maintain the list). Select → `openChat` + `getChatHistory` + `viewMessages` (`messageSourceChatHistory`, `force_read`). Unread badges follow `updateChatReadInbox`; outgoing **read** vs **sent** follows `updateChatReadOutbox`. Photo thumbs in the open chat auto-`downloadFile` (priority 1); clicking a placeholder or document chip downloads that file (priority 32). Progress is `updateFile`. Composer Enter → `sendMessage` with `topic_id` null (text, or `inputMessagePhoto` / `inputMessageDocument` for an explicitly attached local file). Sidebar Search is Cmd/Ctrl+K (`searchRecentlyFoundChats` / `searchChats` + `searchMessages`). Focused history Cmd/Ctrl+F is in-chat `searchInChat` (`searchChatMessages`, 900 ms `AutoSearchTimeout`); jump uses `getChatHistory` around the hit (Unigram offset −25 / limit 50). Esc closes the finder without changing the open chat. Message text and file paths are never written to diagnostics.
- `quill --connect-smoke` is the headless gate (no window): credentials + `QUILL_TDJSON_PATH` → ingest until WaitPhoneNumber or a clear blocker (30s timeout). Before exit it sends `close` and waits for `authorizationStateClosed` so unloading tdjson does not SIGSEGV.

## Releases

Push a `vX.Y.Z` tag that matches the `Cargo.toml` version and `.github/workflows/release.yml` builds the macOS (`quill-macos-aarch64.zip`, ad-hoc signed `Quill.app`), Linux (`quill-linux-x86_64-bundle.tar.gz` and `quill-linux-aarch64-bundle.tar.gz`, each with its updater binary `quill-linux-<arch>`, built natively on GitHub's x86_64 and arm64 runners) and Windows (`quill-windows-x86_64.zip`) packages, archives the LGPL sources, writes the release notes (optional curated top section from `release-notes/vX.Y.Z.md`, then merged PRs since the previous tag), `SHA256SUMS.txt` and `latest.json`, and creates a **draft** release for the owner to publish. `gh workflow run release.yml -f version=X.Y.Z --ref <branch>` is a dry run that uploads the same files as workflow artifacts and creates nothing. Publishing fires `release-published.yml`, which sends the `site-refresh` repository dispatch. Details: `docs/decisions/codex-release-pipeline.md`.
