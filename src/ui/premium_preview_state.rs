//! The Premium feature preview: the promotion clip that is playing.
#![deny(clippy::indexing_slicing)]

use super::video_player::VideoPlayer;
use std::cell::RefCell;

/// The clip slot of the open feature preview.
#[derive(Clone, Default)]
pub(super) enum ClipSlot {
    /// No clip opened for the feature on show.
    #[default]
    Idle,
    /// The muted looping player of `file_id`.
    Playing { file_id: i32, player: VideoPlayer },
    /// `file_id` could not be opened. It is not opened again while the
    /// preview shows it; another feature or a new preview starts afresh.
    Failed { file_id: i32 },
}

impl ClipSlot {
    /// The file this slot is for, when it holds one.
    pub(super) fn file_id(&self) -> Option<i32> {
        match self {
            Self::Idle => None,
            Self::Playing { file_id, .. } | Self::Failed { file_id } => Some(*file_id),
        }
    }
}

/// The player of the open feature preview and the file it plays
/// (`ui/premium_ui/promo.rs`).
#[derive(Default)]
pub(super) struct PremiumPreviewUi {
    pub(super) slot: RefCell<ClipSlot>,
}

/// The clip player is dropped with the value; the Premium box that shows
/// it closes from `PaymentUi`'s farewell.
impl crate::ui::on_screen::Farewell for PremiumPreviewUi {}

impl PremiumPreviewUi {
    /// Drop the player (the preview closed or moved to another feature).
    pub(super) fn stop(&self) {
        if let Ok(mut slot) = self.slot.try_borrow_mut() {
            *slot = ClipSlot::Idle;
        }
    }
}
