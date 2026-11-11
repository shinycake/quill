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

/// The page size `getArchivedStickerSets` is asked for.
pub const ARCHIVED_MASK_PAGE: usize = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MasksTab {
    #[default]
    Installed,
    Archived,
}

#[derive(Debug, Clone, Default)]
pub struct MasksPanel {
    /// The manage dialog is open.
    pub open: bool,
    pub tab: MasksTab,
    /// Installed mask sets, in TDLib's order. `Loading` with no request in
    /// flight means a refetch is owed (a change took); a failed list or
    /// change is `Failed` (the rows stay) and the dialog offers a retry.
    pub installed: Fetch<Vec<StickerSetInfo>>,
    pub archived: Fetch<Vec<StickerSetInfo>>,
    /// The offset of the next archived page; `None` when there is none.
    pub archived_more: Option<i64>,
    /// Stickers of each loaded mask set (`getStickerSet`), by set id.
    pub set_stickers: HashMap<i64, Vec<StickerItem>>,
    /// Set ids whose `getStickerSet` is in flight or failed, so the editor
    /// strip asks once per set.
    pub set_requested: std::collections::HashSet<i64>,
    /// The photo editor's Masks strip is on screen and wants mask stickers.
    pub strip_wanted: bool,
    /// The set whose Remove button was pressed and now asks to confirm.
    pub confirm_remove: Option<i64>,
}

impl MasksPanel {
    /// The installed mask sets held.
    pub fn installed_sets(&self) -> &[StickerSetInfo] {
        self.installed.data().map(Vec::as_slice).unwrap_or_default()
    }

    /// The archived mask sets held.
    pub fn archived_sets(&self) -> &[StickerSetInfo] {
        self.archived.data().map(Vec::as_slice).unwrap_or_default()
    }

    /// A list request or change failed.
    pub fn failed(&self) -> bool {
        self.installed.error().is_some() || self.archived.error().is_some()
    }
}

impl Session {
    pub fn accept_installed_mask_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.masks.installed.succeed(sets);
    }

    pub fn accept_archived_mask_sets(&mut self, sets: Vec<StickerSetInfo>, first_page: bool) {
        let masks = &mut self.stickers.masks;
        let last = sets.last().map(|set| set.id).unwrap_or(0);
        let previous = masks.archived_more.unwrap_or(0);
        masks.archived_more =
            (sets.len() == ARCHIVED_MASK_PAGE && last > 0 && last != previous).then_some(last);
        let mut held = if first_page {
            Vec::new()
        } else {
            masks.archived.take_data().unwrap_or_default()
        };
        let mut seen: std::collections::HashSet<i64> = held.iter().map(|set| set.id).collect();
        held.extend(sets.into_iter().filter(|set| seen.insert(set.id)));
        masks.archived.succeed(held);
    }

    pub(crate) fn accept_mask_set(&mut self, set_id: i64, stickers: Vec<StickerItem>) {
        self.stickers.masks.set_stickers.insert(set_id, stickers);
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
            // Both lists are owed again; the rows show meanwhile.
            for list in [&mut masks.installed, &mut masks.archived] {
                if !list.is_idle() {
                    list.begin();
                }
            }
            masks.archived_more = None;
            masks.confirm_remove = None;
            masks.set_requested.clear();
            masks.set_stickers.clear();
        }
    }

    /// A mask list or change request failed. A single set that won't load
    /// is not worth a banner; the strip skips it (it stays in `set_requested`).
    pub(crate) fn fail_mask_request(&mut self, pending: Option<&PendingRequest>) {
        let purpose = pending.map(|p| p.purpose);
        if let Some(RequestPurpose::Stickers(
            StickersPurpose::GetInstalledMaskSets
            | StickersPurpose::GetArchivedMaskSets { .. }
            | StickersPurpose::ChangeMaskSet { .. }
            | StickersPurpose::ReorderInstalledMaskSets,
        )) = purpose
        {
            let masks = &mut self.stickers.masks;
            let error = "The mask sets request failed.";
            match purpose {
                Some(RequestPurpose::Stickers(StickersPurpose::GetInstalledMaskSets)) => {
                    masks.installed.fail(error)
                }
                Some(RequestPurpose::Stickers(StickersPurpose::GetArchivedMaskSets { .. })) => {
                    masks.archived.fail(error)
                }
                _ => {
                    masks.installed.fail(error);
                    masks.archived.fail(error);
                }
            }
            if let Some(pending) = pending {
                let _ = self.stickers.manager.note_commit_failure(pending.id);
            }
        }
    }
}
