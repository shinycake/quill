//! Picks the file a "save all" copy writes to inside the chosen folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// `dir/name`, or `dir/name (2).ext` when the name is taken on disk or by an
/// earlier file of the same save.
///
/// The bool is true when this call created the (empty) destination file, so
/// the caller may remove it if the copy fails. It is false when the claim
/// failed for a reason other than the name existing; the copy then reports
/// that itself, and the path is not ours to delete.
pub(super) fn unique_destination(
    dir: &Path,
    name: &str,
    used: &mut HashSet<PathBuf>,
) -> (PathBuf, bool) {
    let safe = quill::export::names::safe_file_name(name);
    let name = safe.as_str();
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    let mut n = 1;
    loop {
        let file = match (n, ext) {
            (1, _) => name.to_string(),
            (n, Some(ext)) => format!("{stem} ({n}).{ext}"),
            (n, None) => format!("{stem} ({n})"),
        };
        let candidate = dir.join(file);
        n += 1;
        // Check the names this save already handed out before touching the
        // disk, so a name an earlier file holds never gets an empty file
        // created for it.
        if used.contains(&candidate) {
            continue;
        }
        // `create_new` claims the name atomically, so a file that appears
        // between the check and the copy is never overwritten.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => {
                used.insert(candidate.clone());
                return (candidate, true);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => {
                used.insert(candidate.clone());
                return (candidate, false);
            }
        }
    }
}

#[cfg(test)]
mod save_destination_tests {
    use super::unique_destination;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quill-save-dest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn hostile_names_stay_inside_the_folder() {
        let dir = Path::new("/picked/dir");
        let mut used = HashSet::new();
        for name in ["../../x", "/etc/passwd", "..\\..\\x", "", ".."] {
            let (dest, _) = unique_destination(dir, name, &mut used);
            assert_eq!(dest.parent(), Some(dir), "{name:?} -> {dest:?}");
        }
    }

    #[test]
    fn freed_name_is_not_reclaimed_so_no_empty_file_is_left() {
        let dir = scratch("freed");
        let mut used = HashSet::new();
        let (first, claimed) = unique_destination(&dir, "a.txt", &mut used);
        assert!(claimed);
        // The first copy fails and its empty claim is removed.
        let _ = std::fs::remove_file(&first);
        let (second, claimed) = unique_destination(&dir, "a.txt", &mut used);
        assert!(claimed);
        assert_ne!(first, second);
        assert!(!first.exists());
        assert!(second.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_file_is_skipped_and_left_alone() {
        let dir = scratch("existing");
        let taken = dir.join("a.txt");
        let _ = std::fs::write(&taken, b"keep");
        let mut used = HashSet::new();
        let (dest, claimed) = unique_destination(&dir, "a.txt", &mut used);
        assert!(claimed);
        assert_ne!(dest, taken);
        assert_eq!(std::fs::read(&taken).ok().as_deref(), Some(&b"keep"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unclaimable_destination_reports_not_claimed() {
        // The folder does not exist, so create_new fails with NotFound: the
        // path comes back for the copy to report, flagged as not ours.
        let dir = std::env::temp_dir().join(format!("quill-save-dest-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut used = HashSet::new();
        let (_, claimed) = unique_destination(&dir, "a.txt", &mut used);
        assert!(!claimed);
    }
}
