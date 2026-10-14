//! Connect driver: editing an owned sticker or custom emoji set from the
//! set box (tdesktop `api/api_stickers_creator.cpp`). Every edit names the
//! set by the short name the open view carries, counts itself in the view's
//! `edits`, and once confirmed the view asks `getStickerSet` again.
use super::*;
use crate::ids::{FileId, RequestId};
use crate::state::{RequestPurpose, StickerCreatorPurpose, StickerSetViewStage, StickersPurpose};
use crate::sticker_set_box::{emoji_or_default, valid_set_name};
use crate::telegram::envelope::StickerFormat;
use crate::telegram::requests::{
    NewStickerFile, add_sticker_to_set, delete_sticker_set, get_sticker_set,
    remove_sticker_from_set, set_sticker_position_in_set, set_sticker_set_title,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// The short name of an owned set: the open box's, or an installed
    /// set the user created (the message menu adds to those).
    fn owned_set_name(&self, set_id: i64) -> Option<String> {
        self.session.owned_sticker_set_name(set_id)
    }

    fn creator_request(
        &mut self,
        purpose: StickerCreatorPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Stickers(StickersPurpose::Creator(purpose)),
            None,
        );
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.begin_sticker_set_edit(purpose.set_id());
        Ok(extra)
    }

    /// Add a freshly rendered file at `path` (a PNG fitted to the set's
    /// side, or a WEBM video sticker) tagged with `emojis`
    /// (`addStickerToSet` with `inputFileLocal`; TDLib uploads it).
    pub fn add_new_sticker_to_set(
        &mut self,
        set_id: i64,
        path: &str,
        emojis: &str,
        format: StickerFormat,
    ) -> Result<RequestId, ConnectSendError> {
        let name = self
            .owned_set_name(set_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if path.is_empty() || emojis.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let file = NewStickerFile::Local(path.to_string());
        let emoji = self
            .session
            .owned_sticker_set(set_id)
            .is_some_and(|(_, emoji)| emoji);
        self.creator_request(
            StickerCreatorPurpose::AddNewSticker { set_id, emoji },
            |extra| add_sticker_to_set(extra, &name, &file, format, emojis),
        )
    }

    /// Add a sticker that already exists on the server, under its own
    /// emoji or the default one (`AddExistingStickerToSet`).
    pub fn add_existing_sticker_to_set(
        &mut self,
        set_id: i64,
        file_id: FileId,
        format: StickerFormat,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let name = self
            .owned_set_name(set_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let emojis = emoji_or_default(emoji).to_string();
        let file = NewStickerFile::Existing(file_id);
        let emoji = self
            .session
            .owned_sticker_set(set_id)
            .is_some_and(|(_, emoji)| emoji);
        self.creator_request(
            StickerCreatorPurpose::AddExistingSticker { set_id, emoji },
            |extra| add_sticker_to_set(extra, &name, &file, format, &emojis),
        )
    }

    /// Remove a sticker from the open owned set (`removeStickerFromSet`).
    pub fn remove_sticker_from_set(
        &mut self,
        set_id: i64,
        file_id: FileId,
    ) -> Result<RequestId, ConnectSendError> {
        self.owned_set_name(set_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        self.creator_request(
            StickerCreatorPurpose::RemoveSticker { set_id, file_id },
            |extra| remove_sticker_from_set(extra, file_id),
        )
    }

    /// Reorder mode dropped the sticker at `from` onto `to`: the view moves
    /// it at once and TDLib is told its new position
    /// (`setStickerPositionInSet`). A no-op move sends nothing.
    pub fn move_sticker_in_set(
        &mut self,
        set_id: i64,
        from: usize,
        to: usize,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.owned_set_name(set_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(position) = self.session.move_viewed_sticker(set_id, from, to) else {
            return Ok(None);
        };
        let file_id = match self
            .session
            .stickers
            .sticker_set_view
            .as_ref()
            .map(|view| &view.stage)
        {
            Some(StickerSetViewStage::Ready { stickers, .. }) => {
                stickers.get(position).map(|sticker| sticker.file_id)
            }
            _ => None,
        };
        let Some(file_id) = file_id else {
            return Ok(None);
        };
        let position = i32::try_from(position).unwrap_or(i32::MAX);
        let sent = self.creator_request(StickerCreatorPurpose::MoveSticker { set_id }, |extra| {
            set_sticker_position_in_set(extra, file_id, position)
        });
        if sent.is_err()
            && let Some(view) = self.session.stickers.sticker_set_view.as_mut()
            && view.set_id == set_id
        {
            // The box shows an order the server never got: ask for the
            // server's order.
            view.edits.refresh_needed = true;
        }
        sent.map(Some)
    }

    /// Rename the open owned set (`setStickerSetTitle`); the title is
    /// trimmed and capped as the edit-name box does.
    pub fn rename_sticker_set(
        &mut self,
        set_id: i64,
        title: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let name = self
            .owned_set_name(set_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let title = valid_set_name(title).ok_or(ConnectSendError::InvalidRequest)?;
        self.creator_request(StickerCreatorPurpose::RenameSet { set_id }, |extra| {
            set_sticker_set_title(extra, &name, &title)
        })
    }

    /// Delete the open owned set for everyone (`deleteStickerSet`).
    pub fn delete_owned_sticker_set(&mut self, set_id: i64) -> Result<RequestId, ConnectSendError> {
        let name = self
            .owned_set_name(set_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        self.creator_request(StickerCreatorPurpose::DeleteSet { set_id }, |extra| {
            delete_sticker_set(extra, &name)
        })
    }

    /// After a confirmed edit, ask the server for the open set again so the
    /// box shows the set as it is now (tdesktop re-applies the answered
    /// set). The view keeps its contents while the answer is on its way.
    pub fn sync_sticker_set_view(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.refetch_installed_sets_after_edit()?;
        let Some(view) = self.session.stickers.sticker_set_view.as_ref() else {
            return Ok(None);
        };
        if !view.edits.refresh_needed || view.edits.busy() {
            return Ok(None);
        }
        let set_id = view.set_id;
        if !self.chats_path_active() || set_id == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Stickers(StickersPurpose::ViewStickerSet { set_id });
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&get_sticker_set(extra, set_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(view) = self.session.stickers.sticker_set_view.as_mut() {
            view.edits.refresh_needed = false;
            view.files_requested = false;
        }
        Ok(Some(extra))
    }

    /// An edit renamed, changed or deleted a set: fetch both installed
    /// lists again so the "Add to … Set" menus and the panel's "edit" badge
    /// follow the server (tdesktop `notifyUpdated` after `feedSetFull`).
    fn refetch_installed_sets_after_edit(&mut self) -> Result<(), ConnectSendError> {
        if !self.session.stickers.creator_refetch_sets || !self.chats_path_active() {
            return Ok(());
        }
        self.session.take_sticker_set_refetch();
        if !self
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            let extra = self
                .session
                .request(RequestPurpose::GetInstalledStickerSets, None);
            if let Err(err) =
                self.sender
                    .send_json(&crate::telegram::requests::get_installed_sticker_sets(
                        extra,
                    ))
            {
                // Try again on the next poll.
                self.session.stickers.creator_refetch_sets = true;
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        if !self
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledEmojiSets)
        {
            let extra = self
                .session
                .request(RequestPurpose::GetInstalledEmojiSets, None);
            if let Err(err) =
                self.sender
                    .send_json(&crate::telegram::requests_emoji::get_installed_emoji_sets(
                        extra,
                    ))
            {
                self.session.stickers.creator_refetch_sets = true;
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        Ok(())
    }
}
