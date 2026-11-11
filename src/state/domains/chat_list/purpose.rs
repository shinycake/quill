//! Request purposes for the chat list: loading, folders, archive and pins.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for the chat list: loading, folders, archive and pins; wrapped as
/// [`RequestPurpose::ChatList`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatListPurpose {
    LoadChats,
    /// Phase 7.1: single-shot `loadChats(chatListFolder(id))` when a folder
    /// tab is selected. Separate from `LoadChats` so the ok-response does
    /// not re-trigger main-list paging.
    LoadFolderChats,
    /// `loadChats(chatListArchive)` pages, sent once the main list is
    /// exhausted. Separate from `LoadChats` so its ok pages the archive,
    /// not the main list.
    LoadArchiveChats,
    /// Slice CL: one-shot `getChatHistory` for the chat-list peek preview
    /// (`parity:chatlist-chat-preview`). The `messages` answer lands in
    /// `ChatListState::chat_preview_fetch` — it must NOT merge into the open
    /// chat's history (the `GetHistory` branch drops answers for non-open
    /// chats, and the preview never calls `openChat`).
    GetChatPreview,
    /// `addChatToList` (`chatListArchive` or `chatListMain`). Response is `ok`;
    /// the row moves via position updates.
    AddChatToList,
    /// Slice G1: `deleteChat` (schema 1.8.67, line 11850). Response is
    /// `ok`. The chat is dropped locally; it deletes the chat for all
    /// members and releases the username.
    DeleteChat,
    /// Slice CL1: `toggleChatIsPinned` (schema 1.8.67, line 13678).
    /// Response is `ok`; the authoritative pinned state arrives via
    /// `updateChatPosition`. Rollback rides on `PendingRequest::rollback`.
    ToggleChatIsPinned,
    /// Slice CL1: `toggleChatIsMarkedAsUnread` (schema 1.8.67, line
    /// 13519). Response is `ok`; authoritative state arrives via
    /// `updateChatIsMarkedAsUnread`. Rollback on error.
    ToggleChatIsMarkedAsUnread,
    /// Slice CL1: `deleteChatHistory` (schema 1.8.67, line 11845).
    /// Response is `ok`; history is re-fetched on next open.
    DeleteChatHistory,
    /// Slice CL1: `deleteChatHistory` with `remove_from_chat_list: true`
    /// (the chat-list "Delete chat", Telegram X `Tdlib.deleteChat`).
    /// Response is `ok`; the row drops via its position update (order 0).
    RemoveChatFromList,
    /// `deleteChatHistory` with `remove_from_chat_list: true` for "Hide this
    /// announcement". Response is `ok`; the row drops via its position update.
    HidePromotion,
    /// `leaveChat` sent before the history of a basic group is deleted (the
    /// chat-list "Delete and leave", tdesktop `ApiWrap::deleteConversation`).
    /// Its answer, ok or error, queues the `deleteChatHistory` step.
    LeaveBeforeDelete,
    /// Slice CL2: `setPinnedChats` (schema 1.8.67, line 13681).
    /// Response is `ok`; the new pinned order arrives via
    /// `updateChatPosition`. Rollback restores the swapped `order`
    /// values (`RequestRollback::ChatPinOrder`).
    SetPinnedChats,
    /// Slice CL2: `readChatList` (schema 1.8.67, line 13684).
    /// Response is `ok`; badges clear via `updateChatReadInbox` /
    /// `updateChatUnreadMentionCount`.
    ReadChatList,
    /// Chat-row "Mark as read": `viewMessages` with `force_read` on the
    /// chat's last message. Response is `ok`; the badge clears via
    /// `updateChatReadInbox`. Separate from the open chat's
    /// `ViewMessages`, so an on-screen view in flight never blocks it.
    ReadChatInbox,
    /// Slice CL2: `clearRecentlyFoundChats` (schema 1.8.67, line 11671).
    /// Response is `ok`; the local recents were already cleared
    /// optimistically (TGX `SearchManager.clearRecentlyFoundChats`).
    ClearRecentlyFoundChats,
    /// Slice CL2: `getArchiveChatListSettings` (schema 1.8.67, line
    /// 13421). Response is `archiveChatListSettings`; stored in
    /// `ChatListState::archive_chat_list_settings`.
    GetArchiveChatListSettings,
    /// Slice CL2: `setArchiveChatListSettings` (schema 1.8.67, line
    /// 13424). Response is `ok`; the settings were flipped
    /// optimistically, rollback on refusal.
    SetArchiveChatListSettings,
    /// Parity slice: `createChatFolder`. Response is `chatFolderInfo`;
    /// upserted into `ChatListState::chat_folders` (`updateChatFolders` stays the
    /// source of truth).
    CreateChatFolder,
    /// Parity slice: `editChatFolder`. Response is `chatFolderInfo`;
    /// upserted into `ChatListState::chat_folders`. Also the purpose of the
    /// remove-from-folder chain (chat dropped from the folder's spec);
    /// correlated via `PendingRequest::folder_id`.
    EditChatFolder,
    /// Parity slice: `deleteChatFolder`. Response is `ok`; the folder leaves
    /// `ChatListState::chat_folders` on ok (correlated via
    /// `PendingRequest::folder_id`).
    DeleteChatFolder,
    /// `editChatFolder` sent by the remove-from-folder chain, which no box
    /// waits on (the box edit is `EditChatFolder`): its answer is not kept
    /// in `DialogAnswers`.
    RemoveChatFromFolder,
    /// Parity slice: `reorderChatFolders`. Response is `ok`; the tab order
    /// is applied optimistically at send time and confirmed by the next
    /// `updateChatFolders`.
    ReorderChatFolders,
    /// Parity slice: `toggleChatFolderTags`. Response is `ok`;
    /// `ChatListState::are_folder_tags_enabled` flips optimistically at send.
    ToggleChatFolderTags,
    /// Parity slice: `getChatFolder`. Response is the full `chatFolder`
    /// spec, cached in `ChatListState::folder_specs` (keyed by
    /// `PendingRequest::folder_id`) for the edit dialog prefill and the
    /// remove-from-folder chain.
    GetChatFolder,
    /// Parity slice: `getChatListsToAddChat`. Response is `chatLists`;
    /// cached in `ChatListState::chat_lists_for_add` (keyed by
    /// `PendingRequest::chat_id`) for the per-chat folder picker.
    GetChatListsToAddChat,
    /// Parity slice: `getChatFolderChatsToLeave`. Response is `chats`;
    /// cached in `ChatListState::folder_chats_to_leave` (keyed by
    /// `PendingRequest::folder_id`) for the delete-confirm dialog.
    GetChatFolderChatsToLeave,
    /// `getRecommendedChatFolders`. Response is `recommendedChatFolders`,
    /// cached in `ChatListState::recommended_folders`.
    GetRecommendedChatFolders,
    /// `getChatsForChatFolderInviteLink`. Response is `chats` — the folder
    /// chats a link can grant (`ChatListState::folder_shares`, the
    /// box's `chats`, keyed by `PendingRequest::folder_id`).
    GetChatsForFolderInviteLink,
    /// `getChatFolderInviteLinks`. Response is `chatFolderInviteLinks`
    /// (`ChatListState::folder_shares`, the box's `links`,
    /// keyed by folder id).
    GetChatFolderInviteLinks,
    /// `createChatFolderInviteLink`. Response is `chatFolderInviteLink`.
    CreateChatFolderInviteLink,
    /// `editChatFolderInviteLink`. Response is `chatFolderInviteLink`.
    EditChatFolderInviteLink,
    /// `deleteChatFolderInviteLink`. Response is `ok`; the link leaves the
    /// cache optimistically at send time.
    DeleteChatFolderInviteLink,
    /// `checkChatFolderInviteLink` (an `addlist` link). Response is
    /// `chatFolderInviteLinkInfo`, which goes to the Add folder box
    /// that asked (`DialogAnswers`).
    CheckChatFolderInviteLink,
    /// `addChatFolderByInviteLink`. Response is `ok`.
    AddChatFolderByInviteLink,
    /// `getChat` for a chat an `addlist` link offers that is not loaded
    /// yet (its title shows in the "Add folder" dialog).
    GetFolderInviteChat,
    /// `getChatFolderNewChats`. Response is `chats`
    /// (`ChatListState::folder_new_chats`, keyed by `PendingRequest::folder_id`).
    GetChatFolderNewChats,
    /// `processChatFolderNewChats`. Response is `ok`; fire-and-forget.
    ProcessChatFolderNewChats,
    /// `getPremiumLimit` for a folder or Premium limit box or toast.
    /// Response is `premiumLimit`, which names its own type.
    GetPremiumLimit,
}

flat_purposes!(ChatList(ChatListPurpose) {
    LoadChats,
    LoadFolderChats,
    LoadArchiveChats,
    GetChatPreview,
    AddChatToList,
    DeleteChat,
    ToggleChatIsPinned,
    ToggleChatIsMarkedAsUnread,
    DeleteChatHistory,
    RemoveChatFromList,
    HidePromotion,
    LeaveBeforeDelete,
    SetPinnedChats,
    ReadChatList,
    ReadChatInbox,
    ClearRecentlyFoundChats,
    GetArchiveChatListSettings,
    SetArchiveChatListSettings,
    CreateChatFolder,
    EditChatFolder,
    RemoveChatFromFolder,
    DeleteChatFolder,
    ReorderChatFolders,
    ToggleChatFolderTags,
    GetChatFolder,
    GetChatListsToAddChat,
    GetChatFolderChatsToLeave,
    GetRecommendedChatFolders,
    GetChatsForFolderInviteLink,
    GetChatFolderInviteLinks,
    CreateChatFolderInviteLink,
    EditChatFolderInviteLink,
    DeleteChatFolderInviteLink,
    CheckChatFolderInviteLink,
    AddChatFolderByInviteLink,
    GetFolderInviteChat,
    GetChatFolderNewChats,
    ProcessChatFolderNewChats,
    GetPremiumLimit,
});
