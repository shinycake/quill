//! Connect driver: shareable folders (invite links), recommended folders and
//! adding a folder from an `addlist` link.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    add_chat_folder_by_invite_link, check_chat_folder_invite_link, create_chat_folder_invite_link,
    delete_chat_folder_invite_link, edit_chat_folder_invite_link, get_chat,
    get_chat_folder_invite_links, get_chats_for_chat_folder_invite_link,
    get_recommended_chat_folders,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn send_folder_request(
        &mut self,
        extra: RequestId,
        json: String,
    ) -> Result<RequestId, ConnectSendError> {
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `getRecommendedChatFolders` for the folder settings. Deduped while in
    /// flight; the answer lands in `Session::recommended_folders`.
    pub fn fetch_recommended_chat_folders(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .pending
            .values()
            .any(|p| p.purpose == RequestPurpose::GetRecommendedChatFolders)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetRecommendedChatFolders, None);
        self.send_folder_request(extra, get_recommended_chat_folders(extra))
            .map(Some)
    }

    /// `getChatFolderInviteLinks` + `getChatsForChatFolderInviteLink`: what
    /// the Share Folder dialog shows. Each is deduped per folder.
    pub fn fetch_folder_share(&mut self, folder_id: i32) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::GetChatFolderInviteLinks, folder_id)
        {
            let extra = self
                .session
                .request_for_folder(RequestPurpose::GetChatFolderInviteLinks, folder_id);
            self.send_folder_request(extra, get_chat_folder_invite_links(extra, folder_id))?;
        }
        if !self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::GetChatsForFolderInviteLink, folder_id)
        {
            let extra = self
                .session
                .request_for_folder(RequestPurpose::GetChatsForFolderInviteLink, folder_id);
            self.send_folder_request(
                extra,
                get_chats_for_chat_folder_invite_link(extra, folder_id),
            )?;
        }
        Ok(())
    }

    /// `createChatFolderInviteLink`; the new link joins the cached list when
    /// TDLib answers (`Session::folder_link_saved` flips).
    pub fn create_folder_invite_link(
        &mut self,
        folder_id: i32,
        name: &str,
        chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::CreateChatFolderInviteLink, folder_id);
        self.send_folder_request(
            extra,
            create_chat_folder_invite_link(extra, folder_id, name, chat_ids),
        )
    }

    /// `editChatFolderInviteLink`.
    pub fn edit_folder_invite_link(
        &mut self,
        folder_id: i32,
        invite_link: &str,
        name: &str,
        chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::EditChatFolderInviteLink, folder_id);
        self.send_folder_request(
            extra,
            edit_chat_folder_invite_link(extra, folder_id, invite_link, name, chat_ids),
        )
    }

    /// `deleteChatFolderInviteLink`. The link leaves the cached list at once
    /// (a failure refetches the list so the screen tells the truth).
    pub fn delete_folder_invite_link(
        &mut self,
        folder_id: i32,
        invite_link: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::DeleteChatFolderInviteLink, folder_id);
        let sent = self.send_folder_request(
            extra,
            delete_chat_folder_invite_link(extra, folder_id, invite_link),
        )?;
        if let Some(links) = self.session.folder_invite_links.get_mut(&folder_id) {
            links.retain(|link| link.invite_link != invite_link);
        }
        Ok(sent)
    }

    /// `checkChatFolderInviteLink` for an `addlist` link; the answer lands in
    /// `Session::folder_invite_info`.
    pub fn check_folder_invite_link(
        &mut self,
        invite_link: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.folder_invite_link = Some(invite_link.to_string());
        self.session.folder_invite_info = None;
        self.session.folder_invite_error = None;
        self.session.folder_invite_done = false;
        let extra = self
            .session
            .request(RequestPurpose::CheckChatFolderInviteLink, None);
        self.send_folder_request(extra, check_chat_folder_invite_link(extra, invite_link))
    }

    /// `getChat` for the chats of an `addlist` link that are not loaded,
    /// so the dialog can name them. Deduped per chat.
    pub fn fetch_chats_for_folder_invite(
        &mut self,
        chat_ids: &[i64],
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for &id in chat_ids {
            let chat_id = ChatId(id);
            if self.session.chats.contains_key(&id)
                || self
                    .session
                    .requests
                    .has_purpose_for_chat(RequestPurpose::GetFolderInviteChat, chat_id)
            {
                continue;
            }
            let extra = self
                .session
                .request(RequestPurpose::GetFolderInviteChat, Some(chat_id));
            self.send_folder_request(extra, get_chat(extra, chat_id))?;
        }
        Ok(())
    }

    /// `addChatFolderByInviteLink` with the chats the user kept ticked.
    pub fn add_folder_by_invite_link(
        &mut self,
        invite_link: &str,
        chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.folder_invite_error = None;
        let extra = self
            .session
            .request(RequestPurpose::AddChatFolderByInviteLink, None);
        self.send_folder_request(
            extra,
            add_chat_folder_by_invite_link(extra, invite_link, chat_ids),
        )
    }
}
