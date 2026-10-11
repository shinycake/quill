//! Connect driver: installed chat backgrounds (Telegram wallpapers).
use super::*;
use crate::ids::{ChatId, FileId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::envelope::{Background, BackgroundType};
use crate::telegram::requests::{
    delete_chat_background, delete_default_background, get_background_url,
    get_installed_backgrounds, remove_installed_background, search_background, set_chat_background,
    set_chat_background_local, set_chat_theme, set_default_background,
    set_default_background_local,
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

    /// `setDefaultBackground` with an installed or searched background:
    /// the account's wallpaper for the light or dark theme. `kind` is the
    /// type the preview settled on (`None` keeps the background's own).
    pub fn set_default_background(
        &mut self,
        background_id: i64,
        kind: Option<&BackgroundType>,
        for_dark_theme: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.background_set_for_dark = for_dark_theme;
        let extra = self
            .session
            .request(RequestPurpose::SetDefaultBackground, None);
        if let Err(err) = self.sender.send_json(&set_default_background(
            extra,
            background_id,
            kind,
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
        self.session
            .chats_state
            .default_backgrounds
            .remove(&for_dark_theme);
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
        if let Some(list) = self.session.chats_state.installed_backgrounds.as_mut() {
            list.retain(|b| b.id != background_id);
        }
        Ok(extra)
    }

    /// `setDefaultBackground` with an image file from disk, blurred or not.
    pub fn set_default_background_local(
        &mut self,
        path: &str,
        blurred: bool,
        for_dark_theme: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || path.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.background_set_for_dark = for_dark_theme;
        let extra = self
            .session
            .request(RequestPurpose::SetDefaultBackgroundLocal, None);
        if let Err(err) = self.sender.send_json(&set_default_background_local(
            extra,
            path,
            blurred,
            for_dark_theme,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `searchBackground` for a `bg/` link; the answer lands in
    /// `Session::searched_background`.
    pub fn search_background(&mut self, name: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || name.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.searched_background = None;
        let extra = self.session.request(RequestPurpose::SearchBackground, None);
        if let Err(err) = self.sender.send_json(&search_background(extra, name)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `getBackgroundUrl`: the shareable link; the answer lands in
    /// `Session::chats_state.background_url`.
    pub fn get_background_url(
        &mut self,
        name: &str,
        kind: &BackgroundType,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || name.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.background_url = None;
        let extra = self.session.request(RequestPurpose::GetBackgroundUrl, None);
        if let Err(err) = self
            .sender
            .send_json(&get_background_url(extra, name, kind))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `setChatBackground` with an installed background.
    pub fn set_chat_background(
        &mut self,
        chat_id: ChatId,
        background_id: i64,
        dark_theme_dimming: i32,
        only_for_self: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatBackground, Some(chat_id));
        if let Err(err) = self.sender.send_json(&set_chat_background(
            extra,
            chat_id.0,
            background_id,
            dark_theme_dimming,
            only_for_self,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `setChatBackground` with an image file from disk.
    pub fn set_chat_background_local(
        &mut self,
        chat_id: ChatId,
        path: &str,
        blurred: bool,
        dark_theme_dimming: i32,
        only_for_self: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || path.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatBackground, Some(chat_id));
        if let Err(err) = self.sender.send_json(&set_chat_background_local(
            extra,
            chat_id.0,
            path,
            blurred,
            dark_theme_dimming,
            only_for_self,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `deleteChatBackground`; the wallpaper leaves the chat at once.
    pub fn delete_chat_background(
        &mut self,
        chat_id: ChatId,
        restore_previous: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteChatBackground, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&delete_chat_background(extra, chat_id.0, restore_previous))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.set_chat_background(chat_id.0, None);
        Ok(extra)
    }

    /// `setChatTheme`; an empty `name` removes the theme.
    pub fn set_chat_theme(
        &mut self,
        chat_id: ChatId,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatTheme, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_chat_theme(extra, chat_id.0, name))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Download the wallpaper files one chat shows: its own, its theme's,
    /// and (while a picker is open) every emoji theme's.
    pub fn download_chat_look_files(&mut self, chat_id: i64, all_themes: bool) {
        let mut files: Vec<&crate::telegram::envelope::ParsedFile> = Vec::new();
        if let Some(own) = self.session.chats_state.chat_backgrounds.get(&chat_id) {
            files.extend(own.background.file.as_ref());
        }
        if let Some(gift) = self.session.chats_state.chat_gift_themes.get(&chat_id) {
            for settings in [&gift.light, &gift.dark] {
                files.extend(settings.background.as_ref().and_then(|b| b.file.as_ref()));
            }
        }
        let themes = self
            .session
            .chats_state
            .emoji_chat_themes
            .iter()
            .filter(|t| {
                all_themes
                    || self.session.chats_state.chat_theme_names.get(&chat_id) == Some(&t.name)
            });
        for theme in themes {
            for settings in [&theme.light, &theme.dark] {
                files.extend(settings.background.as_ref().and_then(|b| b.file.as_ref()));
            }
        }
        let ids: Vec<FileId> = files
            .into_iter()
            .filter(|f| {
                !self
                    .session
                    .media
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

    /// Start the downloads the wallpaper gallery and the chat need: the
    /// account wallpapers' files, each installed pattern's file (small),
    /// and only the thumbnail of each installed photo (tdesktop's gallery
    /// draws photos from thumbnails; the full photo loads when previewed).
    pub fn download_background_files(&mut self) {
        let chats = &self.session.chats_state;
        let wanted = gallery_files(
            chats.installed_backgrounds.iter().flatten(),
            chats.default_backgrounds.values(),
        );
        self.fetch_background_files(wanted.into_iter().cloned().collect());
    }

    /// The full file of one background (the preview of a photo).
    pub fn download_background_file(&mut self, background: &Background) {
        if let Some(file) = background.file.clone() {
            self.fetch_background_files(vec![file]);
        }
    }

    fn fetch_background_files(&mut self, files: Vec<crate::telegram::envelope::ParsedFile>) {
        let ids: Vec<FileId> = files
            .iter()
            .filter(|f| {
                !self
                    .session
                    .media
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

/// The files the gallery needs: thumbnails of installed photos, the files
/// of installed patterns, and the full files of the account wallpapers.
pub(crate) fn gallery_files<'a>(
    installed: impl Iterator<Item = &'a Background>,
    defaults: impl Iterator<Item = &'a Background>,
) -> Vec<&'a crate::telegram::envelope::ParsedFile> {
    let mut files = Vec::new();
    for background in installed {
        if background.needs_image() {
            files.extend(background.thumbnail.as_ref().or(background.file.as_ref()));
        } else if background.needs_file() {
            files.extend(background.file.as_ref());
        }
    }
    files.extend(
        defaults
            .filter(|b| b.needs_file())
            .filter_map(|b| b.file.as_ref()),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::gallery_files;
    use crate::telegram::envelope::{Background, BackgroundFill, BackgroundType, ParsedFile};

    fn file(id: i32) -> ParsedFile {
        crate::telegram::envelope::parse_file(Some(
            &serde_json::json!({"@type": "file", "id": id, "size": 1}),
        ))
        .unwrap()
    }

    fn bg(kind: BackgroundType, full: Option<i32>, thumb: Option<i32>) -> Background {
        Background {
            id: 1,
            is_default: false,
            is_dark: false,
            name: String::new(),
            file: full.map(file),
            thumbnail: thumb.map(file),
            kind,
        }
    }

    #[test]
    fn the_gallery_fetches_photo_thumbnails_and_pattern_files() {
        let photo = BackgroundType::Wallpaper {
            blurred: false,
            moving: false,
        };
        let pattern = BackgroundType::Pattern {
            fill: BackgroundFill::Solid(1),
            intensity: 50,
            inverted: false,
            moving: false,
        };
        let installed = [
            bg(photo.clone(), Some(1), Some(2)),
            bg(photo.clone(), Some(3), None),
            bg(pattern, Some(4), Some(5)),
            bg(BackgroundType::Fill(BackgroundFill::Solid(1)), None, None),
        ];
        let defaults = [bg(photo, Some(6), Some(7))];
        let ids: Vec<i32> = gallery_files(installed.iter(), defaults.iter())
            .iter()
            .map(|f| f.id.0)
            .collect();
        assert_eq!(ids, vec![2, 3, 4, 6]);
    }
}
