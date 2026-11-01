//! Where an install keeps its update files, and which of them count.
//!
//! The trust rules: plans, claims and receipts decide what a start deletes
//! and whether it exits, so only files this user wrote count, in a folder
//! nobody else can plant files in. The layout: the install directory or
//! bundle, the plans folder and the stage folder on the same volume.
#![deny(clippy::indexing_slicing)]

use super::is_temp_entry;
#[cfg(target_os = "macos")]
use super::macos;
#[cfg(not(target_os = "macos"))]
use super::{BUNDLE_UPDATES, PACKAGE_EXE, PACKAGE_SENTINEL};
use std::fs;
use std::path::{Path, PathBuf};

// Trust in update files. Plans, claims and receipts decide what a start
// deletes and whether it exits, so only files this user wrote count: owned
// by the effective user and writable by nobody else. The folder holding
// them must be this user's too and closed to everyone else's writes (group
// write stays allowed on a directory, as Linux private-group umasks make it
// the norm). Windows has no such check: the install folder is the user's
// own.

#[cfg(unix)]
pub(super) fn effective_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments, cannot fail and touches no memory.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
pub(super) fn trusted_file_as(meta: &fs::Metadata, uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.is_file() && meta.uid() == uid && meta.mode() & 0o022 == 0
}

#[cfg(unix)]
pub(super) fn trusted_dir_as(meta: &fs::Metadata, uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.is_dir() && meta.uid() == uid && meta.mode() & 0o002 == 0
}

/// Whether the update file at `path` (a plan, claim or receipt, not followed
/// through a link) was written by this user and nobody else can change it.
pub(super) fn trusted_file(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        trusted_file_as(&meta, effective_uid())
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Whether the folder at `path` belongs to this user and nobody else can
/// plant files in it.
pub(super) fn trusted_dir(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        trusted_dir_as(&meta, effective_uid())
    }
    #[cfg(not(unix))]
    {
        meta.is_dir()
    }
}

/// Whether this user owns the entry at `path` (the install itself: a
/// backup owned by someone else could never be cleared).
pub(super) fn owned_by_me(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::symlink_metadata(path).is_ok_and(|meta| meta.uid() == effective_uid())
    }
    #[cfg(not(unix))]
    {
        fs::symlink_metadata(path).is_ok()
    }
}

/// Creates the private update folder (macOS) with no access for anyone
/// else, or accepts an existing one that is still this user's own.
pub(super) fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(dir).is_ok() {
        return if trusted_dir(dir) {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "Update folder belongs to another user",
            ))
        };
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(dir)
    }
    #[cfg(not(unix))]
    fs::DirBuilder::new().create(dir)
}

/// Where an install keeps its pieces.
pub(super) struct Layout {
    /// What an update replaces: the package directory (Linux, Windows) or
    /// the app bundle (macOS).
    pub(super) install: PathBuf,
    /// Plans, claims and receipts: inside the install directory, or beside
    /// the bundle on macOS, whose seal an added file would break.
    pub(super) plans: PathBuf,
    /// Where a staged package waits, on the same volume: beside the install
    /// directory (Linux and macOS exchange the two) or inside it (Windows
    /// moves files).
    pub(super) stage_root: PathBuf,
}

impl Layout {
    /// The layout of the package `exe` belongs to; `None` when `exe` is not
    /// Quill's package executable (a plain cargo build, a renamed binary, a
    /// platform without a package).
    pub(super) fn of(exe: &Path) -> Option<Layout> {
        #[cfg(target_os = "macos")]
        {
            // A private folder beside the bundle, on the same volume: the
            // bundle's own folder (Applications) is shared with every admin,
            // and a file inside the bundle would break its seal.
            let install = macos::bundle_of(exe)?;
            let private = install
                .parent()?
                .join(format!(".quill-updates-{}", effective_uid()));
            Some(Layout {
                install,
                plans: private.clone(),
                stage_root: private,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            if !BUNDLE_UPDATES || !packaged(exe) {
                return None;
            }
            let install = exe.parent()?.to_path_buf();
            let stage_root = if cfg!(windows) {
                install.clone()
            } else {
                install.parent()?.to_path_buf()
            };
            Some(Layout {
                plans: install.clone(),
                install,
                stage_root,
            })
        }
    }

    /// Where the previous install waits after a swap until the new version's
    /// first start: beside the install directory on Linux, in the private
    /// update folder on macOS.
    pub(super) fn backup(&self) -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            let name = self.install.file_name()?.to_string_lossy();
            Some(self.stage_root.join(format!(".{name}.update-backup")))
        } else {
            backup_for(&self.install)
        }
    }
}

/// Where the plans, claims and receipts of the install `exe` belongs to
/// live, and where its staged packages go. A bare executable (a dev build)
/// keeps both beside itself.
pub(super) fn update_dirs(exe: &Path) -> Option<(PathBuf, PathBuf)> {
    Layout::of(exe)
        .map(|layout| (layout.plans, layout.stage_root))
        .or_else(|| {
            let beside = exe.parent()?.to_path_buf();
            Some((beside.clone(), beside))
        })
}

pub(super) fn plans_dir(exe: &Path) -> Option<PathBuf> {
    update_dirs(exe).map(|(plans, _)| plans)
}

/// Whether `staged` is a staged package of the install `exe` belongs to:
/// a temp-prefixed entry right inside its stage folder. Only such an entry
/// is ever deleted on a path read from a plan.
pub(super) fn staged_for(staged: &Path, exe: &Path) -> bool {
    is_temp_entry(staged)
        && update_dirs(exe).is_some_and(|(_, stage_root)| staged.parent() == Some(&*stage_root))
}

pub(super) fn receipt_for(exe: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(layout) = Layout::of(exe) {
        return layout.plans.join("last-update.json");
    }
    exe.with_extension("last-update.json")
}

pub(super) fn result_path() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|p| receipt_for(&p))
}

/// The package executable at `exe` sits next to the package's TDLib.
#[cfg(not(target_os = "macos"))]
pub(super) fn packaged(exe: &Path) -> bool {
    exe.file_name().is_some_and(|n| n == PACKAGE_EXE)
        && exe
            .parent()
            .is_some_and(|install| install.join(PACKAGE_SENTINEL).is_file())
}

/// The executable inside a staged package.
pub(super) fn package_exe(staged: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        macos::executable(staged)
    }
    #[cfg(not(target_os = "macos"))]
    {
        staged.join(PACKAGE_EXE)
    }
}

/// Where a swap keeps the previous install directory or bundle:
/// `.quill.update-backup` (`.Quill.app.update-backup`) beside it.
pub(super) fn backup_for(install: &Path) -> Option<PathBuf> {
    let name = install.file_name()?.to_string_lossy();
    Some(install.with_file_name(format!(".{name}.update-backup")))
}
