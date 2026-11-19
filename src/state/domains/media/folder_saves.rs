//! Documents the user downloaded from a chat row while a download folder
//! was set (or "Ask where to save each file" was on): when TDLib finishes
//! one, the UI copies it out of TDLib's cache into that folder, as
//! Telegram Desktop's `DocumentSaveClickHandler` saves a document to the
//! download path. TDLib only writes into its own files directory, so the
//! copy is Quill's.
//!
//! The reducer only moves a file between stages; the UI does the copy
//! (`take_due`) and reports where it landed (`note_copy`).
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where one document stands. A file whose copy is being made has no
/// entry: a second completion update (a re-ingested file) finds nothing
/// to copy, and `note_copy` puts it back as `Saved`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FolderSave {
    /// The user asked for the download; it has not finished. Left by
    /// the finish (`Due`), a cancel, or the session going away.
    Wanted { name: String },
    /// Finished in TDLib's cache; the UI takes it on its next drain.
    Due { source: PathBuf, name: String },
    /// The copy was written here: Open and Show in folder use it.
    Saved { dest: PathBuf },
}

/// A finished document waiting for its copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderSaveDue {
    pub file_id: i32,
    /// The file in TDLib's cache.
    pub source: PathBuf,
    /// The document's own name (`document.file_name`), empty when it has
    /// none: the copy is named after it, not after TDLib's cache name.
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct FolderSaves {
    files: HashMap<i32, FolderSave>,
}

impl FolderSaves {
    /// The user asked to download `file_id`, a document named `name`.
    pub fn want(&mut self, file_id: i32, name: &str) {
        if file_id != 0 {
            self.files.insert(
                file_id,
                FolderSave::Wanted {
                    name: name.to_string(),
                },
            );
        }
    }

    /// The download was cancelled: nothing to copy. A saved copy stays.
    pub fn cancel(&mut self, file_id: i32) {
        if matches!(self.files.get(&file_id), Some(FolderSave::Wanted { .. })) {
            self.files.remove(&file_id);
        }
    }

    pub fn is_wanted(&self, file_id: i32) -> bool {
        matches!(self.files.get(&file_id), Some(FolderSave::Wanted { .. }))
    }

    /// TDLib reports `file_id` complete at `path` (`None` when the path
    /// is not usable yet: the copy waits for the update that has one).
    pub(crate) fn on_completed(&mut self, file_id: i32, path: Option<&str>) {
        let Some(path) = path.filter(|p| !p.is_empty()) else {
            return;
        };
        let Some(FolderSave::Wanted { name }) = self.files.get(&file_id) else {
            return;
        };
        let due = FolderSave::Due {
            source: PathBuf::from(path),
            name: name.clone(),
        };
        self.files.insert(file_id, due);
    }

    /// The finished documents to copy now, oldest file id first.
    pub fn take_due(&mut self) -> Vec<FolderSaveDue> {
        let mut ids: Vec<i32> = self
            .files
            .iter()
            .filter(|(_, save)| matches!(save, FolderSave::Due { .. }))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|file_id| match self.files.remove(&file_id) {
                Some(FolderSave::Due { source, name }) => Some(FolderSaveDue {
                    file_id,
                    source,
                    name,
                }),
                _ => None,
            })
            .collect()
    }

    /// The copy of `file_id` was written to `dest` (`None`: it failed or
    /// the user closed the save dialog). A newer request for the file
    /// that came in while copying is kept.
    pub fn note_copy(&mut self, file_id: i32, dest: Option<PathBuf>) {
        if let Some(dest) = dest
            && !self.is_wanted(file_id)
        {
            self.files.insert(file_id, FolderSave::Saved { dest });
        }
    }

    /// Where the copy of `file_id` was saved, if one was. The caller
    /// checks it is still on disk.
    pub fn copy_of(&self, file_id: i32) -> Option<&Path> {
        match self.files.get(&file_id) {
            Some(FolderSave::Saved { dest }) => Some(dest.as_path()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wanted_document_is_due_once_when_it_completes_with_a_path() {
        let mut saves = FolderSaves::default();
        saves.want(7, "report.pdf");
        saves.on_completed(7, None);
        assert!(saves.take_due().is_empty());
        saves.on_completed(7, Some("/td/documents/report.pdf"));
        saves.on_completed(7, Some("/td/documents/report.pdf"));
        assert_eq!(
            saves.take_due(),
            vec![FolderSaveDue {
                file_id: 7,
                source: PathBuf::from("/td/documents/report.pdf"),
                name: "report.pdf".to_string(),
            }]
        );
        // Copying: a repeated completion copies nothing more.
        saves.on_completed(7, Some("/td/documents/report.pdf"));
        assert!(saves.take_due().is_empty());
        assert!(!saves.is_wanted(7));
    }

    #[test]
    fn a_file_nobody_asked_for_or_a_cancelled_one_is_not_copied() {
        let mut saves = FolderSaves::default();
        saves.on_completed(1, Some("/td/a"));
        saves.want(2, "b.pdf");
        saves.cancel(2);
        saves.on_completed(2, Some("/td/b"));
        saves.want(0, "zero");
        assert!(!saves.is_wanted(0));
        assert!(saves.take_due().is_empty());
    }

    #[test]
    fn the_copy_is_remembered_and_a_new_request_copies_again() {
        let mut saves = FolderSaves::default();
        saves.want(3, "c.pdf");
        saves.on_completed(3, Some("/td/c"));
        assert_eq!(saves.take_due().len(), 1);
        saves.note_copy(3, Some(PathBuf::from("/dl/c.pdf")));
        assert_eq!(saves.copy_of(3), Some(Path::new("/dl/c.pdf")));
        saves.cancel(3);
        assert_eq!(saves.copy_of(3), Some(Path::new("/dl/c.pdf")));
        saves.want(3, "c.pdf");
        assert_eq!(saves.copy_of(3), None);
        saves.on_completed(3, Some("/td/c"));
        assert_eq!(saves.take_due().len(), 1);
        saves.note_copy(3, None);
        assert_eq!(saves.copy_of(3), None);
    }

    #[test]
    fn a_request_made_while_copying_is_not_overwritten_by_the_copy() {
        let mut saves = FolderSaves::default();
        saves.want(4, "d.pdf");
        saves.on_completed(4, Some("/td/d"));
        assert_eq!(saves.take_due().len(), 1);
        saves.want(4, "d.pdf");
        saves.note_copy(4, Some(PathBuf::from("/dl/d.pdf")));
        assert!(saves.is_wanted(4));
    }
}
