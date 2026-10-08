# Bundle OpenSSL (app bundles depend only on the OS)

## What was wrong

`libtdjson.dylib` built by `scripts/build-tdlib.sh` links Homebrew OpenSSL by absolute path:

```
@rpath/libtdjson.1.8.67.dylib
/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib
/opt/homebrew/opt/openssl@3/lib/libcrypto.3.dylib
/usr/lib/libz.1.dylib ...
```

`scripts/macos-package-smoke.sh` rewrote only libtdjson's own id and added an rpath, so `Quill.app` failed to launch (dyld cannot load libtdjson) on any Mac without Homebrew `openssl@3`. Homebrew's `libssl` in turn references `libcrypto` through `/opt/homebrew/Cellar/...`. The script's final `otool -L` printout was informational only and never failed.

## What changed

- `scripts/macos-package-smoke.sh`
  - New `vendor_deps` step: for every dylib in `Contents/Frameworks`, any dependency that is not `/usr/lib`, `/System/Library` or `@`-relative is copied into Frameworks (resolving symlinks), given id `@rpath/<name>`, and the referencing library is rewritten to `@loader_path/<name>`. It iterates to a fixed point, so libssl's libcrypto reference is fixed too, and it also covers any future transitive dependency rather than hard-coding OpenSSL.
  - Signing: `install_name_tool` invalidates signatures, so nested dylibs and `quill-qr-scanner` are re-signed first, then the app (`codesign --force --sign "${QUILL_CODESIGN_IDENTITY:--}"`). Ad-hoc by default; the local install flow can pass its Apple Development identity. `codesign --verify --deep --strict` passes.
  - Runs `scripts/check-bundle-macho.sh` right after assembly. The release workflows (`quill-ui-build.yml`, `macos-smoke-fallback.yml`) package only through this script, so the check is wired into them with no workflow edit.
  - `QUILL_BIN=<path>` packages an existing binary and skips the cargo build (useful for fast re-packaging and for verifying in a worktree).
- `scripts/check-bundle-macho.sh` (new): walks every Mach-O in the bundle and fails on any install id, linked library or `LC_RPATH` that is not a system path (`/usr/lib`, `/System/Library`) or an `@rpath` / `@loader_path` / `@executable_path` reference that actually resolves to a file inside the bundle.
- `scripts/build-tdlib.sh`: on Linux, sets `CMAKE_INSTALL_RPATH=$ORIGIN` so `libtdjson.so` finds libs bundled beside it. Unverified from macOS, see below.
- Docs: `docs/native-bundle.md`, `docs/build.md`.

The other bundled pieces were already clean: `libntgcalls.dylib` (system frameworks only), `librlottie.dylib` (only libc++/libSystem; id already set to `@rpath/librlottie.dylib`), `quill-qr-scanner` (system frameworks only), the main executable (system frameworks only; its `@executable_path/../Frameworks` rpath is added when tdjson is bundled).

## How verified (macOS, arm64)

Bundle built in the worktree with `QUILL_BIN`, `QUILL_TDJSON_PATH`, `QUILL_NTGCALLS_LIB`, `QUILL_RLOTTIE_PATH` pointing at the main checkout's existing artifacts (read-only; the live `dist/Quill.app` was not touched).

Before (libtdjson in the bundle): `/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib`, `/opt/homebrew/opt/openssl@3/lib/libcrypto.3.dylib`.

After:

```
libtdjson.dylib: @rpath/libtdjson.dylib, @loader_path/libssl.3.dylib, @loader_path/libcrypto.3.dylib, /usr/lib/libz.1.dylib, /usr/lib/libc++.1.dylib, /usr/lib/libSystem.B.dylib
libssl.3.dylib:  @rpath/libssl.3.dylib, @loader_path/libcrypto.3.dylib, /usr/lib/libSystem.B.dylib
libcrypto.3.dylib: @rpath/libcrypto.3.dylib, /usr/lib/libSystem.B.dylib
```

- `check-bundle-macho.sh`: `OK: 7 Mach-O file(s) ... depend only on the OS and the bundle`.
- Negative test: re-pointing libtdjson at `/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib` makes the checker print `BAD ... links /opt/homebrew/...` and exit 1.
- A `dlopen` of the bundled libtdjson with `DYLD_PRINT_LIBRARIES=1` loads `Quill.app/Contents/Frameworks/{libtdjson,libssl.3,libcrypto.3}.dylib`, and nothing from `/opt/homebrew`. (`quill --build-info` does not load tdjson; it is loaded lazily, hence the dlopen harness.) Homebrew's openssl cannot be uninstalled from this machine, but with every reference now `@loader_path`, dyld has no way to reach it.
- `codesign --verify --deep --strict dist/Quill.app` passes.

## Per-platform status

| Platform | Status |
|---|---|
| macOS | Fixed and enforced by the package script and the CI package job. |
| Linux | No AppImage/deb/tarball packaging exists. The only release artifact is the bare binary from `scripts/package-update-binary.sh`. `ffi.rs` already looks for `libtdjson.so` next to the exe, in `Frameworks`, and so on. Added `$ORIGIN` rpath to the TDLib build so a bundled `libssl.so.3`/`libcrypto.so.3` beside `libtdjson.so` is found. Not verifiable from a Mac. |
| Windows | No packaging, CI job, or release asset exists (`package-update-binary.sh` only accepts `macos\|linux` names; `vendor-ntgcalls.sh` supports only Linux x86_64 and macOS arm64). `tdjson.dll` is looked up next to the exe by `ffi.rs`. Nothing to fix in place; needs a design. |

## Follow-ups (cannot be done from a Mac)

1. Linux: write a packaging script (tarball first) that places `quill`, `libtdjson.so`, `libntgcalls.so`, `librlottie.so` together, and a `check-bundle-elf.sh` analogue that runs `readelf -d` (RUNPATH must be `$ORIGIN`, no absolute paths) and `ldd` with `LD_LIBRARY_PATH` unset, failing on `not found` or non-system paths. Decide: bundle `libssl.so.3`/`libcrypto.so.3` (portable) or declare `libssl3`/`zlib1g` as `.deb` dependencies. Confirm `libtdjson.so` actually ends up with `RUNPATH=$ORIGIN` after the `-DCMAKE_INSTALL_RPATH` change, and that `libntgcalls.so`/`librlottie.so` have `$ORIGIN` or are `dlopen`ed by absolute path (ntgcalls-sys loads at runtime).
2. Windows: add a packaging job collecting `quill.exe`, `tdjson.dll`, `libssl-3-x64.dll`, `libcrypto-3-x64.dll`, `zlib1.dll`, `rlottie.dll`, `ntgcalls.dll` in one directory (Windows resolves DLL dependencies from the exe/loaded-DLL directory), plus a `dumpbin /dependents` check that fails on anything outside the bundle and system DLLs. Extend `vendor-ntgcalls.sh` and `package-update-binary.sh` for Windows.
3. The self-updater currently ships and swaps a bare binary; a macOS bundle's Frameworks (now including OpenSSL) are not refreshed by it. Worth keeping in mind if OpenSSL needs a security update.
4. Notarization still deferred; hardened-runtime signing with a Developer ID will need `--options runtime` on each nested item (the loop in the package script is the place).
