//! Mask sticker sets: the installed and archived lists, and the mask
//! stickers the photo editor offers.
//!
//! tdesktop keeps masks apart from regular stickers (its own installed and
//! archived order, `StickersType::Masks`) and shows them in the story
//! editor's Masks tab and a two-tab manage box (Masks, Archived). This is
//! the same split: a `MasksPanel` that never shares a slot with the regular
//! sticker panel.
use crate::state::fetch::Fetch;
use crate::state::{PendingRequest, RequestPurpose, Session, StickersPurpose};
use crate::telegram::envelope::{StickerItem, StickerSetInfo};
use std::collections::HashMap;

/// The line a failed mask list or change leaves under the dialog.
const MASK_LIST_FAILED: &str = "Could not load mask sets";

/// The page size `getArchivedStickerSets` is asked for.
pub const ARCHIVED_MASK_PAGE: usize = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MasksTab {
    #[default]
    Installed,
    Archived,
}

/// What the mask surfaces want loaded: the manage dialog, the photo
/// editor's strip, both or neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MasksWanted {
    #[default]
    Nothing,
    Dialog,
    Strip,
    Both,
}

impl MasksWanted {
    pub fn dialog_open(self) -> bool {
        matches!(self, Self::Dialog | Self::Both)
    }

    pub fn strip_wanted(self) -> bool {
        matches!(self, Self::Strip | Self::Both)
    }

    fn with(dialog: bool, strip: bool) -> Self {
        match (dialog, strip) {
            (false, false) => Self::Nothing,
            (true, false) => Self::Dialog,
            (false, true) => Self::Strip,
            (true, true) => Self::Both,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MasksPanel {
    /// Which surfaces want the lists: the manage dialog, the strip.
    pub wanted: MasksWanted,
    pub tab: MasksTab,
    /// Installed mask sets, in TDLib's order. A failed list request or
    /// change leaves the list `Failed`, which the dialog offers a retry for.
    pub installed: Fetch<Vec<StickerSetInfo>>,
    pub archived: Fetch<Vec<StickerSetInfo>>,
    pub archived_has_more: bool,
    pub archived_next_offset: i64,
    /// Stickers of each mask set (`getStickerSet`), by set id: asked once
    /// per set, so a set that failed stays failed and the strip skips it.
    pub sets: HashMap<i64, Fetch<Vec<StickerItem>>>,
    /// The set whose Remove button was pressed and now asks to confirm.
    pub confirm_remove: Option<i64>,
}

impl MasksPanel {
    /// The manage dialog is open.
    pub fn dialog_open(&self) -> bool {
        self.wanted.dialog_open()
    }

    /// The photo editor's Masks strip is on screen and wants mask stickers.
    pub fn strip_wanted(&self) -> bool {
        self.wanted.strip_wanted()
    }

    pub fn set_dialog_open(&mut self, open: bool) {
        self.wanted = MasksWanted::with(open, self.strip_wanted());
    }

    pub fn set_strip_wanted(&mut self, wanted: bool) {
        self.wanted = MasksWanted::with(self.dialog_open(), wanted);
    }

    /// The loaded stickers of a mask set.
    pub fn set_stickers(&self, set_id: i64) -> &[StickerItem] {
        self.sets.get(&set_id).map(Fetch::items).unwrap_or(&[])
    }
}

impl Session {
    pub fn accept_installed_mask_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.masks.installed.succeed(sets);
    }

    pub fn accept_archived_mask_sets(&mut self, sets: Vec<StickerSetInfo>, first_page: bool) {
        let masks = &mut self.stickers.masks;
        let last = sets.last().map(|set| set.id).unwrap_or(0);
        masks.archived_has_more =
            sets.len() == ARCHIVED_MASK_PAGE && last > 0 && last != masks.archived_next_offset;
        masks.archived_next_offset = last;
        let mut all = if first_page {
            Vec::new()
        } else {
            masks.archived.take_data().unwrap_or_default()
        };
        let mut seen: std::collections::HashSet<i64> = all.iter().map(|set| set.id).collect();
        all.extend(sets.into_iter().filter(|set| seen.insert(set.id)));
        masks.archived.succeed(all);
    }

    pub(crate) fn accept_mask_set(&mut self, set_id: i64, stickers: Vec<StickerItem>) {
        self.stickers
            .masks
            .sets
            .entry(set_id)
            .or_default()
            .succeed(stickers);
    }

    /// `updateInstalledStickerSets` for masks: apply TDLib's order in place.
    pub fn apply_mask_order(&mut self, ids: &[i64]) {
        let Some(sets) = self.stickers.masks.installed.data_mut() else {
            return;
        };
        if sets.is_empty() || ids.is_empty() {
            return;
        }
        let rank: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        sets.sort_by_key(|set| rank.get(&set.id).copied().unwrap_or(usize::MAX));
    }

    /// A mask change (`changeStickerSet` or reorder) is in flight.
    pub fn mask_change_pending(&self) -> bool {
        self.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Stickers(
                    StickersPurpose::ChangeMaskSet { .. }
                        | StickersPurpose::ReorderInstalledMaskSets
                )
            )
        })
    }

    /// A request that changes this mask set is in flight: its own
    /// `changeStickerSet`, or a reorder (which moves every row).
    pub fn mask_set_change_pending(&self, set_id: i64) -> bool {
        self.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Stickers(StickersPurpose::ChangeMaskSet { set_id: id, .. })
                    if id == set_id
            ) || matches!(
                pending.purpose,
                RequestPurpose::Stickers(StickersPurpose::ReorderInstalledMaskSets)
            )
        })
    }

    /// A mask change (`changeStickerSet` or reorder) succeeded: drop both
    /// lists so the dialog refetches what TDLib now holds.
    pub(crate) fn finish_mask_mutation(&mut self, purpose: Option<RequestPurpose>) {
        if matches!(
            purpose,
            Some(
                RequestPurpose::Stickers(StickersPurpose::ChangeMaskSet { .. })
                    | RequestPurpose::Stickers(StickersPurpose::ReorderInstalledMaskSets)
            )
        ) {
            // A list asked before the change answers with what the change
            // replaced: dropped, so it neither lands as the new list nor
            // keeps the refetch from going out.
            for purpose in [
                StickersPurpose::GetInstalledMaskSets,
                StickersPurpose::GetArchivedMaskSets { first_page: true },
                StickersPurpose::GetArchivedMaskSets { first_page: false },
            ] {
                while self.requests.take_purpose(purpose.into()).is_some() {}
            }
            let masks = &mut self.stickers.masks;
            masks.installed = Fetch::Idle;
            masks.archived = Fetch::Idle;
            masks.archived_has_more = false;
            masks.archived_next_offset = 0;
            masks.confirm_remove = None;
            masks.sets.clear();
        }
    }

    /// A mask list or change request failed. A single set that won't load
    /// is not worth a banner; the strip skips it (it stays `Failed`).
    pub(crate) fn fail_mask_request(&mut self, pending: Option<&PendingRequest>) {
        let purpose = pending.map(|p| p.purpose);
        let masks = &mut self.stickers.masks;
        match purpose {
            Some(RequestPurpose::Stickers(StickersPurpose::GetMaskSet { set_id })) => {
                masks.sets.entry(set_id).or_default().fail("failed");
            }
            Some(RequestPurpose::Stickers(StickersPurpose::GetInstalledMaskSets)) => {
                masks.installed.fail(MASK_LIST_FAILED);
            }
            Some(RequestPurpose::Stickers(StickersPurpose::GetArchivedMaskSets { .. })) => {
                masks.archived.fail(MASK_LIST_FAILED);
            }
            Some(RequestPurpose::Stickers(
                StickersPurpose::ChangeMaskSet { .. } | StickersPurpose::ReorderInstalledMaskSets,
            )) => {
                masks.installed.fail(MASK_LIST_FAILED);
                masks.archived.fail(MASK_LIST_FAILED);
            }
            _ => return,
        }
        if !matches!(
            purpose,
            Some(RequestPurpose::Stickers(StickersPurpose::GetMaskSet { .. }))
        ) && let Some(pending) = pending
        {
            let _ = self.stickers.manager.note_commit_failure(pending.id);
        }
    }
}
