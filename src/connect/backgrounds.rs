//! Connect driver: installed chat backgrounds (Telegram wallpapers).
use super::*;
use crate::ids::{FileId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    delete_default_background, get_installed_backgrounds, remove_installed_background,
    set_default_background,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// `getInstalledBackgrounds` for a theme. Deduped while in flight.
    pub fn fetch_installed_backgrounds(
        &mut self,
        for_dark_theme: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .pending
            .values()
            .any(|p| p.purpose == RequestPurpose::GetInstalledBackgrounds)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetInstalledBackgrounds, None);
        if let Err(err) = self
            .sender
            .send_json(&get_installed_backgrounds(extra, for_dark_theme))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `setDefaultBackground` with an installed background: the account's
    /// wallpaper for the light or dark theme.
    pub fn set_default_background(
        &mut self,
        background_id: i64,
        for_dark_theme: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.background_set_for_dark = for_dark_theme;
        let extra = self
            .session
            .request(RequestPurpose::SetDefaultBackground, None);
        if let Err(err) = self.sender.send_json(&set_default_background(
            extra,
            background_id,
            for_dark_theme,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `deleteDefaultBackground`: the account goes back to no wallpaper for
    /// that theme.
    pub fn delete_default_background(
        &mut self,
        for_dark_theme: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteDefaultBackground, None);
        if let Err(err) = self
            .sender
            .send_json(&delete_default_background(extra, for_dark_theme))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.default_backgrounds.remove(&for_dark_theme);
        Ok(extra)
    }

    /// `removeInstalledBackground`; the entry leaves the list at once.
    pub fn remove_installed_background(
        &mut self,
        background_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::RemoveInstalledBackground, None);
        if let Err(err) = self
            .sender
            .send_json(&remove_installed_background(extra, background_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(list) = self.session.installed_backgrounds.as_mut() {
            list.retain(|b| b.id != background_id);
        }
        Ok(extra)
    }

    /// Start downloading the photo of every installed or default wallpaper
    /// that has one and is not on disk yet.
    pub fn download_background_files(&mut self) {
        let ids: Vec<FileId> = self
            .session
            .installed_backgrounds
            .iter()
            .flatten()
            .chain(self.session.default_backgrounds.values())
            .filter(|b| b.needs_image())
            .filter_map(|b| b.file.as_ref())
            .filter(|f| {
                !self
                    .session
                    .files
                    .get(&f.id.0)
                    .unwrap_or(f)
                    .local
                    .is_downloading_completed
            })
            .map(|f| f.id)
            .collect();
        for id in ids {
            let _ = self.download_file(id, 8);
        }
    }
}
