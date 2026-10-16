# In-app updater on Linux and Windows

## Problem

`src/update_install.rs` downloaded one bare executable and swapped it. On Linux the
bundled `lib/` (TDLib, ntgcalls, rlottie, quillvideo + FFmpeg, OpenSSL) never changed, so a
new executable could run against a stale `libtdjson.so` or `libntgcalls.so` (unknown TDLib
classes, a changed ntgcalls ABI). On Windows the updater did not exist at all.

## What tdesktop does

Windows: `Updater.exe` is a separate helper. The app downloads the update package, hands
it to the updater, and exits; the updater replaces files (renaming locked ones to `.old`
beside them) and relaunches the app, and leftover `.old` files go on later runs. Linux
tdesktop is updated by its package or by the same updater on the self-contained tarball.
Quill has no second binary, so the app executable itself is the helper (`--apply-update`),
as before.

## What changed

- `updater::update_asset_name()`: Linux now looks for `quill-linux-<arch>-bundle.tar.gz`,
  Windows for `quill-windows-<arch>.zip` (both are already published on every release with
  GitHub's SHA-256 digest). macOS keeps the bare executable asset. The bare
  `quill-linux-<arch>` asset is still published for older clients.
- `src/update_install/bundle.rs`: extracts the verified archive into a staging directory.
  Rejects `..`/absolute/drive paths, links and special files, more than one top-level
  directory, more than 20,000 entries or 2 GiB unpacked.
- `src/update_install/swap.rs`:
  - Linux, `swap_dir`: staging is a sibling of the install directory (same filesystem);
    rename install to backup, staged to install, relaunch. Any failure (missing stage,
    launch failure) puts the old directory back. The receipt for the new version is written
    inside the staged tree because the old directory is deleted on success.
  - Windows, `swap_files`: every file in the package is renamed aside to `name.old`
    (allowed while running or loaded), the new file moved in, then relaunch; any failure
    restores all files in reverse order. A leftover `*.old` that is still locked gets the
    next free name (`name.1.old`).
  - `remove_leftovers`: `startup_state()` deletes `*.old` files (depth 4) on Windows at
    startup, when the previous version is no longer running.
- Windows helper waits for the old process through `OpenProcess(SYNCHRONIZE)` +
  `WaitForSingleObject` (30 s), and starts detached without a console.
- `can_install_here()` (probed once): Linux/Windows self-update only when the executable
  is the package's `quill`/`quill.exe` next to `lib/libtdjson.so` / `tdjson.dll`, the
  install directory is writable (Linux: and its parent), and the path is not distro
  managed (`/usr`, `/nix`, `/snap`, `/app`, flatpak, AppImage mounts). Otherwise the
  Updates section keeps pointing at the GitHub release page. Program Files without
  elevation fails the write probe; no elevation is requested. A plain cargo build is no
  longer "installable" (a bundle swap would replace its whole directory).
- macOS behavior is unchanged.

## Limits

- Files removed from a future Windows package are not deleted from an existing install
  (only files in the new package are replaced).
- The Linux directory swap is two renames, not a single atomic exchange; a crash between
  them would leave the install at the backup name. Rollback covers every in-process
  failure.
- The staged package is verified by the archive digest at download and by running the new
  executable's `--version`/`--build-info` (which loads the new libraries) before the swap.

## Verification

Unit tests (cross-platform, temp dirs): tar.gz and zip extraction (including deflate,
backslash separators, `..` traversal, mixed roots), `swap_dir` success / launch failure /
missing stage / existing backup, `swap_files` success / rollback / locked leftover naming,
startup `*.old` cleanup, writable and managed-path probes, `installable()` layout checks
(Linux). The Windows branch compiles and its unit tests run in CI via the `cursor/` branch.
Not run here: a real Linux package update against a published release, a real Windows
running-exe rename, and the Windows process wait.
