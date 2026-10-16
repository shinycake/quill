//! State of the send box (the attachment tray above the composer): what
//! was read about its files, the price box and the video editor (the
//! "High quality" and price choices are per chat, `composer_sends.rs`). Behaviour is in
//! `send_box_options.rs` and `video_editor.rs`.
#![deny(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::send_box_options::PriceDialog;
use super::video_editor::VideoEditorDialog;

pub(crate) struct SendBoxUi {
    /// Sizes read from the header of tray photos, once per file.
    pub(super) photo_sizes: RefCell<HashMap<PathBuf, Option<(u32, u32)>>>,
    /// Tray videos whose sound track is being read in the background, and
    /// those already read (tdesktop `hasAudio`).
    pub(super) sound_checking: HashSet<PathBuf>,
    pub(super) sound_checked: HashSet<PathBuf>,
    /// Tray pictures sent as files whose thumbnail is being made, and
    /// those done (`send_box_thumbnails.rs`).
    /// Each job has its own number, so a stale job (a removed and re-added
    /// file) can't settle a newer one.
    pub(super) thumbnailing: HashMap<PathBuf, u64>,
    pub(super) thumbnailed: HashSet<PathBuf>,
    pub(super) thumbnail_jobs: u64,
    /// The "Paid Content" price box.
    pub(super) price: Option<PriceDialog>,
    /// The video editor opened from the tray or the edit box.
    pub(super) video_editor: Option<VideoEditorDialog>,
}

impl SendBoxUi {
    pub(super) fn new() -> Self {
        Self {
            photo_sizes: RefCell::new(HashMap::new()),
            sound_checking: HashSet::new(),
            sound_checked: HashSet::new(),
            thumbnailing: HashMap::new(),
            thumbnailed: HashSet::new(),
            thumbnail_jobs: 0,
            price: None,
            video_editor: None,
        }
    }

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
        }
    }
}

impl Default for SendBoxUi {
    fn default() -> Self {
        Self::new()
    }
}
