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
    /// `Session::chat_preview_fetch` — it must NOT merge into the open
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
    /// Slice CL2: `setPinnedChats` (schema 1.8.67, line 13681).
    /// Response is `ok`; the new pinned order arrives via
    /// `updateChatPosition`. Rollback restores the swapped `order`
    /// values (`RequestRollback::ChatPinOrder`).
    SetPinnedChats,
    /// Slice CL2: `readChatList` (schema 1.8.67, line 13684).
    /// Response is `ok`; badges clear via `updateChatReadInbox` /
    /// `updateChatUnreadMentionCount`.
    ReadChatList,
    /// Slice CL2: `clearRecentlyFoundChats` (schema 1.8.67, line 11671).
    /// Response is `ok`; the local recents were already cleared
    /// optimistically (TGX `SearchManager.clearRecentlyFoundChats`).
    ClearRecentlyFoundChats,
    /// Slice CL2: `getArchiveChatListSettings` (schema 1.8.67, line
    /// 13421). Response is `archiveChatListSettings`; stored in
    /// `Session::archive_chat_list_settings`.
    GetArchiveChatListSettings,
    /// Slice CL2: `setArchiveChatListSettings` (schema 1.8.67, line
    /// 13424). Response is `ok`; the settings were flipped
    /// optimistically, rollback on refusal.
    SetArchiveChatListSettings,
    /// Parity slice: `createChatFolder`. Response is `chatFolderInfo`;
    /// upserted into `Session::chat_folders` (`updateChatFolders` stays the
    /// source of truth).
    CreateChatFolder,
    /// Parity slice: `editChatFolder`. Response is `chatFolderInfo`;
    /// upserted into `Session::chat_folders`. Also the purpose of the
    /// remove-from-folder chain (chat dropped from the folder's spec);
    /// correlated via `PendingRequest::folder_id`.
    EditChatFolder,
    /// Parity slice: `deleteChatFolder`. Response is `ok`; the folder leaves
    /// `Session::chat_folders` on ok (correlated via
    /// `PendingRequest::folder_id`).
    DeleteChatFolder,
    /// Parity slice: `reorderChatFolders`. Response is `ok`; the tab order
    /// is applied optimistically at send time and confirmed by the next
    /// `updateChatFolders`.
    ReorderChatFolders,
    /// Parity slice: `toggleChatFolderTags`. Response is `ok`;
    /// `Session::are_folder_tags_enabled` flips optimistically at send.
    ToggleChatFolderTags,
    /// Parity slice: `getChatFolder`. Response is the full `chatFolder`
    /// spec, cached in `Session::folder_specs` (keyed by
    /// `PendingRequest::folder_id`) for the edit dialog prefill and the
    /// remove-from-folder chain.
    GetChatFolder,
    /// Parity slice: `getChatListsToAddChat`. Response is `chatLists`;
    /// cached in `Session::chat_lists_for_add` (keyed by
    /// `PendingRequest::chat_id`) for the per-chat folder picker.
    GetChatListsToAddChat,
    /// Parity slice: `getChatFolderChatsToLeave`. Response is `chats`;
    /// cached in `Session::folder_chats_to_leave` (keyed by
    /// `PendingRequest::folder_id`) for the delete-confirm dialog.
    GetChatFolderChatsToLeave,
    /// `getRecommendedChatFolders`. Response is `recommendedChatFolders`,
    /// cached in `Session::recommended_folders`.
    GetRecommendedChatFolders,
    /// `getChatsForChatFolderInviteLink`. Response is `chats` — the folder
    /// chats a link can grant (`Session::folder_link_chats`, keyed by
    /// `PendingRequest::folder_id`).
    GetChatsForFolderInviteLink,
    /// `getChatFolderInviteLinks`. Response is `chatFolderInviteLinks`
    /// (`Session::folder_invite_links`, keyed by folder id).
    GetChatFolderInviteLinks,
    /// `createChatFolderInviteLink`. Response is `chatFolderInviteLink`.
    CreateChatFolderInviteLink,
    /// `editChatFolderInviteLink`. Response is `chatFolderInviteLink`.
    EditChatFolderInviteLink,
    /// `deleteChatFolderInviteLink`. Response is `ok`; the link leaves the
    /// cache optimistically at send time.
    DeleteChatFolderInviteLink,
    /// `checkChatFolderInviteLink` (an `addlist` link). Response is
    /// `chatFolderInviteLinkInfo` (`Session::folder_invite_info`).
    CheckChatFolderInviteLink,
    /// `addChatFolderByInviteLink`. Response is `ok`.
    AddChatFolderByInviteLink,
    /// `getChat` for a chat an `addlist` link offers that is not loaded
    /// yet (its title shows in the "Add folder" dialog).
    GetFolderInviteChat,
    /// `getChatFolderNewChats`. Response is `chats`
    /// (`Session::folder_new_chats`, keyed by `PendingRequest::folder_id`).
    GetChatFolderNewChats,
    /// `processChatFolderNewChats`. Response is `ok`; fire-and-forget.
    ProcessChatFolderNewChats,
    /// `getPremiumLimit` for a folder limit box. Response is
    /// `premiumLimit`, which names its own type.
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
    SetPinnedChats,
    ReadChatList,
    ClearRecentlyFoundChats,
    GetArchiveChatListSettings,
    SetArchiveChatListSettings,
    CreateChatFolder,
    EditChatFolder,
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
