//! An account's carrier data folder (`webview-carrier`): the web views'
//! files, cookies the pages set included. It goes when the host shuts down
//! and again before the next run, without blocking either. On Windows
//! WebView2's browser processes may still hold files when the host shuts
//! down (a killed helper does not wait for them), so a folder that will
//! not go at once is moved aside and deleted in the background, and the
//! next start sweeps whatever is still there.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a background delete keeps trying.
const RETRY_FOR: Duration = Duration::from_secs(2);
const RETRY_EVERY: Duration = Duration::from_millis(100);
/// Marks a folder moved aside: `webview-carrier.stale-<pid>-<nanos>`.
const STALE: &str = ".stale-";

/// Empty `dir` for a new run: what the last run left is moved aside, and
/// that and any older leftovers are deleted on a background thread.
pub(super) fn clear_for_start(dir: &Path) {
    let mut doomed = leftovers_beside(dir);
    if let Some(aside) = move_aside(dir) {
        doomed.push(aside);
    } else if dir.exists() {
        // Cannot be moved: delete what can be deleted now, so the new
        // run does not start on the last one's cookies.
        let _ = std::fs::remove_dir_all(dir);
    }
    delete_in_background(doomed, Duration::ZERO);
}

/// Remove `dir` as the host shuts down. Never blocks for long: a folder
/// still in use is moved aside and retried in the background for about
/// two seconds; [`clear_for_start`] sweeps it otherwise.
pub(super) fn clear_at_shutdown(dir: &Path) {
    if std::fs::remove_dir_all(dir).is_ok() || !dir.exists() {
        return;
    }
    if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
        for line in listing(dir, 50) {
            eprintln!("carrier data left: {line}");
        }
    }
    // Only a folder moved aside is retried: `dir` itself may be in use by
    // the account's next host by then.
    if let Some(aside) = move_aside(dir) {
        delete_in_background(vec![aside], RETRY_FOR);
    }
}

/// `dir` renamed to a unique `<name>.stale-…` next to it.
fn move_aside(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_str()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let aside = dir.with_file_name(format!("{name}{}{}-{stamp}", STALE, std::process::id()));
    std::fs::rename(dir, &aside).ok().map(|()| aside)
}

/// Folders an earlier run moved aside and could not delete.
fn leftovers_beside(dir: &Path) -> Vec<PathBuf> {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str()))
    else {
        return Vec::new();
    };
    let prefix = format!("{name}{}", STALE);
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|file| file.starts_with(&prefix))
        })
        .map(|entry| entry.path())
        .collect()
}

/// Delete `folders` on a detached thread, retrying each for `retry_for`.
/// Without a thread they wait for the next start.
fn delete_in_background(folders: Vec<PathBuf>, retry_for: Duration) {
    if folders.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("quill-carrier-sweep".into())
        .spawn(move || {
            for folder in folders {
                let started = Instant::now();
                while std::fs::remove_dir_all(&folder).is_err()
                    && folder.exists()
                    && started.elapsed() < retry_for
                {
                    std::thread::sleep(RETRY_EVERY);
                }
            }
        });
}

/// Up to `limit` entries still under `dir`, as relative paths with sizes,
/// for the debug trace of a folder that would not go away.
fn listing(dir: &Path, limit: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= limit {
                return found;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => {
                    found.push(format!("{relative}/"));
                    pending.push(path);
                }
                Ok(meta) => found.push(format!("{relative} ({} bytes)", meta.len())),
                Err(_) => found.push(format!("{relative} (unreadable)")),
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let root = crate::scratch::process_dir("carrier-data-folder").join(name);
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("webview-carrier");
        let _ = std::fs::create_dir_all(dir.join("EBWebView/Default"));
        let _ = std::fs::write(dir.join("EBWebView/Default/Cookies"), b"cookie");
        dir
    }

    fn wait_until(done: impl Fn() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        done()
    }

    #[test]
    fn shutdown_removes_the_folder() {
        let dir = folder("shutdown");
        clear_at_shutdown(&dir);
        assert!(!dir.exists());
    }

    #[test]
    fn a_start_sweeps_the_last_run_and_older_leftovers() {
        let dir = folder("start");
        let old = dir.with_file_name("webview-carrier.stale-1-2");
        let _ = std::fs::create_dir_all(old.join("EBWebView"));
        let unrelated = dir.with_file_name("webview-carrier-probe");
        let _ = std::fs::create_dir_all(&unrelated);
        clear_for_start(&dir);
        assert!(!dir.exists());
        let parent = dir.parent().map(Path::to_path_buf).unwrap_or_default();
        assert!(wait_until(|| leftovers_beside(&dir).is_empty()));
        assert!(unrelated.exists());
        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn the_listing_is_relative_and_bounded() {
        let dir = folder("listing");
        let lines = listing(&dir, 50);
        assert!(
            lines.contains(&"EBWebView/".to_string()) || lines.contains(&"EBWebView\\".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.ends_with("Cookies (6 bytes)")),
            "{lines:?}"
        );
        assert_eq!(listing(&dir, 1).len(), 1);
        clear_at_shutdown(&dir);
    }
}
