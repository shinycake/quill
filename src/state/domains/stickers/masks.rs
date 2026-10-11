//! Mask sticker sets: the installed and archived lists, and the mask
//! stickers the photo editor offers.
//!
//! tdesktop keeps masks apart from regular stickers (its own installed and
//! archived order, `StickersType::Masks`) and shows them in the story
//! editor's Masks tab and a two-tab manage box (Masks, Archived). This is
//! the same split: a `MasksPanel` that never shares a slot with the regular
//! sticker panel.
use crate::state::{RequestPurpose, Session, StickersPurpose};
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
    /// Installed mask sets, in TDLib's order.
    pub installed: Vec<StickerSetInfo>,
    pub installed_loaded: bool,
    pub archived: Vec<StickerSetInfo>,
    pub archived_loaded: bool,
    pub archived_has_more: bool,
    pub archived_next_offset: i64,
    /// A list request or change failed; the dialog offers a retry.
    pub failed: bool,
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

impl Session {
    pub fn accept_installed_mask_sets(&mut self, sets: Vec<StickerSetInfo>) {
        let masks = &mut self.stickers.masks;
        masks.installed = sets;
        masks.installed_loaded = true;
        masks.failed = false;
    }

    pub fn accept_archived_mask_sets(&mut self, sets: Vec<StickerSetInfo>, first_page: bool) {
        let masks = &mut self.stickers.masks;
        let last = sets.last().map(|set| set.id).unwrap_or(0);
        masks.archived_has_more =
            sets.len() == ARCHIVED_MASK_PAGE && last > 0 && last != masks.archived_next_offset;
        masks.archived_next_offset = last;
        if first_page {
            masks.archived.clear();
        }
        let mut seen: std::collections::HashSet<i64> =
            masks.archived.iter().map(|set| set.id).collect();
        masks
            .archived
            .extend(sets.into_iter().filter(|set| seen.insert(set.id)));
        masks.archived_loaded = true;
        masks.failed = false;
    }

    pub(crate) fn accept_mask_set(&mut self, set_id: i64, stickers: Vec<StickerItem>) {
        self.stickers.masks.set_stickers.insert(set_id, stickers);
    }

    /// `updateInstalledStickerSets` for masks: apply TDLib's order in place.
    pub fn apply_mask_order(&mut self, ids: &[i64]) {
        let sets = &mut self.stickers.masks.installed;
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
            let masks = &mut self.stickers.masks;
            masks.installed_loaded = false;
            masks.archived_loaded = false;
            masks.archived_has_more = false;
            masks.archived_next_offset = 0;
            masks.failed = false;
            masks.confirm_remove = None;
            masks.set_requested.clear();
            masks.set_stickers.clear();
        }
    }

    /// A mask list or change request failed. A single set that won't load
    /// is not worth a banner; the strip skips it (it stays in `set_requested`).
    pub(crate) fn fail_mask_request(&mut self, purpose: Option<RequestPurpose>) {
        if let Some(RequestPurpose::Stickers(
            StickersPurpose::GetInstalledMaskSets
            | StickersPurpose::GetArchivedMaskSets { .. }
            | StickersPurpose::ChangeMaskSet { .. }
            | StickersPurpose::ReorderInstalledMaskSets,
        )) = purpose
        {
            self.stickers.masks.failed = true;
        }
    }
}
