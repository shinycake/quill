//! The Premium feature preview: the promotion clip that is playing.
#![deny(clippy::indexing_slicing)]

use super::video_player::VideoPlayer;
use std::cell::{Cell, RefCell};

/// The muted looping player of the open feature preview and the file it
/// plays (`ui/premium_ui/promo.rs`).
#[derive(Default)]
pub(super) struct PremiumPreviewUi {
    pub(super) video: RefCell<Option<VideoPlayer>>,
    pub(super) file_id: Cell<Option<i32>>,
}

impl PremiumPreviewUi {
    /// Drop the player (the preview closed or moved to another feature).
    pub(super) fn stop(&self) {
        if let Ok(mut video) = self.video.try_borrow_mut() {
            *video = None;
        }
        self.file_id.set(None);
    }
}
