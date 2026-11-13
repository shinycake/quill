//! Mask sticker sets: the installed and archived lists, and the mask
//! stickers the photo editor offers.
//!
//! tdesktop keeps masks apart from regular stickers (its own installed and
//! archived order, `StickersType::Masks`) and shows them in the story
//! editor's Masks tab and a two-tab manage box (Masks, Archived). This is
//! the same split: a `MasksPanel` that never shares a slot with the regular
//! sticker panel.
use crate::state::fetch::Fetch;
use crate::state::{PendingRequest, RequestId, RequestPurpose, Session, StickersPurpose};
use crate::telegram::envelope::{StickerItem, StickerSetInfo};
use std::collections::HashMap;

/// The line a failed mask list or change leaves under the dialog.
const MASK_LIST_FAILED: &str = "Could not load mask sets";
const MASK_CHANGE_FAILED: &str = "Could not change the mask sets";

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

/// The archived mask sets loaded so far and where the next page starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchivedMasks {
    pub sets: Vec<StickerSetInfo>,
    /// The set id the next page follows; `None` when the last page was the
    /// end.
    pub next_offset: Option<i64>,
}

/// What a mask change in flight changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskChangeTarget {
    /// `changeStickerSet` of this set.
    Set(i64),
    /// A reorder, which moves every row.
    Reorder,
}

/// A mask change in flight: it ends with its own request's answer, its
/// error, or the request being lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskChange {
    pub request: RequestId,
    pub target: MaskChangeTarget,
}

#[derive(Debug, Clone, Default)]
pub struct MasksPanel {
    /// Changes sent and not yet answered, one per request.
    pub changes: Vec<MaskChange>,
    /// Which surfaces want the lists: the manage dialog, the strip.
    pub wanted: MasksWanted,
    pub tab: MasksTab,
    /// Installed mask sets, in TDLib's order. A failed list request or
    /// change leaves the list `Failed`, which the dialog offers a retry for.
    pub installed: Fetch<Vec<StickerSetInfo>>,
    pub archived: Fetch<ArchivedMasks>,
    /// Stickers of each mask set (`getStickerSet`), by set id: asked once
    /// per set, so a set that failed stays failed and the strip skips it.
    pub sets: HashMap<i64, Fetch<Vec<StickerItem>>>,
    /// The set whose Remove button was pressed and now asks to confirm.
    pub confirm_remove: Option<i64>,
    /// The refusal of the last change (install, archive, remove, reorder),
    /// beside the lists: a list fetch in flight keeps its place and answers.
    pub change_note: crate::state::fetch::ChangeNote,
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

    /// The manage dialog opens or closes; closing it drops the line of a
    /// refused change, so a reopen shows none.
    pub fn set_dialog_open(&mut self, open: bool) {
        if !open {
            self.change_note.clear();
        }
        self.wanted = MasksWanted::with(open, self.strip_wanted());
    }

    pub fn set_strip_wanted(&mut self, wanted: bool) {
        self.wanted = MasksWanted::with(self.dialog_open(), wanted);
    }

    /// The archived mask sets held, oldest page first.
    pub fn archived_sets(&self) -> &[StickerSetInfo] {
        self.archived
            .data()
            .map_or(&[], |archived| archived.sets.as_slice())
    }

    /// Where the next archived page starts, when there is one.
    pub fn archived_next_offset(&self) -> Option<i64> {
        self.archived
            .data()
            .and_then(|archived| archived.next_offset)
    }

    /// A change goes out as `request`.
    pub fn begin_change(&mut self, request: RequestId, target: MaskChangeTarget) {
        self.change_note.begin(request);
        self.changes.push(MaskChange { request, target });
    }

    /// `request` answered or failed: its change is no longer in flight.
    pub fn settle_change(&mut self, request: RequestId) -> bool {
        self.change_note.settled(request);
        let before = self.changes.len();
        self.changes.retain(|change| change.request != request);
        self.changes.len() < before
    }

    /// A mask change (`changeStickerSet` or reorder) is in flight.
    pub fn change_pending(&self) -> bool {
        !self.changes.is_empty()
    }

    /// A change of this mask set is in flight: its own `changeStickerSet`,
    /// or a reorder (which moves every row).
    pub fn set_change_pending(&self, set_id: i64) -> bool {
        self.changes.iter().any(|change| {
            matches!(change.target, MaskChangeTarget::Reorder)
                || change.target == MaskChangeTarget::Set(set_id)
        })
    }

    /// Whether the stickers of a mask set are still to be asked for.
    pub fn set_wanted(&self, set_id: i64) -> bool {
        self.sets.get(&set_id).is_none_or(Fetch::is_idle)
    }

    /// The loaded stickers of a mask set.
    pub fn set_stickers(&self, set_id: i64) -> &[StickerItem] {
        self.sets.get(&set_id).map(Fetch::items).unwrap_or(&[])
    }
}

impl Session {
    /// The installed mask sets `request` brought: kept only while the list
    /// waits for that request.
    pub fn accept_installed_mask_sets(
        &mut self,
        request: Option<RequestId>,
        sets: Vec<StickerSetInfo>,
    ) {
        self.stickers.masks.installed.answered(request, sets);
    }

    /// A page of archived mask sets: kept only while the list waits for
    /// `request`.
    pub fn accept_archived_mask_sets(
        &mut self,
        request: Option<RequestId>,
        sets: Vec<StickerSetInfo>,
        first_page: bool,
    ) {
        let masks = &mut self.stickers.masks;
        if !request.is_some_and(|request| masks.archived.waits_for(request)) {
            return;
        }
        let last = sets.last().map(|set| set.id).unwrap_or(0);
        let held = if first_page {
            ArchivedMasks::default()
        } else {
            masks.archived.data().cloned().unwrap_or_default()
        };
        // A full page that moved on has a next one.
        let next_offset =
            (sets.len() == ARCHIVED_MASK_PAGE && last > 0 && Some(last) != held.next_offset)
                .then_some(last);
        let mut all = held.sets;
        let mut seen: std::collections::HashSet<i64> = all.iter().map(|set| set.id).collect();
        all.extend(sets.into_iter().filter(|set| seen.insert(set.id)));
        masks.archived.answered(
            request,
            ArchivedMasks {
                sets: all,
                next_offset,
            },
        );
    }

    pub(crate) fn accept_mask_set(
        &mut self,
        request: Option<RequestId>,
        set_id: i64,
        stickers: Vec<StickerItem>,
    ) {
        // A set the strip no longer asks for (a change cleared them) keeps
        // no entry: the next pump asks again.
        if let Some(set) = self.stickers.masks.sets.get_mut(&set_id) {
            set.answered(request, stickers);
        }
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

    /// A mask change (`changeStickerSet` or reorder) succeeded: drop both
    /// lists so the dialog refetches what TDLib now holds.
    pub(crate) fn finish_mask_mutation(&mut self, pending: Option<&PendingRequest>) {
        if let Some(pending) = pending {
            self.stickers.masks.settle_change(pending.id);
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::Stickers(StickersPurpose::ChangeMaskSet { .. })
                    | RequestPurpose::Stickers(StickersPurpose::ReorderInstalledMaskSets)
            )
        ) {
            // A list asked before the change answers with what the change
            // replaced: its fetch is replaced here, so that answer finds no
            // request it waits for and is dropped, and the refetch goes out.
            let masks = &mut self.stickers.masks;
            masks.installed = Fetch::Idle;
            masks.archived = Fetch::Idle;
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
                if let Some(set) = masks.sets.get_mut(&set_id) {
                    set.failed(pending.map(|p| p.id), "failed", false);
                }
            }
            Some(RequestPurpose::Stickers(StickersPurpose::GetInstalledMaskSets)) => {
                masks
                    .installed
                    .failed(pending.map(|p| p.id), MASK_LIST_FAILED, false);
            }
            Some(RequestPurpose::Stickers(StickersPurpose::GetArchivedMaskSets { .. })) => {
                masks
                    .archived
                    .failed(pending.map(|p| p.id), MASK_LIST_FAILED, false);
            }
            Some(RequestPurpose::Stickers(
                StickersPurpose::ChangeMaskSet { .. } | StickersPurpose::ReorderInstalledMaskSets,
            )) => {
                // A list fetch in flight keeps its place: its answer is the
                // truth after the change. The refusal is the change's own
                // line.
                masks
                    .change_note
                    .refused(pending.map(|p| p.id), MASK_CHANGE_FAILED);
                if let Some(pending) = pending {
                    masks.settle_change(pending.id);
                }
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
