//! State of the send box (the attachment tray above the composer): what
//! was read about its files (the app's), the price box and the video editor (the
//! "High quality" and price choices are per chat, `composer_sends.rs`). Behaviour is in
//! `send_box_options.rs` and `video_editor.rs`.
#![deny(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::send_box_options::PriceDialog;
use super::video_editor::VideoEditorDialog;

/// What was read about the files of the send trays (sizes, sound tracks,
/// thumbnails, media checks). A tray is the account's, shown in every
/// window that shows its chat, so what was read about its files is the
/// app's too (`ComposerSends::reads`): a window that opens the same tray
/// neither reads again nor misses the result, and a read one window
/// started lands for all of them.
pub(crate) struct TrayReads {
    /// Sizes read from the header of tray photos, once per file.
    pub(super) photo_sizes: RefCell<HashMap<PathBuf, Option<(u32, u32)>>>,
    /// Tray videos whose sound track is being read in the background, and
    /// those already read (tdesktop `hasAudio`). Each read has its own
    /// number, like the thumbnails below, so a stale read (a removed and
    /// re-added file) can't settle a newer one.
    pub(super) sound_checking: HashMap<PathBuf, u64>,
    pub(super) sound_checked: HashSet<PathBuf>,
    pub(super) sound_jobs: u64,
    /// Tray pictures sent as files whose thumbnail is being made, and
    /// those done (`send_box_thumbnails.rs`).
    /// Each job has its own number, so a stale job (a removed and re-added
    /// file) can't settle a newer one.
    pub(super) thumbnailing: HashMap<PathBuf, u64>,
    pub(super) thumbnailed: HashSet<PathBuf>,
    pub(super) thumbnail_jobs: u64,
    /// Photos and videos being read as they join a tray
    /// (`send_box_media_check.rs`), the job that reads each.
    pub(super) media_checking: HashMap<PathBuf, u64>,
    pub(super) media_checked: HashMap<PathBuf, super::send_box_media_check::MediaCheck>,
    pub(super) media_check_jobs: u64,
}

impl Default for TrayReads {
    fn default() -> Self {
        Self {
            photo_sizes: RefCell::new(HashMap::new()),
            sound_checking: HashMap::new(),
            sound_checked: HashSet::new(),
            sound_jobs: 0,
            thumbnailing: HashMap::new(),
            thumbnailed: HashSet::new(),
            thumbnail_jobs: 0,
            media_checking: HashMap::new(),
            media_checked: HashMap::new(),
            media_check_jobs: 0,
        }
    }
}

impl TrayReads {
    /// Files left their tray: forget what was read about them, so the same
    /// file attached again is read again. Callers pass only files no tray
    /// holds any more (`QuillApp::forget_tray_checks`).
    pub(super) fn forget<'a>(&mut self, paths: impl Iterator<Item = &'a Path>) {
        let mut sizes = self.photo_sizes.borrow_mut();
        for path in paths {
            sizes.remove(path);
            self.sound_checking.remove(path);
            self.sound_checked.remove(path);
            self.thumbnailing.remove(path);
            self.thumbnailed.remove(path);
            self.media_checking.remove(path);
            self.media_checked.remove(path);
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
