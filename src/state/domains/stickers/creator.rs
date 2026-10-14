//! Editing an owned sticker or custom emoji set from the set box, after
//! Telegram Desktop's `api/api_stickers_creator.cpp` and the creator parts
//! of `boxes/sticker_set_box.cpp`: add a sticker (new image or an existing
//! sticker), remove one, move one, rename the set, delete the set. The
//! requests are the TDLib creator methods (`addStickerToSet`,
//! `removeStickerFromSet`, `setStickerPositionInSet`, `setStickerSetTitle`,
//! `deleteStickerSet`); every success refreshes the open set
//! (`getStickerSet`) the way tdesktop re-applies the answered set.
use crate::ids::FileId;
use crate::sticker_set_box::{
    ADDED_NOTE, ADD_EXISTING_FAILED_NOTE, EMOJI_ADDED_NOTE, UPLOAD_FAILED_NOTE,
};
use crate::state::*;

/// Edits of an owned set; wrapped as
/// [`StickersPurpose::Creator`](super::StickersPurpose::Creator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerCreatorPurpose {
    /// `addStickerToSet` with a freshly rendered image (`inputFileLocal`).
    /// Response is `ok`.
    AddNewSticker {
        set_id: i64,
    },
    /// `addStickerToSet` with a sticker that already exists
    /// (`inputFileId`). Response is `ok`.
    AddExistingSticker {
        set_id: i64,
    },
    /// `removeStickerFromSet`. Response is `ok`.
    RemoveSticker {
        set_id: i64,
        file_id: FileId,
    },
    /// `setStickerPositionInSet`. Response is `ok`.
    MoveSticker {
        set_id: i64,
    },
    /// `setStickerSetTitle`. Response is `ok`.
    RenameSet {
        set_id: i64,
    },
    /// `deleteStickerSet`. Response is `ok`; the box closes.
    DeleteSet {
        set_id: i64,
    },
}

impl StickerCreatorPurpose {
    /// The set the edit belongs to.
    pub fn set_id(self) -> i64 {
        match self {
            Self::AddNewSticker { set_id }
            | Self::AddExistingSticker { set_id }
            | Self::RemoveSticker { set_id, .. }
            | Self::MoveSticker { set_id }
            | Self::RenameSet { set_id }
            | Self::DeleteSet { set_id } => set_id,
        }
    }
}

/// What the open set box knows about its edits in flight.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerSetEdits {
    /// Creator requests TDLib has not answered yet.
    pub pending: u32,
    /// An edit was confirmed: the view asks `getStickerSet` again (without
    /// going back to the loading stage) so the box shows the server's set.
    pub refresh_needed: bool,
    /// `deleteStickerSet` was confirmed: the box closes.
    pub deleted: bool,
    /// A one-shot toast for the UI: what tdesktop shows after an edit
    /// ("Sticker added.", the error, ...).
    pub note: Option<String>,
}

impl StickerSetEdits {
    /// An edit is still being answered.
    pub fn busy(&self) -> bool {
        self.pending > 0
    }
}

impl Session {
    /// A creator request went out for the open set.
    pub(crate) fn begin_sticker_set_edit(&mut self, set_id: i64) {
        if let Some(view) = self.stickers.sticker_set_view.as_mut()
            && view.set_id == set_id
        {
            view.edits.pending = view.edits.pending.saturating_add(1);
        }
    }

    /// The one-shot toast of the open set's last edit.
    pub fn take_sticker_set_edit_note(&mut self) -> Option<String> {
        self.stickers
            .sticker_set_view
            .as_mut()
            .and_then(|view| view.edits.note.take())
    }

    /// A creator request was answered (`ok` or an error).
    fn finish_sticker_set_edit(&mut self, set_id: i64, note: Option<String>, apply: bool) {
        let Some(view) = self.stickers.sticker_set_view.as_mut() else {
            return;
        };
        if view.set_id != set_id {
            return;
        }
        view.edits.pending = view.edits.pending.saturating_sub(1);
        if note.is_some() {
            view.edits.note = note;
        }
        if apply {
            view.edits.refresh_needed = true;
        }
    }

    /// Called by [`Session::apply_ok`]: an edit of an owned set succeeded.
    /// The open view refreshes, the panel forgets the set's cached
    /// contents, and the installed list is refetched so renamed or deleted
    /// sets show as the server has them (tdesktop `feedSetFull` +
    /// `notifyUpdated`).
    pub(crate) fn apply_sticker_creator_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Stickers(StickersPurpose::Creator(purpose))) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        let set_id = purpose.set_id();
        match purpose {
            StickerCreatorPurpose::AddNewSticker { .. }
            | StickerCreatorPurpose::AddExistingSticker { .. } => {
                let is_emoji = self.sticker_set_view_is_emoji(set_id);
                let note = if is_emoji { EMOJI_ADDED_NOTE } else { ADDED_NOTE };
                self.finish_sticker_set_edit(set_id, Some(note.into()), true);
            }
            StickerCreatorPurpose::RemoveSticker { file_id, .. } => {
                // The sticker leaves the box at once; the refresh confirms.
                if let Some(view) = self.stickers.sticker_set_view.as_mut()
                    && view.set_id == set_id
                    && let StickerSetViewStage::Ready { stickers, .. } = &mut view.stage
                {
                    stickers.retain(|sticker| sticker.file_id != file_id);
                }
                self.finish_sticker_set_edit(set_id, None, true);
            }
            StickerCreatorPurpose::MoveSticker { .. } => {
                // The move was applied locally when it went out; a refresh
                // would make the stickers blink (tdesktop skips it too).
                self.finish_sticker_set_edit(set_id, None, false);
            }
            StickerCreatorPurpose::RenameSet { .. } => {
                self.finish_sticker_set_edit(set_id, None, true);
            }
            StickerCreatorPurpose::DeleteSet { .. } => {
                if let Some(view) = self.stickers.sticker_set_view.as_mut()
                    && view.set_id == set_id
                {
                    view.edits.deleted = true;
                }
                self.finish_sticker_set_edit(set_id, None, false);
            }
        }
        self.media.media_library.set_stickers.remove(&set_id);
        self.media.media_library.failed.remove(&set_id);
        if !matches!(purpose, StickerCreatorPurpose::MoveSticker { .. }) {
            self.invalidate_installed_sticker_sets();
            self.stickers.emoji.installed_sets.clear();
            self.stickers.emoji.outdated_packs.insert(set_id);
        }
    }

    /// Called by [`Session::apply_stickers_error`]: an edit failed. The
    /// box shows tdesktop's wording for a failed upload or add, and the
    /// error code for the rest (tdesktop toasts the raw error type).
    pub(crate) fn fail_sticker_creator(&mut self, err: &TdError, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Stickers(StickersPurpose::Creator(purpose))) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        let note = match purpose {
            StickerCreatorPurpose::AddNewSticker { .. } => UPLOAD_FAILED_NOTE.to_string(),
            StickerCreatorPurpose::AddExistingSticker { .. } => {
                ADD_EXISTING_FAILED_NOTE.to_string()
            }
            StickerCreatorPurpose::RemoveSticker { .. } => {
                call_request_error_line(err, "Couldn't delete the sticker")
            }
            StickerCreatorPurpose::MoveSticker { .. } => {
                call_request_error_line(err, "Couldn't move the sticker")
            }
            StickerCreatorPurpose::RenameSet { .. } => {
                call_request_error_line(err, "Couldn't rename the set")
            }
            StickerCreatorPurpose::DeleteSet { .. } => {
                call_request_error_line(err, "Couldn't delete the set")
            }
        };
        // A failed move or remove left the box out of step with the
        // server: refresh to show the real order.
        let refresh = matches!(
            purpose,
            StickerCreatorPurpose::MoveSticker { .. } | StickerCreatorPurpose::RemoveSticker { .. }
        );
        self.finish_sticker_set_edit(purpose.set_id(), Some(note), refresh);
    }

    fn sticker_set_view_is_emoji(&self, set_id: i64) -> bool {
        matches!(
            self.stickers.sticker_set_view.as_ref(),
            Some(StickerSetView {
                set_id: id,
                stage: StickerSetViewStage::Ready { is_emoji: true, .. },
                ..
            }) if *id == set_id
        )
    }

    /// Reorder mode moved a sticker: apply it to the open view at once
    /// (tdesktop `base::reorder(_pack, ...)`), returning the sticker's new
    /// position in the set, or `None` when the move is a no-op.
    pub(crate) fn move_viewed_sticker(&mut self, set_id: i64, from: usize, to: usize) -> Option<usize> {
        let view = self.stickers.sticker_set_view.as_mut()?;
        if view.set_id != set_id || from == to {
            return None;
        }
        let StickerSetViewStage::Ready { stickers, .. } = &mut view.stage else {
            return None;
        };
        if from >= stickers.len() || to >= stickers.len() {
            return None;
        }
        let moved = stickers.remove(from);
        stickers.insert(to, moved);
        Some(to)
    }
}
