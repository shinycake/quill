//! A work file nobody has taken yet, removed when dropped.
use std::path::{Path, PathBuf};

/// A file the app made for one piece of work (an encoded clip, a camera
/// shot, a zipped folder) that nobody has taken yet. Dropped untaken, it
/// is removed, with its folder when it was made in one of its own: a
/// continuation that never runs (its window closed, the account switched)
/// drops it with its captures, so the file does not stay behind until the
/// startup sweep. [`OwnedFile::keep`] hands the path on to its new owner.
#[derive(Debug)]
pub struct OwnedFile {
    /// `None` only once kept or dropped.
    path: Option<PathBuf>,
    /// Remove the parent folder instead of the file alone.
    folder: bool,
}

impl OwnedFile {
    /// `path` is removed on drop.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            folder: false,
        }
    }

    /// `path` and the folder it was made in, which holds nothing else,
    /// are removed on drop.
    pub fn with_folder(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            folder: true,
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or(Path::new(""))
    }

    /// The file has a new owner: it stays.
    pub fn keep(mut self) -> PathBuf {
        self.path.take().unwrap_or_default()
    }
}

impl Drop for OwnedFile {
    fn drop(&mut self) {
        let Some(path) = self.path.take() else {
            return;
        };
        match path.parent().filter(|_| self.folder) {
            Some(folder) => {
                let _ = std::fs::remove_dir_all(folder);
            }
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OwnedFile;
    use crate::scratch::unique_dir;

    #[test]
    fn an_untaken_file_goes_with_its_owner() {
        let dir = unique_dir("quill-owned-file");
        let path = dir.join("clip.mp4");
        std::fs::write(&path, b"clip").expect("write");
        drop(OwnedFile::new(path.clone()));
        assert!(!path.exists());
        assert!(dir.is_dir(), "only the file goes");
    }

    #[test]
    fn a_kept_file_stays() {
        let dir = unique_dir("quill-owned-file");
        let path = dir.join("shot.jpg");
        std::fs::write(&path, b"shot").expect("write");
        let file = OwnedFile::new(path.clone());
        assert_eq!(file.path(), path.as_path());
        assert_eq!(file.keep(), path);
        assert!(path.exists());
    }

    #[test]
    fn a_file_made_in_its_own_folder_takes_the_folder() {
        let dir = unique_dir("quill-owned-file");
        let folder = dir.join("zip-1");
        std::fs::create_dir_all(&folder).expect("folder");
        let path = folder.join("Photos.zip");
        std::fs::write(&path, b"zip").expect("write");
        drop(OwnedFile::with_folder(path));
        assert!(!folder.exists());
        assert!(dir.is_dir());
    }

    #[test]
    fn a_file_never_written_drops_quietly() {
        let dir = unique_dir("quill-owned-file");
        drop(OwnedFile::new(dir.join("never.jpg")));
        assert!(dir.is_dir());
    }
}
