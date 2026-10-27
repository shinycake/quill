//! Writing the user's files safely: through a temp file and a rename, with
//! the old file's permissions, and backups that never overwrite each other.

use std::fs::{OpenOptions, Permissions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The file `path` names after following symlinks, which may not exist yet.
/// Writing there keeps a symlinked config a symlink.
pub fn resolve_links(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    // A loop of links ends here too; the write then fails with the OS error.
    for _ in 0..40 {
        let is_link = std::fs::symlink_metadata(&current)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false);
        if !is_link {
            break;
        }
        let Ok(target) = std::fs::read_link(&current) else {
            break;
        };
        current = match current.parent() {
            Some(parent) if target.is_relative() => parent.join(target),
            _ => target,
        };
    }
    current
}

/// The permissions of the file at `path`, when it exists.
pub fn permissions_of(path: &Path) -> Option<Permissions> {
    std::fs::metadata(path).ok().map(|meta| meta.permissions())
}

/// Create `path`, which must not exist, holding `text`. It gets `like`'s
/// permissions, or owner-only on Unix when there is none; on Unix it is
/// created with that mode, so it is never readable by others in between.
pub fn create_new(path: &Path, text: &str, like: Option<&Permissions>) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(like.map_or(0o600, |p| p.mode() & 0o7777));
    }
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    // The umask may have taken bits off the mode it was created with.
    if let Some(like) = like {
        file.set_permissions(like.clone())?;
    }
    Ok(())
}

/// Replace the file at `path` (already resolved) with `text`: write a temp
/// file beside it, then rename it over, so a crash never leaves half a file.
pub fn write_atomic(path: &Path, text: &str, like: Option<&Permissions>) -> std::io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".quill-tmp");
    let temp = PathBuf::from(temp);
    // Left by a run that stopped before its rename.
    let _ = std::fs::remove_file(&temp);
    let result = create_new(&temp, text, like).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Copy `text` (the file's old contents) to the first free name `make(n)`
/// gives for n = 1, 2, …, so an earlier backup is never overwritten.
/// Returns the backup's path.
pub fn back_up(
    text: &str,
    like: Option<&Permissions>,
    make: impl Fn(u32) -> PathBuf,
) -> std::io::Result<PathBuf> {
    let mut n: u32 = 1;
    loop {
        let candidate = make(n);
        match create_new(&candidate, text, like) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => {
                n = n.saturating_add(1);
            }
            Err(e) => return Err(e),
            Ok(()) => return Ok(candidate),
        }
    }
}
