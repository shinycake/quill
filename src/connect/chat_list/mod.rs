//! Connect driver: chat list, folders, selection, read/pin state.
use super::*;
use crate::composer::DraftSaveClock;
use crate::folders::spec_without_chat;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{InfoPanelTarget, RequestPurpose, RequestRollback, SearchStatus};
use crate::telegram::envelope::{ChatFolderSpec, ChatKind};
use crate::telegram::requests::{
    ArchiveChatListSettings, add_chat_to_list, add_chat_to_list_value, clear_recently_found_chats,
    create_chat_folder, create_private_chat, delete_chat, delete_chat_folder, delete_chat_history,
    edit_chat_folder, get_archive_chat_list_settings, get_chat_folder, get_chat_lists_to_add_chat,
    load_chats_list, read_chat_list, remove_chat_action_bar, reorder_chat_folders, report_chat,
    set_archive_chat_list_settings, set_pinned_chats, toggle_chat_folder_tags,
    toggle_chat_is_marked_as_unread, toggle_chat_is_pinned, view_messages,
};

mod folders;

impl<S: JsonSender> ConnectDriver<S> {
    /// Select a chat, inform TDLib it is open, and request history.
    /// Returns `None` if history is already complete or a history request
    /// is already in flight.
    pub fn select_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.open_chat == Some(chat_id) {
            // Re-selecting the open chat leaves its reply thread.
            if self.session.threads.thread.is_some() {
                self.session.close_thread();
            }
            self.maybe_probe_channel_membership()?;
            self.maybe_fetch_bot_info()?;
            self.maybe_fetch_bot_commands()?;
            self.maybe_fetch_reply_markup()?;
            self.maybe_view_open_messages()?;
            self.maybe_download_open_thumbs()?;
            self.maybe_download_open_chat_media()?;
            self.fetch_sponsored_messages(chat_id)?;
            // Phase 5.1: re-selecting an open forum chat also resolves /
            // loads topics (the first select may have raced `is_forum`).
            self.maybe_fetch_supergroup_forum(chat_id)?;
            // Parity slice: channel/supergroup header extras.
            self.maybe_fetch_supergroup_profile(chat_id)?;
            self.maybe_fetch_supergroup_full_info_for_header(chat_id)?;
            self.maybe_fetch_forum_topics(chat_id)?;
            return self.fetch_history();
        }
        self.cancel_outgoing_typing()?;
        self.close_open_chat()?;
        self.draft_clock = DraftSaveClock::idle();
        self.pending_draft = None;
        self.session.open_chat(chat_id);
        self.session.prepare_history_window(chat_id);
        // Channels are ungated since Phase 2.2: they follow the normal
        // openChat / history path; sponsored rows fetch for every channel.
        self.send_open_chat(chat_id)?;
        self.maybe_probe_channel_membership()?;
        self.maybe_fetch_bot_info()?;
        self.maybe_fetch_bot_commands()?;
        self.maybe_fetch_reply_markup()?;
        self.maybe_view_open_messages()?;
        self.maybe_download_open_thumbs()?;
        self.fetch_sponsored_messages(chat_id)?;
        // Phase 5.1: forum supergroups resolve `is_forum` from the
        // `supergroup` object (`chatTypeSupergroup` has no forum flag), then
        // load their topic list.
        self.maybe_fetch_supergroup_forum(chat_id)?;
        // Parity slice: channel/supergroup header extras.
        self.maybe_fetch_supergroup_profile(chat_id)?;
        self.maybe_fetch_supergroup_full_info_for_header(chat_id)?;
        self.maybe_fetch_forum_topics(chat_id)?;
        self.fetch_pinned_messages(chat_id)?;
        self.fetch_history()
    }

    /// Phase 6: open/close the contacts info panel. Live and demo sessions
    /// both keep the target in `Session`; the UI then fetches the panel
    /// data through the driver.
    pub fn set_info_panel(&mut self, target: Option<InfoPanelTarget>) {
        self.session.open_info_panel = target;
    }

    /// Slice G1: `deleteChat` (schema 1.8.67, line 11850) — deletes the
    /// chat along with all messages for all members; releases group
    /// usernames. Gated by `chat.can_be_deleted_for_all_users` (line
    /// 11848). The chat is dropped locally on `ok`.
    pub fn delete_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let deletable = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_be_deleted_for_all_users);
        if !deletable {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteChat;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&delete_chat(extra, chat_id.0)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL1: `toggleChatIsPinned` (schema 1.8.67, line 13678).
    /// The pin is optimistic (the target list comes from the chat's
    /// current membership: archive list when `in_archive`, else main);
    /// the authoritative state arrives via `updateChatPosition` and the
    /// pre-request value rides on the pending entry for rollback. The
    /// pin-limit pre-check lives in the UI (TGX behavior).
    pub fn toggle_chat_pin(
        &mut self,
        chat_id: ChatId,
        pin: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ToggleChatIsPinned;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let (archived, supported) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| (chat.in_archive, chat.supported()))
            .unwrap_or((false, false));
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_chat_is_pinned(extra, chat_id.0, archived, pin))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(chat) = self.session.chats.get_mut(&chat_id.0) {
            let rollback = RequestRollback::ChatPin {
                previous: if archived {
                    chat.archive_is_pinned
                } else {
                    chat.is_pinned
                },
                archived,
            };
            if archived {
                chat.archive_is_pinned = pin;
            } else {
                chat.is_pinned = pin;
            }
            if let Some(pending) = self.session.requests.pending_mut(extra) {
                pending.rollback = Some(rollback);
            }
        }
        Ok(Some(extra))
    }

    /// Slice CL1: `toggleChatIsMarkedAsUnread` (schema 1.8.67, line
    /// 13519). Optimistic with rollback; the authoritative flag arrives
    /// via `updateChatIsMarkedAsUnread`.
    pub fn toggle_chat_marked_as_unread(
        &mut self,
        chat_id: ChatId,
        marked: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ToggleChatIsMarkedAsUnread;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_chat_is_marked_as_unread(extra, chat_id.0, marked))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(chat) = self.session.chats.get_mut(&chat_id.0) {
            let rollback = RequestRollback::ChatMarkedAsUnread {
                previous: chat.is_marked_as_unread,
            };
            chat.is_marked_as_unread = marked;
            if let Some(pending) = self.session.requests.pending_mut(extra) {
                pending.rollback = Some(rollback);
            }
        }
        Ok(Some(extra))
    }

    /// Slice CL1: mark a chat as read the way Telegram X does
    /// (`Tdlib.markChatAsRead` with `MessageSourceChatList`): `viewMessages`
    /// over the newest known message reads real unread history, and the
    /// manual marked-unread flag is cleared when set. Honest noop when the
    /// chat has nothing unread.
    pub fn mark_chat_as_read(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (supported, unread, marked) = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                (
                    chat.supported(),
                    chat.unread_count > 0,
                    chat.is_marked_as_unread,
                )
            })
            .unwrap_or((false, false, false));
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !unread && !marked {
            return Ok(None);
        }
        let mut sent: Option<RequestId> = None;
        // TGX: viewing the newest known message reads the real unread
        // history. Skipped when the chat was never opened (same as TGX's
        // `chat.lastMessage == null` guard); the authoritative
        // `updateChatReadInbox` then confirms.
        if unread {
            let newest = self
                .session
                .histories
                .get(&chat_id.0)
                .and_then(|history| history.messages.keys().next_back().copied())
                .map(MessageId);
            if let Some(id) = newest
                && !self
                    .session
                    .requests
                    .has_purpose_for_chat(RequestPurpose::ViewMessages, chat_id)
            {
                let extra = self
                    .session
                    .request(RequestPurpose::ViewMessages, Some(chat_id));
                match self.sender.send_json(&view_messages(
                    extra,
                    chat_id,
                    &[id],
                    "messageSourceChatList",
                    true,
                )) {
                    Ok(()) => {
                        self.session.begin_viewing(chat_id, &[id]);
                        sent = Some(extra);
                    }
                    Err(err) => {
                        self.session.requests.take(extra);
                        return Err(err);
                    }
                }
            }
        }
        if marked {
            sent = self.toggle_chat_marked_as_unread(chat_id, false)?.or(sent);
        }
        Ok(sent)
    }

    /// Slice CL2: `setPinnedChats` (schema 1.8.67, line 13681) — drag
    /// reorder of the pinned chats. `new_ids` is the full new pinned
    /// order (highest first), built by the UI from the current model
    /// order (TGX `ChatsAdapter.movePinnedChat` sends the reordered
    /// array the same way). The model applies it optimistically via
    /// `Session::reorder_pinned_chats`; a refusal restores the
    /// pre-reorder order values. `Ok(None)` = no-op: id-set mismatch
    /// or a reorder already in flight.
    pub fn set_pinned_chat_order(
        &mut self,
        archived: bool,
        new_ids: Vec<i64>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetPinnedChats;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        if self.session.pinned_chat_ids(archived) == new_ids {
            return Ok(None);
        }
        let previous = self.session.reorder_pinned_chats(archived, &new_ids);
        if previous.is_empty() {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&set_pinned_chats(extra, archived, &new_ids))
        {
            self.session.requests.take(extra);
            // Transport failure: undo the optimistic swap with the
            // inverse permutation (same value multiset, original ids).
            let restored: Vec<i64> = previous.iter().map(|(id, _)| *id).collect();
            self.session.reorder_pinned_chats(archived, &restored);
            return Err(err);
        }
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ChatPinOrder { previous, archived });
        }
        Ok(Some(extra))
    }

    /// Slice CL2: `readChatList` (schema 1.8.67, line 13684) — mark all
    /// chats in the list as read. The badges clear via
    /// `updateChatReadInbox` / `updateChatUnreadMentionCount`; nothing
    /// is faked locally. `Ok(None)` = nothing unread or a read already
    /// in flight.
    pub fn mark_all_chats_as_read(
        &mut self,
        archived: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ReadChatList;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let any_unread = self.session.chats.values().any(|c| {
            let in_list = if archived {
                c.in_archive
            } else {
                c.in_main_list
            };
            in_list && c.is_unread()
        });
        if !any_unread {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&read_chat_list(extra, archived)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL2: `clearRecentlyFoundChats` (schema 1.8.67, line 11671).
    /// The recents are cleared optimistically — the schema defines no
    /// update for this, so the client can't wait for confirmation (TGX
    /// `SearchManager.clearRecentlyFoundChats` clears locally too); a
    /// refusal surfaces via `chat_action_error` and the next recents
    /// fetch restores the truth. `Ok(None)` = recents already empty or
    /// a clear already in flight.
    pub fn clear_recently_found_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ClearRecentlyFoundChats;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        if !(self.session.search.recents && !self.session.search.chat_ids.is_empty()) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&clear_recently_found_chats(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.search.chat_ids.clear();
        self.session.search.status = SearchStatus::Idle;
        Ok(Some(extra))
    }

    /// Slice CL2: `getArchiveChatListSettings` (schema 1.8.67, line
    /// 13421) — one-shot fetch feeding
    /// `Session::archive_chat_list_settings` (TGX
    /// `SettingsArchiveChatListController` fetches on open the same
    /// way). `Ok(None)` = already fetched or a fetch in flight.
    pub fn fetch_archive_chat_list_settings(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetArchiveChatListSettings;
        if self.session.chat_list.archive_chat_list_settings.is_some()
            || self.session.requests.has_purpose(purpose)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&get_archive_chat_list_settings(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.chat_list.archive_settings_loading = true;
        Ok(Some(extra))
    }

    /// Slice CL2: `setArchiveChatListSettings` (schema 1.8.67, line
    /// 13424). Optimistic flip of the cached settings; a refusal
    /// restores the previous values. The optimistic value stands
    /// until the next fetch.
    pub fn set_archive_chat_list_settings(
        &mut self,
        settings: ArchiveChatListSettings,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetArchiveChatListSettings;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let Some(current) = self.session.chat_list.archive_chat_list_settings else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if current == settings {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&set_archive_chat_list_settings(extra, settings))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.chat_list.archive_chat_list_settings = Some(settings);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ArchiveChatListSettings {
                previous: Some(current),
            });
        }
        Ok(Some(extra))
    }

    /// Slice CL2: Saved Messages — `createPrivateChat` with the own user
    /// id (schema 1.8.67, line 9590: "Call createPrivateChat with
    /// getOption(\"my_id\") and open the chat"). The `chat` answer opens
    /// the chat via the `CreatePrivateChat` pending purpose. `Ok(None)`
    /// = own id unknown (call `getMe` first) or a creation already in
    /// flight; the UI prefers the already-listed self chat when one
    /// exists.
    pub fn create_private_chat_with_self(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(my_id) = self.session.my_user_id else {
            return Ok(None);
        };
        self.create_private_chat_for(my_id)
    }

    /// B1: `createPrivateChat` with an arbitrary user id (schema 1.8.67,
    /// line 9588) — the user button on an inline keyboard. The `chat`
    /// answer opens the chat via the `CreatePrivateChat` pending purpose.
    /// `Ok(None)` = a creation already in flight; the UI prefers an
    /// already-listed private chat with the user when one exists.
    pub fn create_private_chat_for(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreatePrivateChat;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&create_private_chat(extra, user_id, false))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL1: `deleteChatHistory` (schema 1.8.67, line 11845). The
    /// chat stays in the chat list (`remove_from_chat_list: false`);
    /// `revoke` clears for everyone when
    /// `chat.can_be_deleted_for_all_users` (the UI gates the choice).
    pub fn clear_chat_history(
        &mut self,
        chat_id: ChatId,
        revoke: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::DeleteChatHistory;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let deletable = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.supported()
                && (chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users)
        });
        if !deletable {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&delete_chat_history(extra, chat_id.0, false, revoke))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL1: remove a chat from the chat list the way Telegram X
    /// does (`Tdlib.deleteChat` for private chats, closed secret chats,
    /// and chats the user already left): `deleteChatHistory` with
    /// `remove_from_chat_list: true` (schema 1.8.67, line 11845).
    /// This is deliberately NOT the destructive `deleteChat`
    /// constructor (line 11850) — that deletes the chat for all
    /// members and stays on the group panel's "Delete group" (G1).
    pub fn remove_chat_from_list(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::RemoveChatFromList;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let removable = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.supported()
                && (chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users)
        });
        if !removable {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&delete_chat_history(extra, chat_id.0, true, false))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL3: `reportChat` (TDLib 1.8.67, schema line 15693) — the
    /// simple spam-report flow (empty option_id/message_ids/text,
    /// schema:3667). Returns `Ok(None)` when the chat is missing or
    /// `can_be_reported` is false; the `ReportChatResult` outcome
    /// surfaces via `Session::report_chat_outcome`.
    pub fn report_chat(&mut self, chat_id: ChatId) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reportable = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_be_reported);
        if !reportable {
            return Ok(None);
        }
        let purpose = RequestPurpose::ReportChat;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&report_chat(extra, chat_id.0)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Batch 8: `removeChatActionBar` — the bar's close button. The bar is
    /// dropped locally at once; TDLib confirms with `updateChatActionBar`.
    pub fn dismiss_chat_action_bar(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.chat_action_bar(chat_id).is_none() {
            return Ok(None);
        }
        let purpose = RequestPurpose::RemoveChatActionBar;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&remove_chat_action_bar(extra, chat_id.0))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.set_chat_action_bar(chat_id.0, None);
        Ok(Some(extra))
    }

    /// Move the chat to `chatListArchive` (`addChatToList`).
    pub fn archive_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.send_add_chat_to_list(chat_id, true)
    }

    /// Move the chat back to `chatListMain` (`addChatToList`).
    pub fn unarchive_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.send_add_chat_to_list(chat_id, false)
    }

    fn send_add_chat_to_list(
        &mut self,
        chat_id: ChatId,
        archive: bool,
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
        let json = add_chat_to_list(extra, chat_id, archive);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
