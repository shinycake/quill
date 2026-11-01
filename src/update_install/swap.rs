//! Installing a staged package over the running one, with rollback.
//!
//! Linux swaps the whole install directory with one atomic
//! `renameat2(RENAME_EXCHANGE)`, macOS the app bundle with
//! `renamex_np(RENAME_SWAP)`, so the install path always holds a complete
//! tree. Filesystems without it get two plain renames, and
//! [`recover_or_clear_backup`] brings an interrupted swap back before the next
//! update. Windows cannot replace a running `quill.exe` or a loaded DLL, but it
//! can rename them, so each file is renamed aside to `*.old`, the new one moved
//! in, and the leftovers (listed in [`LEFTOVERS_FILE`]) are deleted on the next
//! start ([`remove_leftovers`]), as tdesktop's updater does with its own `.old`
//! files.
#![deny(clippy::indexing_slicing)]
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DEPTH: usize = 16;

/// Suffix of files renamed aside during an update.
pub const ASIDE_SUFFIX: &str = ".old";

/// Prefix of every temporary entry an update creates: the staged package,
/// the downloaded archive, the handoff plan and a failed tree on its way out.
pub const TEMP_PREFIX: &str = ".quill-update-";

/// Lists, inside the install directory, the `*.old` files a Windows update
/// left behind. Only files named here are ever deleted at startup.
pub const LEFTOVERS_FILE: &str = ".quill-leftovers";
const MAX_LEFTOVERS_BYTES: u64 = 1024 * 1024;

fn combine(primary: io::Error, rollback: io::Error) -> io::Error {
    io::Error::new(
        primary.kind(),
        format!("{primary}; restoring the previous version also failed: {rollback}"),
    )
}

// Linux: the install directory as a whole.

enum Exchange {
    // Linux and macOS have the exchange; elsewhere the plain renames always
    // run.
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    Done,
    Unsupported,
}

type ExchangeFn = fn(&Path, &Path) -> io::Result<Exchange>;

/// Atomically swaps the two paths (both must exist).
#[cfg(target_os = "linux")]
fn exchange(a: &Path, b: &Path) -> io::Result<Exchange> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let invalid = |_| io::Error::other("Invalid path");
    let a = CString::new(a.as_os_str().as_bytes()).map_err(invalid)?;
    let b = CString::new(b.as_os_str().as_bytes()).map_err(invalid)?;
    // SAFETY: both pointers are NUL-terminated strings owned above that
    // outlive the call; renameat2 only reads them. AT_FDCWD resolves relative
    // paths like rename(2). The raw syscall avoids depending on glibc 2.28's
    // wrapper.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if rc == 0 {
        return Ok(Exchange::Done);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        // Filesystem or kernel without RENAME_EXCHANGE, a sandbox that blocks
        // the syscall, or two volumes (the plain renames then fail safely).
        Some(libc::EINVAL | libc::ENOSYS | libc::EXDEV | libc::EPERM) => Ok(Exchange::Unsupported),
        _ => Err(error),
    }
}

/// Atomically swaps the two paths (both must exist) on APFS and HFS+.
#[cfg(target_os = "macos")]
fn exchange(a: &Path, b: &Path) -> io::Result<Exchange> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let invalid = |_| io::Error::other("Invalid path");
    let a = CString::new(a.as_os_str().as_bytes()).map_err(invalid)?;
    let b = CString::new(b.as_os_str().as_bytes()).map_err(invalid)?;
    // SAFETY: both pointers are NUL-terminated strings owned above that
    // outlive the call; renamex_np only reads them.
    let rc = unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) };
    if rc == 0 {
        return Ok(Exchange::Done);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        // A file system without RENAME_SWAP (a network share), two volumes,
        // or a sandbox: the plain renames then fail safely.
        Some(libc::ENOTSUP | libc::EINVAL | libc::EXDEV | libc::EPERM) => Ok(Exchange::Unsupported),
        _ => Err(error),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn exchange(_: &Path, _: &Path) -> io::Result<Exchange> {
    Ok(Exchange::Unsupported)
}

/// A fresh `.quill-update-<tag>-…` path beside `path`.
fn unique_sibling(path: &Path, tag: &str) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Invalid install location"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    for n in 0..1000u32 {
        let candidate = parent.join(format!("{TEMP_PREFIX}{tag}-{pid}-{nanos}-{n}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }
    Err(io::Error::other("No free temporary name for the update"))
}

/// Replaces the directory `install` with `staged` (a sibling on the same
/// volume) and runs `launch`. The old tree stays at `backup` afterwards; the
/// new version removes it once it has started ([`recover_or_clear_backup`]).
/// Any failure puts the old directory back, and `install` is never left
/// missing by a failed rollback.
pub fn swap_dir(
    install: &Path,
    staged: &Path,
    backup: &Path,
    launch: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    swap_dir_with(install, staged, backup, launch, exchange)
}

fn swap_dir_with(
    install: &Path,
    staged: &Path,
    backup: &Path,
    launch: impl FnOnce() -> io::Result<()>,
    exchange: ExchangeFn,
) -> io::Result<()> {
    if fs::symlink_metadata(backup).is_ok() {
        return Err(io::Error::other("Previous update backup still exists"));
    }
    let atomic = put_in_place(install, staged, backup, exchange)?;
    if let Err(error) = launch() {
        return Err(match restore_dir(install, backup, atomic, exchange) {
            Ok(()) => error,
            Err(rollback) => combine(error, rollback),
        });
    }
    Ok(())
}

/// Moves `staged` to `install` and the old tree to `backup`. Returns whether
/// the atomic exchange did it.
fn put_in_place(
    install: &Path,
    staged: &Path,
    backup: &Path,
    exchange: ExchangeFn,
) -> io::Result<bool> {
    if let Exchange::Done = exchange(staged, install)? {
        // `staged` now holds the old tree; give it the backup name.
        if let Err(error) = fs::rename(staged, backup) {
            // Swap back. `install` holds a complete tree either way.
            return Err(match exchange(staged, install) {
                Ok(Exchange::Done) => error,
                Ok(Exchange::Unsupported) => {
                    combine(error, io::Error::other("Atomic exchange stopped working"))
                }
                Err(undo) => combine(error, undo),
            });
        }
        return Ok(true);
    }
    fs::rename(install, backup)?;
    if let Err(error) = fs::rename(staged, install) {
        return Err(match fs::rename(backup, install) {
            Ok(()) => error,
            Err(undo) => combine(error, undo),
        });
    }
    Ok(false)
}

/// Puts the old tree from `backup` back at `install` and deletes the new one.
fn restore_dir(
    install: &Path,
    backup: &Path,
    atomic: bool,
    exchange: ExchangeFn,
) -> io::Result<()> {
    let failed = unique_sibling(install, "failed")?;
    if atomic && let Exchange::Done = exchange(backup, install)? {
        // `backup` now holds the failed tree.
        if fs::rename(backup, &failed).is_ok() {
            let _ = fs::remove_dir_all(&failed);
        } else {
            let _ = fs::remove_dir_all(backup);
        }
        return Ok(());
    }
    fs::rename(install, &failed)?;
    if let Err(error) = fs::rename(backup, install) {
        // Better the new version at the install path than nothing.
        return Err(match fs::rename(&failed, install) {
            Ok(()) => error,
            Err(undo) => combine(error, undo),
        });
    }
    let _ = fs::remove_dir_all(&failed);
    Ok(())
}

pub(super) fn remove_any(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        // remove_dir_all never follows links; a link or file goes as itself.
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Settles what an earlier update left at `backup`. When `install_exe` (the
/// executable inside the install directory) is present, the install is
/// complete and the backup is stale: it is deleted, so it can never block the
/// next update. When the install directory is gone (a two-rename swap was
/// interrupted between its renames), the backup is moved back.
pub fn recover_or_clear_backup(
    install: &Path,
    install_exe: &Path,
    backup: &Path,
) -> io::Result<()> {
    if fs::symlink_metadata(backup).is_err() {
        return Ok(());
    }
    if fs::symlink_metadata(install_exe).is_ok_and(|m| m.is_file()) {
        return remove_any(backup);
    }
    if fs::symlink_metadata(install).is_err() {
        return fs::rename(backup, install);
    }
    Err(io::Error::other(
        "Update backup and an incomplete install both exist",
    ))
}

/// Deletes the `.quill-update-*` entries in `dir` (staged packages, archives,
/// plans, failed trees) that an interrupted update left behind. Call only
/// when no update is in progress. Returns the number removed.
pub fn clear_temp_entries(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(TEMP_PREFIX))
        .filter(|entry| remove_any(&entry.path()).is_ok())
        .count()
}

// Windows: file by file inside the install directory.

/// Every regular file under `root`, relative to it. Links are refused.
pub(super) fn list_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> io::Result<()> {
        if depth > MAX_DEPTH {
            return Err(io::Error::other("Update package is nested too deeply"));
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                walk(root, &path, depth + 1, out)?;
            } else if kind.is_file() {
                if let Ok(relative) = path.strip_prefix(root) {
                    out.push(relative.to_path_buf());
                }
            } else {
                return Err(io::Error::other("Update package contains a link"));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, 0, &mut out)?;
    out.sort();
    Ok(out)
}

/// Runs `op` again on a transient Windows lock for about 4 s.
pub(super) fn retry<T>(op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    crate::fs_retry::retry(crate::fs_retry::LONG, op)
}

fn file_name_of(path: &Path) -> io::Result<String> {
    Ok(path
        .file_name()
        .ok_or_else(|| io::Error::other("Invalid file name"))?
        .to_string_lossy()
        .into_owned())
}

/// `quill.exe` -> `quill.exe.old`, or `quill.exe.1.old` when a previous
/// leftover is still locked.
fn aside_name(dest: &Path) -> io::Result<PathBuf> {
    let name = file_name_of(dest)?;
    for n in 0..100 {
        let candidate = if n == 0 {
            format!("{name}{ASIDE_SUFFIX}")
        } else {
            format!("{name}.{n}{ASIDE_SUFFIX}")
        };
        let path = dest.with_file_name(candidate);
        if fs::symlink_metadata(&path).is_ok() {
            // Try to clear a stale leftover; if it is still locked, pick the next name.
            if fs::remove_file(&path).is_err() {
                continue;
            }
        }
        return Ok(path);
    }
    Err(io::Error::other("Too many update leftovers"))
}

/// Where rollback parks the new file while the old one goes back: a name no
/// file has (unlike [`aside_name`], it never deletes anything).
fn parking_name(dest: &Path) -> io::Result<PathBuf> {
    let name = file_name_of(dest)?;
    for n in 0..100 {
        let path = dest.with_file_name(format!("{name}.rollback-{n}{ASIDE_SUFFIX}"));
        if fs::symlink_metadata(&path).is_err() {
            return Ok(path);
        }
    }
    Err(io::Error::other("Too many update leftovers"))
}

struct Moved {
    dest: PathBuf,
    aside: Option<PathBuf>,
}

/// Puts one file back. The new file is renamed out of the way first, so
/// `dest` is never left empty: if the old file cannot return, the new one
/// does. Parked files that cannot be deleted go to `leftovers`.
fn restore_file(item: &Moved, leftovers: &mut Vec<PathBuf>) -> io::Result<()> {
    let present = fs::symlink_metadata(&item.dest).is_ok();
    let Some(aside) = &item.aside else {
        // Nothing was there before: the new file just goes.
        return if present {
            retry(|| fs::remove_file(&item.dest))
        } else {
            Ok(())
        };
    };
    let parked = if present {
        let parked = parking_name(&item.dest)?;
        retry(|| fs::rename(&item.dest, &parked))?;
        Some(parked)
    } else {
        None
    };
    if let Err(error) = retry(|| fs::rename(aside, &item.dest)) {
        if let Some(parked) = &parked {
            let _ = retry(|| fs::rename(parked, &item.dest));
        }
        return Err(error);
    }
    if let Some(parked) = parked
        && retry(|| fs::remove_file(&parked)).is_err()
    {
        leftovers.push(parked);
    }
    Ok(())
}

/// Restores every moved file, newest first. Returns the first error and the
/// parked files that could not be deleted.
fn rollback(moved: Vec<Moved>) -> (Option<io::Error>, Vec<PathBuf>) {
    let mut first_error = None;
    let mut leftovers = Vec::new();
    for item in moved.into_iter().rev() {
        if let Err(error) = restore_file(&item, &mut leftovers) {
            first_error.get_or_insert(error);
        }
    }
    (first_error, leftovers)
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    retry(|| fs::rename(from, to)).or_else(|_| {
        retry(|| fs::copy(from, to))?;
        // The staged copy is deleted with the staging directory anyway.
        let _ = fs::remove_file(from);
        Ok(())
    })
}

fn install_files(
    install: &Path,
    staged: &Path,
    files: &[PathBuf],
    moved: &mut Vec<Moved>,
) -> io::Result<()> {
    for relative in files {
        let dest = install.join(relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let aside = if fs::symlink_metadata(&dest).is_ok() {
            let aside = aside_name(&dest)?;
            retry(|| fs::rename(&dest, &aside))?;
            Some(aside)
        } else {
            None
        };
        moved.push(Moved {
            dest: dest.clone(),
            aside,
        });
        move_file(&staged.join(relative), &dest)?;
    }
    Ok(())
}

/// Installs every file of `staged` into `install`: existing files are renamed
/// aside (allowed while they run or are loaded), the new ones moved in, with
/// retries while another process briefly holds a file. If `launch` or any
/// step fails, everything is put back.
pub fn swap_files(
    install: &Path,
    staged: &Path,
    launch: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let files = list_files(staged)?;
    let mut moved: Vec<Moved> = Vec::new();
    let result = install_files(install, staged, &files, &mut moved).and_then(|()| {
        // Listed before the new version starts, so its first start clears them.
        let asides: Vec<PathBuf> = moved.iter().filter_map(|m| m.aside.clone()).collect();
        record_leftovers(install, &asides)?;
        launch()
    });
    match result {
        Ok(()) => {
            let _ = fs::remove_dir_all(staged);
            Ok(())
        }
        Err(error) => {
            let (rollback_error, leftovers) = rollback(moved);
            // Entries for asides that went back no longer exist and are
            // dropped at the next start.
            if !leftovers.is_empty() {
                let _ = record_leftovers(install, &leftovers);
            }
            Err(match rollback_error {
                Some(rollback) => combine(error, rollback),
                None => error,
            })
        }
    }
}

/// `licenses/a.txt.old` from a marker line; `None` for anything that is not a
/// plain relative path to a `*.old` file.
fn leftover_from_line(line: &str) -> Option<PathBuf> {
    if !line.ends_with(ASIDE_SUFFIX) {
        return None;
    }
    let mut path = PathBuf::new();
    for part in line.split('/') {
        let plain = !part.is_empty() && part != "." && part != ".." && !part.contains(['\\', ':']);
        if !plain {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

fn line_for_leftover(relative: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        parts.push(name.to_str()?.to_string());
    }
    let line = parts.join("/");
    leftover_from_line(&line).map(|_| line)
}

fn read_leftovers(install: &Path) -> Vec<PathBuf> {
    use std::io::Read;
    let mut text = String::new();
    let read = fs::File::open(install.join(LEFTOVERS_FILE))
        .and_then(|file| file.take(MAX_LEFTOVERS_BYTES).read_to_string(&mut text));
    if read.is_err() {
        return Vec::new();
    }
    text.lines().filter_map(leftover_from_line).collect()
}

fn write_leftovers(install: &Path, relative: &[PathBuf]) -> io::Result<()> {
    let marker = install.join(LEFTOVERS_FILE);
    if relative.is_empty() {
        return match fs::remove_file(marker) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let mut text = String::new();
    for line in relative.iter().filter_map(|path| line_for_leftover(path)) {
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(marker, text)
}

/// Adds `paths` (absolute, inside `install`) to the leftovers list.
fn record_leftovers(install: &Path, paths: &[PathBuf]) -> io::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut listed = read_leftovers(install);
    for path in paths {
        if let Ok(relative) = path.strip_prefix(install)
            && !listed.iter().any(|known| known == relative)
        {
            listed.push(relative.to_path_buf());
        }
    }
    write_leftovers(install, &listed)
}

/// Deletes the `*.old` files a Windows update listed in [`LEFTOVERS_FILE`]
/// (they could not go while the previous version still ran). Nothing else is
/// touched, so a folder that was never updated keeps its own `.old` files.
/// Files still locked stay listed for the next start. Returns the number
/// removed.
pub fn remove_leftovers(install: &Path) -> usize {
    let listed = read_leftovers(install);
    if listed.is_empty() {
        let _ = fs::remove_file(install.join(LEFTOVERS_FILE));
        return 0;
    }
    let mut removed = 0;
    let mut remaining = Vec::new();
    for relative in listed {
        let path = install.join(&relative);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => {
                if fs::remove_file(&path).is_ok() {
                    removed += 1;
                } else {
                    remaining.push(relative);
                }
            }
            // Gone already (or not a plain file): drop it from the list.
            _ => (),
        }
    }
    let _ = write_leftovers(install, &remaining);
    removed
}

// Shared probes.

/// Whether a file can be created in `dir` (no elevation prompt is ever raised).
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".quill-write-probe-{}", std::process::id()));
    let created = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if created {
        let _ = fs::remove_file(probe);
    }
    created
}

/// Package-manager or sandbox locations Quill must not rewrite.
pub fn distro_managed(path: &Path) -> bool {
    const PREFIXES: [&str; 7] = [
        "/usr",
        "/bin",
        "/sbin",
        "/nix",
        "/snap",
        "/app",
        "/var/lib/flatpak",
    ];
    // Windows package managers: winget's portable installs
    // (%LOCALAPPDATA%\Microsoft\WinGet\Packages) and Scoop (user or global
    // `scoop\apps`). Compared on a lowercased, slash-normalized path.
    const WINDOWS_MANAGED: [&str; 2] = ["/microsoft/winget/packages/", "/scoop/apps/"];
    let normalized = format!("{}/", path.to_string_lossy())
        .replace('\\', "/")
        .to_lowercase();
    PREFIXES.iter().any(|prefix| path.starts_with(prefix))
        // AppImage runtime mounts are read-only squashfs images.
        || path.to_string_lossy().starts_with("/tmp/.mount_")
        || WINDOWS_MANAGED.iter().any(|part| normalized.contains(part))
}

#[cfg(test)]
mod tests;
