//! Applies TDLib updates and answers for the chat list: loading, folders, archive and pins.
use crate::state::*;
use crate::telegram::envelope::ChatListPayload;

impl Session {
    /// Applies one chat list payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_chat_list_payload(
        &mut self,
        payload: ChatListPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            // Slice CL1: `updateChatIsMarkedAsUnread` (schema 1.8.67,
            // line 10588) — the authoritative marked-as-unread flag; the
            // row shows the unread badge while set.
            ChatListPayload::UpdateChatIsMarkedAsUnread {
                chat_id,
                is_marked_as_unread,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .is_marked_as_unread = is_marked_as_unread;
            }
            ChatListPayload::UpdateUnreadMessageCount {
                list,
                unread_count,
                unread_unmuted_count,
            } => {
                if std::env::var_os("QUILL_TRACE_STATUS").is_some() {
                    eprintln!(
                        "status: unread totals messages list={list:?} unread_count={unread_count} unread_unmuted_count={unread_unmuted_count}"
                    );
                }
                if let Some(totals) = self.unread_totals.list_mut(&list) {
                    totals.messages = Some(UnreadPair {
                        all: unread_count,
                        unmuted: unread_unmuted_count,
                    });
                }
            }
            ChatListPayload::UpdateUnreadChatCount {
                list,
                unread_count,
                unread_unmuted_count,
                total_count,
                marked_as_unread_count,
                marked_as_unread_unmuted_count,
            } => {
                if std::env::var_os("QUILL_TRACE_STATUS").is_some() {
                    eprintln!(
                        "status: unread totals chats list={list:?} total_count={total_count} unread_count={unread_count} unread_unmuted_count={unread_unmuted_count} marked_as_unread_count={marked_as_unread_count} marked_as_unread_unmuted_count={marked_as_unread_unmuted_count}"
                    );
                }
                let pair = UnreadPair {
                    all: unread_count,
                    unmuted: unread_unmuted_count,
                };
                if let ChatList::Folder(id) = &list {
                    self.folder_unread_chats.insert(*id, pair);
                }
                if let Some(totals) = self.unread_totals.list_mut(&list) {
                    totals.chats = Some(pair);
                }
            }
            // `updateChatAddedToList` / `updateChatRemovedFromList` track
            // `chat.chat_lists`, which is *not* list placement: "A chat can
            // have a non-zero position in a chat list even if it doesn't
            // belong to the chat list and have no position in a chat list
            // even if it belongs to the chat list" (schema 1.8.67, line
            // 3595). Rows come only from positions (`updateChatPosition` /
            // the full sets on last-message and draft updates), as in
            // Telegram X, which keeps `chat.chatLists` apart from
            // `chat.positions` (`Tdlib.updateChatAddedToList`).
            ChatListPayload::UpdateChatAddedToList { .. }
            | ChatListPayload::UpdateChatRemovedFromList { .. } => {}
            ChatListPayload::UpdateChatLastMessage {
                chat_id,
                last_message,
                positions,
            } => {
                if let Some(ref message) = last_message {
                    self.remember_files(&message.files);
                }
                // Slice chatlist-list-style: the preview's style inputs
                // (media icon, formatted-text entities) and the 3-line
                // sender name are pure functions of the same content.
                self.set_chat_last_message(chat_id, last_message.as_ref());
                // `positions` is the full set of lists this chat belongs to.
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            ChatListPayload::UpdateChatPosition(pos) => {
                self.apply_position_fields(pos);
                self.rebuild_main_order();
            }
            ChatListPayload::UpdateChatFolders {
                folders,
                are_tags_enabled,
            } => {
                // The update carries the full ordered list — replace.
                self.chat_folders = folders;
                self.are_folder_tags_enabled = are_tags_enabled;
            }
            ChatListPayload::ChatFolderInfo(info) => {
                // Parity slice: `createChatFolder` / `editChatFolder`
                // response — upsert into the tab list so the UI reflects the
                // change without waiting for `updateChatFolders` (which
                // stays the source of truth).
                match self.chat_folders.iter_mut().find(|f| f.id == info.id) {
                    Some(existing) => *existing = info,
                    None => self.chat_folders.push(info),
                }
            }
            ChatListPayload::ChatFolder { spec } => {
                // Parity slice: `getChatFolder` response — cache the full
                // spec for the edit dialog prefill / remove-from-folder
                // chain (correlated via `PendingRequest::folder_id`).
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    self.folder_specs.insert(folder_id, spec);
                }
            }
            ChatListPayload::ChatFolderInviteLink(link) => {
                // `createChatFolderInviteLink` / `editChatFolderInviteLink`:
                // upsert into the folder's cached link list.
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    let links = self.folder_invite_links.entry(folder_id).or_default();
                    match links
                        .iter_mut()
                        .find(|existing| existing.invite_link == link.invite_link)
                    {
                        Some(existing) => *existing = link,
                        None => links.push(link),
                    }
                    if let Some(info) = self.chat_folders.iter_mut().find(|f| f.id == folder_id) {
                        info.is_shareable = true;
                        info.has_my_invite_links = true;
                    }
                    self.folder_link_saved = true;
                }
            }
            ChatListPayload::ChatFolderInviteLinks(links) => {
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    self.folder_invite_links.insert(folder_id, links);
                }
            }
            ChatListPayload::PremiumLimit {
                type_name,
                default_value,
                premium_value,
            } => {
                self.folder_limits
                    .apply_premium_limit(&type_name, default_value, premium_value);
            }
            ChatListPayload::RecommendedChatFolders(folders) => {
                self.recommended_folders = Some(folders);
            }
            ChatListPayload::ChatFolderInviteLinkInfo(info) => {
                if pending.is_some_and(|p| p.purpose == RequestPurpose::CheckChatFolderInviteLink) {
                    self.folder_invite_info = Some(info);
                }
            }
            ChatListPayload::ChatLists { lists } => {
                // Parity slice: `getChatListsToAddChat` response — cache per
                // chat for the folder picker (correlated via
                // `PendingRequest::chat_id`).
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_lists_for_add.insert(chat_id.0, lists);
                }
            }
            ChatListPayload::Chats { chat_ids, .. } => {
                self.apply_chats(chat_ids, pending);
            }
            ChatListPayload::ArchiveChatListSettings { settings } => {
                // Slice CL2: `getArchiveChatListSettings` answer — only
                // our own in-flight request writes the cache.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetArchiveChatListSettings) {
                    self.archive_chat_list_settings = Some(settings);
                    self.archive_settings_loading = false;
                }
            }
        }
    }
}
