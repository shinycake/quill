//! What an unpacked package held when it was staged, checked again before it
//! is installed (tdesktop keeps the downloaded package and verifies it again
//! in `checkReadyUpdate` before unpacking it at the next start).
//!
//! The archive's SHA-256 is checked once, at download. A staged tree can then
//! wait on disk for days before the next start, so the plan carries the size
//! and SHA-256 of every file in it, and the helper refuses a tree with a file
//! missing, added or changed.
#![deny(clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct FileHash {
    /// Relative to the package root.
    path: PathBuf,
    size: u64,
    sha256: String,
}

/// Every regular file under `root`, sorted, with its size and SHA-256.
/// A link anywhere in the tree is an error.
pub(super) fn of(root: &Path) -> io::Result<Vec<FileHash>> {
    super::swap::list_files(root)?
        .into_iter()
        .map(|path| {
            let full = root.join(&path);
            Ok(FileHash {
                size: std::fs::metadata(&full)?.len(),
                sha256: super::digest_file(&full)?,
                path,
            })
        })
        .collect()
}

/// Whether the tree at `root` is still exactly what `expected` recorded.
/// An empty manifest (a plan written before manifests existed) never matches.
pub(super) fn verify(root: &Path, expected: &[FileHash]) -> io::Result<()> {
    if expected.is_empty() {
        return Err(io::Error::other("Staged update has no file manifest"));
    }
    if of(root)? != expected {
        return Err(io::Error::other(
            "Staged update changed before installation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{of, verify};
    use std::fs;

    fn tree(tag: &str) -> std::path::PathBuf {
        let root = crate::scratch::process_dir(&format!("quill-manifest-{tag}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("quill"), b"app").unwrap();
        fs::write(root.join("lib/libtdjson.so"), b"td").unwrap();
        root
    }

    #[test]
    fn an_untouched_tree_matches_its_manifest() {
        let root = tree("same");
        let manifest = of(&root).unwrap();
        assert_eq!(manifest.len(), 2);
        assert!(verify(&root, &manifest).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_changed_or_added_file_fails_the_check() {
        let root = tree("changed");
        let manifest = of(&root).unwrap();
        fs::write(root.join("lib/libtdjson.so"), b"tx").unwrap();
        assert!(verify(&root, &manifest).is_err(), "same size, other bytes");
        fs::write(root.join("lib/libtdjson.so"), b"td").unwrap();
        fs::remove_file(root.join("quill")).unwrap();
        assert!(verify(&root, &manifest).is_err(), "missing file");
        fs::write(root.join("quill"), b"app").unwrap();
        fs::write(root.join("extra"), b"x").unwrap();
        assert!(verify(&root, &manifest).is_err(), "added file");
        fs::remove_file(root.join("extra")).unwrap();
        assert!(verify(&root, &manifest).is_ok());
        assert!(verify(&root, &[]).is_err(), "no manifest");
        fs::remove_dir_all(root).unwrap();
    }
}
