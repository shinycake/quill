//! State of the send box (the attachment tray above the composer): the
//! "High quality" and paid media choices, the price box and the video
//! editor. Behaviour is in `send_box_options.rs` and `video_editor.rs`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::send_box_options::PriceDialog;
use super::video_editor::VideoEditorDialog;

pub(crate) struct SendBoxUi {
    /// "High quality": photos keep up to 2560 px instead of 1280 px.
    pub(super) hd_photos: bool,
    /// Stars to unlock the media; 0 sends a normal message.
    pub(super) paid_stars: i64,
    /// Sizes read from the header of tray photos, once per file.
    pub(super) photo_sizes: RefCell<HashMap<PathBuf, Option<(u32, u32)>>>,
    /// Tray videos whose sound track is being read in the background, and
    /// those already read (tdesktop `hasAudio`).
    pub(super) sound_checking: HashSet<PathBuf>,
    pub(super) sound_checked: HashSet<PathBuf>,
    /// Tray pictures sent as files whose thumbnail is being made, and
    /// those done (`send_box_thumbnails.rs`).
    pub(super) thumbnailing: HashSet<PathBuf>,
    pub(super) thumbnailed: HashSet<PathBuf>,
    /// The "Paid Content" price box.
    pub(super) price: Option<PriceDialog>,
    /// The video editor opened from the tray or the edit box.
    pub(super) video_editor: Option<VideoEditorDialog>,
}

impl SendBoxUi {
    pub(super) fn new() -> Self {
        Self {
            hd_photos: false,
            paid_stars: 0,
            photo_sizes: RefCell::new(HashMap::new()),
            sound_checking: HashSet::new(),
            sound_checked: HashSet::new(),
            thumbnailing: HashSet::new(),
            thumbnailed: HashSet::new(),
            price: None,
            video_editor: None,
        }
    }

    /// The choices made for one batch end with it.
    pub(super) fn reset(&mut self) {
        self.hd_photos = false;
        self.paid_stars = 0;
        self.photo_sizes.borrow_mut().clear();
        self.sound_checking.clear();
        self.sound_checked.clear();
        self.thumbnailing.clear();
        self.thumbnailed.clear();
    }
}

impl Default for SendBoxUi {
    fn default() -> Self {
        Self::new()
    }
}
