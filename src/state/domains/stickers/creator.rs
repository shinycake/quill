//! Editing an owned sticker or custom emoji set from the set box, after
//! Telegram Desktop's `api/api_stickers_creator.cpp` and the creator parts
//! of `boxes/sticker_set_box.cpp`: add a sticker (new image or an existing
//! sticker), remove one, move one, rename the set, delete the set. The
//! requests are the TDLib creator methods (`addStickerToSet`,
//! `removeStickerFromSet`, `setStickerPositionInSet`, `setStickerSetTitle`,
//! `deleteStickerSet`); every success refreshes the open set
//! (`getStickerSet`) the way tdesktop re-applies the answered set.
use crate::ids::FileId;
use crate::state::*;
use crate::sticker_set_box::{
    ADD_EXISTING_FAILED_NOTE, ADDED_NOTE, EMOJI_ADDED_NOTE, UPLOAD_FAILED_NOTE,
};

/// Edits of an owned set; wrapped as
/// [`StickersPurpose::Creator`](super::StickersPurpose::Creator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickerCreatorPurpose {
    /// `addStickerToSet` with a freshly rendered image (`inputFileLocal`).
    /// `emoji` says whether the set is a custom emoji set, so the toast
    /// reads "Emoji added." even when no box is open. Response is `ok`.
    AddNewSticker { set_id: i64, emoji: bool },
    /// `addStickerToSet` with a sticker that already exists
    /// (`inputFileId`). Response is `ok`.
    AddExistingSticker { set_id: i64, emoji: bool },
    /// `removeStickerFromSet`. Response is `ok`.
    RemoveSticker { set_id: i64, file_id: FileId },
    /// `setStickerPositionInSet`. Response is `ok`.
    MoveSticker { set_id: i64 },
    /// `setStickerSetTitle`. Response is `ok`.
    RenameSet { set_id: i64 },
    /// `deleteStickerSet`. Response is `ok`; the box closes.
    DeleteSet { set_id: i64 },
}

impl StickerCreatorPurpose {
    /// The set the edit belongs to.
    pub fn set_id(self) -> i64 {
        match self {
            Self::AddNewSticker { set_id, .. }
            | Self::AddExistingSticker { set_id, .. }
            | Self::RemoveSticker { set_id, .. }
            | Self::MoveSticker { set_id }
            | Self::RenameSet { set_id }
            | Self::DeleteSet { set_id } => set_id,
        }
    }
}

/// What the open set box knows about its edits: one stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StickerSetEdits {
    /// No edit out and the box shows the server's set.
    #[default]
    Idle,
    /// Creator requests TDLib has not answered yet, each answer matched by
    /// its request: an answer of an edit made under an earlier box never
    /// ends one of this box's.
    InFlight(Vec<RequestId>),
    /// An edit was confirmed (or never reached the server): the view asks
    /// `getStickerSet` again (without going back to the loading stage) so
    /// the box shows the server's set. `out` are the edits still being
    /// answered; the ask waits for them.
    NeedsRefresh { out: Vec<RequestId> },
    /// The refresh is out as `request`: only its answer or error ends it.
    /// An edit confirmed meanwhile makes it stale (`NeedsRefresh` again, so
    /// its answer is dropped and a new one asked); `out` are the edits
    /// still being answered.
    Refreshing {
        request: RequestId,
        out: Vec<RequestId>,
    },
    /// `deleteStickerSet` was confirmed: the box closes.
    Deleted,
}

impl StickerSetEdits {
    /// An edit is still being answered.
    pub fn busy(&self) -> bool {
        match self {
            Self::InFlight(out) | Self::NeedsRefresh { out } | Self::Refreshing { out, .. } => {
                !out.is_empty()
            }
            Self::Idle | Self::Deleted => false,
        }
    }

    /// The edits still being answered.
    pub fn in_flight(&self) -> &[RequestId] {
        match self {
            Self::InFlight(out) | Self::NeedsRefresh { out } | Self::Refreshing { out, .. } => out,
            Self::Idle | Self::Deleted => &[],
        }
    }

    /// The box owes a `getStickerSet`.
    pub fn refresh_needed(&self) -> bool {
        matches!(self, Self::NeedsRefresh { .. })
    }

    /// The set was deleted: the box closes.
    pub fn deleted(&self) -> bool {
        matches!(self, Self::Deleted)
    }

    /// A creator request went out.
    pub fn begin(&mut self, request: RequestId) {
        match self {
            Self::Idle => *self = Self::InFlight(vec![request]),
            Self::InFlight(out) | Self::NeedsRefresh { out } | Self::Refreshing { out, .. } => {
                out.push(request);
            }
            Self::Deleted => {}
        }
    }

    /// An edit was confirmed or never reached the server: ask again once
    /// the edits out have been answered.
    pub fn need_refresh(&mut self) {
        match std::mem::take(self) {
            Self::Idle => *self = Self::NeedsRefresh { out: Vec::new() },
            Self::InFlight(out) | Self::NeedsRefresh { out } | Self::Refreshing { out, .. } => {
                *self = Self::NeedsRefresh { out };
            }
            Self::Deleted => *self = Self::Deleted,
        }
    }

    /// The refresh went out as `request`.
    pub fn refresh_sent(&mut self, request: RequestId) {
        if let Self::NeedsRefresh { out } = std::mem::take(self) {
            *self = Self::Refreshing { request, out };
        }
    }

    /// The refresh could not be sent: it is not asked again from every
    /// poll.
    pub fn refresh_unsent(&mut self) {
        if let Self::NeedsRefresh { out } = std::mem::take(self) {
            *self = Self::after_refresh(out, false);
        }
    }

    /// The refresh `request` was refused: the box keeps what it shows.
    /// `false` when it is not the refresh this box waits for.
    pub fn refresh_failed(&mut self, request: Option<RequestId>) -> bool {
        match std::mem::take(self) {
            Self::Refreshing {
                request: asked,
                out,
            } if Some(asked) == request => {
                *self = Self::after_refresh(out, false);
                true
            }
            other => {
                *self = other;
                false
            }
        }
    }

    /// The set's contents arrived as `request`. Only the refresh this box
    /// waits for lands. An edit still in flight (a move applied locally)
    /// may have changed the set after the answer was built, so the box asks
    /// again once it is answered; otherwise the answer is current. `false`
    /// when it is not the refresh this box waits for.
    pub fn refresh_arrived(&mut self, request: Option<RequestId>) -> bool {
        match std::mem::take(self) {
            Self::Refreshing {
                request: asked,
                out,
            } if Some(asked) == request => {
                *self = Self::after_refresh(out, true);
                true
            }
            other => {
                *self = other;
                false
            }
        }
    }

    /// The first load of the set arrived (the view had no refresh out).
    pub fn set_arrived(&mut self) {
        *self = match std::mem::take(self) {
            Self::InFlight(out) | Self::NeedsRefresh { out } if !out.is_empty() => {
                Self::NeedsRefresh { out }
            }
            Self::Deleted => Self::Deleted,
            _ => Self::Idle,
        };
    }

    fn after_refresh(out: Vec<RequestId>, landed: bool) -> Self {
        match (out.is_empty(), landed) {
            (true, _) => Self::Idle,
            (false, true) => Self::NeedsRefresh { out },
            (false, false) => Self::InFlight(out),
        }
    }

    /// The set was deleted.
    pub fn delete(&mut self) {
        *self = Self::Deleted;
    }

    /// `request` was answered (`ok` or an error); `apply` says the answer
    /// changed the set, so the box refreshes.
    pub fn finish(&mut self, request: Option<RequestId>, apply: bool) {
        match std::mem::take(self) {
            Self::InFlight(mut out) => {
                out.retain(|held| Some(*held) != request);
                *self = if out.is_empty() {
                    Self::Idle
                } else {
                    Self::InFlight(out)
                };
            }
            Self::NeedsRefresh { mut out } => {
                out.retain(|held| Some(*held) != request);
                *self = Self::NeedsRefresh { out };
            }
            Self::Refreshing {
                request: asked,
                mut out,
            } => {
                out.retain(|held| Some(*held) != request);
                *self = Self::Refreshing {
                    request: asked,
                    out,
                };
            }
            other => *self = other,
        }
        if apply {
            self.need_refresh();
        }
    }
}

impl Session {
    /// A creator request went out for the open set.
    pub(crate) fn begin_sticker_set_edit(&mut self, set_id: i64, request: RequestId) {
        if let Some(view) = self.stickers.sticker_set_view.as_mut()
            && view.set_id == set_id
        {
            view.edits.begin(request);
        }
    }

    /// The next toast of an edit of an owned set, oldest first.
    pub fn take_sticker_set_edit_note(&mut self) -> Option<String> {
        self.stickers.creator_notes.pop_front()
    }

    /// A creator request was answered (`ok` or an error).
    fn finish_sticker_set_edit(
        &mut self,
        set_id: i64,
        request: Option<RequestId>,
        note: Option<String>,
        apply: bool,
    ) {
        if let Some(note) = note {
            self.stickers.creator_notes.push_back(note);
        }
        let Some(view) = self.stickers.sticker_set_view.as_mut() else {
            return;
        };
        if view.set_id != set_id {
            return;
        }
        view.edits.finish(request, apply);
    }

    /// The current user created `set_id` (tdesktop `AmCreator`): the open
    /// box says so, or the installed sticker or emoji lists do.
    pub fn is_owned_sticker_set(&self, set_id: i64) -> bool {
        self.owned_sticker_set_name(set_id).is_some()
    }

    /// The short name of an owned set, from the open box or the installed
    /// lists; `None` when the set is not ours or unknown.
    pub fn owned_sticker_set_name(&self, set_id: i64) -> Option<String> {
        self.owned_sticker_set(set_id).map(|(name, _)| name)
    }

    /// The short name of an owned set and whether it is a custom emoji
    /// set, from the open box or the installed lists.
    pub fn owned_sticker_set(&self, set_id: i64) -> Option<(String, bool)> {
        if let Some(view) = self.stickers.sticker_set_view.as_ref()
            && view.set_id == set_id
            && let StickerSetViewStage::Ready {
                name,
                is_owned: true,
                is_emoji,
                ..
            } = &view.stage
            && !name.is_empty()
        {
            return Some((name.clone(), *is_emoji));
        }
        let stickers = self.stickers.stickers.sets.iter().map(|set| (set, false));
        let emoji = self
            .stickers
            .emoji
            .lists
            .installed
            .items()
            .iter()
            .map(|set| (set, true));
        stickers
            .chain(emoji)
            .find(|(set, _)| set.id == set_id && set.is_owned && !set.name.is_empty())
            .map(|(set, is_emoji)| (set.name.clone(), is_emoji))
    }

    /// How many stickers an owned set holds, from the open box or the
    /// installed lists (tdesktop checks `set->count` before adding).
    pub fn owned_sticker_set_count(&self, set_id: i64) -> Option<usize> {
        if let Some(view) = self.stickers.sticker_set_view.as_ref()
            && view.set_id == set_id
            && let StickerSetViewStage::Ready { stickers, .. } = &view.stage
        {
            return Some(stickers.len());
        }
        self.stickers
            .stickers
            .sets
            .iter()
            .chain(self.stickers.emoji.lists.installed.items())
            .find(|set| set.id == set_id)
            .map(|set| usize::try_from(set.size).unwrap_or(0))
    }

    /// The installed sets the user created, of one kind, as `(id, title)`
    /// (tdesktop `AddAddToOwnedSetAction` lists them under "Add to Sticker
    /// Set" / "Add to Emoji Set").
    pub fn owned_sticker_sets(&self, emoji: bool) -> Vec<(i64, String)> {
        let sets = if emoji {
            self.stickers.emoji.lists.installed.items()
        } else {
            &self.stickers.stickers.sets
        };
        sets.iter()
            .filter(|set| set.is_owned)
            .map(|set| {
                let title = if set.title.is_empty() {
                    set.name.clone()
                } else {
                    set.title.clone()
                };
                (set.id, title)
            })
            .collect()
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
        let request = pending.map(|p| p.id);
        match purpose {
            StickerCreatorPurpose::AddNewSticker { emoji, .. }
            | StickerCreatorPurpose::AddExistingSticker { emoji, .. } => {
                let note = if emoji { EMOJI_ADDED_NOTE } else { ADDED_NOTE };
                self.finish_sticker_set_edit(set_id, request, Some(note.into()), true);
            }
            StickerCreatorPurpose::RemoveSticker { file_id, .. } => {
                // The sticker leaves the box at once; the refresh confirms.
                if let Some(view) = self.stickers.sticker_set_view.as_mut()
                    && view.set_id == set_id
                    && let StickerSetViewStage::Ready { stickers, .. } = &mut view.stage
                {
                    stickers.retain(|sticker| sticker.file_id != file_id);
                }
                self.finish_sticker_set_edit(set_id, request, None, true);
            }
            StickerCreatorPurpose::MoveSticker { .. } => {
                // The move was applied locally when it went out; a refresh
                // would make the stickers blink (tdesktop skips it too).
                self.finish_sticker_set_edit(set_id, request, None, false);
            }
            StickerCreatorPurpose::RenameSet { .. } => {
                self.finish_sticker_set_edit(set_id, request, None, true);
            }
            StickerCreatorPurpose::DeleteSet { .. } => {
                if let Some(view) = self.stickers.sticker_set_view.as_mut()
                    && view.set_id == set_id
                {
                    view.edits.delete();
                }
                self.finish_sticker_set_edit(set_id, request, None, false);
            }
        }
        self.media.media_library.remove(&set_id);
        if !matches!(purpose, StickerCreatorPurpose::MoveSticker { .. }) {
            self.invalidate_installed_sticker_sets();
            // Outdated, not idle: the driver fetches both installed lists
            // again so menus and the panel show the server's sets.
            self.stickers.stickers.lists.installed.outdate();
            self.stickers.emoji.lists.installed.outdate();
        }
    }

    /// Called by [`Session::apply_stickers_error`]: an edit failed. The
    /// box shows tdesktop's wording for a failed upload or add (tdesktop
    /// shows the server's error type first, but [`TdError`] does not keep
    /// the message, so the fallback is used), and the error code for the
    /// rest.
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
        self.finish_sticker_set_edit(purpose.set_id(), pending.map(|p| p.id), Some(note), refresh);
    }

    /// Reorder mode moved a sticker: apply it to the open view at once
    /// (tdesktop `base::reorder(_pack, ...)`), returning the sticker's new
    /// position in the set, or `None` when the move is a no-op.
    pub fn move_viewed_sticker(&mut self, set_id: i64, from: usize, to: usize) -> Option<usize> {
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
