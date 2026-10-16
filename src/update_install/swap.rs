//! Installing a staged package over the running one, with rollback.
//!
//! Linux swaps the whole install directory (two renames). Windows cannot
//! replace a running `quill.exe` or a loaded DLL, but it can rename them, so
//! each file is renamed aside to `*.old`, the new one moved in, and the leftovers
//! are deleted on the next start ([`remove_leftovers`]), as tdesktop's updater
//! does with its own `.old` files.
#![deny(clippy::indexing_slicing)]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 16;

/// Suffix of files renamed aside during an update.
pub const ASIDE_SUFFIX: &str = ".old";

/// Replaces the directory `install` with `staged` (a sibling on the same
/// volume), keeping the old one at `backup` until `launch` succeeds. Any
/// failure puts the old directory back.
pub fn swap_dir(
    install: &Path,
    staged: &Path,
    backup: &Path,
    launch: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if fs::symlink_metadata(backup).is_ok() {
        return Err(io::Error::other("Previous update backup still exists"));
    }
    fs::rename(install, backup)?;
    if let Err(error) = fs::rename(staged, install) {
        fs::rename(backup, install)?;
        return Err(error);
    }
    if let Err(error) = launch() {
        let failed = install.with_extension("update-failed");
        fs::rename(install, &failed)?;
        fs::rename(backup, install)?;
        let _ = fs::remove_dir_all(failed);
        return Err(error);
    }
    let _ = fs::remove_dir_all(backup);
    Ok(())
}

/// Every regular file under `root`, relative to it. Links are refused.
fn list_files(root: &Path) -> io::Result<Vec<PathBuf>> {
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

/// `quill.exe` -> `quill.exe.old`, or `quill.exe.1.old` when a previous
/// leftover is still locked.
fn aside_name(dest: &Path) -> io::Result<PathBuf> {
    let name = dest
        .file_name()
        .ok_or_else(|| io::Error::other("Invalid file name"))?
        .to_string_lossy()
        .into_owned();
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

struct Moved {
    dest: PathBuf,
    aside: Option<PathBuf>,
}

fn rollback(moved: Vec<Moved>) {
    for item in moved.into_iter().rev() {
        let _ = fs::remove_file(&item.dest);
        if let Some(aside) = item.aside {
            let _ = fs::rename(aside, &item.dest);
        }
    }
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to).or_else(|_| {
        fs::copy(from, to)?;
        fs::remove_file(from)
    })
}

/// Installs every file of `staged` into `install`: existing files are renamed
/// aside (allowed while they run or are loaded), the new ones moved in. If
/// `launch` or any step fails, everything is put back.
pub fn swap_files(
    install: &Path,
    staged: &Path,
    launch: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let files = list_files(staged)?;
    let mut moved: Vec<Moved> = Vec::new();
    let result = (|| {
        for relative in &files {
            let dest = install.join(relative);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            let aside = if fs::symlink_metadata(&dest).is_ok() {
                let aside = aside_name(&dest)?;
                fs::rename(&dest, &aside)?;
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
        launch()
    })();
    match result {
        Ok(()) => {
            let _ = fs::remove_dir_all(staged);
            Ok(())
        }
        Err(error) => {
            rollback(moved);
            Err(error)
        }
    }
}

/// Deletes `*.old` files left by [`swap_files`] (they could not be removed
/// while the previous version still ran). Best effort; returns the number
/// removed. Bounded depth, never follows links.
pub fn remove_leftovers(install: &Path) -> usize {
    fn walk(dir: &Path, depth: usize) -> usize {
        if depth > 4 {
            return 0;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                removed += walk(&path, depth + 1);
            } else if kind.is_file()
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(ASIDE_SUFFIX))
                && fs::remove_file(&path).is_ok()
            {
                removed += 1;
            }
        }
        removed
    }
    walk(install, 0)
}

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
    PREFIXES.iter().any(|prefix| path.starts_with(prefix))
        // AppImage runtime mounts are read-only squashfs images.
        || path.to_string_lossy().starts_with("/tmp/.mount_")
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-swap-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tree(root: &Path, files: &[(&str, &str)]) {
        for (name, data) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }
    }

    #[test]
    fn swap_dir_replaces_every_file_and_drops_the_backup() {
        let root = scratch("dir-ok");
        let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
        tree(
            &install,
            &[("quill", "old"), ("lib/libtdjson.so", "old-td")],
        );
        tree(&staged, &[("quill", "new"), ("lib/libtdjson.so", "new-td")]);
        swap_dir(&install, &staged, &backup, || Ok(())).unwrap();
        assert_eq!(fs::read_to_string(install.join("quill")).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(install.join("lib/libtdjson.so")).unwrap(),
            "new-td"
        );
        assert!(!backup.exists() && !staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_dir_restores_the_old_install_when_launch_fails() {
        let root = scratch("dir-launch");
        let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
        tree(
            &install,
            &[("quill", "old"), ("lib/libtdjson.so", "old-td")],
        );
        tree(&staged, &[("quill", "new"), ("lib/libtdjson.so", "new-td")]);
        let result = swap_dir(&install, &staged, &backup, || {
            Err(io::Error::other("cannot start"))
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(install.join("quill")).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(install.join("lib/libtdjson.so")).unwrap(),
            "old-td"
        );
        assert!(!backup.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_dir_restores_the_old_install_when_the_stage_is_missing() {
        let root = scratch("dir-missing");
        let (install, backup) = (root.join("quill"), root.join("bak"));
        tree(&install, &[("quill", "old")]);
        let result = swap_dir(&install, &root.join("absent"), &backup, || Ok(()));
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(install.join("quill")).unwrap(), "old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_dir_refuses_when_a_previous_backup_exists() {
        let root = scratch("dir-backup");
        let (install, staged, backup) = (root.join("quill"), root.join("new"), root.join("bak"));
        tree(&install, &[("quill", "old")]);
        tree(&staged, &[("quill", "new")]);
        tree(&backup, &[("keep", "me")]);
        assert!(swap_dir(&install, &staged, &backup, || Ok(())).is_err());
        assert_eq!(fs::read_to_string(install.join("quill")).unwrap(), "old");
        assert!(backup.join("keep").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_files_moves_new_files_in_and_leaves_old_ones_aside() {
        let root = scratch("files-ok");
        let (install, staged) = (root.join("quill"), root.join("new"));
        tree(
            &install,
            &[
                ("quill.exe", "old"),
                ("tdjson.dll", "old-td"),
                ("keep.txt", "k"),
            ],
        );
        tree(
            &staged,
            &[
                ("quill.exe", "new"),
                ("tdjson.dll", "new-td"),
                ("licenses/a.txt", "a"),
            ],
        );
        swap_files(&install, &staged, || Ok(())).unwrap();
        assert_eq!(
            fs::read_to_string(install.join("quill.exe")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(install.join("tdjson.dll")).unwrap(),
            "new-td"
        );
        assert_eq!(
            fs::read_to_string(install.join("licenses/a.txt")).unwrap(),
            "a"
        );
        assert_eq!(fs::read_to_string(install.join("keep.txt")).unwrap(), "k");
        assert_eq!(
            fs::read_to_string(install.join("quill.exe.old")).unwrap(),
            "old"
        );
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_files_rolls_back_everything_when_launch_fails() {
        let root = scratch("files-rollback");
        let (install, staged) = (root.join("quill"), root.join("new"));
        tree(&install, &[("quill.exe", "old"), ("tdjson.dll", "old-td")]);
        tree(
            &staged,
            &[
                ("quill.exe", "new"),
                ("tdjson.dll", "new-td"),
                ("extra.dll", "x"),
            ],
        );
        let result = swap_files(&install, &staged, || Err(io::Error::other("no")));
        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(install.join("quill.exe")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(install.join("tdjson.dll")).unwrap(),
            "old-td"
        );
        assert!(!install.join("extra.dll").exists());
        assert!(!install.join("quill.exe.old").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn swap_files_picks_a_fresh_name_when_a_leftover_cannot_be_cleared() {
        let root = scratch("files-aside");
        let install = root.join("quill");
        tree(&install, &[("quill.exe", "old")]);
        // A leftover directory named like the aside file cannot be removed with
        // remove_file, standing in for a locked file.
        fs::create_dir_all(install.join("quill.exe.old")).unwrap();
        let aside = aside_name(&install.join("quill.exe")).unwrap();
        assert_eq!(aside, install.join("quill.exe.1.old"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_cleanup_removes_old_leftovers_only() {
        let root = scratch("cleanup");
        tree(
            &root,
            &[
                ("quill.exe", "x"),
                ("quill.exe.old", "x"),
                ("tdjson.dll.1.old", "x"),
                ("licenses/deep/ffmpeg.dll.old", "x"),
                ("notes.txt", "x"),
            ],
        );
        assert_eq!(remove_leftovers(&root), 3);
        assert!(root.join("quill.exe").exists() && root.join("notes.txt").exists());
        assert!(!root.join("quill.exe.old").exists());
        assert!(!root.join("tdjson.dll.1.old").exists());
        assert!(!root.join("licenses/deep/ffmpeg.dll.old").exists());
        assert_eq!(remove_leftovers(&root), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writable_probe_and_managed_paths() {
        let root = scratch("probe");
        assert!(dir_writable(&root));
        assert!(!dir_writable(&root.join("missing")));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        assert!(distro_managed(Path::new("/usr/lib/quill")));
        assert!(distro_managed(Path::new("/tmp/.mount_Quill1/usr")));
        assert!(!distro_managed(Path::new("/home/me/.local/opt/quill")));
        fs::remove_dir_all(root).unwrap();
    }
}
