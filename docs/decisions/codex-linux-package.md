# Linux package (relocatable tarball)

## Layout

```
quill-linux-x86_64/
  quill                       RUNPATH $ORIGIN/lib
  lib/libtdjson.so            RUNPATH $ORIGIN
  lib/libntgcalls.so          RUNPATH $ORIGIN
  lib/librlottie.so           RUNPATH $ORIGIN
  lib/libssl.so.3, libcrypto.so.3   RUNPATH $ORIGIN
  share/applications/quill.desktop
  share/icons/hicolor/scalable/apps/quill.svg
  install.sh  README.txt  LICENSE  THIRD_PARTY.md
```

Shipped as `quill-linux-x86_64-bundle.tar.gz` (+ `.sha256`). Built by `scripts/linux-package.sh`, which ends by running the checker. The name `-bundle` keeps it distinct from the bare self-update binary `quill-linux-x86_64` produced by `package-update-binary.sh` (unchanged; the self-updater still swaps only the binary).

## Loader paths

Quill's loaders previously searched beside the executable and in `Frameworks`/`../Frameworks`, which is the macOS shape. This change adds `<exe dir>/lib` to the three loaders (`telegram/ffi.rs` for tdjson, `sticker_playback.rs` for rlottie, `ntgcalls-sys::Loader::load_default`). Env overrides (`QUILL_TDJSON_PATH`, `QUILL_RLOTTIE_PATH`, `QUILL_NTGCALLS_LIB`) keep priority. The `$ORIGIN` RUNPATH on every bundled library is what lets libtdjson find the bundled OpenSSL; `patchelf` sets it explicitly rather than trusting the CMake `CMAKE_INSTALL_RPATH` change in `build-tdlib.sh` (kept, harmless).

## Policy: bundle OpenSSL, not the GTK/GPU stack

- OpenSSL 3 (`libssl.so.3`, `libcrypto.so.3`) is bundled, resolved via `ldd` from the shipped libs. Reason: it is the same policy as the macOS bundle, it removes dependence on the distro's OpenSSL build and sonames (most current distros ship 3.x, but older LTS releases and some derivatives do not), and TDLib is security-sensitive enough that we want to know which OpenSSL it runs against. Cost: ~5 MB, and OpenSSL security updates now require a new package (same caveat as macOS; the self-updater does not refresh `lib/`).
- zlib (`libz.so.1`), libstdc++/libgcc_s and glibc are system libraries: universal, ABI-stable.
- The GPUI/tray stack (Vulkan loader, X11/xcb/xkbcommon/Wayland, fontconfig, freetype, GTK3, ALSA) is system-provided. These talk to GPU drivers, the compositor and the session, so bundling them would be wrong or impossible.
- The build host is `ubuntu-22.04` (glibc 2.35) to keep the glibc floor low; the package runs on distros with glibc >= 2.35.
- AppImage and `.deb` are not done. An AppImage would need `linuxdeploy` and a decision about bundling GTK; the tarball + `install.sh` is the minimal honest artifact. Follow-up.

## Checker: `scripts/check-bundle-elf.sh <dir>`

For every ELF in the package: parses `readelf -d`; every `RUNPATH`/`RPATH` entry must be `$ORIGIN`-relative and resolve to a directory inside the package; every `NEEDED` must be present in one of the file's resolved search dirs or match the system allowlist (glibc family, libstdc++, libgcc_s, zlib, X11/xcb/xkbcommon/Wayland, Vulkan/EGL/GL, fontconfig, freetype, ALSA, dbus, GTK3 and its GLib/cairo/pango/atk deps, appindicator). `libssl`/`libcrypto` are deliberately not allowlisted, so an un-bundled OpenSSL fails. On a Linux host it additionally runs `ldd` with `LD_LIBRARY_PATH` unset and fails on `not found`. `scripts/check-bundle-dlopen.py` goes further: it `dlopen`s tdjson/ntgcalls/rlottie and asserts via `/proc/self/maps` that `libssl.so.3`/`libcrypto.so.3` were mapped from the package `lib/`, not the host.

## CI: `linux-package` job (`.github/workflows/ci.yml`)

Separate from `linux-fmt-clippy-test` (unchanged; still the required check). Not required: it needs a cold ~20 min TDLib build and builds the whole UI. Steps: apt deps, TDLib and rlottie builds restored/saved with `actions/cache` (TDLib keyed on `src/pins.rs`, `build-tdlib.sh`, `native/patches/*`), `vendor-ntgcalls.sh` (SHA-256 pinned), `cargo build --release --features ui --locked`, `linux-package.sh`, the ELF checker, `quill --version` / `--build-info` from the package with `LD_LIBRARY_PATH` unset, the dlopen check, extraction of the tarball and an `install.sh` run into a temp prefix, then upload of `quill-linux-x86_64-bundle`.

## Verified vs not

Verified in CI: the UI release build links on Ubuntu; package layout; RUNPATHs; no unresolved or foreign NEEDED; the bundled libs load and use the bundled OpenSSL; `--version`/`--build-info` run from the extracted tarball; install script relocation.

Not verified (no GPU, display or Telegram account on the runner): window creation under X11 or Wayland, Vulkan rendering, tray icon behavior, call audio (ALSA), a live TDLib login, and other distros/glibc versions. `.desktop` `StartupWMClass` and the icon are placeholders: `assets/quill.svg` is a simple stand-in until a real app icon exists (there is none for macOS either).

## Follow-ups

- Windows: same shape (`quill.exe` + DLLs in one directory, `dumpbin /dependents` checker); `vendor-ntgcalls.sh` and `package-update-binary.sh` need Windows support.
- AppImage/.deb; aarch64 (ntgcalls vendor script is x86_64-only on Linux).
- Self-update of `lib/` (OpenSSL security updates).
