# Windows package (zip) and CI

## Layout

```
quill-windows-x86_64/
  quill.exe
  tdjson.dll  libssl-3-x64.dll  libcrypto-3-x64.dll  zlib1.dll
  ntgcalls.dll
  rlottie.dll
  vcruntime140.dll  vcruntime140_1.dll  msvcp140*.dll   (whatever the PE closure needs)
  README.txt  LICENSE  THIRD_PARTY.md
```

Shipped as `quill-windows-x86_64.zip` (+ `.sha256`). Built by `scripts/windows-package.ps1`, which ends by running the checker. The directory is deliberately flat: Windows resolves a DLL's own dependencies from the application directory (and, for `LoadLibrary` on an absolute path, the DLL's directory), so putting everything beside `quill.exe` needs no `PATH`, no `SetDllDirectory` and no manifest. Quill's loaders already looked beside the executable (`telegram/ffi.rs` for `tdjson.dll`, `sticker_playback.rs` for `rlottie.dll`); `ntgcalls-sys::Loader::load_default` gained a Windows-only search of the executable's directory and of `vendor/ntgcalls/lib/Release` (where the upstream zip puts the DLL). Env overrides keep priority. A missing `ntgcalls.dll` is handled as on other platforms: the call engine reports `LoadError::LibraryMissing` and calls are unavailable.

## Native libraries

- TDLib: built from the pinned commit with the reviewed export patch by `scripts/build-tdlib-windows.ps1`, following TDLib's own Windows instructions (MSVC, vcpkg `openssl`/`zlib`/`gperf`, `x64-windows` dynamic triplet). Only the `tdjson` target is built. No LTO (MSVC `/LTCG` on tdjson roughly doubles an already long link). OpenSSL 3 and zlib come from vcpkg's preinstalled copy on the runner and are bundled, same policy as the macOS and Linux packages (we know which OpenSSL TDLib runs against; costs a new package for OpenSSL security fixes).
- rlottie: pinned commit (same as `build-rlottie.sh`), `scripts/build-rlottie-windows.ps1`, MSVC with `CMAKE_WINDOWS_EXPORT_ALL_SYMBOLS` so the C API is exported.
- ntgcalls: upstream publishes `ntgcalls.windows-x86_64-shared_libs.zip` in the same v3.0.0 release; `scripts/vendor-ntgcalls.sh` now handles Git Bash on Windows with the SHA-256 pinned (`454ee228...`). Calls are therefore available on Windows in principle, but nothing in CI exercises them.

## Policy: VC++ runtime is bundled app-locally

`vcruntime140*.dll` / `msvcp140*.dll` are copied from the MSVC redistributable directory into the package (the packager computes the import closure with `dumpbin /dependents`). Reasons: Rust's MSVC target, tdjson and rlottie all link the dynamic VC runtime; shipping them removes the "install the VC++ redistributable first" step on a clean machine, and app-local deployment of these DLLs is permitted by the redistribution terms. Cost: ~1 MB and no automatic runtime servicing by Windows Update for these copies. The Universal CRT (`ucrtbase.dll`, `api-ms-win-crt-*`) is an OS component on Windows 10+ and is not bundled; Windows 10 is the minimum anyway.

Not bundled (system-provided): everything in System32 (kernel32, user32, d3d11, dxgi, dwmapi, ...), the GPU driver stack, and the audio stack. The GPUI Windows backend uses DirectX 11.

## Subsystem and CLI output

Release UI builds are `windows_subsystem = "windows"` so starting Quill does not open a console window. A GUI-subsystem process has no standard handles when launched from a terminal, so `main` attaches to the parent console and points stdout/stderr at `CONOUT$` only when those handles are missing (redirected handles are left alone). CI reads `--version`/`--build-info` through a redirected file, so it does not exercise the console-attach path itself; debug builds stay console-subsystem.

## Icon and line endings

`build.rs` (Windows hosts only; a no-op elsewhere) writes a one-line `.rc` and embeds `assets/icons/Quill.ico` as `quill.exe`'s application icon via the `embed-resource` build-dependency (already in the lockfile through GPUI). `.gitattributes` marks `schema/**` and `native/patches/**` as `-text` and forces LF for `*.sh`/`*.py`, because a CRLF checkout on Windows broke the byte-exact vendored schema test and would break the TDLib patch and Git Bash scripts.

## Checker: `scripts/check-bundle-pe.ps1 <dir>`

For every `.exe`/`.dll` in the package, `dumpbin /dependents` (static and delay-load imports) must resolve to a file in the package, an `api-ms-win-*`/`ext-ms-win-*` API set, or a DLL that exists in `%SystemRoot%\System32` and is not on a short denylist of things that exist on developer machines but not on clean installs (`vcruntime*`, `msvcp*`, `concrt*`, `vcomp*`, `libssl*`, `libcrypto*`, `zlib*`, `vulkan-1`). The job also runs a negative test (removing `libcrypto-3-x64.dll` must make the checker fail). `scripts/check-bundle-load.ps1` loads `tdjson.dll`/`ntgcalls.dll`/`rlottie.dll` from the extracted zip and asserts the OpenSSL/zlib modules mapped into the process came from the package.

## CI (all non-required; required checks `linux-fmt-clippy-test` and `macos-ui-build` are unchanged)

- `windows-build` (windows-latest): `cargo build --release --features ui --locked`, then `cargo test --no-default-features --locked` (the same core + replay set Linux runs). Uploads `quill.exe`.
- `windows-native` and `windows-rlottie` (windows-latest, MSVC dev environment via `ilammy/msvc-dev-cmd`, separate jobs so they run in parallel): TDLib and rlottie restored/saved with `actions/cache` (TDLib keyed on `src/pins.rs`, the build scripts and `native/patches/*`), ntgcalls vendored with its pinned checksum. Runs in parallel with `windows-build`; uploads the DLL set.
- `windows-package`: downloads both, packages, runs the checker (plus negative test), extracts the zip, runs `quill.exe --version` and `--build-info` from the extracted copy, the load check, and uploads `quill-windows-x86_64`.

## Verified vs not

Verified in CI: the UI release build links on `windows-latest`; core tests pass on Windows; TDLib/rlottie build with MSVC; package layout; no unresolved or foreign PE imports; the bundled DLLs load and use the bundled OpenSSL; `--version`/`--build-info` run from the extracted zip.

Not verified (no GPU, display, audio device or Telegram account on the runner): window creation and DirectX rendering, tray icon, clipboard and drag-drop, call audio (WASAPI) and ntgcalls media, a live TDLib login, the console-attach path, behaviour on a clean machine without the runner's installed tooling, DPI scaling, IME.

## Known gaps

- Secret storage: `FileSecretStore` writes `db-encryption.key` with default ACLs (the 0600 hardening is Unix-only). A DPAPI (`CryptProtectData`) or Credential Manager backend is the right fix.
- Self-update is Unix-only (`update_install.rs`); the Windows updater needs a rename-on-restart dance and must also refresh the DLLs.
- No installer/MSIX/code signing: SmartScreen will warn on the unsigned zip.
- macOS-only features compile out with fallbacks: AVPlayer inline video (stub), picture-in-picture, QR scanner, NSSpellChecker spellcheck, native editor emoji raster.
- aarch64 Windows is not built.
