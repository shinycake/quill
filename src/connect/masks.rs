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
        if self.asked.is_out(&self.session.requests, purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        self.asked.sent(&self.session.requests, purpose, extra);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// A mask request that never left fails what it was asking for.
    fn note_mask_send_failed(&mut self, purpose: StickersPurpose, err: &ConnectSendError) {
        let line = err.stage_line();
        let masks = &mut self.session.stickers.masks;
        match purpose {
            StickersPurpose::GetInstalledMaskSets => masks.installed.fail(line),
            StickersPurpose::GetArchivedMaskSets { .. } => masks.archived.fail(line),
            _ => {
                masks.installed.fail(line.clone());
                masks.archived.fail(line);
            }
        }
    }

    fn request_installed_masks(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let sent = self.mask_request(
            StickersPurpose::GetInstalledMaskSets.into(),
            get_installed_mask_sets,
        );
        match &sent {
            Ok(Some(id)) => {
                let installed = &mut self.session.stickers.masks.installed;
                installed.begin();
                installed.sent(*id);
            }
            Err(err) => self.note_mask_send_failed(StickersPurpose::GetInstalledMaskSets, err),
            Ok(None) => {}
        }
        sent
    }

    fn request_archived_masks(
        &mut self,
        more: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let masks = &self.session.stickers.masks;
        let offset = if more {
            let Some(next) = masks.archived_next_offset() else {
                return Ok(None);
            };
            next
        } else {
            0
        };
        let purpose = StickersPurpose::GetArchivedMaskSets { first_page: !more };
        let sent = self.mask_request(purpose.into(), |id| get_archived_mask_sets(id, offset));
        match &sent {
            Ok(Some(id)) => {
                let archived = &mut self.session.stickers.masks.archived;
                archived.begin();
                archived.sent(*id);
            }
            Err(err) => self.note_mask_send_failed(purpose, err),
            Ok(None) => {}
        }
        sent
    }

    /// Open the Masks dialog on its installed tab.
    pub fn open_mask_sets(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.masks.set_dialog_open(true);
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
        if self.session.mask_set_change_pending(set_id) {
            return Ok(None);
        }
        let purpose = StickersPurpose::ChangeMaskSet {
            set_id,
            installed,
            archived,
        };
        let sent = self.mask_request(purpose.into(), |id| {
            change_sticker_set(id, set_id, installed, archived)
        });
        if let Err(err) = &sent {
            self.note_mask_send_failed(purpose, err);
        }
        sent
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
            .items()
            .iter()
            .map(|set| set.id)
            .collect();
        let (Some(from), Some(to)) = (
            ids.iter().position(|id| *id == source),
            ids.iter().position(|id| *id == target),
        ) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let busy = self.session.mask_change_pending()
            || self.session.stickers.masks.installed.is_loading();
        if from == to || busy {
            return Ok(None);
        }
        ids.remove(from);
        ids.insert(to, source);
        let sent = self.mask_request(StickersPurpose::ReorderInstalledMaskSets.into(), |id| {
            reorder_installed_mask_sets(id, &ids)
        });
        if let Err(err) = &sent {
            self.note_mask_send_failed(StickersPurpose::ReorderInstalledMaskSets, err);
        }
        sent
    }

    /// The editor's Masks strip came on screen: load the installed mask
    /// sets, then the stickers of the first few.
    pub fn load_editor_masks(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.masks.set_strip_wanted(true);
        self.pump_masks()
    }

    /// The editor's Masks strip is gone.
    pub fn release_editor_masks(&mut self) {
        self.session.stickers.masks.set_strip_wanted(false);
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
        let (open, tab, strip) = (masks.dialog_open(), masks.tab, masks.strip_wanted());
        if !open && !strip {
            return Ok(());
        }
        if self.session.mask_change_pending() {
            return Ok(());
        }
        let masks = &self.session.stickers.masks;
        if (strip || tab == MasksTab::Installed) && masks.installed.is_idle() {
            self.request_installed_masks()?;
        }
        let masks = &self.session.stickers.masks;
        if open && tab == MasksTab::Archived && masks.archived.is_idle() {
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
            .items()
            .iter()
            .take(STRIP_SETS)
            .map(|set| set.id)
            .filter(|id| !self.session.stickers.masks.sets.contains_key(id))
            .collect();
        for set_id in wanted {
            match self.mask_request(StickersPurpose::GetMaskSet { set_id }.into(), |id| {
                get_sticker_set(id, set_id)
            }) {
                Ok(Some(id)) => {
                    let set = self.session.stickers.masks.sets.entry(set_id).or_default();
                    set.begin();
                    set.sent(id);
                }
                Ok(None) => {}
                // Never left: asked again on the next strip refresh.
                Err(err) => return Err(err),
            }
        }
        let files: Vec<_> = self
            .session
            .stickers
            .masks
            .installed
            .items()
            .iter()
            .take(STRIP_SETS)
            .flat_map(|set| self.session.stickers.masks.set_stickers(set.id))
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
