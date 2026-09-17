//! Display-path sandbox: only files under allowed roots (account `tdlib_files`, demo allowlist).

use std::path::{Path, PathBuf};

/// Canonicalize `candidate` and return it only if it is an existing file under an allowed root.
///
/// Roots may be directories (`tdlib_files`, demo fixtures) or an explicit file allowlist.
/// Symlinks that resolve outside a root are rejected.
pub fn sandboxed_display_path(candidate: &str, allowed_roots: &[PathBuf]) -> Option<PathBuf> {
    if candidate.is_empty() {
        return None;
    }
    let file = canonical_file(Path::new(candidate))?;
    for root in allowed_roots {
        let Ok(root_canon) = std::fs::canonicalize(root) else {
            continue;
        };
        if file == root_canon || file.starts_with(&root_canon) {
            return Some(file);
        }
    }
    None
}

fn canonical_file(path: &Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(path).ok()?;
    canon.is_file().then_some(canon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "quill-sandbox-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn accepts_file_under_tdlib_files() {
        let root = scratch("ok");
        let tdlib_files = root.join("files");
        fs::create_dir_all(&tdlib_files).unwrap();
        let photo = tdlib_files.join("thumb.png");
        fs::write(&photo, [1, 2, 3]).unwrap();
        let got =
            sandboxed_display_path(photo.to_str().unwrap(), std::slice::from_ref(&tdlib_files));
        assert_eq!(got, Some(std::fs::canonicalize(&photo).unwrap()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_path_outside_roots() {
        let root = scratch("out");
        let tdlib_files = root.join("files");
        fs::create_dir_all(&tdlib_files).unwrap();
        let outside = root.join("evil.png");
        fs::write(&outside, [9]).unwrap();
        assert_eq!(
            sandboxed_display_path(outside.to_str().unwrap(), &[tdlib_files]),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_empty_and_missing() {
        assert_eq!(sandboxed_display_path("", &[PathBuf::from("/tmp")]), None);
        assert_eq!(
            sandboxed_display_path("/no/such/quill-media.png", &[PathBuf::from("/tmp")]),
            None
        );
    }

    #[test]
    fn demo_allowlist_file_is_accepted() {
        let root = scratch("demo");
        let fixture = root.join("demo-thumb.png");
        fs::write(&fixture, [4, 5]).unwrap();
        let got = sandboxed_display_path(fixture.to_str().unwrap(), std::slice::from_ref(&fixture));
        assert_eq!(got, Some(std::fs::canonicalize(&fixture).unwrap()));
        let dir_root = scratch("demo-dir");
        let fixtures = dir_root.join("fixtures");
        fs::create_dir_all(&fixtures).unwrap();
        let nested = fixtures.join("demo-thumb.png");
        fs::write(&nested, [6]).unwrap();
        let got = sandboxed_display_path(nested.to_str().unwrap(), &[fixtures]);
        assert_eq!(got, Some(std::fs::canonicalize(&nested).unwrap()));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&dir_root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let root = scratch("link");
        let tdlib_files = root.join("files");
        fs::create_dir_all(&tdlib_files).unwrap();
        let outside = root.join("outside.png");
        fs::write(&outside, [7]).unwrap();
        let link = tdlib_files.join("alias.png");
        match std::os::unix::fs::symlink(&outside, &link) {
            Ok(()) => {
                assert_eq!(
                    sandboxed_display_path(link.to_str().unwrap(), &[tdlib_files]),
                    None
                );
            }
            Err(_) => {
                // Environment without symlink permission: skip the escape case.
            }
        }
        let _ = fs::remove_dir_all(&root);
    }
}
