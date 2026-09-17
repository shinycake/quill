# Build

Pinned toolchain: **Rust 1.98.1** (`rust-toolchain.toml`; MSRV 1.92 for `oo7` via GPUI Kit).  
UI: **gpui-kit 0.6.1**.  
TDLib schema/runtime: **1.8.67** at `d1085f9cebc5a62379991ae1652673954f229c1f`.

## Developer (synthetic UI, no Telegram login)

```bash
rustup show   # should pick 1.98.1 from rust-toolchain.toml
cargo test --no-default-features
cargo run --features ui
```

The window is a GPUI Kit `Root` wrapping a mixed-height synthetic chat and composer. It does **not** connect to Telegram.

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

On macOS, `bash scripts/macos-package-smoke.sh` copies the binary into `dist/Quill.app`. Nested `libtdjson` is included only when `QUILL_TDJSON_PATH` points at a locally built library (see `docs/native-bundle.md`).

## Native TDLib (optional)

Ordinary `cargo test` / `cargo run` do **not** fetch or execute tdjson. To build the pinned runtime:

1. Install TDLib's documented build deps (C++17 compiler, CMake, gperf, OpenSSL, zlib).
2. `bash scripts/build-tdlib.sh`
3. `export QUILL_TDJSON_PATH=$PWD/native/prefix/lib/libtdjson.dylib` (or `.so`)

The loader searches, in order: `QUILL_TDJSON_PATH`, then paths relative to the executable (`Contents/Frameworks`, …). It does **not** search Homebrew prefixes.

## Next live step (blocked on credentials)

Quill will not call Telegram until the owner provides **their own** `api_id` / `api_hash` from https://my.telegram.org (never commit them). After that:

- macOS Keychain holds the per-account database encryption key
- `setTdlibParameters` uses the pinned signature with `use_secret_chats=false`
- First request after `td_create_client_id` must be sent so updates start (`getAuthorizationState`)
