//! Connect driver: shareable folders (invite links), recommended folders and
//! adding a folder from an `addlist` link.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    add_chat_folder_by_invite_link, check_chat_folder_invite_link, create_chat_folder_invite_link,
    delete_chat_folder_invite_link, edit_chat_folder_invite_link, get_chat,
    get_chat_folder_invite_links, get_chat_folder_new_chats, get_chats_for_chat_folder_invite_link,
    get_premium_limit, get_recommended_chat_folders, process_chat_folder_new_chats,
    read_chat_folder,
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
        if let Some(links) = self
            .session
            .chat_list
            .folder_invite_links
            .get_mut(&folder_id)
        {
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
        self.session.chat_list.folder_invite_link = Some(invite_link.to_string());
        self.session.chat_list.folder_invite_info = None;
        self.session.chat_list.folder_invite_error = None;
        self.session.chat_list.folder_invite_done = false;
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
        self.session.chat_list.folder_invite_error = None;
        let extra = self
            .session
            .request(RequestPurpose::AddChatFolderByInviteLink, None);
        self.send_folder_request(
            extra,
            add_chat_folder_by_invite_link(extra, invite_link, chat_ids),
        )
    }

    /// `getChatFolderNewChats` for a shared folder, at most once per
    /// `chat_folder_new_chats_update_period` (TDLib requires it). Does
    /// nothing for folders that are not shared.
    pub fn fetch_folder_new_chats(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let shared = self
            .session
            .chat_list
            .chat_folders
            .iter()
            .any(|f| f.id == folder_id && f.is_shareable);
        if !shared
            || self
                .session
                .requests
                .has_purpose_for_folder(RequestPurpose::GetChatFolderNewChats, folder_id)
        {
            return Ok(None);
        }
        let period = std::time::Duration::from_secs(
            u64::try_from(self.session.chat_list.folder_limits.new_chats_period_secs())
                .unwrap_or(3600),
        );
        if self
            .session
            .chat_list
            .folder_new_chats_asked
            .get(&folder_id)
            .is_some_and(|asked| asked.elapsed() < period)
        {
            return Ok(None);
        }
        self.session
            .chat_list
            .folder_new_chats_asked
            .insert(folder_id, std::time::Instant::now());
        let extra = self
            .session
            .request_for_folder(RequestPurpose::GetChatFolderNewChats, folder_id);
        self.send_folder_request(extra, get_chat_folder_new_chats(extra, folder_id))
            .map(Some)
    }

    /// `processChatFolderNewChats`: join the chosen new chats, or dismiss the
    /// bar with an empty list. The bar goes away at once.
    pub fn process_folder_new_chats(
        &mut self,
        folder_id: i32,
        chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::ProcessChatFolderNewChats, folder_id);
        let sent = self.send_folder_request(
            extra,
            process_chat_folder_new_chats(extra, folder_id, chat_ids),
        )?;
        self.session.chat_list.folder_new_chats.remove(&folder_id);
        Ok(sent)
    }

    /// `readChatList` for one folder ("Mark as read" in the folder menu).
    /// Skipped when nothing in the folder is unread.
    pub fn mark_folder_as_read(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ReadChatList;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let any_unread = self
            .session
            .chats
            .values()
            .any(|c| c.folder_positions.contains_key(&folder_id) && c.is_unread());
        if !any_unread {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        self.send_folder_request(extra, read_chat_folder(extra, folder_id))
            .map(Some)
    }

    /// `getPremiumLimit` for the limit box (its free and Premium values).
    /// Skipped when already known or in flight.
    pub fn fetch_premium_limit(
        &mut self,
        kind: crate::folder_limits::FolderLimitKind,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(type_name) = kind.td_limit_type() else {
            return Ok(None);
        };
        let code = kind.request_code();
        if self.session.chat_list.folder_limits.has_premium_pair(kind)
            || self
                .session
                .requests
                .has_purpose_for_folder(RequestPurpose::GetPremiumLimit, code)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::GetPremiumLimit, code);
        self.send_folder_request(extra, get_premium_limit(extra, type_name))
            .map(Some)
    }
}
