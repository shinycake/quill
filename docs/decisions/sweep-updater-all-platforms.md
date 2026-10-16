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
  - Linux, `swap_dir`: staging is a sibling of the install directory (same filesystem).
    The staged tree and the install directory trade places with one
    `renameat2(RENAME_EXCHANGE)` (raw syscall through `libc`, so no glibc 2.28 wrapper is
    needed), then the old tree is renamed to `.<install>.update-backup` and the app
    relaunched. If launch fails, a second exchange puts the old tree back and the failed one
    goes to a unique `.quill-update-failed-<pid>-<nanos>-<n>` name before deletion (the old
    `with_extension("update-failed")` turned `quill-0.5` into `quill-0.update-failed` and
    could collide). The install path holds a complete tree at every instant, including
    when a rollback step fails. Filesystems or kernels without the exchange (`EINVAL`,
    `ENOSYS`, `EXDEV`, or `EPERM` from a seccomp filter) fall back to two plain renames;
    there a failed rollback rename puts the new tree back rather than leave the install
    path empty, and the error reports both failures instead of returning on the first `?`.
  - Linux backup lifetime: the backup is no longer deleted right after spawning the new
    version. The new version deletes it on its first start (`startup_state()`, on a
    background thread, only for a packaged install). `recover_or_clear_backup` settles a
    backup in every other place too: before staging (`download_and_stage`) and right
    before the swap (helper). If the install's executable is present the backup is stale
    and is deleted, so it can no longer block every later update. If the install directory
    is missing (a fallback swap killed between its two renames) the backup is moved back.
    The receipt for the new version is written inside the staged tree, which becomes the
    install directory.
  - Interrupted updates: before staging, `.quill-update-*` entries (staged packages,
    archives, plans, failed trees) in the install directory and the staging directory are
    deleted. One Quill runs per data directory (single-instance socket). Two instances
    with separate data directories updating the same install at the same moment can make
    one of them fail with a retry message; the install itself is never left broken.
  - Windows, `swap_files`: every file in the package is renamed aside to `name.old`
    (allowed while running or loaded), the new file moved in, then relaunch. Renames,
    moves and deletes retry up to 10 times with 100 to 500 ms pauses (about 4 s) on
    `ERROR_ACCESS_DENIED`, `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`, the
    errors an antivirus scanner or the indexer causes by holding a file for a moment. Any
    failure restores all files in reverse order. Rollback renames the new file aside to
    `name.rollback-<n>.old` first and only then brings the old file back; if that fails the
    new file is put back, so no file is ever missing. Rollback errors are returned with the
    original error. A leftover `*.old` that is still locked gets the next free name
    (`name.1.old`).
  - `remove_leftovers`: `swap_files` lists every aside file in `.quill-leftovers` inside
    the install directory before relaunching. On the next start `startup_state()` deletes
    only the files listed there (plain relative `*.old` paths; anything with `..`, a root,
    a drive or a backslash is ignored) and keeps the ones that are still locked for the
    start after. A folder that was never updated has no list, so nothing is deleted there.
    Before, every Windows start deleted any `*.old` file up to four levels below wherever
    the executable happened to be.
- Windows helper waits for the old process through `OpenProcess(SYNCHRONIZE)` +
  `WaitForSingleObject` (30 s), and starts detached without a console.
- `can_install_here()` (probed once): Linux/Windows self-update only when the executable
  is the package's `quill`/`quill.exe` next to `lib/libtdjson.so` / `tdjson.dll`, the
  install directory is writable (Linux: and its parent), and the path is not distro
  managed (`/usr`, `/nix`, `/snap`, `/app`, flatpak, AppImage mounts, winget's
  `Microsoft\WinGet\Packages`, Scoop's `scoop\apps`). Otherwise the
  Updates section keeps pointing at the GitHub release page. Program Files without
  elevation fails the write probe; no elevation is requested. A plain cargo build is no
  longer "installable" (a bundle swap would replace its whole directory).
- macOS behavior is unchanged.

## Limits

- Files removed from a future Windows package are not deleted from an existing install
  (only files in the new package are replaced).
- The Linux swap replaces the whole install directory, so files the user added inside it
  (notes, extra scripts, a hand-placed library) are not carried over. They stay in the
  backup until the new version's first start, then they are gone. Quill keeps its data in
  the profile directory, never in the install directory.
- Only on the fallback path (no `RENAME_EXCHANGE`): a kill between the two renames leaves
  the install at `.<install>.update-backup`. Nothing of Quill can start from the missing
  path to repair it, so the user renames it back; `recover_or_clear_backup` does so before
  any later update.
- "Launch succeeded" means the new process was spawned. If it then crashes at startup,
  the old tree is still in the backup for a manual restore (the backup is cleared only
  when a version gets as far as reading the update state, or by the next update).
- The staged package is verified by the archive digest at download and by running the new
  executable's `--version`/`--build-info` (which loads the new libraries) before the swap.
- The zip review point (directory entries written with backslashes, `pkg\licenses\`) did
  not reproduce: zip 9.0.3's `is_dir` already accepts a trailing backslash. The extractor
  now decides from its own normalized name, and a test covers it.

## Verification

Unit tests (temp dirs): tar.gz and zip extraction (deflate, backslash separators and
backslash directory entries, `..` traversal, mixed roots). `swap_dir` with the real exchange
and with the forced two-rename path: success that keeps the backup, launch failure under a
dotted install name next to a foreign `quill-0.update-failed`, missing stage, a rollback
rename that fails (the install path still holds a tree), existing backup. Linux only:
`renameat2` swaps two directories. Backup recovery: a stale backup beside an intact install
is cleared and the swap proceeds, a missing install comes back from its backup, an
incomplete install plus a backup is left alone. Interrupted `.quill-update-*` entries are
cleared. `swap_files`: success with the leftovers list, rollback, a rollback rename that
fails (the file is not missing), aside and parking names. Startup removes only listed
leftovers, and list entries cannot point outside the install. Writable and managed-path
probes (winget, Scoop). Windows only (`#[cfg(windows)]`, run by the windows CI job): a file
held open without `FILE_SHARE_DELETE` is swapped once the handle closes (retry), a file
held for the whole retry window rolls everything back, a held leftover stays listed until
it can go. End to end on Unix: `apply_plan` with a bundle plan installs the staged tree,
writes the receipt, clears a stale backup first and keeps the new backup; a staged
package that fails its version probe leaves the old tree, a failed receipt and no staging.
Staging first clears a stale backup and interrupted entries. `installable()` layout checks
(Linux).

Not run here: a real Linux package update against a published release, a real Windows
running-exe rename, and the Windows process wait.
