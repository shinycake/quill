//! Connect driver: chat folders (load, create, edit, reorder, add and remove chats).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 7.1 (extended by the parity slice): `loadChats(chatListFolder)`
    /// when a folder tab is selected, so TDLib delivers the folder's chats /
    /// positions. Like the main list, folders page eagerly: each `ok`
    /// re-enters `maybe_load_folder_chats` (via `ingest`) until a 404 marks
    /// the folder exhausted in the reducer.
    pub fn load_folder_chats(&mut self, folder_id: i32) -> Result<RequestId, ConnectSendError> {
        self.maybe_load_folder_chats(folder_id)?
            .ok_or(ConnectSendError::InvalidRequest)
    }

    /// One `loadChats(chatListFolder)` page, unless the folder is exhausted
    /// or a page is already in flight for it.
    pub fn maybe_load_folder_chats(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .chat_list
            .folder_chats_exhausted
            .contains(&folder_id)
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::LoadFolderChats, folder_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::LoadFolderChats, folder_id);
        if let Err(err) = self.sender.send_json(&load_chats_list(
            extra,
            serde_json::json!({ "@type": "chatListFolder", "chat_folder_id": folder_id }),
            MAIN_CHAT_LOAD_LIMIT,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `createChatFolder`. Response is `chatFolderInfo`
    /// (upserted by the reducer); the full list still arrives via
    /// `updateChatFolders`.
    pub fn create_chat_folder(
        &mut self,
        spec: &ChatFolderSpec,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::CreateChatFolder, None);
        if let Err(err) = self.sender.send_json(&create_chat_folder(extra, spec)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Parity slice: `editChatFolder`. Response is `chatFolderInfo`
    /// (upserted by the reducer). Drops the cached spec so the next edit
    /// refetches.
    pub fn edit_chat_folder(
        &mut self,
        folder_id: i32,
        spec: &ChatFolderSpec,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chat_list.folder_specs.remove(&folder_id);
        let extra = self
            .session
            .request_for_folder(RequestPurpose::EditChatFolder, folder_id);
        if let Err(err) = self
            .sender
            .send_json(&edit_chat_folder(extra, folder_id, spec))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Parity slice: `deleteChatFolder`. Response is `ok`; the reducer drops
    /// the tab on ok. `leave_chat_ids` are chats to leave with the folder
    /// (empty = keep every chat in the main list).
    pub fn delete_chat_folder(
        &mut self,
        folder_id: i32,
        leave_chat_ids: &[i64],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::DeleteChatFolder, folder_id);
        if let Err(err) =
            self.sender
                .send_json(&delete_chat_folder(extra, folder_id, leave_chat_ids))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Parity slice: `reorderChatFolders` with the full new folder-id order
    /// (`main_chat_list_position` is always 0 — non-zero is Premium-only).
    /// The tab order is applied optimistically; `updateChatFolders`
    /// confirms (or corrects, on error).
    pub fn reorder_chat_folders(
        &mut self,
        folder_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReorderChatFolders, None);
        if let Err(err) = self
            .sender
            .send_json(&reorder_chat_folders(extra, folder_ids))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        let order: std::collections::HashMap<i32, usize> = folder_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        self.session
            .chat_list
            .chat_folders
            .sort_by_key(|f| order.get(&f.id).copied().unwrap_or(usize::MAX));
        Ok(extra)
    }

    /// Parity slice: `toggleChatFolderTags`. Flips
    /// `are_folder_tags_enabled` optimistically; `updateChatFolders`
    /// confirms.
    pub fn toggle_chat_folder_tags(
        &mut self,
        enabled: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleChatFolderTags, None);
        if let Err(err) = self
            .sender
            .send_json(&toggle_chat_folder_tags(extra, enabled))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.chat_list.are_folder_tags_enabled = enabled;
        Ok(extra)
    }

    /// Parity slice: `getChatFolder` for the edit dialog prefill (or the
    /// remove-from-folder chain). The full spec lands in
    /// `Session::folder_specs`, keyed by folder id. In-flight deduped; the
    /// response always overwrites the cache.
    pub fn fetch_chat_folder(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::GetChatFolder, folder_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::GetChatFolder, folder_id);
        if let Err(err) = self.sender.send_json(&get_chat_folder(extra, folder_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `getChatFolderChatsToLeave` for the delete-confirm
    /// dialog (schema 1.8.67 line 13367 — chats suggested to leave with the
    /// folder). In-flight deduped per folder.
    pub fn fetch_chat_folder_chats_to_leave(
        &mut self,
        folder_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_folder(RequestPurpose::GetChatFolderChatsToLeave, folder_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_folder(RequestPurpose::GetChatFolderChatsToLeave, folder_id);
        if let Err(err) = self.sender.send_json(&{
            serde_json::json!({
                "@type": "getChatFolderChatsToLeave",
                "@extra": extra.as_extra(),
                "chat_folder_id": folder_id,
            })
            .to_string()
        }) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `getChatListsToAddChat` (schema 1.8.67 line 13347 —
    /// "Returns chat lists to which the chat can be added. This is an
    /// offline method"). Drives the per-chat folder picker as the schema
    /// intends. In-flight deduped per chat.
    pub fn fetch_chat_lists_to_add_chat(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatListsToAddChat, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatListsToAddChat, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_lists_to_add_chat(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Parity slice: `addChatToList` with `chatListFolder` (schema 1.8.67
    /// lines 13352 + 3524). The row appears via `updateChatPosition`,
    /// like archive.
    pub fn add_chat_to_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatToList, Some(chat_id));
        let json = add_chat_to_list_value(
            extra,
            chat_id,
            serde_json::json!({ "@type": "chatListFolder", "chat_folder_id": folder_id }),
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: remove a chat from a folder. There is no
    /// `removeChatFromList` in 1.8.67 — removal is `editChatFolder` with the
    /// chat dropped from the spec (and added to `excluded_chat_ids` when it
    /// would still match the folder's filter flags). Queues the intent and
    /// fetches the full spec; `maybe_finish_folder_removals` (called from
    /// `ingest`) sends the edit once the spec arrives.
    pub fn remove_chat_from_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .chat_list
            .folder_remove_queue
            .contains(&(chat_id, folder_id))
        {
            self.session
                .chat_list
                .folder_remove_queue
                .push((chat_id, folder_id));
        }
        // Skip the fetch when the spec is already cached (the edit dialog
        // keeps it fresh); otherwise the edit waits for `getChatFolder`.
        if !self.session.chat_list.folder_specs.contains_key(&folder_id) {
            self.fetch_chat_folder(folder_id)?;
        }
        self.maybe_finish_folder_removals()?;
        Ok(())
    }

    /// Send `editChatFolder` for queued remove-from-folder intents whose
    /// full spec is cached and which have no edit already in flight.
    pub(crate) fn maybe_finish_folder_removals(&mut self) -> Result<(), ConnectSendError> {
        let queue = std::mem::take(&mut self.session.chat_list.folder_remove_queue);
        let mut still_pending = Vec::new();
        for (chat_id, folder_id) in queue {
            let Some(spec) = self.session.chat_list.folder_specs.get(&folder_id).cloned() else {
                still_pending.push((chat_id, folder_id));
                continue;
            };
            if self
                .session
                .requests
                .has_purpose_for_folder(RequestPurpose::EditChatFolder, folder_id)
            {
                still_pending.push((chat_id, folder_id));
                continue;
            }
            let Some(chat) = self.session.chats.get(&chat_id.0) else {
                continue;
            };
            let user_id = match &chat.kind {
                ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => Some(user_id.0),
                _ => None,
            };
            let user = user_id.and_then(|id| self.session.users.get(&id));
            let edited = spec_without_chat(&spec, chat, user);
            let extra = self
                .session
                .request_for_folder(RequestPurpose::EditChatFolder, folder_id);
            if let Err(err) = self
                .sender
                .send_json(&edit_chat_folder(extra, folder_id, &edited))
            {
                self.session.requests.take(extra);
                // Restore the current intent so a transient send failure
                // retries on the next ingest instead of silently dropping it.
                still_pending.push((chat_id, folder_id));
                self.session.chat_list.folder_remove_queue = still_pending;
                return Err(err);
            }
        }
        self.session.chat_list.folder_remove_queue = still_pending;
        Ok(())
    }
}
