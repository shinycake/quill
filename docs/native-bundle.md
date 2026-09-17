# Native tdjson bundle experiment

Goal: a clean-machine macOS binary that loads official `tdjson` without Homebrew `rpath`s.

## Chosen path

1. Build TDLib from the pinned commit with `scripts/build-tdlib.sh` (source checkout, not a GitHub-release binary).
2. Copy `libtdjson.dylib` into `Quill.app/Contents/Frameworks/`.
3. Set the library id to `@rpath/libtdjson.dylib` and add `@executable_path/../Frameworks` to the executable.
4. At runtime, `quill::telegram::ffi` loads that bundled file (or `QUILL_TDJSON_PATH` for developers). Homebrew prefixes are never searched.

Static vs dynamic: this experiment uses a **bundled dylib**. Static linking of TDLib is possible later if the dylib + rpath approach fails notarization; it is not universally simpler (OpenSSL/zlib still need a policy).

## What this agent ran

This implementation session is a Linux x86_64 cloud environment. It:

- Vendored `schema/td_api.tl` from the pinned commit and checksummed it.
- Implemented the modern C JSON API (`td_create_client_id` / `td_send` / `td_receive` / `td_execute` / `td_set_log_message_callback`).
- Did **not** compile TDLib C++ here (long, and the product target is macOS Apple Silicon).
- Wired `scripts/macos-package-smoke.sh` for GitHub `macos-latest` to assemble `dist/Quill.app` after `cargo build --release`.

## Manual Mac follow-up

On an Apple Silicon Mac with CMake/gperf/OpenSSL:

```bash
bash scripts/build-tdlib.sh
export QUILL_TDJSON_PATH=$PWD/native/prefix/lib/libtdjson.dylib
cargo build --features ui --release
bash scripts/macos-package-smoke.sh
otool -L dist/Quill.app/Contents/MacOS/quill
otool -L dist/Quill.app/Contents/Frameworks/libtdjson.dylib
```

Reject the bundle if `otool -L` shows `/opt/homebrew` or `/usr/local/opt` for tdjson.

Signing/notarization is deferred (no Developer ID in this phase).
