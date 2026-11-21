//! Documents the user downloaded from a chat row while a download folder
//! was set (or "Ask where to save each file" was on): when TDLib finishes
//! one, the UI copies it out of TDLib's cache to where the user chose, as
//! Telegram Desktop's `DocumentSaveClickHandler` saves a document to the
//! download path. TDLib only writes into its own files directory, so the
//! copy is Quill's. TDLib's copy stays: every surface keeps reading the
//! file map as before.
//!
//! Where the copy goes is decided at the click and travels with the
//! download intent (`DownloadIntents::save_to`), so a cancel, a failure, a
//! removal or the session closing drops it with the intent. A finished one
//! is queued here (`due`); the UI takes the queue and owns each copy while
//! it runs (`ui::folder_saves::FolderCopies`), then notes where it landed
//! (`note_saved`).
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where the user wants a downloaded document, chosen at the click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveTarget {
    /// Into the download folder set in Settings, named after the
    /// document's own name (`document.file_name`; empty when it has none:
    /// the copy keeps TDLib's cache name). The folder is checked when the
    /// copy runs, off the UI thread: one that cannot be reached counts as
    /// unset and the file stays in the cache only.
    Folder { dir: PathBuf, name: String },
    /// The exact path the save dialog returned ("Ask where to save").
    Exact(PathBuf),
}

/// A finished document waiting for its copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderSaveDue {
    pub file_id: i32,
    /// The file in TDLib's cache.
    pub source: PathBuf,
    pub target: SaveTarget,
}

#[derive(Clone, Debug, Default)]
pub struct FolderSaves {
    /// Finished documents the UI has not taken yet, in finishing order.
    due: Vec<FolderSaveDue>,
    /// Where the copy of each file was written this session: Open and
    /// Show in folder use it while it is still there.
    saved: HashMap<i32, PathBuf>,
}

impl FolderSaves {
    /// `file_id` finished in TDLib's cache at `source`; copy it to `target`.
    pub(crate) fn push_due(&mut self, file_id: i32, source: PathBuf, target: SaveTarget) {
        if file_id != 0 {
            self.due.push(FolderSaveDue {
                file_id,
                source,
                target,
            });
        }
    }

    /// The finished documents to copy now, oldest first.
    pub fn take_due(&mut self) -> Vec<FolderSaveDue> {
        std::mem::take(&mut self.due)
    }

    pub fn has_due(&self) -> bool {
        !self.due.is_empty()
    }

    /// The copy of `file_id` was written to `dest`.
    pub fn note_saved(&mut self, file_id: i32, dest: PathBuf) {
        if file_id != 0 {
            self.saved.insert(file_id, dest);
        }
    }

    /// Where the copy of `file_id` was saved, if one was. The caller
    /// checks it is still on disk.
    pub fn copy_of(&self, file_id: i32) -> Option<&Path> {
        self.saved.get(&file_id).map(PathBuf::as_path)
    }

    /// Everything goes (the session closed: TDLib's file ids belong to the
    /// instance that ended).
    pub fn clear(&mut self) {
        self.due.clear();
        self.saved.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> SaveTarget {
        SaveTarget::Folder {
            dir: PathBuf::from("/dl"),
            name: name.to_string(),
        }
    }

    #[test]
    fn due_documents_are_taken_once_in_finishing_order() {
        let mut saves = FolderSaves::default();
        saves.push_due(9, PathBuf::from("/td/b"), folder("b.pdf"));
        saves.push_due(3, PathBuf::from("/td/a"), folder("a.pdf"));
        saves.push_due(0, PathBuf::from("/td/z"), folder("z.pdf"));
        let ids: Vec<i32> = saves.take_due().iter().map(|due| due.file_id).collect();
        assert_eq!(ids, [9, 3]);
        assert!(saves.take_due().is_empty());
    }

    #[test]
    fn a_saved_copy_is_remembered_until_the_session_closes() {
        let mut saves = FolderSaves::default();
        saves.note_saved(3, PathBuf::from("/dl/c.pdf"));
        assert_eq!(saves.copy_of(3), Some(Path::new("/dl/c.pdf")));
        saves.push_due(4, PathBuf::from("/td/d"), folder("d.pdf"));
        saves.clear();
        assert_eq!(saves.copy_of(3), None);
        assert!(!saves.has_due());
    }
}
