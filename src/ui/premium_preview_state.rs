//! The Premium feature preview: the promotion clip that is playing.

use super::native_video::NativeVideo;
use std::cell::{Cell, RefCell};

/// The muted looping player of the open feature preview and the file it
/// plays (`ui/premium_ui/promo.rs`).
#[derive(Default)]
pub(super) struct PremiumPreviewUi {
    pub(super) video: RefCell<Option<NativeVideo>>,
    pub(super) file_id: Cell<Option<i32>>,
}

impl PremiumPreviewUi {
    /// Drop the player (the preview closed or moved to another feature).
    pub(super) fn stop(&self) {
        *self.video.borrow_mut() = None;
        self.file_id.set(None);
    }
}
