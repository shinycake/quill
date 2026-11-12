//! State of the send box (the attachment tray above the composer): what
//! was read about its files (the app's), the price box and the video editor (the
//! "High quality" and price choices are per chat, `composer_sends.rs`). Behaviour is in
//! `send_box_options.rs` and `video_editor.rs`.
#![deny(clippy::indexing_slicing)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::send_box_options::PriceDialog;
use super::video_editor::VideoEditorDialog;

/// Where the read of one tray file stands: running (its job number tells a
/// stale answer from the current one) or done with what it found. The tasks
/// of a running read (its worker and its give-up timer) live in the stage: a
/// read that leaves `Reading` (its answer, the give-up, the file leaving the
/// tray) drops them, which cancels the ones still pending, so no timer
/// outlives its read.
pub(super) enum Read<T> {
    Reading {
        job: u64,
        _tasks: Vec<gpui_kit::Task<()>>,
    },
    Done(T),
}

/// One kind of read, by file: a path has exactly one stage, so a file
/// cannot be both being read and read.
pub(crate) struct Reads<T>(HashMap<PathBuf, Read<T>>);

impl<T> Default for Reads<T> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<T> Reads<T> {
    /// Nothing started or finished for `path`.
    pub(super) fn is_unread(&self, path: &Path) -> bool {
        !self.0.contains_key(path)
    }

    pub(super) fn is_reading(&self, path: &Path) -> bool {
        matches!(self.0.get(path), Some(Read::Reading { .. }))
    }

    pub(super) fn done(&self, path: &Path) -> Option<&T> {
        match self.0.get(path) {
            Some(Read::Done(found)) => Some(found),
            _ => None,
        }
    }

    /// A read starts with job number `job` (replacing any earlier stage), its
    /// `tasks` owned by the stage.
    pub(super) fn start(&mut self, path: PathBuf, job: u64, tasks: Vec<gpui_kit::Task<()>>) {
        self.0.insert(path, Read::Reading { job, _tasks: tasks });
    }

    /// Job `job` is the one reading `path` now.
    pub(super) fn is_current(&self, path: &Path, job: u64) -> bool {
        matches!(self.0.get(path), Some(Read::Reading { job: current, .. }) if *current == job)
    }

    /// Job `job` found `found` for `path`: whether it was the current job
    /// (a stale one, from a removed and re-added file, changes nothing).
    pub(super) fn finish(&mut self, path: &Path, job: u64, found: T) -> bool {
        let Some(stage) = self.0.get_mut(path) else {
            return false;
        };
        if !matches!(stage, Read::Reading { job: current, .. } if *current == job) {
            return false;
        }
        *stage = Read::Done(found);
        true
    }

    /// `path` is known without a read (the editor saved it).
    pub(super) fn set_done(&mut self, path: PathBuf, found: T) {
        self.0.insert(path, Read::Done(found));
    }

    /// Forget what was found (a read running stays), so the files are read
    /// again.
    pub(super) fn clear_done(&mut self) {
        self.0
            .retain(|_, stage| matches!(stage, Read::Reading { .. }));
    }

    pub(super) fn forget(&mut self, path: &Path) {
        self.0.remove(path);
    }
}

/// What was read about the files of the send trays (sizes, sound tracks,
/// thumbnails, media checks). A tray is the account's, shown in every
/// window that shows its chat, so what was read about its files is the
/// app's too (`ComposerSends::reads`): a window that opens the same tray
/// neither reads again nor misses the result, and a read one window
/// started lands for all of them.
#[derive(Default)]
pub(crate) struct TrayReads {
    /// The size of each tray photo's picture, read from its header off the
    /// UI thread (`send_box_options.rs`), by the photo's own path.
    pub(super) photo_sizes: Reads<PhotoSize>,
    pub(super) photo_size_jobs: u64,
    /// Tray videos whose sound track is being read in the background, or
    /// already read (tdesktop `hasAudio`). Each read has its own number,
    /// like the thumbnails below, so a stale read (a removed and re-added
    /// file) can't settle a newer one.
    pub(super) sound: Reads<()>,
    pub(super) sound_jobs: u64,
    /// Tray pictures sent as files whose thumbnail is being made, or done
    /// (`send_box_thumbnails.rs`). Each job has its own number, so a stale
    /// job (a removed and re-added file) can't settle a newer one.
    pub(super) thumbnails: Reads<()>,
    pub(super) thumbnail_jobs: u64,
    /// Photos and videos being read as they join a tray, or read
    /// (`send_box_media_check.rs`), with the job that reads each.
    pub(super) media: Reads<super::send_box_media_check::MediaFound>,
    pub(super) media_check_jobs: u64,
    /// The first frame and length of tray videos (`send_card/video_preview.rs`).
    pub(super) previews: Reads<super::send_card::VideoPreview>,
    pub(super) preview_jobs: u64,
    /// Bytes of tray files for the file rows (`send_card/file_size.rs`);
    /// `None` when the file could not be read.
    pub(super) file_sizes: Reads<Option<u64>>,
    pub(super) file_size_jobs: u64,
}

/// What reading the header of the picture a tray photo goes out as found:
/// the file that was read (the photo's JPEG once one is made, else the file
/// itself) and its size, `None` when the header could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PhotoSize {
    pub(super) file: PathBuf,
    pub(super) size: Option<(u32, u32)>,
}

impl TrayReads {
    /// Files left their tray: forget what was read about them, so the same
    /// file attached again is read again. Callers pass only files no tray
    /// holds any more (`QuillApp::forget_tray_checks`).
    pub(super) fn forget<'a>(&mut self, paths: impl Iterator<Item = &'a Path>) {
        for path in paths {
            self.photo_sizes.forget(path);
            self.sound.forget(path);
            self.thumbnails.forget(path);
            self.media.forget(path);
            if let Some(preview) = self.previews.done(path) {
                super::send_card::discard_preview(preview);
            }
            self.previews.forget(path);
            self.file_sizes.forget(path);
        }
    }
}

/// The send box's boxes of one window: the price box and the video editor.
pub(crate) struct SendBoxUi {
    /// The "Paid Content" price box.
    pub(super) price: Option<PriceDialog>,
    /// The video editor opened from the tray or the edit box.
    pub(super) video_editor: Option<VideoEditorDialog>,
}

impl SendBoxUi {
    pub(super) fn new() -> Self {
        Self {
            price: None,
            video_editor: None,
        }
    }
}

impl Default for SendBoxUi {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_has_one_stage_and_only_its_current_job_settles_it() {
        let mut reads: Reads<u8> = Reads::default();
        let path = PathBuf::from("a.mp4");
        assert!(reads.is_unread(&path));
        reads.start(path.clone(), 1, Vec::new());
        assert!(reads.is_reading(&path) && reads.done(&path).is_none());
        // The file was removed and added again: job 2 supersedes job 1.
        reads.start(path.clone(), 2, Vec::new());
        assert!(!reads.finish(&path, 1, 9), "a stale job changes nothing");
        assert!(reads.is_reading(&path));
        assert!(reads.finish(&path, 2, 7));
        assert_eq!(reads.done(&path), Some(&7));
        assert!(!reads.is_reading(&path));
        assert!(
            !reads.finish(&path, 2, 8),
            "a done read is not settled twice"
        );
    }

    #[test]
    fn clearing_the_done_reads_keeps_those_still_running() {
        let mut reads: Reads<()> = Reads::default();
        reads.set_done(PathBuf::from("done"), ());
        reads.start(PathBuf::from("running"), 3, Vec::new());
        reads.clear_done();
        assert!(reads.is_unread(Path::new("done")));
        assert!(reads.is_reading(Path::new("running")));
    }
}
