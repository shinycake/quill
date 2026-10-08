# Build

Pinned toolchain: **Rust 1.98.1** (`rust-toolchain.toml`; MSRV 1.92 for `oo7` via GPUI Kit).  
UI: **gpui-kit 0.7.0**.  
TDLib schema/runtime: **1.8.67** at `d1085f9cebc5a62379991ae1652673954f229c1f`.

## Developer (synthetic UI, no Telegram login)

```bash
rustup show   # should pick 1.98.1 from rust-toolchain.toml
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
sudo apt-get install libgtk-3-dev
```

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

On macOS, `bash scripts/macos-package-smoke.sh` copies the binary into `dist/Quill.app`. Nested `libtdjson` is included only when `QUILL_TDJSON_PATH` points at a locally built library (see `docs/native-bundle.md`); its OpenSSL dylibs are bundled automatically and `scripts/check-bundle-macho.sh` fails the step if anything references a non-system absolute path. `QUILL_BIN=<path>` skips the cargo build and packages an existing binary.

## Linux package

`bash scripts/linux-package.sh` (after `cargo build --release --features ui`, `scripts/build-tdlib.sh`, `scripts/build-rlottie.sh`, `scripts/vendor-ntgcalls.sh`) writes `dist/linux/quill-linux-x86_64/` and `quill-linux-x86_64-bundle.tar.gz`: `quill` plus `lib/` (tdjson, ntgcalls, rlottie, bundled OpenSSL 3), RUNPATH `$ORIGIN/lib`, a `.desktop` file, icon, `install.sh` and a README. `scripts/check-bundle-elf.sh <dir>` fails if any ELF needs a library that is neither bundled nor an allowed system library. Needs `patchelf`. The CI `linux-package` job runs all of this and uploads the tarball. Details: `docs/decisions/codex-linux-package.md`.

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
