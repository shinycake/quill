//! Locating the external media tools (`ffmpeg`, `ffprobe`, `ffplay`).
//!
//! An app opened from Finder or the Dock inherits launchd's minimal `PATH`
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), which misses Homebrew and MacPorts,
//! so a bare `Command::new("ffmpeg")` fails there even when it is installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

/// Package-manager folders searched after `PATH`.
const FALLBACK_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

static RESOLVED: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A `Command` for media tool `name`, resolved once per process: next to
/// the app binary (a bundled copy), then `PATH`, then the package-manager
/// folders. Falls back to the bare name, so a missing tool fails as before.
pub fn command(name: &str) -> Command {
    Command::new(resolve(name))
}

fn resolve(name: &str) -> PathBuf {
    if let Ok(cache) = RESOLVED.lock()
        && let Some(path) = cache.get(name)
    {
        return path.clone();
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let found = find_in(
        name,
        beside_exe
            .into_iter()
            .chain(path_dirs)
            .chain(FALLBACK_DIRS.iter().map(PathBuf::from)),
    )
    .unwrap_or_else(|| PathBuf::from(name));
    if let Ok(mut cache) = RESOLVED.lock() {
        cache.insert(name.to_string(), found.clone());
    }
    found
}

fn find_in(name: &str, dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn finds_the_first_executable_and_skips_plain_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("quill-media-tools-{}", std::process::id()));
        let (plain, tool) = (root.join("plain"), root.join("tool"));
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::create_dir_all(&tool).unwrap();
        std::fs::write(plain.join("ffmpeg"), b"").unwrap();
        let exe = tool.join("ffmpeg");
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let missing = root.join("missing");
        assert_eq!(find_in("ffmpeg", [missing, plain, tool.clone()]), Some(exe));
        assert_eq!(find_in("ffprobe", [tool]), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
