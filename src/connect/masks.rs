//! Connect driver: mask sticker sets (the manage dialog and the photo
//! editor's Masks strip). Kept apart from the regular sticker panel, like
//! tdesktop's separate `StickersType::Masks` lists.
use super::*;
use crate::ids::RequestId;
use crate::state::{MasksTab, RequestPurpose, StickersPurpose};
use crate::telegram::requests::{
    change_sticker_set, get_archived_mask_sets, get_installed_mask_sets, get_sticker_set,
    reorder_installed_mask_sets,
};

/// Mask sets whose stickers the editor strip loads, in installed order.
const STRIP_SETS: usize = 4;
/// Mask stickers whose pictures the editor strip downloads.
const STRIP_STICKERS: usize = 24;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send one mask request unless the same one is already in flight.
    pub(super) fn mask_request(
        &mut self,
        purpose: RequestPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            self.session.stickers.masks.failed = true;
            return Err(err);
        }
        Ok(Some(extra))
    }

    fn request_installed_masks(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.stickers.masks.failed = false;
        self.mask_request(
            StickersPurpose::GetInstalledMaskSets.into(),
            get_installed_mask_sets,
        )
    }

    fn request_archived_masks(
        &mut self,
        more: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let masks = &self.session.stickers.masks;
        if more && !masks.archived_has_more {
            return Ok(None);
        }
        let offset = if more { masks.archived_next_offset } else { 0 };
        self.session.stickers.masks.failed = false;
        self.mask_request(
            StickersPurpose::GetArchivedMaskSets { first_page: !more }.into(),
            |id| get_archived_mask_sets(id, offset),
        )
    }

    /// Open the Masks dialog on its installed tab.
    pub fn open_mask_sets(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.masks.open = true;
        self.select_mask_tab(MasksTab::Installed)
    }

    pub fn select_mask_tab(
        &mut self,
        tab: MasksTab,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.masks.tab = tab;
        match tab {
            MasksTab::Installed => self.request_installed_masks(),
            MasksTab::Archived => self.request_archived_masks(false),
        }
    }

    pub fn more_archived_masks(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.request_archived_masks(true)
    }

    /// Install, archive or remove one mask set (`changeStickerSet`). The
    /// lists refetch once TDLib confirms.
    pub fn change_mask_set(
        &mut self,
        set_id: i64,
        installed: bool,
        archived: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || set_id <= 0 || installed && archived {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.mask_change_pending() {
            return Ok(None);
        }
        self.mask_request(
            StickersPurpose::ChangeMaskSet {
                set_id,
                installed,
                archived,
            }
            .into(),
            |id| change_sticker_set(id, set_id, installed, archived),
        )
    }

    /// Move an installed mask set to the target's slot; the list commits
    /// only after TDLib confirms.
    pub fn reorder_mask_set(
        &mut self,
        source: i64,
        target: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut ids: Vec<i64> = self
            .session
            .stickers
            .masks
            .installed
            .iter()
            .map(|set| set.id)
            .collect();
        let (Some(from), Some(to)) = (
            ids.iter().position(|id| *id == source),
            ids.iter().position(|id| *id == target),
        ) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let busy = self.session.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Stickers(
                    StickersPurpose::ChangeMaskSet { .. }
                        | StickersPurpose::ReorderInstalledMaskSets
                        | StickersPurpose::GetInstalledMaskSets
                )
            )
        });
        if from == to || busy {
            return Ok(None);
        }
        ids.remove(from);
        ids.insert(to, source);
        self.mask_request(StickersPurpose::ReorderInstalledMaskSets.into(), |id| {
            reorder_installed_mask_sets(id, &ids)
        })
    }

    /// The editor's Masks strip came on screen: load the installed mask
    /// sets, then the stickers of the first few.
    pub fn load_editor_masks(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.masks.strip_wanted = true;
        self.pump_masks()
    }

    /// The editor's Masks strip is gone.
    pub fn release_editor_masks(&mut self) {
        self.session.stickers.masks.strip_wanted = false;
    }

    /// After every envelope: fetch what the open dialog or strip is
    /// missing. TDLib also pushes `updateInstalledStickerSets` while it
    /// loads, so a list dropped by a change is requested again here.
    pub(crate) fn refresh_open_mask_lists(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        self.pump_masks()
    }

    fn pump_masks(&mut self) -> Result<(), ConnectSendError> {
        let masks = &self.session.stickers.masks;
        let (open, tab, strip) = (masks.open, masks.tab, masks.strip_wanted);
        if !open && !strip {
            return Ok(());
        }
        if self.session.mask_change_pending() {
            return Ok(());
        }
        let masks = &self.session.stickers.masks;
        if (strip || tab == MasksTab::Installed) && !masks.installed_loaded && !masks.failed {
            self.request_installed_masks()?;
        }
        let masks = &self.session.stickers.masks;
        if open && tab == MasksTab::Archived && !masks.archived_loaded && !masks.failed {
            self.request_archived_masks(false)?;
        }
        if !strip {
            return Ok(());
        }
        let wanted: Vec<i64> = self
            .session
            .stickers
            .masks
            .installed
            .iter()
            .take(STRIP_SETS)
            .map(|set| set.id)
            .filter(|id| !self.session.stickers.masks.set_requested.contains(id))
            .collect();
        for set_id in wanted {
            self.session.stickers.masks.set_requested.insert(set_id);
            self.mask_request(StickersPurpose::GetMaskSet { set_id }.into(), |id| {
                get_sticker_set(id, set_id)
            })?;
        }
        let files: Vec<_> = self
            .session
            .stickers
            .masks
            .installed
            .iter()
            .take(STRIP_SETS)
            .filter_map(|set| self.session.stickers.masks.set_stickers.get(&set.id))
            .flatten()
            .take(STRIP_STICKERS)
            .filter_map(|item| item.display_file_id())
            .filter(|id| self.session.should_download(*id))
            .collect();
        for id in files {
            self.download_file(id, THUMB_DOWNLOAD_PRIORITY)?;
        }
        Ok(())
    }
}
