use crate::auth::{AuthView, view_for};
use crate::composer::{CommandMenuItem, merge_command_menu_items};
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::{
    AccountGeneration, AccountKey, ChatId, FileId, MessageId, RequestId, ViewGeneration,
};
use crate::notify::{self, OsNotification, QueuedNotification};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AnimationItem, AuthorizationState, BotCommand, BotInfo, CallbackQueryAnswer,
    ChannelMemberStatus, ChatAction, ChatActiveStoriesView, ChatDraft, ChatFolderInfo,
    ChatFolderSpec, ChatJoinResult, ChatKind, ChatList, ChatNotificationSettings,
    ChatPositionUpdate, ConnectionState, EnvelopePayload, ErrorClass, ForumTopic, InlineKeyboard,
    MessageContent, MessageForwardInfo, MessageInteractionInfo, MessageOrigin, MessageReaction,
    MessageReplyTo, MessageSender, NotificationSettingsScope, NotificationSound, ParsedChatMember,
    ParsedFile, ParsedMessage, ParsedStory, ParsedUser, Poll, ReportOption, ReportSponsoredResult,
    ScopeNotificationSettings, SponsoredMessage, StickerFormat, StickerItem, StickerSetInfo,
    StoryAvailableReactionView, StoryListView,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPurpose {
    GetAuthorizationState,
    SetParameters,
    SetPhoneNumber,
    CheckAuthenticationCode,
    CheckAuthenticationPassword,
    LoadChats,
    /// Phase 7.1: single-shot `loadChats(chatListFolder(id))` when a folder
    /// tab is selected. Separate from `LoadChats` so the ok-response does
    /// not re-trigger main-list paging.
    LoadFolderChats,
    GetHistory,
    /// Any `sendMessage` (text / photo / document). Response `message` is pending.
    SendMessage,
    /// `sendMessageAlbum`. Response `messages` are pending until send-succeeded.
    SendMessageAlbum,
    OpenChat,
    CloseChat,
    ViewMessages,
    DownloadFile,
    SearchChats,
    SearchMessages,
    SearchRecentlyFoundChats,
    /// Phase 7.2: `searchPublicChats` — public username/title lookup across
    /// all public chats (not just known ones). Sent alongside `searchChats`.
    SearchPublicChats,
    AddRecentlyFoundChat,
    SearchChatMessages,
    /// `getChatHistory` around a jump target (Unigram `LoadMessageSliceImpl`).
    GetHistoryAround,
    /// `editMessageText` / `editMessageCaption`. Response is `message`.
    EditMessage,
    /// `deleteMessages`. Response is `ok`; rows leave via `updateDeleteMessages`.
    DeleteMessages,
    /// `forwardMessages`. Response is `messages`.
    ForwardMessages,
    /// `addMessageReaction`. Response is `ok`; chips via `updateMessageInteractionInfo`.
    AddMessageReaction,
    /// `removeMessageReaction`. Response is `ok`; chips via `updateMessageInteractionInfo`.
    RemoveMessageReaction,
    /// `pinChatMessage`. Response is `ok`; pin via `updateMessageIsPinned`.
    PinChatMessage,
    /// `setPollAnswer`. Response is `ok`; counts refresh via `updatePoll`.
    SetPollAnswer,
    /// `unpinChatMessage`. Response is `ok`; pin via `updateMessageIsPinned`.
    UnpinChatMessage,
    /// `setChatNotificationSettings`. Response is `ok`; mute via
    /// `updateChatNotificationSettings`.
    SetChatNotificationSettings,
    /// Parity slice: `getSavedNotificationSounds`. Response is
    /// `notificationSounds`; drives the sound picker and custom-sound
    /// playback.
    GetSavedNotificationSounds,
    /// Parity slice: `getScopeNotificationSettings`. Response is
    /// `scopeNotificationSettings`; the scope is correlated via
    /// `PendingRequest::scope`.
    GetScopeNotificationSettings,
    /// Parity slice: `setScopeNotificationSettings`. Response is `ok`;
    /// the new defaults arrive as `updateScopeNotificationSettings`.
    SetScopeNotificationSettings,
    /// `addChatToList` (`chatListArchive` or `chatListMain`). Response is `ok`;
    /// list membership via position / added-to-list updates.
    AddChatToList,
    /// `sendChatAction` (`chatActionTyping` / `chatActionCancel` /
    /// `chatActionRecordingVoiceNote`). Response is `ok`.
    SendChatAction,
    /// `openMessageContent` when a voice note or video note starts playing.
    /// Response is `ok`. `is_listened` / `is_viewed` arrive as
    /// `updateMessageContentOpened`.
    OpenMessageContent,
    /// `getInstalledStickerSets` (`stickerTypeRegular`). Response is `stickerSets`.
    GetInstalledStickerSets,
    /// `getStickerSet`. Response is `stickerSet`.
    GetStickerSet,
    /// `getSavedAnimations`. Response is `animations`.
    GetSavedAnimations,
    /// `setChatDraftMessage`. Response is `ok`; the draft also arrives as
    /// `updateChatDraftMessage`.
    SetChatDraftMessage,
    /// `getChatSponsoredMessages` (channel / bot chats). Response is
    /// `sponsoredMessages`; rows render Sponsored / Recommended.
    GetChatSponsoredMessages,
    /// `reportChatSponsoredMessage`. Response is `ReportSponsoredResult`;
    /// `OptionRequired` opens the report-option picker.
    ReportChatSponsoredMessage,
    /// `viewSponsoredChat`. Response is `ok`.
    ViewSponsoredChat,
    /// `clickChatSponsoredMessage`. Response is `ok`; fire-and-forget.
    ClickChatSponsoredMessage,
    /// `getMe`. Response is `user`; only the id is kept.
    GetMe,
    /// `getChatMember` for the current user in a channel. Response is
    /// `chatMember`; drives the composer gate and join/leave affordance.
    GetChatMember,
    /// `getUserFullInfo` for a bot user. Response is `userFullInfo`; the
    /// user id is resolved from the request's chat (`ChatKind::Private`).
    GetUserFullInfo,
    /// `getCommands` for a bot's global (default) command scope (Phase
    /// 3.3). Response is `botCommands`; the user id is resolved from the
    /// request's chat. The schema annotates the method "for bots only",
    /// so a user session gets an `error` answer — absorbed silently, no
    /// retry loop.
    GetCommands,
    /// `getCallbackQueryAnswer` for an inline keyboard callback-button press
    /// (Phase 3.2). Response is `callbackQueryAnswer`; the answer is shown
    /// via the transient status line (URL answers open in the OS browser).
    GetCallbackQueryAnswer,
    /// `joinChat`. Response is `ChatJoinResult`; own status also arrives via
    /// `updateChatMember`.
    JoinChat,
    /// `leaveChat`. Response is `ok`; own status also arrives via
    /// `updateChatMember`.
    LeaveChat,
    /// Phase 5.1: `getSupergroup`. Response is `supergroup`; resolves
    /// `ChatSummary::is_forum`.
    GetSupergroup,
    /// Phase 5.1: `getForumTopics` (first page). Response is `forumTopics`.
    GetForumTopics,
    /// Phase 5.1: `searchChatMessages` with `topic_id = messageTopicForum`
    /// and an empty query — per-topic history. Response is
    /// `foundChatMessages`; correlated via
    /// `PendingRequest::forum_topic_id`.
    GetTopicHistory,
    /// Phase 6: `getContacts`. Response is `users`; the user ids land in
    /// `Session::contacts`, the user objects via `updateUser`.
    GetContacts,
    /// Phase 6: `addContact`. Response is `ok`; the contact row refreshes
    /// via `updateUser` (and the contacts list is invalidated for refetch).
    AddContact,
    /// Phase 6: `getSupergroupFullInfo`. Response is `supergroupFullInfo`;
    /// correlated via `PendingRequest::supergroup_id`.
    GetSupergroupFullInfo,
    /// Phase A1: `setChatSlowModeDelay`. Response is `ok`; the new delay
    /// arrives via `updateSupergroupFullInfo`.
    SetChatSlowModeDelay,
    /// Phase 9.1: `loadActiveStories` (`storyListMain`). The stories
    /// arrive as `updateChatActiveStories` updates; feed the story tray.
    LoadActiveStories,
    /// Phase 9.1: `getChatActiveStories`. Response is `chatActiveStories`;
    /// handled like `updateChatActiveStories`.
    GetChatActiveStories,
    /// Phase 9.1: `getStory`. Response is `story`; the viewer prefetches
    /// every story in the tray entry before opening. `Ok` answers of
    /// `openStory` / `closeStory` need no handling (fire-and-forget).
    GetStory,
    OpenStory,
    CloseStory,
    /// Phase 9.2: `getStoryAvailableReactions`. Response is
    /// `availableReactions`; cached in
    /// `Session::story_available_reactions` for the viewer picker.
    GetStoryAvailableReactions,
    /// Phase 9.2: `setStoryReaction` (set) / removing the chosen reaction.
    /// Responses are `ok`; the new state arrives via `updateStory`.
    SetStoryReaction,
    RemoveStoryReaction,
    /// Phase 9.2: `deleteStory`. Response is `ok`; the deletion lands as
    /// `updateStoryDeleted`.
    DeleteStory,
    /// Phase 9.2: story reply — `sendMessage` with
    /// `inputMessageReplyToStory`. Response is `message`; the normal
    /// message-send updates handle it.
    SendStoryReply,
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
    Close,
    LogOut,
    Other,
}

fn is_auth_submit(purpose: RequestPurpose) -> bool {
    matches!(
        purpose,
        RequestPurpose::SetPhoneNumber
            | RequestPurpose::CheckAuthenticationCode
            | RequestPurpose::CheckAuthenticationPassword
    )
}

/// Classified TDLib error for an auth submit. Never includes the native message
/// (codes, passwords, and phone numbers live there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthRequestError {
    pub purpose: RequestPurpose,
    pub class: ErrorClass,
}

impl AuthRequestError {
    pub fn user_message(self) -> &'static str {
        match (self.purpose, self.class) {
            (RequestPurpose::SetPhoneNumber, ErrorClass::Invalid) => "phone not accepted",
            (RequestPurpose::SetPhoneNumber, ErrorClass::Flood) => {
                "too many phone attempts — wait and try again"
            }
            (RequestPurpose::CheckAuthenticationCode, ErrorClass::Invalid) => "code not accepted",
            (RequestPurpose::CheckAuthenticationCode, ErrorClass::Flood) => {
                "too many code attempts — wait and try again"
            }
            (RequestPurpose::CheckAuthenticationPassword, ErrorClass::Invalid) => {
                "password not accepted"
            }
            (RequestPurpose::CheckAuthenticationPassword, ErrorClass::Flood) => {
                "too many password attempts — wait and try again"
            }
            (_, ErrorClass::Unauthorized) => "session is no longer authorized",
            _ => "Telegram rejected the request",
        }
    }
}

/// Source + dest frozen when `forwardMessages` is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardFlight {
    pub extra: RequestId,
    pub dest_chat_id: ChatId,
    pub from_chat_id: ChatId,
    pub requested: usize,
}

/// Result of `forwardMessages` (`messages` or `error`). Dest title is resolved
/// from the loaded chat list (tdesktop ShareBox success names the peer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardResult {
    pub dest_chat_id: ChatId,
    pub dest_title: String,
    pub from_chat_id: ChatId,
    pub requested: usize,
    pub forwarded_ids: Vec<MessageId>,
}

impl ForwardResult {
    pub fn success_label(&self) -> String {
        let n = self.forwarded_ids.len();
        if n == 0 {
            format!("Could not forward to {}", self.dest_title)
        } else if n == 1 {
            format!("Forwarded to {}", self.dest_title)
        } else {
            format!("Forwarded {n} messages to {}", self.dest_title)
        }
    }
}

/// Sponsored messages for one chat (`getChatSponsoredMessages` response).
/// Rows render with a **Sponsored** / **Recommended** label.
#[derive(Debug, Clone, Default)]
pub struct ChatSponsoredMessages {
    pub messages: Vec<SponsoredMessage>,
    /// Schema `messages_between`: minimum number of ordinary messages between
    /// shown sponsored rows (0 = after all ordinary messages).
    pub messages_between: i32,
}

impl ChatSponsoredMessages {
    /// Rows in TDLib's response order. The schema does not promise an order,
    /// so the vector order is preserved verbatim.
    pub fn ordered(&self) -> Vec<&SponsoredMessage> {
        self.messages.iter().collect()
    }
}

/// `reportChatSponsoredMessage` waiting on the user's option choice
/// (`reportSponsoredResultOptionRequired`).
#[derive(Debug, Clone)]
pub struct SponsoredReportFlight {
    pub extra: RequestId,
    pub chat_id: ChatId,
    /// int53 sponsored message id.
    pub message_id: i64,
    pub title: String,
    pub options: Vec<ReportOption>,
}

/// Last `reportChatSponsoredMessage` outcome (no TDLib text is echoed).
#[derive(Debug, Clone)]
pub struct SponsoredReportOutcome {
    pub chat_id: ChatId,
    /// int53 sponsored message id.
    pub message_id: i64,
    pub result: ReportSponsoredResult,
}

impl SponsoredReportOutcome {
    pub fn user_message(&self) -> &'static str {
        self.result.user_message()
    }
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub id: RequestId,
    pub account_generation: AccountGeneration,
    pub purpose: RequestPurpose,
    pub chat_id: Option<ChatId>,
    pub view_generation: Option<ViewGeneration>,
    pub file_id: Option<i32>,
    pub search_generation: Option<u64>,
    pub around_message_id: Option<MessageId>,
    /// Phase 5.1: `forum_topic_id` for `GetTopicHistory` requests so the
    /// `foundChatMessages` response lands in the right topic history.
    pub forum_topic_id: Option<i32>,
    /// Phase 6: `user_id` for `GetUserFullInfo` (panel opened from the
    /// contacts list, where there is no chat) and `AddContact` requests so
    /// id-less responses (`userFullInfo`) land on the right user.
    pub user_id: Option<i64>,
    /// Phase 6: `supergroup_id` for `GetSupergroupFullInfo` requests so the
    /// id-less `supergroupFullInfo` response lands on the right group.
    pub supergroup_id: Option<i64>,
    /// Phase 9.1: `story_id` for `GetStory` requests so in-flight
    /// per-story dedupe distinguishes stories of the same chat.
    pub story_id: Option<i32>,
    /// Parity slice: `folder_id` for folder-scoped requests
    /// (`GetChatFolder`, `EditChatFolder`, `DeleteChatFolder`,
    /// `LoadFolderChats`) so responses correlate to the folder.
    pub folder_id: Option<i32>,
    /// Parity slice: `scope` for `GetScopeNotificationSettings` /
    /// `SetScopeNotificationSettings` so the id-less
    /// `scopeNotificationSettings` response lands on the right scope.
    pub scope: Option<NotificationSettingsScope>,
}

#[derive(Debug, Default)]
pub struct RequestRegistry {
    next: u64,
    pending: HashMap<u64, PendingRequest>,
}

impl RequestRegistry {
    pub fn register(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        view_generation: Option<ViewGeneration>,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id,
                view_generation,
                file_id: None,
                search_generation: None,
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                story_id: None,
                folder_id: None,
                scope: None,
            },
        );
        id
    }

    pub fn register_search(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        search_generation: u64,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id: None,
                view_generation: None,
                file_id: None,
                search_generation: Some(search_generation),
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                story_id: None,
                folder_id: None,
                scope: None,
            },
        );
        id
    }

    pub fn register_chat_search(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        chat_id: ChatId,
        search_generation: u64,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id: Some(chat_id),
                view_generation: None,
                file_id: None,
                search_generation: Some(search_generation),
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                story_id: None,
                folder_id: None,
                scope: None,
            },
        );
        id
    }

    pub fn register_around(
        &mut self,
        account_generation: AccountGeneration,
        chat_id: ChatId,
        view_generation: ViewGeneration,
        around_message_id: MessageId,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose: RequestPurpose::GetHistoryAround,
                chat_id: Some(chat_id),
                view_generation: Some(view_generation),
                file_id: None,
                search_generation: None,
                around_message_id: Some(around_message_id),
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                story_id: None,
                folder_id: None,
                scope: None,
            },
        );
        id
    }

    pub fn register_download(
        &mut self,
        account_generation: AccountGeneration,
        file_id: FileId,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose: RequestPurpose::DownloadFile,
                chat_id: None,
                view_generation: None,
                file_id: Some(file_id.0),
                search_generation: None,
                around_message_id: None,
                forum_topic_id: None,
                user_id: None,
                supergroup_id: None,
                story_id: None,
                folder_id: None,
                scope: None,
            },
        );
        id
    }

    pub fn take(&mut self, id: RequestId) -> Option<PendingRequest> {
        self.pending.remove(&id.0)
    }

    pub fn invalidate_account(&mut self) {
        self.pending.clear();
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn purpose(&self, id: RequestId) -> Option<RequestPurpose> {
        self.pending.get(&id.0).map(|p| p.purpose)
    }

    pub fn has_purpose(&self, purpose: RequestPurpose) -> bool {
        self.pending.values().any(|p| p.purpose == purpose)
    }

    pub fn has_purpose_for_chat(&self, purpose: RequestPurpose, chat_id: ChatId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.chat_id == Some(chat_id))
    }

    /// Mutable access to a pending request (parity slice: stamping
    /// `PendingRequest::scope` after `register`).
    pub fn pending_mut(&mut self, id: RequestId) -> Option<&mut PendingRequest> {
        self.pending.get_mut(&id.0)
    }

    /// Parity slice: per-scope in-flight check for
    /// `GetScopeNotificationSettings` (the purpose alone is shared by all
    /// three scopes).
    pub fn has_purpose_for_scope(
        &self,
        purpose: RequestPurpose,
        scope: NotificationSettingsScope,
    ) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.scope == Some(scope))
    }

    /// Phase 6: an in-flight request for a purpose/user pair (user-scoped
    /// `GetUserFullInfo` / `AddContact`).
    pub fn has_purpose_for_user(&self, purpose: RequestPurpose, user_id: i64) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.user_id == Some(user_id))
    }

    /// Phase 6: an in-flight request for a purpose/supergroup pair
    /// (`GetSupergroupFullInfo`).
    pub fn has_purpose_for_supergroup(&self, purpose: RequestPurpose, supergroup_id: i64) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.supergroup_id == Some(supergroup_id))
    }

    /// Phase 9.1: an in-flight request for a purpose/chat/story triple
    /// (`GetStory` prefetch of a chat's stories).
    pub fn has_purpose_for_story(
        &self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_id: i32,
    ) -> bool {
        self.pending.values().any(|p| {
            p.purpose == purpose && p.chat_id == Some(chat_id) && p.story_id == Some(story_id)
        })
    }

    /// Parity slice: an in-flight request for a purpose/folder pair
    /// (`GetChatFolder` / `LoadFolderChats` / folder mutations).
    pub fn has_purpose_for_folder(&self, purpose: RequestPurpose, folder_id: i32) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.folder_id == Some(folder_id))
    }

    /// Parity slice: the folder id stamped on an in-flight request, if any.
    pub fn folder_id_for(&self, id: RequestId) -> Option<i32> {
        self.pending.get(&id.0).and_then(|p| p.folder_id)
    }

    /// The in-flight request id for a purpose/chat pair (test hook; the live
    /// path matches responses by `@extra`).
    pub fn pending_extra_for(
        &self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
    ) -> Option<RequestId> {
        self.pending
            .values()
            .find(|p| p.purpose == purpose && p.chat_id == chat_id)
            .map(|p| p.id)
    }

    /// Parity slice: the in-flight request id for a purpose/folder pair
    /// (test hook; the live path matches responses by `@extra`).
    pub fn pending_extra_for_folder(
        &self,
        purpose: RequestPurpose,
        folder_id: i32,
    ) -> Option<RequestId> {
        self.pending
            .values()
            .find(|p| p.purpose == purpose && p.folder_id == Some(folder_id))
            .map(|p| p.id)
    }

    pub fn has_download(&self, file_id: FileId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == RequestPurpose::DownloadFile && p.file_id == Some(file_id.0))
    }
}

/// Outgoing read-receipt state from `last_read_outbox_message_id`.
/// Schema supports this; `0` means nothing outgoing has been read yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxReceipt {
    /// Not an outgoing server message (incoming or still pending).
    None,
    /// Reached the server; peer has not read past this id.
    Sent,
    /// `last_read_outbox_message_id` is >= this outgoing id.
    Read,
}

/// Badge label for the chat list. `None` when the chat is fully read.
pub fn unread_badge_text(count: i32) -> Option<String> {
    if count <= 0 {
        None
    } else if count > 99 {
        Some("99+".into())
    } else {
        Some(count.to_string())
    }
}

pub fn outgoing_status_label(pending: bool, receipt: OutboxReceipt) -> &'static str {
    if pending {
        "You (sending)"
    } else {
        match receipt {
            OutboxReceipt::Read => "You · read",
            OutboxReceipt::Sent | OutboxReceipt::None => "You · sent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatSummary {
    pub id: ChatId,
    pub title: String,
    pub kind: ChatKind,
    pub unread_count: i32,
    pub last_read_inbox_message_id: MessageId,
    pub last_read_outbox_message_id: MessageId,
    pub order: i64,
    pub is_pinned: bool,
    pub in_main_list: bool,
    /// `chatListArchive` membership (`updateChatPosition` / add-remove-from-list).
    pub in_archive: bool,
    pub archive_order: i64,
    pub archive_is_pinned: bool,
    /// `chatListFolder` membership: folder id → TDLib order
    /// (`updateChatPosition` / `updateChatLastMessage` positions /
    /// add-remove-from-list). Order 0 means membership confirmed but order
    /// not yet known (from `updateChatAddedToList` before the position
    /// arrives); sorting treats 0 as last. `is_pinned` is not tracked —
    /// pinned folder chats already sort first by order.
    pub folder_positions: BTreeMap<i32, i64>,
    /// `chat.notification_settings` / `updateChatNotificationSettings`.
    pub notification_settings: ChatNotificationSettings,
    /// Sidebar preview from `updateChatLastMessage`. Not logged.
    pub last_preview: String,
    /// Senders with an active `chatActionTyping` (`updateChatAction`).
    pub typing_senders: Vec<MessageSender>,
    /// `chat.draft_message` text draft. Voice/rich drafts are not stored.
    pub draft: Option<ChatDraft>,
    /// Own `chatMemberStatus*` in a broadcast channel (`getChatMember` /
    /// `updateChatMember`). `None` until the first fetch completes; drives the
    /// composer gate and the join/leave affordance.
    pub my_member_status: Option<ChannelMemberStatus>,
    /// `rights.can_post_messages` from `chatMemberStatusAdministrator`
    /// (TDLib 1.8.67). `Some` only when the status is Administrator and the
    /// rights block parsed; `None` means "no explicit restriction" — a bare
    /// admin still posts.
    pub my_admin_can_post_messages: Option<bool>,
    /// Phase 5.1: `supergroup.is_forum` (TDLib 1.8.67). `None` until
    /// `updateSupergroup` / the `getSupergroup` response resolves it; only
    /// meaningful for non-channel supergroups.
    pub is_forum: Option<bool>,
    /// Parity slice: `chat.photo.small` file id (`chatPhotoInfo`, schema
    /// 1.8.67 line 762). `None` when the chat has no photo. Updated by
    /// `updateChatPhoto`; the file itself lives in `Session::files`.
    pub photo_file_id: Option<i32>,
    /// Parity slice 4: `chat.permissions.can_send_basic_messages`
    /// (`chatPermissions`, schema 1.8.67 line 1070), refreshed by
    /// `updateChatPermissions` (line 10500). Gates the topic composer
    /// alongside `ForumTopic.is_closed`.
    pub can_send_basic_messages: bool,
}

impl ChatSummary {
    pub fn supported(&self) -> bool {
        self.kind.is_supported_cloud_chat()
    }

    /// `chatTypeSupergroup` with `is_channel: true`.
    pub fn is_channel(&self) -> bool {
        self.kind.is_channel()
    }

    /// Whether the composer is shown for this chat. In 2.3 admins get the
    /// composer in broadcast channels (derived from own membership, see
    /// `channel_admin_can_post`); everyone else in a channel keeps it hidden.
    /// All other supported chats keep the composer.
    pub fn can_post(&self) -> bool {
        if self.is_channel() {
            return self.channel_admin_can_post();
        }
        self.supported()
    }

    /// 2.3: channel posting rights derive from own membership. The creator
    /// always posts; an administrator posts unless their
    /// `rights.can_post_messages` is explicitly false. Unknown/absent
    /// membership (or any other status) keeps the composer hidden.
    pub fn channel_admin_can_post(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_post_messages.unwrap_or(true)
            }
            _ => false,
        }
    }

    /// Record own channel membership (`getChatMember` / `updateChatMember` /
    /// join/leave responses). `admin_can_post_messages` is the parsed
    /// `rights.can_post_messages` for an administrator, `None` otherwise.
    /// Returns true when the status changed.
    pub fn set_member_status(
        &mut self,
        status: ChannelMemberStatus,
        admin_can_post_messages: Option<bool>,
    ) -> bool {
        let changed = self.my_member_status != Some(status);
        self.my_member_status = Some(status);
        self.my_admin_can_post_messages = admin_can_post_messages;
        changed
    }

    pub fn is_forum_chat(&self) -> bool {
        self.is_forum == Some(true)
    }

    pub fn is_muted(&self) -> bool {
        self.notification_settings.is_muted()
    }

    pub fn is_peer_typing(&self) -> bool {
        !self.typing_senders.is_empty()
    }

    pub fn set_sender_action(&mut self, sender: MessageSender, action: ChatAction) {
        self.typing_senders.retain(|existing| *existing != sender);
        if action == ChatAction::Typing {
            self.typing_senders.push(sender);
        }
    }

    pub fn sidebar_preview(&self) -> String {
        if let Some(reason) = self.kind.gate_reason() {
            return reason.to_string();
        }
        if self.is_peer_typing() {
            return "typing…".into();
        }
        if let Some(draft) = &self.draft {
            let text = draft.text.replace('\n', " ");
            let text = text.trim();
            if !text.is_empty() {
                return format!("Draft: {text}");
            }
            if draft.reply_to_message_id.is_some() {
                return "Draft:".into();
            }
        }
        if !self.last_preview.is_empty() {
            return self.last_preview.clone();
        }
        if self.unread_count > 0 {
            return format!("{} unread", self.unread_count);
        }
        "cloud chat".into()
    }

    pub fn outbox_receipt(&self, message: &HistoryMessage) -> OutboxReceipt {
        if !message.is_outgoing || message.pending {
            return OutboxReceipt::None;
        }
        if self.last_read_outbox_message_id.0 > 0
            && message.id.0 <= self.last_read_outbox_message_id.0
        {
            OutboxReceipt::Read
        } else {
            OutboxReceipt::Sent
        }
    }
}

/// Parity slice: map a chat to its `NotificationSettingsScope` for
/// `use_default_*` fallback (`notificationSettingsScope*`, td_api.tl lines
/// 3337–3343). Secret chats share the private-chat scope; unknown kinds
/// fall back to groups.
pub fn scope_for_chat_kind(kind: &ChatKind) -> NotificationSettingsScope {
    match kind {
        ChatKind::Private { .. } | ChatKind::Secret { .. } => {
            NotificationSettingsScope::PrivateChats
        }
        ChatKind::Supergroup {
            is_channel: true, ..
        } => NotificationSettingsScope::ChannelChats,
        _ => NotificationSettingsScope::GroupChats,
    }
}

fn placeholder_chat(chat_id: ChatId) -> ChatSummary {
    ChatSummary {
        id: chat_id,
        title: format!("chat {}", chat_id.0),
        kind: ChatKind::Unknown,
        unread_count: 0,
        last_read_inbox_message_id: MessageId(0),
        last_read_outbox_message_id: MessageId(0),
        order: 0,
        is_pinned: false,
        in_main_list: false,
        in_archive: false,
        archive_order: 0,
        archive_is_pinned: false,
        folder_positions: BTreeMap::new(),
        notification_settings: ChatNotificationSettings::default(),
        last_preview: String::new(),
        typing_senders: Vec::new(),
        draft: None,
        my_member_status: None,
        my_admin_can_post_messages: None,
        is_forum: None,
        photo_file_id: None,
        // Parity slice 4: lenient default true — the real `chat` object
        // always carries `permissions`; only `updateNewChat` /
        // `updateChatPermissions` ever set it to false.
        can_send_basic_messages: true,
    }
}

fn preview_from_content(content: &MessageContent) -> String {
    content.preview()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    pub content: MessageContent,
    pub pending: bool,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.is_pinned` / `updateMessageIsPinned`.
    pub is_pinned: bool,
    /// Schema `message.media_album_id`. `0` is not an album.
    pub media_album_id: i64,
    /// Schema `message.reply_markup` — `replyMarkupInlineKeyboard` only
    /// (Phase 3.2). Rendered as the button grid under the message.
    pub reply_markup: Option<InlineKeyboard>,
}

impl HistoryMessage {
    pub fn can_react(&self) -> bool {
        !self.pending && self.id.0 > 0
    }

    /// Already-sent messages can be pinned/unpinned (live `messageProperties.can_be_pinned`
    /// stays out — same default as edit/forward/react).
    pub fn can_pin(&self) -> bool {
        !self.pending && self.id.0 > 0
    }

    pub fn emoji_reaction_chips(&self) -> Vec<&MessageReaction> {
        self.interaction_info
            .as_ref()
            .map(MessageInteractionInfo::emoji_chips)
            .unwrap_or_default()
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.interaction_info
            .as_ref()
            .is_some_and(|info| info.chosen_emoji(emoji))
    }
}

#[derive(Debug, Default)]
pub struct HistoryState {
    pub messages: BTreeMap<i64, HistoryMessage>,
    pub tombstones: HashSet<i64>,
    pub loaded_complete: bool,
    pub view_generation: ViewGeneration,
    /// Message ids TDLib has accepted for `viewMessages` this open generation.
    pub viewed: HashSet<i64>,
    /// In-flight `viewMessages` ids. Cleared on send failure or TDLib error so we can retry.
    pub viewing: HashSet<i64>,
}

impl HistoryState {
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    pub fn oldest_id(&self) -> Option<MessageId> {
        self.messages.keys().next().copied().map(MessageId)
    }

    fn upsert(&mut self, message: HistoryMessage) {
        if self.tombstones.contains(&message.id.0) {
            return;
        }
        self.messages.insert(message.id.0, message);
    }

    fn remove(&mut self, id: MessageId, permanent: bool) {
        self.messages.remove(&id.0);
        if permanent {
            self.tombstones.insert(id.0);
        }
    }

    fn replace_id(&mut self, old: MessageId, new_message: HistoryMessage) {
        self.messages.remove(&old.0);
        self.upsert(new_message);
    }

    pub fn contains(&self, id: MessageId) -> bool {
        self.messages.contains_key(&id.0)
    }

    pub fn is_tombstone(&self, id: MessageId) -> bool {
        self.tombstones.contains(&id.0)
    }

    fn update_content(&mut self, id: MessageId, content: MessageContent) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.content = content;
            true
        } else {
            false
        }
    }

    /// Phase 3.2: `updateMessageEdited` replaces the message's inline
    /// keyboard (or removes it when `None`).
    fn update_reply_markup(&mut self, id: MessageId, reply_markup: Option<InlineKeyboard>) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.reply_markup = reply_markup;
            true
        } else {
            false
        }
    }

    fn update_interaction_info(
        &mut self,
        id: MessageId,
        interaction_info: Option<MessageInteractionInfo>,
    ) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.interaction_info = interaction_info;
            true
        } else {
            false
        }
    }

    fn update_is_pinned(&mut self, id: MessageId, is_pinned: bool) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.is_pinned = is_pinned;
            true
        } else {
            false
        }
    }

    fn mark_content_opened(&mut self, id: MessageId) {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.content.mark_content_opened();
        }
    }

    /// Newest pinned message in loaded history (`getChatPinnedMessage` is newest).
    pub fn newest_pinned(&self) -> Option<&HistoryMessage> {
        self.messages
            .values()
            .rev()
            .find(|message| message.is_pinned)
    }
}

/// Global search (official sidebar field): recents, then `searchChats` + `searchMessages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    Closed,
    Idle,
    Searching,
    Ready,
    Empty,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMessageHit {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
    pub is_outgoing: bool,
    pub content: MessageContent,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    pub is_pinned: bool,
    pub media_album_id: i64,
    pub reply_markup: Option<InlineKeyboard>,
}

impl SearchMessageHit {
    fn from_parsed(message: &ParsedMessage) -> Self {
        Self {
            chat_id: message.chat_id,
            message_id: message.id,
            preview: message.content.preview(),
            is_outgoing: message.is_outgoing,
            content: message.content.clone(),
            reply_to: message.reply_to.clone(),
            forward_info: message.forward_info.clone(),
            interaction_info: message.interaction_info.clone(),
            is_pinned: message.is_pinned,
            media_album_id: message.media_album_id,
            reply_markup: message.reply_markup.clone(),
        }
    }

    fn into_history(self) -> HistoryMessage {
        HistoryMessage {
            id: self.message_id,
            chat_id: self.chat_id,
            is_outgoing: self.is_outgoing,
            content: self.content,
            pending: false,
            reply_to: self.reply_to,
            forward_info: self.forward_info,
            interaction_info: self.interaction_info,
            is_pinned: self.is_pinned,
            media_album_id: self.media_album_id,
            reply_markup: self.reply_markup,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub open: bool,
    pub query: String,
    pub generation: u64,
    pub status: SearchStatus,
    pub chat_ids: Vec<ChatId>,
    pub messages: Vec<SearchMessageHit>,
    /// Phase 7.2: `searchPublicChats` results (username/title lookup across
    /// all public chats — not just known ones). Tracked separately from
    /// `chat_ids` (offline `searchChats`) with its own done/error flags so
    /// the status waits for all three requests.
    pub public_chat_ids: Vec<ChatId>,
    /// Empty-query surface: `searchRecentlyFoundChats` (official Recent).
    pub recents: bool,
    chats_done: bool,
    messages_done: bool,
    public_done: bool,
    chats_error: bool,
    messages_error: bool,
    public_error: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            generation: 0,
            status: SearchStatus::Closed,
            chat_ids: Vec::new(),
            messages: Vec::new(),
            public_chat_ids: Vec::new(),
            recents: false,
            chats_done: false,
            messages_done: false,
            public_done: false,
            chats_error: false,
            messages_error: false,
            public_error: false,
        }
    }
}

impl SearchState {
    pub fn open_field(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        self.query.clear();
        self.recents = true;
        self.status = SearchStatus::Idle;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.recents = false;
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.recents = true;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
        self.status = if self.open {
            SearchStatus::Idle
        } else {
            SearchStatus::Closed
        };
    }

    /// Empty search field: wait only for `searchRecentlyFoundChats`.
    pub fn begin_recents(&mut self) -> u64 {
        self.open = true;
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.recents = true;
        self.messages_done = true;
        self.public_done = true;
        self.generation
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.open = true;
        self.query = query.to_string();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.recents = false;
        self.generation
    }

    fn clear_results(&mut self) {
        self.chat_ids.clear();
        self.messages.clear();
        self.public_chat_ids.clear();
        self.chats_done = false;
        self.messages_done = false;
        self.public_done = false;
        self.chats_error = false;
        self.messages_error = false;
        self.public_error = false;
    }

    fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.open
    }

    pub(crate) fn accept_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.chat_ids = chat_ids;
        self.chats_done = true;
        self.chats_error = error;
        self.finish_if_complete();
    }

    pub(crate) fn accept_messages(&mut self, messages: Vec<SearchMessageHit>, error: bool) {
        self.messages = messages;
        self.messages_done = true;
        self.messages_error = error;
        self.finish_if_complete();
    }

    pub(crate) fn accept_public_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.public_chat_ids = chat_ids;
        self.public_done = true;
        self.public_error = error;
        self.finish_if_complete();
    }

    fn finish_if_complete(&mut self) {
        if !(self.chats_done && self.messages_done && self.public_done) {
            return;
        }
        let any = !self.chat_ids.is_empty()
            || !self.messages.is_empty()
            || !self.public_chat_ids.is_empty();
        self.status = if any {
            SearchStatus::Ready
        } else if self.chats_error || self.messages_error || self.public_error {
            SearchStatus::Failed
        } else if self.recents {
            SearchStatus::Idle
        } else {
            SearchStatus::Empty
        };
    }
}

/// Jump-to-message after an in-chat hit (Unigram `LoadMessageSliceAsync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJump {
    None,
    /// `getChatHistory` around the target is in flight — not yet loaded.
    Loading {
        message_id: MessageId,
    },
    Ready {
        message_id: MessageId,
    },
    /// Deleted (tombstone) or inaccessible after the around-load returned.
    Missing {
        message_id: MessageId,
    },
}

impl ChatSearchJump {
    pub fn message_id(self) -> Option<MessageId> {
        match self {
            Self::None => None,
            Self::Loading { message_id }
            | Self::Ready { message_id }
            | Self::Missing { message_id } => Some(message_id),
        }
    }

    pub fn is_ready_at(self, id: MessageId) -> bool {
        matches!(self, Self::Ready { message_id } if message_id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJumpNeed {
    AlreadyReady,
    Missing,
    LoadAround,
}

/// Phase 5.1: per-topic history for a forum supergroup, fetched with
/// `searchChatMessages` (`topic_id = messageTopicForum`, empty query).
/// `next_from_message_id` pages older messages the same way `foundChatMessages`
/// does for in-chat search; `loaded_complete` once a page returns
/// `next_from_message_id` 0 (or an empty page).
#[derive(Debug)]
pub struct TopicHistory {
    pub messages: BTreeMap<i64, HistoryMessage>,
    pub next_from_message_id: MessageId,
    pub loaded_complete: bool,
}

impl Default for TopicHistory {
    fn default() -> Self {
        Self {
            messages: BTreeMap::new(),
            next_from_message_id: MessageId(0),
            loaded_complete: false,
        }
    }
}

impl TopicHistory {
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    /// Parity slice 4: live topic messages (incoming updates and own
    /// sends) land here when the topic is loaded. Entries are only ever
    /// created by the `GetTopicHistory` fetch — upserting into a missing
    /// topic would corrupt the paging cursor.
    fn upsert(&mut self, message: HistoryMessage) {
        self.messages.insert(message.id.0, message);
    }

    /// Parity slice 4: `updateMessageSendSucceeded` / `Failed` replace the
    /// pending row, mirroring `HistoryState::replace_id`.
    fn replace_id(&mut self, old: MessageId, new_message: HistoryMessage) {
        self.messages.remove(&old.0);
        self.upsert(new_message);
    }
}

/// In-chat search (tdesktop `searchInChat` / ComposeSearch): `searchChatMessages`.
#[derive(Debug, Clone)]
pub struct ChatSearchState {
    pub open: bool,
    pub chat_id: Option<ChatId>,
    pub query: String,
    pub generation: u64,
    pub status: SearchStatus,
    pub hits: Vec<SearchMessageHit>,
    pub total_count: i32,
    pub next_from_message_id: MessageId,
    pub selected: Option<usize>,
    pub jump: ChatSearchJump,
}

impl Default for ChatSearchState {
    fn default() -> Self {
        Self {
            open: false,
            chat_id: None,
            query: String::new(),
            generation: 0,
            status: SearchStatus::Closed,
            hits: Vec::new(),
            total_count: 0,
            next_from_message_id: MessageId(0),
            selected: None,
            jump: ChatSearchJump::None,
        }
    }
}

impl ChatSearchState {
    pub fn open_for(&mut self, chat_id: ChatId) {
        if self.open && self.chat_id == Some(chat_id) {
            return;
        }
        self.open = true;
        self.chat_id = Some(chat_id);
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Idle;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chat_id = None;
        self.query.clear();
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.open = true;
        self.query = query.to_string();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.generation
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
        self.status = if self.open {
            SearchStatus::Idle
        } else {
            SearchStatus::Closed
        };
    }

    fn clear_results(&mut self) {
        self.hits.clear();
        self.total_count = 0;
        self.next_from_message_id = MessageId(0);
        self.selected = None;
        self.jump = ChatSearchJump::None;
    }

    fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.open
            && pending.and_then(|p| p.chat_id) == self.chat_id
    }

    pub(crate) fn accept_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
        error: bool,
    ) {
        self.hits = hits;
        self.total_count = total_count;
        self.next_from_message_id = next_from_message_id;
        self.selected = if self.hits.is_empty() { None } else { Some(0) };
        self.jump = ChatSearchJump::None;
        self.status = if !self.hits.is_empty() {
            SearchStatus::Ready
        } else if error {
            SearchStatus::Failed
        } else {
            SearchStatus::Empty
        };
    }

    pub fn selected_hit(&self) -> Option<&SearchMessageHit> {
        self.selected.and_then(|i| self.hits.get(i))
    }

    /// Newer hit (Unigram `NextExecute`: lower index; results are newest-first).
    pub fn select_newer(&mut self) -> Option<MessageId> {
        let index = self.selected?;
        if index == 0 {
            return None;
        }
        self.selected = Some(index - 1);
        self.hits.get(index - 1).map(|hit| hit.message_id)
    }

    /// Older hit (Unigram `PreviousExecute`: higher index).
    pub fn select_older(&mut self) -> Option<MessageId> {
        let index = self.selected?;
        if index + 1 >= self.hits.len() {
            return None;
        }
        self.selected = Some(index + 1);
        self.hits.get(index + 1).map(|hit| hit.message_id)
    }

    pub fn select_message(&mut self, message_id: MessageId) -> bool {
        let Some(index) = self
            .hits
            .iter()
            .position(|hit| hit.message_id == message_id)
        else {
            return false;
        };
        self.selected = Some(index);
        true
    }

    pub fn position_label(&self) -> String {
        match (self.selected, self.hits.len()) {
            (Some(i), n) if n > 0 => format!("{} of {n}", i + 1),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    CloseRequested,
    WaitingClosed,
    Closed,
}

/// Composer sticker panel (Unigram `StickerDrawerViewModel` installed regular sets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerPanel {
    pub open: bool,
    pub sets: Vec<StickerSetInfo>,
    pub selected_set_id: Option<i64>,
    pub stickers: Vec<StickerItem>,
    pub loaded_set_id: Option<i64>,
    pub loading_sets: bool,
    pub loading_set: bool,
    pub failed: bool,
}

/// Saved GIFs (`getSavedAnimations`). tdesktop Gifs tab / Unigram animation drawer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifPanel {
    pub open: bool,
    pub animations: Vec<AnimationItem>,
    pub loading: bool,
    pub failed: bool,
    /// `updateSavedAnimations` arrived while the panel was open.
    pub stale: bool,
}

impl GifPanel {
    pub fn close(&mut self) {
        self.open = false;
    }
}

impl StickerPanel {
    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn selected_needs_load(&self) -> Option<i64> {
        let id = self.selected_set_id?;
        if self.loading_set || self.loaded_set_id == Some(id) {
            None
        } else {
            Some(id)
        }
    }
}

pub struct Session {
    pub account: AccountKey,
    pub account_generation: AccountGeneration,
    pub auth: AuthorizationState,
    pub auth_view: AuthView,
    pub connection: ConnectionState,
    pub chats: HashMap<i64, ChatSummary>,
    pub main_order: Vec<ChatId>,
    pub archive_order: Vec<ChatId>,
    /// Phase 7.1: folder list from `updateChatFolders` (schema 1.8.67 line
    /// 10606). Empty until TDLib pushes the update (after authorization).
    pub chat_folders: Vec<ChatFolderInfo>,
    /// Parity slice: `are_tags_enabled` from `updateChatFolders` (schema
    /// 1.8.67 line 10606). When true, chat rows render folder-name tag
    /// chips; toggled via `toggleChatFolderTags` (`:13376`).
    pub are_folder_tags_enabled: bool,
    /// Parity slice: cached full `chatFolder` specs from `getChatFolder`
    /// responses (keyed by folder id) — edit dialog prefill and the
    /// remove-from-folder chain. Stale entries are dropped when the folder
    /// is edited or deleted.
    pub folder_specs: HashMap<i32, ChatFolderSpec>,
    /// Parity slice: `getChatListsToAddChat` results per chat id — the chat
    /// lists a chat may be added to. Drives the per-chat folder picker as
    /// the schema intends (`:13347` doc comment).
    pub chat_lists_for_add: HashMap<i64, Vec<ChatList>>,
    /// Parity slice: folder ids whose `loadChats(chatListFolder)` paging hit
    /// 404 — no "load more" for these folders.
    pub folder_chats_exhausted: HashSet<i32>,
    /// Parity slice: queued remove-from-folder intents `(chat_id,
    /// folder_id)`. The driver sends `getChatFolder`, then `editChatFolder`
    /// with the chat dropped from the spec — there is no
    /// `removeChatFromList` in 1.8.67.
    pub folder_remove_queue: Vec<(ChatId, i32)>,
    /// Parity slice: `getChatFolderChatsToLeave` results per folder id —
    /// chats the delete-confirm dialog offers to leave with the folder.
    pub folder_chats_to_leave: HashMap<i32, Vec<i64>>,
    pub histories: HashMap<i64, HistoryState>,
    pub open_chat: Option<ChatId>,
    /// Phase 8.1: whether the OS considers our window focused. The UI sets
    /// this from `Window::is_window_active` on every render; it defaults to
    /// true so the reducer never notifies before the first paint measures it.
    pub app_active: bool,
    /// Phase 8.1: mirror of `settings::Preferences::hide_notification_previews`
    /// (default true). There is no settings UI yet, so the value lives on the
    /// session for the reducer to apply.
    pub hide_notification_previews: bool,
    /// Phase 8.1: notifications decided by the reducer, drained by the UI for
    /// OS dispatch. Same-chat bursts coalesce into one entry ("N new messages").
    pub pending_notifications: Vec<QueuedNotification>,
    /// Parity slice: `getSavedNotificationSounds` cache (titles / durations
    /// for the sound picker; `sound` files download on demand).
    pub saved_notification_sounds: Vec<NotificationSound>,
    /// Parity slice: the saved-sound list has been fetched at least once.
    pub saved_sounds_loaded: bool,
    /// Parity slice: `updateSavedNotificationSounds` arrived since the last
    /// fetch — the driver refetches on the next ingest.
    pub saved_sounds_stale: bool,
    /// Parity slice: `getScopeNotificationSettings` results per scope; used
    /// for `use_default_*` fallback (e.g. default sound) and the scope
    /// defaults settings view.
    pub scope_notification_settings: HashMap<NotificationSettingsScope, ScopeNotificationSettings>,
    /// Parity slice: scopes with a `getScopeNotificationSettings` in flight.
    pub scope_settings_loading: HashSet<NotificationSettingsScope>,
    /// Parity slice: downloaded-file id → notification sound id, for files
    /// fetched as notification sounds.
    pub sound_file_ids: HashMap<i32, i64>,
    /// Parity slice: sound ids with playback requested whose file is not
    /// local yet. When the file completes, its path lands in
    /// `pending_sound_plays`.
    pub pending_sound_downloads: HashSet<i64>,
    /// Parity slice: local sound-file paths the UI should play, drained by
    /// `flush_notifications`. The reducer never spawns processes.
    pub pending_sound_plays: Vec<std::path::PathBuf>,
    /// Phase 5.1: selected forum topic (`forum_topic_id`) of the open chat.
    /// `None` = topic list (or a non-forum chat). Reset by `open_chat`.
    pub open_topic: Option<i32>,
    /// Phase 5.1: cached `forumTopics` per forum chat id (first page only).
    pub forum_topics: HashMap<i64, Vec<ForumTopic>>,
    /// Phase 5.1: per-topic histories keyed by `(chat_id, forum_topic_id)`.
    pub topic_histories: HashMap<(i64, i32), TopicHistory>,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    pub chats_exhausted: bool,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// Last classified error for phone / code / password submit. Never a secret.
    pub last_auth_error: Option<AuthRequestError>,
    /// In-flight `forwardMessages` (dest / source / requested count).
    pub in_flight_forward: Option<ForwardFlight>,
    /// Last `forwardMessages` outcome for the dest picker success surface.
    pub last_forward: Option<ForwardResult>,
    /// Last `callbackQueryAnswer` to an inline keyboard callback-button press
    /// (Phase 3.2). The UI takes it on the next poll and shows the answer in
    /// the status line (URL answers open in the OS browser).
    pub last_callback_answer: Option<CallbackQueryAnswer>,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: HashMap<i32, ParsedFile>,
    /// `downloadFile` in flight (until completed, undownloadable, idle, or error).
    pub downloading: HashSet<i32>,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    download_extras: HashMap<u64, i32>,
    pub search: SearchState,
    pub chat_search: ChatSearchState,
    /// Installed regular sticker sets + the loaded `stickerSet` for the picker.
    pub stickers: StickerPanel,
    /// Saved animations (`getSavedAnimations`) for the GIF picker.
    pub gifs: GifPanel,
    /// `userTypeBot` ids from `updateUser`. Private chats with these users skip drafts.
    bot_user_ids: HashSet<i64>,
    /// Cached `botInfo` from `getUserFullInfo` / `updateUserFullInfo`, keyed
    /// by bot user id. `None` records "fetched, not a bot" so a null
    /// `bot_info` does not trigger a refetch loop.
    pub bot_info: HashMap<i64, Option<BotInfo>>,
    /// Cached `getCommands` results for the default scope (a null `scope`
    /// selects `botCommandScopeDefault`, Phase 3.3), keyed by bot user id. Presence records "fetched"
    /// so the driver never retries — including when the response was an
    /// `error` (user sessions; `getCommands` is annotated "for bots only").
    pub bot_commands: HashMap<i64, Vec<BotCommand>>,
    /// Composer text changed since the last persisted draft. Remote
    /// `updateChatDraftMessage` must not replace it (schema comment).
    draft_dirty: HashSet<i64>,
    /// Send succeeded; UI clears the server draft if the composer is still empty.
    pub draft_clears: Vec<ChatId>,
    /// Sponsored messages per chat (`getChatSponsoredMessages`).
    pub sponsored: HashMap<i64, ChatSponsoredMessages>,
    /// In-flight sponsored-message report waiting on an option choice.
    pub sponsored_report: Option<SponsoredReportFlight>,
    /// Report target chosen by the user (chat + sponsored message id); cleared
    /// when the flow resolves.
    sponsored_report_target: Option<(ChatId, i64)>,
    /// Last `reportChatSponsoredMessage` outcome note.
    pub last_sponsored_report: Option<SponsoredReportOutcome>,
    /// Own user id from `getMe` (TDLib 1.8.67). `None` until the first
    /// `getMe` response; needed to resolve `getChatMember` ownership.
    pub my_user_id: Option<i64>,
    /// Phase 6: user directory from `updateUser`, keyed by user id. Feeds
    /// the contacts list and the user info panel.
    pub users: HashMap<i64, ParsedUser>,
    /// Phase 6: `getContacts` result — user ids, in server order. `None`
    /// until the first `users` response; `contacts_error` records a failed
    /// fetch so the UI can offer a retry.
    pub contacts: Option<Vec<i64>>,
    pub contacts_error: bool,
    /// Phase 6: cached `getUserFullInfo` bios, keyed by user id. Presence
    /// records "fetched" so the driver never refetches.
    pub user_full_infos: HashMap<i64, UserFullInfoData>,
    /// Phase 6: cached `getSupergroupFullInfo`, keyed by supergroup id.
    /// Presence records "fetched".
    pub supergroup_full_infos: HashMap<i64, SupergroupFullInfoData>,
    /// Parity slice: first active username per supergroup (`supergroup`
    /// object / `updateSupergroup`, schema 1.8.67 line 2746), keyed by
    /// supergroup id. Feeds the channel/supergroup header's @username.
    pub supergroup_usernames: HashMap<i64, String>,
    /// Phase A1: the viewer's own `chatMemberStatus*` per supergroup
    /// (`supergroup.status` / `updateSupergroup`, schema 1.8.67 line 2746).
    /// Drives the slow-mode bypass (admins/creators are exempt) and gates
    /// the admin slow-mode control. Absent = unknown (gated, no bypass).
    pub supergroup_member_status: HashMap<i64, ChannelMemberStatus>,
    /// Phase A1: the viewer's `rights.can_restrict_members` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, lines
    /// 2500/1092). `setChatSlowModeDelay` requires this right (line
    /// 13551). Absent = unknown, treated as lacking the right.
    pub supergroup_restrict_right: HashMap<i64, bool>,
    /// Phase 6: the open user / supergroup info panel, if any.
    pub open_info_panel: Option<InfoPanelTarget>,
    /// Phase 9.1: active stories per chat from `updateChatActiveStories` /
    /// `getChatActiveStories` (TDLib 1.8.67, `schema/td_api.tl:6776-6783`),
    /// keyed by chat id. Entries whose `list` is not `Main` (archived or
    /// not shown in any story list) are dropped on insert.
    pub story_tray: HashMap<i64, ChatActiveStoriesView>,
    /// Phase 9.1: full story objects from `getStory` (and `updateStory`
    /// updates), keyed by `(poster_chat_id, story_id)`. The viewer
    /// prefetches every story in a tray entry before opening.
    pub stories: HashMap<(i64, i32), ParsedStory>,
    /// Phase 9.2: emoji reactions the story picker can offer — the
    /// `getStoryAvailableReactions` response (`availableReactions`,
    /// `schema/td_api.tl:13802`).
    pub story_available_reactions: Option<Vec<StoryAvailableReactionView>>,
    /// Phase 9.2: poster chat ids whose active stories the driver should
    /// refresh with `getChatActiveStories`. Filled by the reducer on
    /// `updateStoryPostSucceeded` (a story posted from another client goes
    /// live — e.g. our own) and drained by the UI each render, like
    /// `pending_story_open`.
    pub story_tray_refresh: HashSet<i64>,
    /// Phase 9.1: `loadActiveStories(storyListMain)` was issued. A retry is
    /// allowed (the flag is reset) if the attempt failed.
    pub stories_active_loaded: bool,
    diagnostics: Arc<dyn DiagnosticSink>,
}

/// Phase 6: which info panel is open in the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoPanelTarget {
    User(i64),
    Supergroup(i64),
}

/// Phase 6: cached `userFullInfo` subset (schema 1.8.67, line 2468) — the
/// bio and the preferred profile-photo file from `photo:chatPhoto`
/// (parsed in `EnvelopePayload::UserFullInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserFullInfoData {
    pub bio: String,
    /// File id of the preferred `chatPhoto` size (`None` = no photo).
    /// The `ParsedFile` is cached in `Session::files` by the apply arm.
    pub photo_file_id: Option<i32>,
}

/// Phase 6: cached `supergroupFullInfo` subset (schema 1.8.67, line 2792).
#[derive(Debug, Clone, PartialEq)]
pub struct SupergroupFullInfoData {
    pub description: String,
    pub member_count: i32,
    /// Parity slice: `linked_chat_id` (schema 1.8.67, line 2792) — the
    /// discussion-group chat id for a channel; 0 when none.
    pub linked_chat_id: i64,
    /// Phase A1: `slow_mode_delay` (schema 1.8.67, line 2758) — seconds
    /// between messages for non-administrator members; 0 = disabled.
    pub slow_mode_delay: i32,
    /// Phase A1: `slow_mode_delay_expires_in` (schema 1.8.67, line 2759)
    /// — seconds left at fetch time. Decays against `fetched_at_ms`: the
    /// schema warns no `updateSupergroupFullInfo` fires when only this
    /// changes while both old and new values are non-zero.
    pub slow_mode_delay_expires_in: f64,
    /// Phase A1: `my_boost_count` (schema 1.8.67, line 2779).
    pub my_boost_count: i32,
    /// Phase A1: `unrestrict_boost_count` (schema 1.8.67, line 2780) — the
    /// boosts needed to ignore slow mode; 0 if unspecified.
    pub unrestrict_boost_count: i32,
    /// Phase A1: wall-clock ms when this full info arrived (reducer
    /// stamp). `slow_mode_delay_expires_in` decays against it.
    pub fetched_at_ms: u64,
}

impl Default for SupergroupFullInfoData {
    fn default() -> Self {
        Self {
            description: String::new(),
            member_count: 0,
            linked_chat_id: 0,
            slow_mode_delay: 0,
            slow_mode_delay_expires_in: 0.0,
            my_boost_count: 0,
            unrestrict_boost_count: 0,
            fetched_at_ms: 0,
        }
    }
}

/// Phase A1: wall-clock milliseconds. Used to timestamp
/// `supergroupFullInfo` arrivals so the slow-mode expiry decays locally.
pub fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Phase 6: one rendered contacts-list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactRow {
    pub user_id: i64,
    pub name: String,
    pub status_text: String,
    pub is_online: bool,
    pub is_contact: bool,
}

impl Session {
    pub fn new(account: AccountKey, diagnostics: Arc<dyn DiagnosticSink>) -> Self {
        let auth = AuthorizationState::WaitTdlibParameters;
        Self {
            account,
            account_generation: AccountGeneration(1),
            auth_view: view_for(&auth),
            auth,
            connection: ConnectionState::WaitingForNetwork,
            chats: HashMap::new(),
            main_order: Vec::new(),
            archive_order: Vec::new(),
            chat_folders: Vec::new(),
            are_folder_tags_enabled: false,
            folder_specs: HashMap::new(),
            chat_lists_for_add: HashMap::new(),
            folder_chats_exhausted: HashSet::new(),
            folder_remove_queue: Vec::new(),
            folder_chats_to_leave: HashMap::new(),
            histories: HashMap::new(),
            open_chat: None,
            app_active: true,
            hide_notification_previews: true,
            pending_notifications: Vec::new(),
            saved_notification_sounds: Vec::new(),
            saved_sounds_loaded: false,
            saved_sounds_stale: false,
            scope_notification_settings: HashMap::new(),
            scope_settings_loading: HashSet::new(),
            sound_file_ids: HashMap::new(),
            pending_sound_downloads: HashSet::new(),
            pending_sound_plays: Vec::new(),
            open_topic: None,
            forum_topics: HashMap::new(),
            topic_histories: HashMap::new(),
            view_generation: ViewGeneration(1),
            requests: RequestRegistry::default(),
            chats_exhausted: false,
            shutdown: ShutdownPhase::Running,
            last_seq: 0,
            last_auth_error: None,
            in_flight_forward: None,
            last_forward: None,
            last_callback_answer: None,
            files: HashMap::new(),
            downloading: HashSet::new(),
            download_extras: HashMap::new(),
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            stickers: StickerPanel::default(),
            gifs: GifPanel::default(),
            bot_user_ids: HashSet::new(),
            bot_info: HashMap::new(),
            bot_commands: HashMap::new(),
            draft_dirty: HashSet::new(),
            draft_clears: Vec::new(),
            sponsored: HashMap::new(),
            sponsored_report: None,
            sponsored_report_target: None,
            last_sponsored_report: None,
            my_user_id: None,
            users: HashMap::new(),
            contacts: None,
            contacts_error: false,
            user_full_infos: HashMap::new(),
            supergroup_full_infos: HashMap::new(),
            supergroup_usernames: HashMap::new(),
            supergroup_member_status: HashMap::new(),
            supergroup_restrict_right: HashMap::new(),
            open_info_panel: None,
            story_tray: HashMap::new(),
            stories: HashMap::new(),
            stories_active_loaded: false,
            story_available_reactions: None,
            story_tray_refresh: HashSet::new(),
            diagnostics,
        }
    }

    /// Private chats only, and not a known bot. Channels, groups, secret chats skip drafts.
    pub fn accepts_composer_draft(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Private { user_id } => !self.bot_user_ids.contains(&user_id.0),
            _ => false,
        }
    }

    /// Phase 3.1: bot chats ride the ordinary private-chat path
    /// (`is_supported_cloud_chat` / `can_post`) — no special gate. This
    /// resolves the peer bot user id for a private chat whose user is a
    /// known `userTypeBot`, feeding the lazy `getUserFullInfo` fetch.
    /// `None` for every other chat kind.
    pub fn bot_user_id_for_chat(&self, chat_id: ChatId) -> Option<i64> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } if self.bot_user_ids.contains(&user_id.0) => {
                Some(user_id.0)
            }
            _ => None,
        }
    }

    /// Cached `botInfo` for the open chat's bot, if the lazy fetch (or an
    /// `updateUserFullInfo`) already populated it.
    pub fn bot_info_for_chat(&self, chat_id: ChatId) -> Option<&BotInfo> {
        self.bot_user_id_for_chat(chat_id)
            .and_then(|user_id| self.bot_info.get(&user_id))
            .and_then(|info| info.as_ref())
    }

    /// Phase 6: the user id behind any private chat (bot or not).
    pub fn private_chat_user_id(&self, chat_id: ChatId) -> Option<i64> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } => Some(user_id.0),
            _ => None,
        }
    }

    /// Phase 6: info-panel target for a chat header — the peer user for a
    /// private chat, the supergroup for a group/channel chat, `None` for
    /// basic groups, secret chats and unknown kinds.
    pub fn info_panel_target_for_chat(&self, chat_id: ChatId) -> Option<InfoPanelTarget> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } => Some(InfoPanelTarget::User(user_id.0)),
            ChatKind::Supergroup { supergroup_id, .. } => {
                Some(InfoPanelTarget::Supergroup(supergroup_id))
            }
            _ => None,
        }
    }

    /// Phase 6: cached user object, if an `updateUser` has been seen.
    pub fn user(&self, user_id: i64) -> Option<&ParsedUser> {
        self.users.get(&user_id)
    }

    /// Phase 6: cached `userFullInfo` bio, if fetched.
    pub fn user_full_info(&self, user_id: i64) -> Option<&UserFullInfoData> {
        self.user_full_infos.get(&user_id)
    }

    /// Phase 6: cached `supergroupFullInfo`, if fetched.
    pub fn supergroup_full_info(&self, supergroup_id: i64) -> Option<&SupergroupFullInfoData> {
        self.supergroup_full_infos.get(&supergroup_id)
    }

    /// Phase A1: the viewer's own `chatMemberStatus*` in a supergroup
    /// (`supergroup.status`, schema 1.8.67 line 2746), if seen yet.
    pub fn supergroup_own_status(&self, supergroup_id: i64) -> Option<ChannelMemberStatus> {
        self.supergroup_member_status.get(&supergroup_id).copied()
    }

    /// Phase A1: whether the viewer's own administrator rights in a
    /// supergroup include `can_restrict_members` (schema 1.8.67, lines
    /// 2500/1092), which `setChatSlowModeDelay` requires (line 13551).
    /// Creators hold all rights implicitly — check
    /// `supergroup_own_status` for that. Absent = unknown, treated as
    /// lacking the right (the admin control stays hidden).
    pub fn supergroup_can_restrict_members(&self, supergroup_id: i64) -> bool {
        self.supergroup_restrict_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Phase A1: slow-mode gate for a chat. Returns the remaining wait in
    /// whole seconds when all of these hold:
    /// - `chat_id` is a non-channel supergroup with cached full info whose
    ///   `slow_mode_delay` (schema 1.8.67, line 2758) is positive;
    /// - the viewer lacks bypass rights: not an administrator/creator
    ///   (`supergroup.status`, schema line 2746) and not boost-exempt
    ///   (`my_boost_count >= unrestrict_boost_count > 0`, schema lines
    ///   2779–2780);
    /// - the server-reported `slow_mode_delay_expires_in` (schema line
    ///   2759), decayed against `fetched_at_ms`, is still positive. The
    ///   schema warns no `updateSupergroupFullInfo` fires when only the
    ///   expiry changes (both old and new non-zero), so local decay is the
    ///   countdown; callers re-fetch on blocked sends for a fresh value.
    ///
    /// `None` = no gate (send freely). Pure in `now_ms` for replay tests.
    pub fn slow_mode_wait_secs(&self, chat_id: ChatId, now_ms: u64) -> Option<u64> {
        let chat = self.chats.get(&chat_id.0)?;
        let supergroup_id = match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => supergroup_id,
            _ => return None,
        };
        let info = self.supergroup_full_infos.get(&supergroup_id)?;
        if info.slow_mode_delay <= 0 {
            return None;
        }
        // Bypass: administrators and the creator are exempt (tdesktop
        // slow-mode applies to non-administrator members, schema line
        // 2758 comment). Unknown status = no bypass.
        if self
            .supergroup_own_status(supergroup_id)
            .is_some_and(ChannelMemberStatus::is_admin)
        {
            return None;
        }
        // Bypass: enough boosts ignore slow mode (schema 1.8.67, line 2780
        // comment); `unrestrict_boost_count` 0 = unspecified.
        if info.unrestrict_boost_count > 0 && info.my_boost_count >= info.unrestrict_boost_count {
            return None;
        }
        let elapsed_s = now_ms.saturating_sub(info.fetched_at_ms) as f64 / 1000.0;
        let remaining = info.slow_mode_delay_expires_in - elapsed_s;
        if remaining > 0.0 {
            Some(remaining.ceil() as u64)
        } else {
            None
        }
    }

    /// Phase 6: contacts-list rows in server order with a name/status view
    /// model, sorted case-insensitively by display name. Users not yet
    /// seen via `updateUser` are skipped (their rows fill in when the
    /// updates arrive).
    pub fn contact_rows(&self) -> Vec<ContactRow> {
        let mut rows: Vec<ContactRow> = self
            .contacts
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|id| self.users.get(id))
            .map(|user| ContactRow {
                user_id: user.id,
                name: user.display_name(),
                status_text: user.status.display(),
                is_online: user.status.is_online(),
                is_contact: user.is_contact,
            })
            .collect();
        rows.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.user_id.cmp(&b.user_id))
        });
        rows
    }

    /// Phase 6: `true` once a `users` answer (or a failed attempt) settled —
    /// the contacts tab shows rows, an error, or a loading state.
    pub fn contacts_settled(&self) -> bool {
        self.contacts.is_some() || self.contacts_error
    }

    /// Phase 3.3: merged `/`-menu rows for the open chat's bot — the
    /// bot's `botInfo` commands first, then cached `getCommands`
    /// (global scope) results below, deduped by command name. Empty for
    /// non-bot chats, unknown chats, or when no commands are known yet.
    pub fn command_menu_items(&self, chat_id: ChatId) -> Vec<CommandMenuItem> {
        let Some(user_id) = self.bot_user_id_for_chat(chat_id) else {
            return Vec::new();
        };
        let specific: &[BotCommand] = self
            .bot_info_for_chat(chat_id)
            .map(|info| info.commands.as_slice())
            .unwrap_or(&[]);
        let global: &[BotCommand] = self
            .bot_commands
            .get(&user_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        merge_command_menu_items(specific, global)
    }

    pub fn mark_draft_dirty(&mut self, chat_id: ChatId) {
        self.draft_dirty.insert(chat_id.0);
    }

    pub fn draft_is_dirty(&self, chat_id: ChatId) -> bool {
        self.draft_dirty.contains(&chat_id.0)
    }

    /// Local persist (after `setChatDraftMessage`, or the demo path). Clears the dirty bit.
    pub fn store_composer_draft(&mut self, chat_id: ChatId, draft: Option<ChatDraft>) {
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.draft = draft;
        }
        self.draft_dirty.remove(&chat_id.0);
    }

    pub fn apply(&mut self, owned: OwnedEnvelope) {
        if owned.seq <= self.last_seq && self.last_seq != 0 {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: owned.envelope.extra.map(|id| id.0),
                seq: Some(owned.seq),
                note: "out-of-order-ignored",
            });
            return;
        }
        self.last_seq = owned.seq;
        let extra = owned.envelope.extra;
        let pending = extra.and_then(|id| self.requests.take(id));
        if let Some(pending) = pending.as_ref()
            && pending.account_generation != self.account_generation
        {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: Some(pending.id.0),
                seq: Some(owned.seq),
                note: "stale-account-generation",
            });
            return;
        }
        self.apply_payload(owned.envelope.payload, pending.as_ref(), extra, owned.seq);
    }

    fn apply_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            EnvelopePayload::UpdateAuthorizationState(state) => self.set_auth(state),
            EnvelopePayload::UpdateConnectionState(state) => self.connection = state,
            EnvelopePayload::UpdateNewChat {
                chat_id,
                title,
                kind,
                unread_count,
                last_read_inbox_message_id,
                last_read_outbox_message_id,
                notification_settings,
                draft,
                photo,
                can_send_basic_messages,
            } => {
                // Parity slice: keep the chat photo (`chatPhotoInfo.small`)
                // file id so the chat list can render avatars. The file
                // object is remembered first (separate borrow) so the
                // driver can download it.
                let photo_file_id = photo.as_ref().map(|file| file.id.0);
                if let Some(file) = &photo {
                    self.remember_files(std::slice::from_ref(file));
                }
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.title = title;
                chat.kind = kind;
                chat.unread_count = unread_count;
                chat.last_read_inbox_message_id = last_read_inbox_message_id;
                chat.last_read_outbox_message_id = last_read_outbox_message_id;
                chat.notification_settings = notification_settings;
                chat.photo_file_id = photo_file_id;
                chat.can_send_basic_messages = can_send_basic_messages;
                if !self.draft_dirty.contains(&chat_id.0) {
                    chat.draft = draft;
                }
            }
            // Parity slice: `updateChatPhoto` — swap the cached small
            // photo file id (the chat list re-renders avatars from it).
            EnvelopePayload::UpdateChatPhoto { chat_id, photo } => {
                let photo_file_id = photo.as_ref().map(|file| file.id.0);
                if let Some(file) = &photo {
                    self.remember_files(std::slice::from_ref(file));
                }
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .photo_file_id = photo_file_id;
            }
            EnvelopePayload::UpdateChatPermissions {
                chat_id,
                can_send_basic_messages,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .can_send_basic_messages = can_send_basic_messages;
            }
            EnvelopePayload::UpdateChatDraftMessage {
                chat_id,
                draft,
                positions,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                if !self.draft_dirty.contains(&chat_id.0) {
                    chat.draft = draft;
                }
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateUser { user_id, user } => {
                // Phase 6: keep the full user object for the contacts list
                // and info panels.
                self.users.insert(user_id.0, user.clone());
                if user.is_bot {
                    self.bot_user_ids.insert(user_id.0);
                } else {
                    // No longer a bot: drop any cached bot info so the panel
                    // cannot show stale description/commands (Phase 3.1).
                    self.bot_user_ids.remove(&user_id.0);
                    self.bot_info.remove(&user_id.0);
                }
            }
            EnvelopePayload::UpdateUserStatus { user_id, status } => {
                // Phase 6: live online / last-seen for the contacts list.
                if let Some(user) = self.users.get_mut(&user_id.0) {
                    user.status = status;
                }
            }
            EnvelopePayload::Users { user_ids } => {
                // Phase 6: `getContacts` answer — only answers to our own
                // fetch are accepted (matched by `@extra`); the user
                // objects themselves arrive via `updateUser`.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetContacts) {
                    self.contacts = Some(user_ids);
                    self.contacts_error = false;
                }
            }
            EnvelopePayload::SupergroupFullInfo {
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
            } => {
                // Phase 6: `getSupergroupFullInfo` answer — the response
                // carries no id, so it is correlated via the pending
                // request's `supergroup_id`.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupergroupFullInfo)
                    && let Some(pending) = pending
                    && let Some(supergroup_id) = pending.supergroup_id
                {
                    self.supergroup_full_infos.insert(
                        supergroup_id,
                        SupergroupFullInfoData {
                            description,
                            member_count,
                            linked_chat_id,
                            slow_mode_delay,
                            slow_mode_delay_expires_in,
                            my_boost_count,
                            unrestrict_boost_count,
                            // Phase A1: timestamp the arrival — the schema
                            // (1.8.67, line 2759) warns no update fires
                            // when only the expiry changes, so the gate
                            // decays it locally against this stamp.
                            fetched_at_ms: unix_ms_now(),
                        },
                    );
                }
            }
            // Parity slice: `updateSupergroupFullInfo` — the update carries
            // its own id, so it applies whenever it arrives (no pending
            // correlation).
            EnvelopePayload::UpdateSupergroupFullInfo {
                supergroup_id,
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
            } => {
                self.supergroup_full_infos.insert(
                    supergroup_id,
                    SupergroupFullInfoData {
                        description,
                        member_count,
                        linked_chat_id,
                        slow_mode_delay,
                        slow_mode_delay_expires_in,
                        my_boost_count,
                        unrestrict_boost_count,
                        fetched_at_ms: unix_ms_now(),
                    },
                );
            }
            EnvelopePayload::UpdateChatNotificationSettings {
                chat_id,
                notification_settings,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .notification_settings = notification_settings;
            }
            EnvelopePayload::UpdateChatAction {
                chat_id,
                sender,
                action,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .set_sender_action(sender, action);
            }
            EnvelopePayload::UpdateChatTitle { chat_id, title } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .title = title;
            }
            EnvelopePayload::UpdateChatReadInbox {
                chat_id,
                last_read_inbox_message_id,
                unread_count,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.last_read_inbox_message_id = last_read_inbox_message_id;
                chat.unread_count = unread_count;
            }
            EnvelopePayload::UpdateChatReadOutbox {
                chat_id,
                last_read_outbox_message_id,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .last_read_outbox_message_id = last_read_outbox_message_id;
            }
            EnvelopePayload::UpdateChatAddedToList { chat_id, list } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                match list {
                    ChatList::Main => chat.in_main_list = true,
                    ChatList::Archive => chat.in_archive = true,
                    ChatList::Folder(folder_id) => {
                        // Membership is confirmed; the position (with order)
                        // arrives separately via `updateChatPosition`.
                        chat.folder_positions.entry(folder_id).or_insert(0);
                    }
                    _ => {}
                }
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatRemovedFromList { chat_id, list } => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    match list {
                        ChatList::Main => chat.in_main_list = false,
                        ChatList::Archive => chat.in_archive = false,
                        ChatList::Folder(folder_id) => {
                            chat.folder_positions.remove(&folder_id);
                        }
                        _ => {}
                    }
                    self.rebuild_main_order();
                }
            }
            EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message,
                positions,
            } => {
                if let Some(ref message) = last_message {
                    self.remember_files(&message.files);
                }
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.last_preview = last_message
                    .as_ref()
                    .map(|message| preview_from_content(&message.content))
                    .unwrap_or_default();
                // `positions` is the full set of lists this chat belongs to.
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatPosition(pos) => {
                self.apply_position_fields(pos);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatFolders {
                folders,
                are_tags_enabled,
            } => {
                // The update carries the full ordered list — replace.
                self.chat_folders = folders;
                self.are_folder_tags_enabled = are_tags_enabled;
            }
            EnvelopePayload::ChatFolderInfo(info) => {
                // Parity slice: `createChatFolder` / `editChatFolder`
                // response — upsert into the tab list so the UI reflects the
                // change without waiting for `updateChatFolders` (which
                // stays the source of truth).
                match self.chat_folders.iter_mut().find(|f| f.id == info.id) {
                    Some(existing) => *existing = info,
                    None => self.chat_folders.push(info),
                }
            }
            EnvelopePayload::ChatFolder { spec } => {
                // Parity slice: `getChatFolder` response — cache the full
                // spec for the edit dialog prefill / remove-from-folder
                // chain (correlated via `PendingRequest::folder_id`).
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    self.folder_specs.insert(folder_id, spec);
                }
            }
            EnvelopePayload::ChatLists { lists } => {
                // Parity slice: `getChatListsToAddChat` response — cache per
                // chat for the folder picker (correlated via
                // `PendingRequest::chat_id`).
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_lists_for_add.insert(chat_id.0, lists);
                }
            }
            EnvelopePayload::UpdateChatActiveStories { active_stories } => {
                // Phase 9.1: keep the tray entry only for the main story
                // list; archived / hidden chats drop out of the tray.
                self.upsert_story_tray_entry(active_stories);
            }
            EnvelopePayload::ChatActiveStories { active_stories } => {
                // Phase 9.1: `getChatActiveStories` answer — refresh the
                // tray entry (matched by `@extra` in the UI's fetch guard,
                // but the object itself is authoritative).
                self.upsert_story_tray_entry(active_stories);
            }
            EnvelopePayload::Story { story, files } => {
                // Phase 9.1: `getStory` response or `updateStory` update.
                self.remember_files(&files);
                self.stories.insert((story.poster_chat_id, story.id), story);
            }
            EnvelopePayload::UpdateStoryDeleted {
                poster_chat_id,
                story_id,
            } => {
                // Phase 9.2: drop the story from the cache and from the
                // poster's tray entry. The UI closes the viewer when its
                // current story disappears from the cache.
                self.stories.remove(&(poster_chat_id, story_id));
                let empty = if let Some(tray) = self.story_tray.get_mut(&poster_chat_id) {
                    tray.stories.retain(|info| info.story_id != story_id);
                    tray.stories.is_empty()
                } else {
                    false
                };
                if empty {
                    self.story_tray.remove(&poster_chat_id);
                }
            }
            EnvelopePayload::UpdateStoryPostSucceeded {
                story,
                files,
                old_story_id: _,
            } => {
                // Phase 9.2: a story posted from another client is live —
                // upsert it and refresh the poster's tray row so an own
                // story appears in the tray.
                self.remember_files(&files);
                let poster_chat_id = story.poster_chat_id;
                self.stories.insert((story.poster_chat_id, story.id), story);
                self.story_tray_refresh.insert(poster_chat_id);
            }
            EnvelopePayload::UpdateStoryPostFailed { story, error: _ } => {
                // Phase 9.2: a story failed to post — drop it like a delete
                // (it never went live). Unreachable without `sendStory`,
                // which is absent from TDLib 1.8.67.
                self.stories.remove(&(story.poster_chat_id, story.id));
                let empty = if let Some(tray) = self.story_tray.get_mut(&story.poster_chat_id) {
                    tray.stories.retain(|info| info.story_id != story.id);
                    tray.stories.is_empty()
                } else {
                    false
                };
                if empty {
                    self.story_tray.remove(&story.poster_chat_id);
                }
            }
            EnvelopePayload::StoryAvailableReactions { reactions } => {
                // Phase 9.2: `getStoryAvailableReactions` answer — the
                // viewer picker options.
                self.story_available_reactions = Some(reactions);
            }
            EnvelopePayload::UpdateNewMessage(message) => {
                // Phase 8.1: decide before upserting; the queue is drained by
                // the UI for OS dispatch. The sound decision is made at the
                // same moment (parity slice: notification sounds).
                let notification = self.notification_for_new_message(&message);
                let sound = notification.as_ref().and_then(|_| {
                    self.chats
                        .get(&message.chat_id.0)
                        .and_then(|chat| self.notification_sound_for(chat))
                });
                self.upsert_message(message, false);
                if let Some(notification) = notification {
                    self.queue_notification_with_sound(notification, sound);
                }
            }
            EnvelopePayload::UpdateMessageSendSucceeded {
                message,
                old_message_id,
            } => {
                let chat_id = message.chat_id;
                let topic_id = message.topic_id;
                self.remember_files(&message.files);
                let row = history_message(message, false);
                let history = self.histories.entry(chat_id.0).or_default();
                history.replace_id(old_message_id, row.clone());
                // Parity slice 4: the pending row in the topic's history
                // resolves the same way (the succeeded message carries its
                // topic).
                if let Some(topic_id) = topic_id
                    && let Some(topic_history) =
                        self.topic_histories.get_mut(&(chat_id.0, topic_id))
                {
                    topic_history.replace_id(old_message_id, row);
                }
                self.draft_clears.push(chat_id);
            }
            EnvelopePayload::UpdateMessageSendFailed {
                message,
                old_message_id,
                ..
            } => {
                let chat_id = message.chat_id;
                let topic_id = message.topic_id;
                self.remember_files(&message.files);
                let row = history_message(message, true);
                let history = self.histories.entry(chat_id.0).or_default();
                history.replace_id(old_message_id, row.clone());
                // Parity slice 4: the failed pending row shows in the topic
                // view too.
                if let Some(topic_id) = topic_id
                    && let Some(topic_history) =
                        self.topic_histories.get_mut(&(chat_id.0, topic_id))
                {
                    topic_history.replace_id(old_message_id, row);
                }
            }
            EnvelopePayload::UpdateMessageSendAcknowledged { .. } => {
                // Not success. Keep the pending row until Succeeded/Failed.
            }
            EnvelopePayload::UpdateMessageInteractionInfo {
                chat_id,
                message_id,
                interaction_info,
            } => {
                if let Some(history) = self.histories.get_mut(&chat_id.0) {
                    history.update_interaction_info(message_id, interaction_info);
                }
            }
            EnvelopePayload::UpdateMessageIsPinned {
                chat_id,
                message_id,
                is_pinned,
            } => {
                if let Some(history) = self.histories.get_mut(&chat_id.0) {
                    history.update_is_pinned(message_id, is_pinned);
                }
            }
            EnvelopePayload::UpdateMessageContentOpened {
                chat_id,
                message_id,
            } => {
                if let Some(history) = self.histories.get_mut(&chat_id.0) {
                    history.mark_content_opened(message_id);
                }
            }
            EnvelopePayload::UpdateMessageEdited {
                chat_id,
                message_id,
                reply_markup,
                ..
            } => {
                // Phase 3.2: bots edit inline keyboards via `updateMessageEdited`
                // (schema 1.8.67 line 10431) — the new `reply_markup` (possibly
                // None) replaces the message's keyboard.
                if let Some(history) = self.histories.get_mut(&chat_id.0) {
                    history.update_reply_markup(message_id, reply_markup);
                }
            }
            EnvelopePayload::UpdatePoll { poll } => {
                // Phase 4.2: `updatePoll` (schema 1.8.67 line 11179) carries
                // only the new `poll` — no chat or message id — so every
                // loaded history is scanned for a `messagePoll` with a
                // matching poll id and the poll is replaced in place.
                self.apply_update_poll(poll);
            }
            EnvelopePayload::UpdateMessageContent {
                chat_id,
                message_id,
                content,
                files,
            } => {
                self.remember_files(&files);
                let preview = content.preview();
                let updated = self
                    .histories
                    .get_mut(&chat_id.0)
                    .is_some_and(|history| history.update_content(message_id, content));
                if updated {
                    let is_last = self
                        .histories
                        .get(&chat_id.0)
                        .and_then(|history| history.messages.keys().next_back().copied())
                        == Some(message_id.0);
                    if is_last && let Some(chat) = self.chats.get_mut(&chat_id.0) {
                        chat.last_preview = preview;
                    }
                }
            }
            EnvelopePayload::UpdateDeleteMessages {
                chat_id,
                message_ids,
                is_permanent,
                from_cache,
            } => {
                let history = self.histories.entry(chat_id.0).or_default();
                for id in message_ids {
                    if is_permanent {
                        history.remove(id, true);
                    } else if from_cache {
                        history.remove(id, false);
                    } else {
                        history.remove(id, true);
                    }
                    if is_permanent
                        && matches!(
                            self.chat_search.jump,
                            ChatSearchJump::Ready { message_id }
                                | ChatSearchJump::Loading { message_id }
                                if message_id == id
                        )
                    {
                        self.chat_search.jump = ChatSearchJump::Missing { message_id: id };
                    }
                }
            }
            EnvelopePayload::Chats { chat_ids, .. } => {
                // Parity slice: `getChatFolderChatsToLeave` response for the
                // delete-confirm dialog (correlated via folder_id). Runs
                // before the search branch below consumes `chat_ids`.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatFolderChatsToLeave)
                    && let Some(folder_id) = pending.and_then(|p| p.folder_id)
                {
                    self.folder_chats_to_leave
                        .insert(folder_id, chat_ids.iter().map(|id| id.0).collect());
                }
                if self.search.matches_generation(pending) {
                    match pending.map(|p| p.purpose) {
                        Some(
                            RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats,
                        ) => {
                            self.search.accept_chats(chat_ids, false);
                        }
                        Some(RequestPurpose::SearchPublicChats) => {
                            self.search.accept_public_chats(chat_ids, false);
                        }
                        _ => {}
                    }
                }
            }
            EnvelopePayload::FoundMessages { messages, .. } => {
                if self.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchMessages)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.accept_messages(hits, false);
                }
            }
            EnvelopePayload::FoundChatMessages {
                messages,
                total_count,
                next_from_message_id,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTopicHistory) {
                    // Phase 5.1: per-topic history page. Correlated by chat +
                    // topic; stored separately from the chat's general history.
                    if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                        && let Some(forum_topic_id) = pending.and_then(|p| p.forum_topic_id)
                    {
                        for message in &messages {
                            self.remember_files(&message.files);
                        }
                        let entry = self
                            .topic_histories
                            .entry((chat_id.0, forum_topic_id))
                            .or_default();
                        let empty = messages.is_empty();
                        for message in messages {
                            entry
                                .messages
                                .insert(message.id.0, history_message(message, false));
                        }
                        if next_from_message_id.0 == 0 || empty {
                            entry.loaded_complete = true;
                        }
                        entry.next_from_message_id = next_from_message_id;
                    }
                    return;
                }
                if self.chat_search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.chat_search
                        .accept_hits(hits, total_count, next_from_message_id, false);
                }
            }
            // Phase 5.1: `supergroup.is_forum` via `updateSupergroup` (an
            // update — applies whenever it arrives) or the `getSupergroup`
            // response (gated on the pending purpose). Parity slice: the
            // first active username is cached alongside, for the
            // channel/supergroup header.
            EnvelopePayload::UpdateSupergroup {
                supergroup_id,
                is_forum,
                username,
                status,
                can_restrict_members,
            } => {
                self.set_supergroup_forum(supergroup_id, is_forum);
                self.set_supergroup_username(supergroup_id, username);
                // Phase A1: own member status drives the slow-mode bypass.
                self.supergroup_member_status.insert(supergroup_id, status);
                // Phase A1: `can_restrict_members` gates the slow-mode
                // admin control; absent = unknown → treated as lacking.
                self.supergroup_restrict_right
                    .insert(supergroup_id, can_restrict_members.unwrap_or(false));
            }
            EnvelopePayload::Supergroup {
                supergroup_id,
                is_forum,
                username,
                status,
                can_restrict_members,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupergroup) {
                    self.set_supergroup_forum(supergroup_id, is_forum);
                    self.set_supergroup_username(supergroup_id, username);
                    // Phase A1: own member status drives the slow-mode bypass.
                    self.supergroup_member_status.insert(supergroup_id, status);
                    self.supergroup_restrict_right
                        .insert(supergroup_id, can_restrict_members.unwrap_or(false));
                }
            }
            // Phase 5.1: `getForumTopics` response — cache the first page
            // against the requesting chat.
            EnvelopePayload::ForumTopics {
                total_count: _,
                topics,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetForumTopics)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.forum_topics.insert(chat_id.0, topics);
                }
            }
            EnvelopePayload::Messages(messages) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::SendMessageAlbum
                {
                    for message in messages {
                        self.upsert_message(message, true);
                    }
                    return;
                }
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ForwardMessages
                {
                    self.finish_forward(pending, &messages, false);
                    return;
                }
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetHistoryAround
                {
                    self.apply_history_around(pending, &messages, seq);
                    return;
                }
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetHistory
                {
                    if pending.view_generation != Some(self.view_generation) {
                        self.diagnostics.record(Diagnostic {
                            category: "reducer",
                            type_name: Some("messages".into()),
                            extra: Some(pending.id.0),
                            seq: Some(seq),
                            note: "stale-view-generation",
                        });
                        return;
                    }
                    if let Some(chat_id) = pending.chat_id {
                        if self.open_chat != Some(chat_id) {
                            self.diagnostics.record(Diagnostic {
                                category: "reducer",
                                type_name: Some("messages".into()),
                                extra: Some(pending.id.0),
                                seq: Some(seq),
                                note: "stale-chat-history",
                            });
                            return;
                        }
                        if messages.is_empty() {
                            self.histories.entry(chat_id.0).or_default().loaded_complete = true;
                        }
                        for message in messages {
                            self.upsert_message(message, false);
                        }
                    }
                }
            }
            EnvelopePayload::Message(message) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMessage) {
                    self.upsert_message(message, true);
                } else {
                    self.upsert_message(message, false);
                }
            }
            EnvelopePayload::UpdateFile(file) | EnvelopePayload::File(file) => {
                self.upsert_file(file, true);
            }
            EnvelopePayload::StickerSets { sets, .. } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
                    self.accept_installed_sticker_sets(sets);
                }
            }
            EnvelopePayload::StickerSet {
                id,
                stickers,
                files,
                ..
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
                    self.remember_files(&files);
                    self.accept_sticker_set(id, stickers);
                }
            }
            EnvelopePayload::Animations { animations, files } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
                    self.remember_files(&files);
                    self.accept_saved_animations(animations);
                }
            }
            EnvelopePayload::SponsoredMessages {
                messages,
                files,
                messages_between,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatSponsoredMessages)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    if self.open_chat != Some(chat_id) {
                        self.diagnostics.record(Diagnostic {
                            category: "reducer",
                            type_name: Some("sponsoredMessages".into()),
                            extra: Some(pending.id.0),
                            seq: Some(seq),
                            note: "stale-chat-sponsored",
                        });
                        return;
                    }
                    self.accept_sponsored_messages(chat_id, messages, messages_between, &files);
                }
            }
            EnvelopePayload::ReportSponsoredResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage)
                    && let Some(pending) = pending
                {
                    self.accept_sponsored_report(pending, result);
                }
            }
            EnvelopePayload::Me { user_id } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetMe) {
                    self.my_user_id = Some(user_id);
                }
            }
            EnvelopePayload::ChatMember { member } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatMember)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_own_chat_member(chat_id, member);
                }
            }
            EnvelopePayload::UserFullInfo {
                bot_info,
                bio,
                photo,
            } => {
                // `getUserFullInfo` response: resolve the user id from the
                // pending request's explicit `user_id` (contacts-panel
                // fetch) or its private chat (chat-header fetch). Responses
                // for chats that stopped being private chats are dropped.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetUserFullInfo)
                    && let Some(pending) = pending
                {
                    let user_id = pending.user_id.or_else(|| {
                        pending
                            .chat_id
                            .and_then(|chat_id| self.private_chat_user_id(chat_id))
                    });
                    if let Some(user_id) = user_id {
                        let photo_file_id = photo.map(|file| {
                            let id = file.id.0;
                            self.upsert_file(file, false);
                            id
                        });
                        self.user_full_infos
                            .insert(user_id, UserFullInfoData { bio, photo_file_id });
                        if let Some(bot_id) = pending
                            .chat_id
                            .and_then(|chat_id| self.bot_user_id_for_chat(chat_id))
                        {
                            self.bot_info.insert(bot_id, bot_info);
                        } else if pending.user_id.is_some() && self.bot_user_ids.contains(&user_id)
                        {
                            self.bot_info.insert(user_id, bot_info);
                        }
                    }
                }
            }
            EnvelopePayload::UpdateUserFullInfo {
                user_id,
                bot_info,
                bio,
                photo,
            } => {
                self.bot_info.insert(user_id.0, bot_info);
                let photo_file_id = photo.map(|file| {
                    let id = file.id.0;
                    self.upsert_file(file, false);
                    id
                });
                self.user_full_infos
                    .insert(user_id.0, UserFullInfoData { bio, photo_file_id });
            }
            EnvelopePayload::BotCommands {
                bot_user_id,
                commands,
            } => {
                // Phase 3.3: `getCommands` response — cache the global-scope
                // commands for the bot. Only answers to our own fetch are
                // cached (matched by `@extra`); a user session gets an
                // `error` instead of `botCommands` (schema: "for bots
                // only"), recorded as an empty set by the `Error` arm.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCommands) {
                    self.bot_commands.insert(bot_user_id.0, commands);
                }
            }
            EnvelopePayload::UpdateChatMember { chat_id, member } => {
                self.accept_own_chat_member(chat_id, member);
            }
            EnvelopePayload::JoinChatResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::JoinChat)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_join_chat_result(chat_id, result);
                }
            }
            EnvelopePayload::CallbackQueryAnswer(answer) => {
                // `getCallbackQueryAnswer` response: only answers to our own
                // inline-button presses are surfaced (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCallbackQueryAnswer) {
                    self.last_callback_answer = Some(answer);
                }
            }
            EnvelopePayload::UpdateSavedAnimations { .. } => {
                if self.gifs.open {
                    self.gifs.stale = true;
                }
            }
            EnvelopePayload::NotificationSounds { sounds } => {
                // Parity slice: `getSavedNotificationSounds` answer.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedNotificationSounds) {
                    let files: Vec<ParsedFile> = sounds.iter().map(|s| s.sound.clone()).collect();
                    self.remember_files(&files);
                    // Parity slice: a refetch replaces the saved list, so
                    // evict `sound_file_ids` entries for sounds that are no
                    // longer saved — stale file→sound mappings would
                    // otherwise accumulate forever.
                    let live_ids: HashSet<i64> = sounds.iter().map(|s| s.id).collect();
                    self.sound_file_ids
                        .retain(|_, sound_id| live_ids.contains(sound_id));
                    self.saved_notification_sounds = sounds;
                    self.saved_sounds_loaded = true;
                    self.saved_sounds_stale = false;
                }
            }
            EnvelopePayload::UpdateSavedNotificationSounds { .. } => {
                // The list changed server-side; refetch on the next ingest.
                self.saved_sounds_stale = true;
            }
            EnvelopePayload::ScopeNotificationSettings { settings, .. } => {
                // Parity slice: `getScopeNotificationSettings` answer; the
                // scope is correlated via the pending request (the response
                // carries no scope field).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
                    && let Some(scope) = pending.and_then(|p| p.scope)
                {
                    self.scope_notification_settings.insert(scope, settings);
                    self.scope_settings_loading.remove(&scope);
                }
            }
            EnvelopePayload::UpdateScopeNotificationSettings { scope, settings } => {
                // Parity slice: scope defaults changed (or our own
                // `setScopeNotificationSettings` was confirmed).
                self.scope_notification_settings.insert(scope, settings);
                self.scope_settings_loading.remove(&scope);
            }
            EnvelopePayload::Ok => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) {
                    // A short OK is not exhaustion; 404 is.
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::DeleteChatFolder)
                    && let Some(folder_id) = pending.and_then(|p| p.folder_id)
                {
                    // Parity slice: `deleteChatFolder` confirmed — drop the
                    // tab and any cached spec. `updateChatFolders` stays the
                    // source of truth and will confirm.
                    self.chat_folders.retain(|f| f.id != folder_id);
                    self.folder_specs.remove(&folder_id);
                    self.folder_chats_exhausted.remove(&folder_id);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.commit_viewed(chat_id);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LeaveChat)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    // Optimistic: `updateChatMember` confirms. TDLib errors
                    // keep the old status (Error arm below does not touch it).
                    chat.set_member_status(ChannelMemberStatus::Left, None);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::AddContact) {
                    // Phase 6: the new contact arrives via `updateUser`
                    // (`is_contact` flips); invalidate the list so the
                    // contacts tab refetches it.
                    self.contacts = None;
                    self.contacts_error = false;
                }
                if pending.is_some_and(|p| is_auth_submit(p.purpose)) {
                    self.last_auth_error = None;
                }
            }
            EnvelopePayload::Error(err) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) && err.code == 404
                {
                    self.chats_exhausted = true;
                }
                // Parity slice: folder `loadChats` paging ends the same way
                // as the main list — a 404 marks that folder exhausted.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadFolderChats)
                    && err.code == 404
                    && let Some(folder_id) = pending.and_then(|p| p.folder_id)
                {
                    self.folder_chats_exhausted.insert(folder_id);
                }
                // Phase 6: a failed `getContacts` surfaces a retry in the
                // contacts tab instead of a stuck spinner.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetContacts) {
                    self.contacts_error = true;
                }
                // Parity slice: a failed `getScopeNotificationSettings` must
                // not leave the scope in `scope_settings_loading` — otherwise
                // `maybe_fetch_scope_notification_settings` skips it on every
                // later ingest and every "Defaults for all chats…" open.
                // Dropping it here means the next fetch retries.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
                    && let Some(scope) = pending.and_then(|p| p.scope)
                {
                    self.scope_settings_loading.remove(&scope);
                }
                // Phase 3.3: `getCommands` failed — on a user session the
                // method is annotated "for bots only" (schema 1.8.67 line
                // 14953), so the error is permanent. Record an empty set
                // so the fetch is never retried; the `/` menu falls back
                // to the bot's `botInfo` commands.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCommands)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(user_id) = self.bot_user_id_for_chat(chat_id)
                {
                    self.bot_commands.entry(user_id).or_default();
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.abort_viewing(chat_id);
                }
                if self.search.matches_generation(pending) {
                    match pending.map(|p| p.purpose) {
                        Some(
                            RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats,
                        ) => {
                            self.search.accept_chats(Vec::new(), true);
                        }
                        Some(RequestPurpose::SearchMessages) => {
                            self.search.accept_messages(Vec::new(), true);
                        }
                        Some(RequestPurpose::SearchPublicChats) => {
                            self.search.accept_public_chats(Vec::new(), true);
                        }
                        _ => {}
                    }
                }
                if self.chat_search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
                {
                    self.chat_search
                        .accept_hits(Vec::new(), 0, MessageId(0), true);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetHistoryAround)
                    && let Some(message_id) = pending.and_then(|p| p.around_message_id)
                {
                    self.finish_history_around(pending, message_id, true, seq);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ForwardMessages)
                    && let Some(pending) = pending
                {
                    self.finish_forward(pending, &[], true);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
                    self.stickers.loading_sets = false;
                    self.stickers.failed = true;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
                    self.stickers.loading_set = false;
                    self.stickers.failed = true;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
                    self.gifs.loading = false;
                    self.gifs.failed = true;
                    self.gifs.stale = false;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage) {
                    // A TDLib error dismisses the option picker; no outcome is shown.
                    self.sponsored_report = None;
                    self.sponsored_report_target = None;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCallbackQueryAnswer) {
                    // TDLib returns error 502 when the bot misses the query
                    // timeout: surface it as an answer note (no TDLib text is
                    // echoed) so the press gets visible feedback.
                    self.last_callback_answer = Some(CallbackQueryAnswer {
                        text: "bot did not answer".to_string(),
                        show_alert: false,
                        url: String::new(),
                    });
                }
                let download_id = pending
                    .filter(|p| p.purpose == RequestPurpose::DownloadFile)
                    .and_then(|p| p.file_id)
                    .or_else(|| extra.and_then(|id| self.download_extras.get(&id.0).copied()));
                if let Some(file_id) = download_id {
                    self.unstick_download(file_id);
                }
                if let Some(pending) = pending
                    && is_auth_submit(pending.purpose)
                {
                    self.last_auth_error = Some(AuthRequestError {
                        purpose: pending.purpose,
                        class: err.class,
                    });
                }
            }
            EnvelopePayload::Unknown(kind) => {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some(kind.type_name),
                    extra: pending.map(|p| p.id.0),
                    seq: Some(seq),
                    note: "unknown-variant",
                });
            }
        }
    }

    fn set_auth(&mut self, state: AuthorizationState) {
        if matches!(state, AuthorizationState::Closed) {
            self.shutdown = ShutdownPhase::Closed;
            self.requests.invalidate_account();
            self.account_generation.bump();
            self.files.clear();
            self.downloading.clear();
            self.download_extras.clear();
            self.search.close();
            self.chat_search.close();
            self.in_flight_forward = None;
            self.last_forward = None;
        }
        if matches!(state, AuthorizationState::LoggingOut) {
            self.requests.invalidate_account();
            self.search.close();
            self.chat_search.close();
            self.in_flight_forward = None;
        }
        if matches!(state, AuthorizationState::Closing) {
            self.shutdown = ShutdownPhase::WaitingClosed;
        }
        self.auth = state;
        self.auth_view = view_for(&self.auth);
        self.last_auth_error = None;
    }

    fn apply_position_fields(&mut self, pos: ChatPositionUpdate) {
        // A position on one list is not an eviction from the other.
        // A single `updateChatPosition` updates only that list. A full
        // `updateChatLastMessage` positions set replaces both memberships.
        let chat = self
            .chats
            .entry(pos.chat_id.0)
            .or_insert_with(|| placeholder_chat(pos.chat_id));
        match pos.list {
            ChatList::Main => {
                if pos.order == 0 {
                    chat.in_main_list = false;
                } else {
                    chat.order = pos.order;
                    chat.is_pinned = pos.is_pinned;
                    chat.in_main_list = true;
                }
            }
            ChatList::Archive => {
                if pos.order == 0 {
                    chat.in_archive = false;
                } else {
                    chat.archive_order = pos.order;
                    chat.archive_is_pinned = pos.is_pinned;
                    chat.in_archive = true;
                }
            }
            // Phase 7.1: folder membership is positional, like Main/Archive.
            // `getChatListsToAddChat` is *not* folder membership — it lists
            // chat lists a chat can be added to for `addChatToList`.
            ChatList::Folder(folder_id) => {
                if pos.order == 0 {
                    chat.folder_positions.remove(&folder_id);
                } else {
                    chat.folder_positions.insert(folder_id, pos.order);
                }
            }
            ChatList::Unknown => {}
        }
    }

    fn replace_main_list_from_positions(
        &mut self,
        chat_id: ChatId,
        positions: &[ChatPositionUpdate],
    ) {
        match positions
            .iter()
            .find(|pos| pos.list == ChatList::Main)
            .cloned()
        {
            Some(pos) => self.apply_position_fields(pos),
            None => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_main_list = false;
                }
            }
        }
        match positions
            .iter()
            .find(|pos| pos.list == ChatList::Archive)
            .cloned()
        {
            Some(pos) => self.apply_position_fields(pos),
            None => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_archive = false;
                }
            }
        }
        // Folder positions are a full set too: drop folder ids that are no
        // longer present, then apply the ones that are.
        let folder_ids: HashSet<i32> = positions
            .iter()
            .filter_map(|pos| match pos.list {
                ChatList::Folder(id) => Some(id),
                _ => None,
            })
            .collect();
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.folder_positions
                .retain(|id, _| folder_ids.contains(id));
        }
        for pos in positions {
            if matches!(pos.list, ChatList::Folder(_)) {
                self.apply_position_fields(pos.clone());
            }
        }
    }

    fn upsert_message(&mut self, message: ParsedMessage, pending: bool) {
        self.remember_files(&message.files);
        let chat_id = message.chat_id;
        let topic_id = message.topic_id;
        let row = history_message(message, pending);
        let history = self.histories.entry(chat_id.0).or_default();
        history.upsert(row.clone());
        // Parity slice 4: a message addressed to a forum topic also lands
        // in that topic's history when the topic is loaded (the topic view
        // reads `topic_histories`, never the chat's main history). Missing
        // entries are left alone so the paging cursor stays fetch-owned.
        if let Some(topic_id) = topic_id
            && let Some(topic_history) = self.topic_histories.get_mut(&(chat_id.0, topic_id))
        {
            topic_history.upsert(row);
        }
    }

    fn remember_files(&mut self, files: &[ParsedFile]) {
        for file in files {
            // Nested message files can still be idle while a download is in flight.
            self.upsert_file(file.clone(), false);
        }
    }

    /// Phase 9.1: insert or drop a story-tray entry. Only the main story
    /// list shows in the tray; archived (`list == Archive`) and hidden
    /// (`list == None`) chats are removed.
    fn upsert_story_tray_entry(&mut self, entry: ChatActiveStoriesView) {
        if entry.list == Some(StoryListView::Main) {
            self.story_tray.insert(entry.chat_id, entry);
        } else {
            self.story_tray.remove(&entry.chat_id);
        }
    }

    /// Phase 9.1: tray entries for the story row above the chat list:
    /// main-list entries only, sorted by `(order, chat_id)` descending
    /// (schema `chatActiveStories` comment, line 6781).
    pub fn ordered_story_tray(&self) -> Vec<&ChatActiveStoriesView> {
        let mut entries: Vec<&ChatActiveStoriesView> = self
            .story_tray
            .values()
            .filter(|entry| entry.list == Some(StoryListView::Main))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse((entry.order, entry.chat_id)));
        entries
    }

    fn upsert_file(&mut self, file: ParsedFile, from_file_update: bool) {
        let idle_incomplete = file.local.is_idle_incomplete();
        if file.local.is_downloading_completed
            || !file.local.can_be_downloaded
            || (from_file_update && idle_incomplete)
        {
            self.unstick_download(file.id.0);
        }
        let sound_id = self.sound_file_ids.get(&file.id.0).copied();
        if let Some(sound_id) = sound_id {
            if let Some(path) = file.usable_path() {
                if self.pending_sound_downloads.remove(&sound_id) {
                    // Parity slice: a completed notification-sound download
                    // with playback requested → hand the path to the UI for
                    // ffplay. The reducer never spawns processes.
                    self.pending_sound_plays.push(path.into());
                }
            } else if from_file_update && file.local.is_idle_incomplete() {
                // Parity slice: a sound download that errored/cancelled
                // (active → idle without completing) must not leave the id in
                // `pending_sound_downloads` — otherwise a stale late
                // completion could trigger a belated play.
                self.pending_sound_downloads.remove(&sound_id);
            }
        }
        self.files.insert(file.id.0, file);
    }

    fn unstick_download(&mut self, file_id: i32) {
        self.downloading.remove(&file_id);
        self.download_extras.retain(|_, id| *id != file_id);
    }

    pub fn file(&self, id: FileId) -> Option<&ParsedFile> {
        self.files.get(&id.0)
    }

    pub fn should_download(&self, file_id: FileId) -> bool {
        if file_id.0 == 0 {
            return false;
        }
        if self.downloading.contains(&file_id.0) || self.requests.has_download(file_id) {
            return false;
        }
        match self.files.get(&file_id.0) {
            Some(file) => file.needs_download(),
            None => true,
        }
    }

    pub fn begin_download(&mut self, file_id: FileId) {
        if file_id.0 != 0 {
            self.downloading.insert(file_id.0);
        }
    }

    pub fn abort_download(&mut self, file_id: FileId) {
        self.unstick_download(file_id.0);
    }

    /// Photo thumbs in the open chat that are not secret/spoiler and still need a download.
    pub fn thumb_file_ids_to_download(&self) -> Vec<FileId> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        for message in history.messages.values() {
            match &message.content {
                MessageContent::Photo(photo) => {
                    if photo.is_secret || photo.has_spoiler {
                        continue;
                    }
                    if let Some(size) = photo.thumb_size()
                        && self.should_download(size.file_id)
                    {
                        ids.push(size.file_id);
                    }
                }
                MessageContent::Text(text) => {
                    if let Some(preview) = &text.link_preview
                        && let Some(photo) = &preview.photo
                        && let Some(size) = photo.thumb_size()
                        && self.should_download(size.file_id)
                    {
                        ids.push(size.file_id);
                    }
                }
                MessageContent::Sticker(sticker) => {
                    if let Some(file_id) = sticker.display_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::Animation(animation) => {
                    if animation.is_secret || animation.has_spoiler {
                        continue;
                    }
                    if let Some(file_id) = animation.thumb_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::Video(video) => {
                    if video.is_secret || video.has_spoiler {
                        continue;
                    }
                    if let Some(file_id) = video.thumb_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::VideoNote(note) => {
                    if note.is_secret {
                        continue;
                    }
                    if let Some(file_id) = note.thumb_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                MessageContent::Audio(audio) => {
                    if let Some(file_id) = audio.cover_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                _ => {}
            }
        }
        if self.gifs.open {
            for animation in &self.gifs.animations {
                let file_id = animation.thumb_file_id.filter(|id| id.0 != 0);
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        if self.stickers.open {
            for sticker in &self.stickers.stickers {
                let file_id = sticker.thumb_file_id.filter(|id| id.0 != 0).or_else(|| {
                    (sticker.format == StickerFormat::Webp && sticker.file_id.0 != 0)
                        .then_some(sticker.file_id)
                });
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        // Sponsored rows in the open chat: content + sponsor thumbs at priority 1.
        if let Some(chat_id) = self.open_chat
            && let Some(entry) = self.sponsored.get(&chat_id.0)
        {
            for message in &entry.messages {
                for file_id in message.thumb_file_ids() {
                    if self.should_download(file_id) {
                        ids.push(file_id);
                    }
                }
            }
        }
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }

    pub fn accept_installed_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.loading_sets = false;
        self.stickers.failed = false;
        self.stickers.sets = sets;
        let still_selected = self
            .stickers
            .selected_set_id
            .is_some_and(|id| self.stickers.sets.iter().any(|set| set.id == id));
        if !still_selected {
            self.stickers.selected_set_id = self.stickers.sets.first().map(|set| set.id);
            self.stickers.loaded_set_id = None;
            self.stickers.stickers.clear();
        }
    }

    pub fn select_sticker_set(&mut self, set_id: i64) {
        if self.stickers.selected_set_id == Some(set_id) {
            return;
        }
        self.stickers.selected_set_id = Some(set_id);
        self.stickers.loaded_set_id = None;
        self.stickers.stickers.clear();
        self.stickers.loading_set = false;
        self.stickers.failed = false;
    }

    pub fn mark_sticker_set_loading(&mut self) {
        self.stickers.loading_set = true;
        self.stickers.failed = false;
    }

    pub fn accept_saved_animations(&mut self, animations: Vec<AnimationItem>) {
        self.gifs.loading = false;
        self.gifs.failed = false;
        self.gifs.stale = false;
        self.gifs.animations = animations;
    }

    pub fn accept_sticker_set(&mut self, id: i64, stickers: Vec<StickerItem>) {
        self.stickers.loading_set = false;
        if self
            .stickers
            .selected_set_id
            .is_some_and(|selected| selected != id)
        {
            return;
        }
        self.stickers.failed = false;
        self.stickers.loaded_set_id = Some(id);
        self.stickers.stickers = stickers;
    }

    /// Record own channel membership from `getChatMember` / `updateChatMember`.
    /// The member is only trusted when `member_id` is the current user.
    pub fn accept_own_chat_member(&mut self, chat_id: ChatId, member: ParsedChatMember) {
        let own = self
            .my_user_id
            .is_some_and(|me| member.member_id == MessageSender::User { user_id: me });
        if !own {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("chatMember".into()),
                extra: Some(chat_id.0 as u64),
                seq: None,
                note: "foreign-member-ignored",
            });
            return;
        }
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.set_member_status(member.status, member.admin_can_post_messages);
        }
    }

    /// Record a `joinChat` outcome. `Success` flips status optimistically;
    /// `updateChatMember` confirms. The other variants keep the old status and
    /// are logged (the UI shows a fixed note).
    pub fn accept_join_chat_result(&mut self, chat_id: ChatId, result: ChatJoinResult) {
        match result {
            ChatJoinResult::Success { .. } => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.set_member_status(ChannelMemberStatus::Member, None);
                }
            }
            other => {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some("joinChat".into()),
                    extra: Some(chat_id.0 as u64),
                    seq: None,
                    note: match other {
                        ChatJoinResult::RequestSent => "join-request-sent",
                        ChatJoinResult::GuardBotApprovalRequired => "join-guard-bot-approval",
                        ChatJoinResult::Declined => "join-declined",
                        ChatJoinResult::Success { .. } => "join-success",
                    },
                });
            }
        }
    }

    /// Store `sponsoredMessages` for a chat (TDLib display order kept; files
    /// remembered for the download sandbox).
    pub fn accept_sponsored_messages(
        &mut self,
        chat_id: ChatId,
        messages: Vec<SponsoredMessage>,
        messages_between: i32,
        files: &[ParsedFile],
    ) {
        self.remember_files(files);
        self.sponsored.insert(
            chat_id.0,
            ChatSponsoredMessages {
                messages,
                messages_between,
            },
        );
    }

    /// Sponsored rows for the open chat in TDLib's response order. Empty for
    /// gated chats without a fetch and for chats that never had one.
    pub fn open_sponsored_rows(&self) -> Vec<&SponsoredMessage> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        self.sponsored
            .get(&chat_id.0)
            .map(ChatSponsoredMessages::ordered)
            .unwrap_or_default()
    }

    pub fn sponsored_message(&self, chat_id: ChatId, message_id: i64) -> Option<&SponsoredMessage> {
        self.sponsored
            .get(&chat_id.0)
            .and_then(|entry| entry.messages.iter().find(|m| m.message_id == message_id))
    }

    /// Begin a `reportChatSponsoredMessage` flow. Returns the request
    /// identifiers when the row exists and `can_be_reported` is set.
    pub fn begin_sponsored_report(
        &mut self,
        chat_id: ChatId,
        message_id: i64,
    ) -> Option<(ChatId, i64)> {
        let reportable = self
            .sponsored_message(chat_id, message_id)
            .is_some_and(|message| message.can_be_reported);
        if !reportable {
            return None;
        }
        self.sponsored_report = None;
        self.sponsored_report_target = Some((chat_id, message_id));
        self.last_sponsored_report = None;
        Some((chat_id, message_id))
    }

    /// Apply a `ReportSponsoredResult` for a finished `ReportChatSponsoredMessage`.
    /// `OptionRequired` arms the option picker; any other result closes it.
    pub fn accept_sponsored_report(
        &mut self,
        pending: &PendingRequest,
        result: ReportSponsoredResult,
    ) {
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let message_id = self
            .sponsored_report
            .as_ref()
            .map(|flight| flight.message_id)
            // Fallback for a response that arrives after its picker was
            // dismissed: attribute to the latest report target. If the user
            // starts a second report before the first responds, the first
            // response is attributed to the second row — acceptable: reports
            // are fire-and-forget and the outcome banner is per-chat.
            .or_else(|| self.sponsored_report_target.map(|(_, id)| id))
            .unwrap_or(0);
        match result {
            ReportSponsoredResult::OptionRequired { title, options } => {
                self.sponsored_report = Some(SponsoredReportFlight {
                    extra: pending.id,
                    chat_id,
                    message_id,
                    title,
                    options,
                });
            }
            result => {
                self.sponsored_report = None;
                self.sponsored_report_target = None;
                self.last_sponsored_report = Some(SponsoredReportOutcome {
                    chat_id,
                    message_id,
                    result,
                });
            }
        }
    }

    /// Drop the report picker flight and its target. Called when the user
    /// cancels, when a send fails, or when a result is applied elsewhere —
    /// a dismissed report must not attribute a late response to a stale row.
    pub fn dismiss_sponsored_report(&mut self) {
        self.sponsored_report = None;
        self.sponsored_report_target = None;
    }

    pub fn clear_sponsored_report_outcome(&mut self) {
        self.last_sponsored_report = None;
    }

    pub fn request_download(&mut self, file_id: FileId) -> RequestId {
        let extra = self
            .requests
            .register_download(self.account_generation, file_id);
        self.download_extras.insert(extra.0, file_id.0);
        extra
    }

    fn rebuild_main_order(&mut self) {
        let mut rows: Vec<ChatSummary> = self
            .chats
            .values()
            .filter(|c| c.in_main_list)
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.order.cmp(&a.order).then(b.id.0.cmp(&a.id.0)));
        self.main_order = rows.into_iter().map(|c| c.id).collect();
        let mut archived: Vec<ChatSummary> = self
            .chats
            .values()
            .filter(|c| c.in_archive)
            .cloned()
            .collect();
        archived.sort_by(|a, b| {
            b.archive_order
                .cmp(&a.archive_order)
                .then(b.id.0.cmp(&a.id.0))
        });
        self.archive_order = archived.into_iter().map(|c| c.id).collect();
    }

    pub fn open_chat(&mut self, chat_id: ChatId) {
        if self.chat_search.chat_id != Some(chat_id) {
            self.chat_search.close();
        }
        self.open_chat = Some(chat_id);
        // Phase 5.1: switching chats leaves the topic view.
        self.open_topic = None;
        self.view_generation.bump();
        let history = self.histories.entry(chat_id.0).or_default();
        history.view_generation = self.view_generation;
        history.viewed.clear();
        history.viewing.clear();
    }

    /// Parity slice: scope-defaulted settings for a chat's scope — the fetched
    /// `ScopeNotificationSettings`, or the schema defaults while the
    /// `getScopeNotificationSettings` fetch is still in flight. Chats keep
    /// `use_default_*` flags until the user overrides one (Unigram clones
    /// settings and clears the default flag), so the scope values are what
    /// `chatNotificationSettings` means when a flag is set (td_api.tl line
    /// 3348: "If true, the value for the relevant type of chat ... is used
    /// instead of mute_for").
    fn scope_settings_for(&self, scope: NotificationSettingsScope) -> ScopeNotificationSettings {
        self.scope_notification_settings
            .get(&scope)
            .cloned()
            .unwrap_or_default()
    }

    /// Effective mute for the toast/sound decisions: the chat's own
    /// exception mute, or the scope default's `mute_for` when the chat keeps
    /// `use_default_mute_for` (td_api.tl line 3348).
    // Public (not just crate-visible): the `ui` binary crate calls these.
    pub fn effective_muted(&self, chat: &ChatSummary) -> bool {
        if chat.is_muted() {
            return true;
        }
        let settings = &chat.notification_settings;
        if !settings.use_default_mute_for {
            return false;
        }
        let scope = scope_for_chat_kind(&chat.kind);
        self.scope_settings_for(scope).mute_for > 0
    }

    /// Effective message-preview allowance: the chat's own flag, or the
    /// scope default's `show_preview` when the chat keeps
    /// `use_default_show_preview` (td_api.tl line 3350).
    pub fn effective_preview_allowed(&self, chat: &ChatSummary) -> bool {
        let settings = &chat.notification_settings;
        if !settings.use_default_show_preview {
            return settings.show_preview;
        }
        let scope = scope_for_chat_kind(&chat.kind);
        self.scope_settings_for(scope).show_preview
    }

    /// Phase 8.1: pure notify / don't-notify decision for an `updateNewMessage`.
    /// Both the UI's `app_active` write and the reducer run on the UI thread,
    /// so no locking is needed. Returns `None` when the chat is unknown (no
    /// title, no verified mute/read state) rather than guessing.
    fn notification_for_new_message(&self, message: &ParsedMessage) -> Option<OsNotification> {
        let chat = self.chats.get(&message.chat_id.0)?;
        let chat_muted = self.effective_muted(chat);
        let chat_preview_allowed = self.effective_preview_allowed(chat);
        notify::decide_notify(&notify::NotifyInput {
            message,
            chat_title: Some(&chat.title),
            chat_muted,
            last_read_inbox_message_id: Some(chat.last_read_inbox_message_id),
            open_chat: self.open_chat,
            app_active: self.app_active,
            hide_previews: self.hide_notification_previews,
            chat_preview_allowed,
        })
    }

    /// Parity slice: pure play / don't-play decision for a notification's
    /// sound. Made at message-arrival time, together with the toast decision,
    /// so the sound reflects the mute/focus state the toast was decided on.
    fn notification_sound_for(&self, chat: &ChatSummary) -> Option<notify::NotificationSoundKind> {
        let settings = &chat.notification_settings;
        let scope = scope_for_chat_kind(&chat.kind);
        let scope_sound_id = self
            .scope_notification_settings
            .get(&scope)
            .map(|s| s.sound_id);
        notify::decide_notification_sound(&notify::SoundInput {
            app_active: self.app_active,
            chat_muted: self.effective_muted(chat),
            use_default_sound: settings.use_default_sound,
            chat_sound_id: settings.sound_id,
            scope_sound_id,
        })
    }

    /// Phase 8.1: append with same-chat burst coalescing; the first
    /// message's sound wins so a burst plays exactly once.
    fn queue_notification_with_sound(
        &mut self,
        notification: OsNotification,
        sound: Option<notify::NotificationSoundKind>,
    ) {
        notify::coalesce_notification_with_sound(
            &mut self.pending_notifications,
            notification,
            sound,
        );
    }

    /// Phase 5.1: enter a forum topic's view. Returns the topic's cached
    /// info, if the chat's topic list is already loaded.
    pub fn select_topic(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Option<ForumTopic> {
        self.open_topic = Some(forum_topic_id);
        self.view_generation.bump();
        self.open_topic_info(chat_id)
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub fn deselect_topic(&mut self) {
        self.open_topic = None;
        self.view_generation.bump();
    }

    /// Phase 5.1: cached info for the open topic, if any.
    pub fn open_topic_info(&self, chat_id: ChatId) -> Option<ForumTopic> {
        let topic_id = self.open_topic?;
        self.forum_topics
            .get(&chat_id.0)?
            .iter()
            .find(|t| t.forum_topic_id == topic_id)
            .cloned()
    }

    /// Phase 5.1: topics for a forum chat, sorted by `order` descending
    /// (schema: "Topics must be sorted by the order in descending order").
    pub fn ordered_forum_topics(&self, chat_id: ChatId) -> Vec<ForumTopic> {
        let mut topics: Vec<ForumTopic> = self
            .forum_topics
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        topics.sort_by(|a, b| b.order.cmp(&a.order).then(a.name.cmp(&b.name)));
        topics
    }

    /// Phase 5.1: record `supergroup.is_forum` for the chat backed by this
    /// supergroup (via `updateSupergroup` or the `getSupergroup` response).
    /// Returns true when a chat was updated.
    pub fn set_supergroup_forum(&mut self, supergroup_id: i64, is_forum: bool) -> bool {
        let mut changed = false;
        for chat in self.chats.values_mut() {
            if matches!(
                chat.kind,
                ChatKind::Supergroup {
                    supergroup_id: id,
                    ..
                } if id == supergroup_id
            ) && chat.is_forum != Some(is_forum)
            {
                chat.is_forum = Some(is_forum);
                changed = true;
            }
        }
        changed
    }

    /// Parity slice: cache the first active username for a supergroup
    /// (`updateSupergroup` / `getSupergroup` response). An empty username is
    /// stored as an empty sentinel (not removed) so `maybe_fetch_supergroup_profile`
    /// doesn't re-send `getSupergroup` on every re-open of a username-less
    /// supergroup; server-pushed `updateSupergroup` still refreshes it.
    /// Render sites must filter empty before display.
    pub fn set_supergroup_username(&mut self, supergroup_id: i64, username: String) {
        self.supergroup_usernames.insert(supergroup_id, username);
    }

    /// Parity slice: cached first active username for a supergroup, if any.
    /// May be an empty sentinel when the supergroup has no username —
    /// callers should filter empty before rendering.
    pub fn supergroup_username(&self, supergroup_id: i64) -> Option<&str> {
        self.supergroup_usernames
            .get(&supergroup_id)
            .map(String::as_str)
    }

    /// Parity slice: the cached `chat.photo.small` file for a chat, if it
    /// has been marked downloaded (its `local.path` usable). `None` when
    /// the chat has no photo or the file is not local yet.
    pub fn chat_photo_path(&self, chat_id: ChatId) -> Option<&str> {
        let file_id = self.chats.get(&chat_id.0)?.photo_file_id?;
        self.files.get(&file_id)?.usable_path()
    }

    /// Parity slice: `chat.photo.small` file ids for every known chat that
    /// still needs a download — the driver's chat-list avatar hook. Like
    /// the history-thumb hook, this is deduped by `should_download`
    /// (in-flight + local), and the `small` variant is the cheap 160px
    /// thumbnail, so one pass over all chats stays cheap.
    pub fn chat_list_photo_file_ids(&self) -> Vec<FileId> {
        self.chats
            .values()
            .filter_map(|chat| chat.photo_file_id)
            .filter(|id| self.should_download(FileId(*id)))
            .map(FileId)
            .collect()
    }

    /// Parity slice: the discussion-group chat id for a channel's
    /// "Discuss" affordance — `supergroupFullInfo.linked_chat_id` (0 =
    /// none). Only meaningful for channels.
    pub fn discussion_chat_id(&self, chat_id: ChatId) -> Option<i64> {
        let supergroup_id = match self.chats.get(&chat_id.0)?.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: true,
            } => supergroup_id,
            _ => return None,
        };
        let linked = self
            .supergroup_full_infos
            .get(&supergroup_id)?
            .linked_chat_id;
        // Only offer Discuss when the linked chat is actually known —
        // unknown ids degrade poorly (no history, no title), so hide it.
        (linked != 0 && self.chats.contains_key(&linked)).then_some(linked)
    }

    /// Server message ids in the open history that have not yet been sent to `viewMessages`.
    pub fn message_ids_to_view(&self, chat_id: ChatId) -> Vec<MessageId> {
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        history
            .messages
            .values()
            .filter(|message| {
                message.id.0 > 0
                    && !history.viewed.contains(&message.id.0)
                    && !history.viewing.contains(&message.id.0)
            })
            .map(|message| message.id)
            .collect()
    }

    pub fn mark_viewed(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.histories.entry(chat_id.0).or_default();
        for id in ids {
            history.viewing.remove(&id.0);
            history.viewed.insert(id.0);
        }
    }

    /// After a successful `viewMessages` send, hold ids until TDLib ok/error.
    pub fn begin_viewing(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.histories.entry(chat_id.0).or_default();
        for id in ids {
            history.viewing.insert(id.0);
        }
    }

    fn commit_viewed(&mut self, chat_id: ChatId) {
        let history = self.histories.entry(chat_id.0).or_default();
        history.viewed.extend(history.viewing.drain());
    }

    fn abort_viewing(&mut self, chat_id: ChatId) {
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.viewing.clear();
        }
    }

    pub fn ordered_chats(&self) -> Vec<&ChatSummary> {
        self.main_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.in_main_list)
            .collect()
    }

    /// Chats in `chatListArchive`, highest TDLib order first (same as main).
    pub fn ordered_archived_chats(&self) -> Vec<&ChatSummary> {
        self.archive_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.in_archive)
            .collect()
    }

    /// Phase 7.1: chats in `chatListFolder(folder_id)`, highest TDLib folder
    /// order first (same convention as main/archive).
    pub fn ordered_folder_chats(&self, folder_id: i32) -> Vec<&ChatSummary> {
        let mut rows: Vec<&ChatSummary> = self
            .chats
            .values()
            .filter(|chat| chat.folder_positions.contains_key(&folder_id))
            .collect();
        rows.sort_by(|a, b| {
            b.folder_positions
                .get(&folder_id)
                .cmp(&a.folder_positions.get(&folder_id))
                .then(b.id.0.cmp(&a.id.0))
        });
        rows
    }

    /// Phase 7.1: display name of a folder tab, from `updateChatFolders`.
    pub fn folder_name(&self, folder_id: i32) -> Option<&str> {
        self.chat_folders
            .iter()
            .find(|f| f.id == folder_id)
            .map(|f| f.name.as_str())
    }

    pub fn request(&mut self, purpose: RequestPurpose, chat_id: Option<ChatId>) -> RequestId {
        let view = if matches!(purpose, RequestPurpose::GetHistory) {
            Some(self.view_generation)
        } else {
            None
        };
        self.requests
            .register(self.account_generation, purpose, chat_id, view)
    }

    /// Parity slice: like `request`, but also stamps the notification
    /// settings scope for `GetScopeNotificationSettings` /
    /// `SetScopeNotificationSettings` correlation (`PendingRequest::scope`).
    pub fn request_for_scope(
        &mut self,
        purpose: RequestPurpose,
        scope: NotificationSettingsScope,
    ) -> RequestId {
        let extra = self.request(purpose, None);
        if let Some(pending) = self.requests.pending_mut(extra) {
            pending.scope = Some(scope);
        }
        extra
    }

    /// Phase 5.1: like `request`, but also stamps the forum topic id for
    /// `GetTopicHistory` correlation (`PendingRequest::forum_topic_id`).
    pub fn request_for_topic(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        forum_topic_id: i32,
    ) -> RequestId {
        let id = self.request(purpose, chat_id);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.forum_topic_id = Some(forum_topic_id);
        }
        id
    }

    /// Phase 6: like `request`, but stamps the user id for user-scoped
    /// requests (`GetUserFullInfo` from the contacts panel, `AddContact`)
    /// so id-less responses correlate (`PendingRequest::user_id`).
    pub fn request_for_user(&mut self, purpose: RequestPurpose, user_id: i64) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.user_id = Some(user_id);
        }
        id
    }

    /// Phase 6: like `request`, but stamps the supergroup id for
    /// `GetSupergroupFullInfo` correlation
    /// (`PendingRequest::supergroup_id`).
    pub fn request_for_supergroup(
        &mut self,
        purpose: RequestPurpose,
        supergroup_id: i64,
    ) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.supergroup_id = Some(supergroup_id);
        }
        id
    }

    /// Phase 9.1: like `request`, but stamps the chat id and story id for
    /// `GetStory` correlation and per-story in-flight dedupe
    /// (`PendingRequest::story_id`).
    pub fn request_for_story(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_id: i32,
    ) -> RequestId {
        let id = self.request(purpose, Some(chat_id));
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.story_id = Some(story_id);
        }
        id
    }

    /// Parity slice: like `request`, but stamps the folder id for
    /// folder-scoped requests (`GetChatFolder`, `EditChatFolder`,
    /// `DeleteChatFolder`, `LoadFolderChats`) so responses correlate
    /// (`PendingRequest::folder_id`).
    pub fn request_for_folder(&mut self, purpose: RequestPurpose, folder_id: i32) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.folder_id = Some(folder_id);
        }
        id
    }

    pub fn request_search(&mut self, purpose: RequestPurpose, search_generation: u64) -> RequestId {
        self.requests
            .register_search(self.account_generation, purpose, search_generation)
    }

    pub fn request_chat_search(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        search_generation: u64,
    ) -> RequestId {
        self.requests.register_chat_search(
            self.account_generation,
            purpose,
            chat_id,
            search_generation,
        )
    }

    pub fn request_history_around(&mut self, chat_id: ChatId, message_id: MessageId) -> RequestId {
        self.requests.register_around(
            self.account_generation,
            chat_id,
            self.view_generation,
            message_id,
        )
    }

    pub fn open_search(&mut self) {
        self.search.open_field();
    }

    pub fn close_search(&mut self) {
        self.search.close();
    }

    pub fn open_chat_search(&mut self) -> bool {
        let Some(chat_id) = self.open_chat else {
            return false;
        };
        if !self
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return false;
        }
        self.chat_search.open_for(chat_id);
        true
    }

    pub fn close_chat_search(&mut self) {
        self.chat_search.close();
    }

    fn apply_history_around(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        seq: u64,
    ) {
        if pending.view_generation != Some(self.view_generation) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-view-generation",
            });
            return;
        }
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        if self.open_chat != Some(chat_id) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-chat-history",
            });
            return;
        }
        for message in messages {
            self.upsert_message(message.clone(), false);
        }
        if let Some(message_id) = pending.around_message_id {
            self.finish_history_around(Some(pending), message_id, false, seq);
        }
    }

    fn finish_history_around(
        &mut self,
        pending: Option<&PendingRequest>,
        message_id: MessageId,
        _error: bool,
        seq: u64,
    ) {
        if !matches!(
            self.chat_search.jump,
            ChatSearchJump::Loading { message_id: current } if current == message_id
        ) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: pending.map(|p| p.id.0),
                seq: Some(seq),
                note: "stale-chat-search-jump",
            });
            return;
        }
        let Some(chat_id) = pending.and_then(|p| p.chat_id).or(self.chat_search.chat_id) else {
            return;
        };
        let history = self.histories.entry(chat_id.0).or_default();
        // Tombstone, 404/error, or around-load without the id: deleted/inaccessible.
        self.chat_search.jump = if history.contains(message_id) {
            ChatSearchJump::Ready { message_id }
        } else {
            ChatSearchJump::Missing { message_id }
        };
    }

    /// Resolve a hit: already loaded, tombstoned/deleted, or needs `getChatHistory` around.
    pub fn begin_chat_search_jump(&mut self, message_id: MessageId) -> ChatSearchJumpNeed {
        let Some(chat_id) = self.chat_search.chat_id.or(self.open_chat) else {
            self.chat_search.jump = ChatSearchJump::Missing { message_id };
            return ChatSearchJumpNeed::Missing;
        };
        self.chat_search.select_message(message_id);
        let history = self.histories.entry(chat_id.0).or_default();
        if history.is_tombstone(message_id) {
            self.chat_search.jump = ChatSearchJump::Missing { message_id };
            return ChatSearchJumpNeed::Missing;
        }
        if history.contains(message_id) {
            self.chat_search.jump = ChatSearchJump::Ready { message_id };
            return ChatSearchJumpNeed::AlreadyReady;
        }
        self.chat_search.jump = ChatSearchJump::Loading { message_id };
        ChatSearchJumpNeed::LoadAround
    }

    pub fn apply_local_chat_search_filter(&mut self, query: &str) {
        let Some(chat_id) = self.open_chat else {
            return;
        };
        if !self.chat_search.open {
            self.chat_search.open_for(chat_id);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.chat_search.clear_query();
            return;
        }
        let _ = self.chat_search.begin_query(trimmed);
        let needle = trimmed.to_lowercase();
        let hits: Vec<SearchMessageHit> = self
            .histories
            .get(&chat_id.0)
            .map(|history| {
                history
                    .ordered()
                    .into_iter()
                    .rev()
                    .filter(|message| message.content.preview().to_lowercase().contains(&needle))
                    .map(|message| SearchMessageHit {
                        chat_id: message.chat_id,
                        message_id: message.id,
                        preview: message.content.preview(),
                        is_outgoing: message.is_outgoing,
                        content: message.content.clone(),
                        reply_to: message.reply_to.clone(),
                        forward_info: message.forward_info.clone(),
                        interaction_info: message.interaction_info.clone(),
                        is_pinned: message.is_pinned,
                        media_album_id: message.media_album_id,
                        reply_markup: message.reply_markup.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = hits.len() as i32;
        self.chat_search
            .accept_hits(hits, total, MessageId(0), false);
        if let Some(id) = self.chat_search.selected_hit().map(|hit| hit.message_id) {
            let _ = self.begin_chat_search_jump(id);
        }
    }

    /// Newest pinned message in the open chat's loaded history.
    pub fn open_chat_pinned_message(&self) -> Option<&HistoryMessage> {
        let chat_id = self.open_chat?;
        self.histories.get(&chat_id.0)?.newest_pinned()
    }

    /// Insert a found message into that chat's history so open-chat can show it
    /// without a separate history pagination scheme.
    pub fn promote_search_message(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(index) = self
            .search
            .messages
            .iter()
            .position(|hit| hit.chat_id == chat_id && hit.message_id == message_id)
        else {
            return;
        };
        let hit = self.search.messages[index].clone();
        let history = self.histories.entry(chat_id.0).or_default();
        history.upsert(hit.into_history());
    }

    /// Main-list chats whose title contains `query` (case-insensitive). Demo-only
    /// local filter when no live TDLib replies are injected.
    pub fn local_search_chats(&self, query: &str) -> Vec<&ChatSummary> {
        let needle = query.trim().to_lowercase();
        self.ordered_chats()
            .into_iter()
            .filter(|chat| needle.is_empty() || chat.title.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn apply_local_search_filter(&mut self, query: &str) {
        self.search.open = true;
        self.search.query = query.to_string();
        self.search.generation = self.search.generation.saturating_add(1);
        self.search.clear_results();
        self.search.recents = query.trim().is_empty();
        self.search.chat_ids = self
            .local_search_chats(query)
            .into_iter()
            .map(|chat| chat.id)
            .collect();
        self.search.chats_done = true;
        self.search.messages_done = true;
        if query.trim().is_empty() {
            self.search.chat_ids.clear();
            self.search.status = SearchStatus::Idle;
        } else {
            self.search.finish_if_complete();
        }
    }

    pub fn begin_close(&mut self) {
        self.shutdown = ShutdownPhase::CloseRequested;
    }

    pub fn begin_logout(&mut self) {
        self.requests.invalidate_account();
        self.account_generation.bump();
        self.shutdown = ShutdownPhase::CloseRequested;
    }
}

fn history_message(message: ParsedMessage, pending: bool) -> HistoryMessage {
    HistoryMessage {
        id: message.id,
        chat_id: message.chat_id,
        is_outgoing: message.is_outgoing,
        content: message.content,
        pending,
        reply_to: message.reply_to,
        forward_info: message.forward_info,
        interaction_info: message.interaction_info,
        is_pinned: message.is_pinned,
        media_album_id: message.media_album_id,
        reply_markup: message.reply_markup,
    }
}

impl Session {
    /// Compact quote label: chosen `textQuote`, else the loaded original,
    /// else `messageReplyToMessage.content` preview.
    pub fn reply_quote_preview(&self, message: &HistoryMessage) -> Option<String> {
        let reply = message.reply_to.as_ref()?;
        Some(self.resolve_reply_preview(reply, message.chat_id))
    }

    /// Phase 4.2: apply `updatePoll` (schema 1.8.67 line 11179). The update
    /// carries no chat or message id, so every loaded history is scanned for
    /// a `messagePoll` whose `poll.id` matches; the poll is replaced in
    /// place (vote counts, percentages, chosen marks). Returns the number
    /// of rows updated.
    pub fn apply_update_poll(&mut self, poll: Poll) -> usize {
        let mut updated = 0;
        for history in self.histories.values_mut() {
            for message in history.messages.values_mut() {
                if let MessageContent::Poll(poll_content) = &mut message.content
                    && poll_content.poll.id == poll.id
                {
                    poll_content.poll = poll.clone();
                    updated += 1;
                }
            }
        }
        updated
    }

    pub fn resolve_reply_preview(&self, reply: &MessageReplyTo, fallback_chat: ChatId) -> String {
        if let Some(quote) = reply
            .quote_text
            .as_ref()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
        {
            return quote.to_string();
        }
        let chat_id = if reply.chat_id.0 != 0 {
            reply.chat_id
        } else {
            fallback_chat
        };
        if let Some(original) = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&reply.message_id.0))
        {
            return original.content.preview();
        }
        reply
            .content_preview
            .clone()
            .unwrap_or_else(|| "Message".into())
    }

    /// Loaded Main-list destinations for the forward picker. Local title filter
    /// (tdesktop ShareBox search field). Unsupported kinds stay out.
    pub fn forward_destinations(&self, query: &str) -> Vec<&ChatSummary> {
        self.local_search_chats(query)
            .into_iter()
            .filter(|chat| chat.supported())
            .collect()
    }

    /// Official "Forwarded from" label from `messageForwardInfo.origin`.
    pub fn forward_from_label(&self, info: &MessageForwardInfo) -> String {
        match &info.origin {
            MessageOrigin::HiddenUser { sender_name } if !sender_name.trim().is_empty() => {
                format!("Forwarded from {sender_name}")
            }
            MessageOrigin::User { user_id } => self
                .chats
                .values()
                .find(
                    |chat| matches!(chat.kind, ChatKind::Private { user_id: id } if id == *user_id),
                )
                .map(|chat| format!("Forwarded from {}", chat.title))
                .unwrap_or_else(|| "Forwarded message".into()),
            MessageOrigin::Chat {
                chat_id,
                author_signature,
            }
            | MessageOrigin::Channel {
                chat_id,
                author_signature,
                ..
            } => {
                if let Some(chat) = self.chats.get(&chat_id.0) {
                    if author_signature.is_empty() {
                        format!("Forwarded from {}", chat.title)
                    } else {
                        format!("Forwarded from {} ({author_signature})", chat.title)
                    }
                } else if !author_signature.is_empty() {
                    format!("Forwarded from {author_signature}")
                } else {
                    "Forwarded message".into()
                }
            }
            _ => "Forwarded message".into(),
        }
    }

    fn finish_forward(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        failed: bool,
    ) {
        let forwarded: Vec<ParsedMessage> = if failed {
            Vec::new()
        } else {
            messages.to_vec()
        };
        for message in &forwarded {
            self.remember_files(&message.files);
            self.upsert_message(message.clone(), message.id.0 < 0);
        }
        let flight = self
            .in_flight_forward
            .take()
            .filter(|flight| flight.extra == pending.id);
        let dest_chat_id = flight
            .as_ref()
            .map(|f| f.dest_chat_id)
            .or(pending.chat_id)
            .unwrap_or(ChatId(0));
        let dest_title = self
            .chats
            .get(&dest_chat_id.0)
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("chat {}", dest_chat_id.0));
        self.last_forward = Some(ForwardResult {
            dest_chat_id,
            dest_title,
            from_chat_id: flight.as_ref().map(|f| f.from_chat_id).unwrap_or(ChatId(0)),
            requested: flight
                .as_ref()
                .map(|f| f.requested)
                .unwrap_or(forwarded.len()),
            forwarded_ids: forwarded.iter().map(|m| m.id).collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::telegram::client::copy_and_parse;
    use crate::telegram::envelope::LocalFileState;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    #[test]
    fn send_success_replaces_pending_id() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":-1,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageSendSucceeded","old_message_id":-1,"message":{"id":88,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        let history = session.histories.get(&1).unwrap();
        assert!(!history.messages.contains_key(&-1));
        assert!(history.messages.contains_key(&88));
        assert!(!history.messages.get(&88).unwrap().pending);
    }

    #[test]
    fn stale_history_does_not_mix_chats() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
        session.open_chat(ChatId(2));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":5,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"from-a","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert!(
            session
                .histories
                .get(&1)
                .map(|h| h.messages.is_empty())
                .unwrap_or(true)
        );
        assert!(
            session
                .histories
                .get(&2)
                .map(|h| h.messages.is_empty())
                .unwrap_or(true)
        );
    }

    #[test]
    fn permanent_delete_wins_over_old_fetch() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"bye","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[9],"is_permanent":true,"from_cache":false}"#,
        );
        let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":9,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"bye","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert!(!session.histories.get(&1).unwrap().messages.contains_key(&9));
        assert!(session.histories.get(&1).unwrap().tombstones.contains(&9));
    }

    #[test]
    fn cache_eviction_is_not_permanent() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[9],"is_permanent":false,"from_cache":true}"#,
        );
        let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":9,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert!(session.histories.get(&1).unwrap().messages.contains_key(&9));
    }

    #[test]
    fn chat_list_sorts_by_tdlib_order_then_id() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":2,"title":"b","type":{"@type":"chatTypePrivate","user_id":2},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":1,"title":"a","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":1,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":2,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
        );
        let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![2, 1]);
    }

    #[test]
    fn channel_is_supported() {
        let kind = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        assert!(kind.is_supported_cloud_chat());
        assert!(kind.gate_reason().is_none());
        assert!(kind.is_channel());
        let group = ChatKind::Supergroup {
            supergroup_id: 2,
            is_channel: false,
        };
        assert!(!group.is_channel());
    }

    #[test]
    fn load_chats_404_marks_exhaustion() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::LoadChats, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.chats_exhausted);
        assert!(!sink.rendered().contains("Not Found"));
    }

    #[test]
    fn auth_code_error_is_classified_without_native_message() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::CheckAuthenticationCode, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"PHONE_CODE_INVALID CANARY_CODE_999","@extra":"{}"}}"#,
                extra.0
            ),
        );
        let err = session.last_auth_error.expect("classified auth error");
        assert_eq!(err.purpose, RequestPurpose::CheckAuthenticationCode);
        assert_eq!(err.class, ErrorClass::Invalid);
        assert_eq!(err.user_message(), "code not accepted");
        let logs = sink.rendered();
        assert!(!logs.contains("CANARY_CODE"));
        assert!(!logs.contains("PHONE_CODE_INVALID"));
        let debug = format!("{err:?}");
        assert!(!debug.contains("CANARY_CODE"));
    }

    #[test]
    fn new_chat_after_position_keeps_main_list_membership() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":9,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":true}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":9,"title":"after","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":2}}"#,
        );
        let chat = session.chats.get(&9).unwrap();
        assert_eq!(chat.title, "after");
        assert!(chat.in_main_list);
        assert!(chat.is_pinned);
        assert_eq!(chat.order, 4);
        assert_eq!(session.ordered_chats().len(), 1);
    }

    #[test]
    fn last_message_positions_replace_main_list_membership() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":3,"title":"c","type":{"@type":"chatTypePrivate","user_id":3},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":{"id":1,"chat_id":3,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_PREVIEW_hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
        );
        let chat = session.chats.get(&3).unwrap();
        assert!(chat.in_main_list);
        assert_eq!(chat.last_preview, "CANARY_PREVIEW_hi");
        assert_eq!(session.ordered_chats()[0].id.0, 3);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":null,"positions":[]}"#,
        );
        assert!(!session.chats.get(&3).unwrap().in_main_list);
        assert!(session.ordered_chats().is_empty());
        assert!(!sink.rendered().contains("CANARY_PREVIEW"));
    }

    #[test]
    fn archive_position_does_not_clear_main_list() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":5,"title":"keep","type":{"@type":"chatTypePrivate","user_id":5},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"6","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"3","is_pinned":false}}"#,
        );
        let chat = session.chats.get(&5).unwrap();
        assert!(chat.in_main_list);
        assert!(chat.in_archive);
        assert_eq!(chat.archive_order, 3);
        assert_eq!(chat.order, 6);
        assert_eq!(session.ordered_chats().len(), 1);
        assert_eq!(session.ordered_archived_chats().len(), 1);
    }

    #[test]
    fn last_message_positions_are_a_full_set_including_archive() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":6,"title":"mixed","type":{"@type":"chatTypePrivate","user_id":6},"unread_count":0}}"#,
        );
        // Archive after Main in the array must not wipe Main membership.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":6,"last_message":{"id":2,"chat_id":6,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":true},{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"1","is_pinned":false}]}"#,
        );
        let chat = session.chats.get(&6).unwrap();
        assert!(chat.in_main_list);
        assert!(chat.is_pinned);
        assert_eq!(chat.order, 9);
        // Full set without Main removes from the main list.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":6,"last_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"1","is_pinned":false}]}"#,
        );
        let chat = session.chats.get(&6).unwrap();
        assert!(!chat.in_main_list);
        assert!(chat.in_archive);
        assert!(session.ordered_chats().is_empty());
        assert_eq!(session.ordered_archived_chats()[0].id.0, 6);
    }

    #[test]
    fn notification_settings_mute_and_unmute() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
        );
        let chat = session.chats.get(&7).unwrap();
        assert!(chat.is_muted());
        assert!(chat.notification_settings.is_muted_forever());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
        );
        assert!(!session.chats.get(&7).unwrap().is_muted());
    }

    /// Parity slice: `scopeNotificationSettings` answer JSON for a
    /// `request_for_scope` extra (notification-sounds regression tests).
    fn scope_settings_json(extra: &str, mute_for: i32, show_preview: bool) -> String {
        format!(
            r#"{{"@type":"scopeNotificationSettings","mute_for":{mute_for},"sound_id":"-1","show_preview":{show_preview},"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"-1","use_default_show_story_poster":true,"show_story_poster":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false,"@extra":"{extra}"}}"#,
        )
    }

    /// Blocking-issue regression: the scope default mute
    /// (`use_default_mute_for` + scope `mute_for`) must suppress both the
    /// toast and the sound — previously only the chat's exception mute
    /// gated the decisions, so "Forever" under Groups changed server state
    /// while Quill kept toasting and sounding.
    #[test]
    fn scope_mute_default_suppresses_toast_and_sound() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // A group chat that keeps the default mute setting.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
        );
        assert!(!session.chats.get(&14).unwrap().is_muted());
        // The GroupChats scope is muted "Forever" (as if set through the
        // scope-defaults dialog).
        let extra = session.request_for_scope(
            RequestPurpose::GetScopeNotificationSettings,
            NotificationSettingsScope::GroupChats,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &scope_settings_json(&extra.0.to_string(), 2147483647, true),
        );
        // App in background: the message would normally notify…
        session.app_active = false;
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey","entities":[]}}}}"#,
        );
        // …but the scope default mute suppresses both the toast and the sound.
        assert!(session.pending_notifications.is_empty());
        assert!(session.pending_sound_plays.is_empty());
        let chat = session.chats.get(&14).unwrap();
        assert!(session.effective_muted(chat));
        assert!(session.notification_sound_for(chat).is_none());
        // Clearing the scope mute restores the toast and a sound decision.
        let extra = session.request_for_scope(
            RequestPurpose::GetScopeNotificationSettings,
            NotificationSettingsScope::GroupChats,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &scope_settings_json(&extra.0.to_string(), 0, true),
        );
        let chat = session.chats.get(&14).unwrap();
        assert!(!session.effective_muted(chat));
        // App background, unmuted, default sound → the app default tone.
        assert_eq!(
            session.notification_sound_for(chat),
            Some(notify::NotificationSoundKind::Default)
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":43,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey again","entities":[]}}}}"#,
        );
        assert_eq!(session.pending_notifications.len(), 1);
        assert_eq!(
            session.pending_notifications[0].sound,
            Some(notify::NotificationSoundKind::Default)
        );
    }

    /// Blocking-issue regression: the scope default `show_preview` governs
    /// the preview when the chat keeps `use_default_show_preview` — the old
    /// `use_default_show_preview || show_preview` always allowed previews.
    #[test]
    fn scope_show_preview_default_gates_preview_body() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
        );
        // Scope fetched with message previews off.
        let extra = session.request_for_scope(
            RequestPurpose::GetScopeNotificationSettings,
            NotificationSettingsScope::GroupChats,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &scope_settings_json(&extra.0.to_string(), 0, false),
        );
        session.app_active = false;
        // Global previews enabled, but the scope default disables them.
        session.hide_notification_previews = false;
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":14,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"secret text","entities":[]}}}}"#,
        );
        assert_eq!(session.pending_notifications.len(), 1);
        assert_eq!(
            session.pending_notifications[0].for_display().body,
            "New message"
        );
        let chat = session.chats.get(&14).unwrap();
        assert!(!session.effective_preview_allowed(chat));
    }

    /// Blocking-issue regression: a failed `getScopeNotificationSettings`
    /// must not keep the scope in `scope_settings_loading` — otherwise every
    /// later fetch skips it and the scope stays unfetchable forever.
    #[test]
    fn failed_scope_settings_fetch_retries() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_scope(
            RequestPurpose::GetScopeNotificationSettings,
            NotificationSettingsScope::GroupChats,
        );
        // `maybe_fetch_scope_notification_settings` marks the scope in-flight
        // when it sends the request.
        session
            .scope_settings_loading
            .insert(NotificationSettingsScope::GroupChats);
        // TDLib answers with an error.
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":500,"message":"CANARY_SCOPE_ERR","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(
            !session
                .scope_settings_loading
                .contains(&NotificationSettingsScope::GroupChats),
            "failed fetch must free the scope for retry"
        );
        assert!(
            !session
                .scope_notification_settings
                .contains_key(&NotificationSettingsScope::GroupChats)
        );
        // A later fetch for the same scope lands normally.
        let extra = session.request_for_scope(
            RequestPurpose::GetScopeNotificationSettings,
            NotificationSettingsScope::GroupChats,
        );
        session
            .scope_settings_loading
            .insert(NotificationSettingsScope::GroupChats);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &scope_settings_json(&extra.0.to_string(), 0, true),
        );
        assert!(
            session
                .scope_notification_settings
                .contains_key(&NotificationSettingsScope::GroupChats)
        );
        assert!(
            !session
                .scope_settings_loading
                .contains(&NotificationSettingsScope::GroupChats)
        );
    }

    /// Nit regression: a notification-sound download that errors (active →
    /// idle without completing) must not leave its id in
    /// `pending_sound_downloads` — a stale late completion could otherwise
    /// trigger a belated play.
    #[test]
    fn sound_download_error_drops_pending_playback() {
        let (mut session, _sink) = session();
        session.sound_file_ids.insert(91, 7);
        session.pending_sound_downloads.insert(7);
        session.upsert_file(
            ParsedFile {
                id: FileId(91),
                size: 0,
                expected_size: 100,
                local: LocalFileState {
                    path: String::new(),
                    can_be_downloaded: true,
                    is_downloading_active: false,
                    is_downloading_completed: false,
                },
            },
            true,
        );
        assert!(!session.pending_sound_downloads.contains(&7));
        assert!(session.pending_sound_plays.is_empty());
        // The file→sound mapping itself stays (the list refetch prunes it).
        assert_eq!(session.sound_file_ids.get(&91), Some(&7));
    }

    /// Nit regression: a `getSavedNotificationSounds` refetch evicts
    /// `sound_file_ids` entries for sounds that are no longer saved.
    #[test]
    fn sound_list_refetch_prunes_file_ids() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sound_file_ids.insert(91, 7); // dropped from the list
        session.sound_file_ids.insert(92, 8); // still saved
        let extra = session.request(RequestPurpose::GetSavedNotificationSounds, None);
        let sound_file = r#"{"@type":"file","id":92,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"notificationSounds","notification_sounds":[{{"@type":"notificationSound","id":8,"duration":2,"date":0,"title":"Chime","data":"","sound":{}}}],"@extra":"{}"}}"#,
                sound_file, extra.0
            ),
        );
        assert!(session.saved_sounds_loaded);
        assert!(!session.sound_file_ids.contains_key(&91));
        assert_eq!(session.sound_file_ids.get(&92), Some(&8));
    }

    #[test]
    fn phase81_desktop_notification_replay() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // Unmuted private chat, nothing read yet.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        );
        // App in background: incoming unread message queues a notification.
        // `hide_notification_previews` defaults to true → generic body.
        session.app_active = false;
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":42,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey you","entities":[]}}}}"#,
        );
        assert_eq!(session.pending_notifications.len(), 1);
        let queued = &session.pending_notifications[0];
        assert_eq!(queued.chat_id, ChatId(7));
        assert_eq!(queued.title, "Ada");
        assert_eq!(queued.count, 1);
        assert_eq!(queued.for_display().body, "New message");
        session.pending_notifications.clear();

        // Previews enabled → body is the message preview.
        session.hide_notification_previews = false;
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":43,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hey you","entities":[]}}}}"#,
        );
        assert_eq!(session.pending_notifications.len(), 1);
        assert_eq!(
            session.pending_notifications[0].for_display().body,
            "hey you"
        );

        // Second message for the same chat coalesces into a burst summary.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":44,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"and more","entities":[]}}}}"#,
        );
        assert_eq!(session.pending_notifications.len(), 1);
        assert_eq!(session.pending_notifications[0].count, 2);
        assert_eq!(
            session.pending_notifications[0].for_display().body,
            "2 new messages"
        );
        session.pending_notifications.clear();
    }

    #[test]
    fn phase81_desktop_notification_suppressed_cases() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // Muted chat.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_show_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
        );
        // Unmuted chat.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Noor","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
        );
        session.app_active = false;
        session.hide_notification_previews = false;
        let incoming = |id: i64, chat_id: i64| {
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
            )
        };
        // Muted → nothing.
        apply_json(&mut session, &seq, &sink, &incoming(1, 7));
        assert!(session.pending_notifications.is_empty());
        // Outgoing → nothing.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":2,"chat_id":8,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        assert!(session.pending_notifications.is_empty());
        // Unknown chat → nothing.
        apply_json(&mut session, &seq, &sink, &incoming(3, 99));
        assert!(session.pending_notifications.is_empty());
        // Currently open chat while the app is active → nothing.
        session.app_active = true;
        session.open_chat(ChatId(8));
        apply_json(&mut session, &seq, &sink, &incoming(4, 8));
        assert!(session.pending_notifications.is_empty());
        // Same chat, app in background → notifies.
        session.app_active = false;
        apply_json(&mut session, &seq, &sink, &incoming(5, 8));
        assert_eq!(session.pending_notifications.len(), 1);
        session.pending_notifications.clear();
        // Already-read message (at/below the inbox read marker) → nothing.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadInbox","chat_id":8,"last_read_inbox_message_id":6,"unread_count":0}"#,
        );
        apply_json(&mut session, &seq, &sink, &incoming(6, 8));
        assert!(session.pending_notifications.is_empty());
    }

    #[test]
    fn chat_action_typing_then_cancel() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAction","chat_id":7,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionTyping"}}"#,
        );
        let chat = session.chats.get(&7).unwrap();
        assert!(chat.is_peer_typing());
        assert_eq!(chat.sidebar_preview(), "typing…");
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionCancel"}}"#,
        );
        let chat = session.chats.get(&7).unwrap();
        assert!(!chat.is_peer_typing());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":9},"action":{"@type":"chatActionRecordingVoiceNote"}}"#,
        );
        assert!(!session.chats.get(&7).unwrap().is_peer_typing());
    }

    #[test]
    fn chat_title_and_unread_updates() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":4,"title":"old","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatTitle","chat_id":4,"title":"new title"}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":1,"unread_count":7}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAddedToList","chat_id":4,"chat_list":{"@type":"chatListMain"}}"#,
        );
        let chat = session.chats.get(&4).unwrap();
        assert_eq!(chat.title, "new title");
        assert_eq!(chat.unread_count, 7);
        assert_eq!(chat.last_read_inbox_message_id.0, 1);
        assert!(chat.in_main_list);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatRemovedFromList","chat_id":4,"chat_list":{"@type":"chatListMain"}}"#,
        );
        assert!(!session.chats.get(&4).unwrap().in_main_list);
    }

    #[test]
    fn auth_password_ok_clears_last_error() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::CheckAuthenticationPassword, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"PASSWORD_HASH_INVALID CANARY_PW","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.last_auth_error.is_some());
        let extra = session.request(RequestPurpose::CheckAuthenticationPassword, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(session.last_auth_error.is_none());
        assert!(!sink.rendered().contains("CANARY_PW"));
    }

    #[test]
    fn unread_badge_hides_when_zero_and_caps_at_99() {
        assert_eq!(unread_badge_text(0), None);
        assert_eq!(unread_badge_text(-1), None);
        assert_eq!(unread_badge_text(1).as_deref(), Some("1"));
        assert_eq!(unread_badge_text(3).as_deref(), Some("3"));
        assert_eq!(unread_badge_text(99).as_deref(), Some("99"));
        assert_eq!(unread_badge_text(100).as_deref(), Some("99+"));
    }

    #[test]
    fn read_inbox_and_outbox_update_cursors() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":4,"title":"inbox","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":3,"last_read_inbox_message_id":10,"last_read_outbox_message_id":0}}"#,
        );
        let chat = session.chats.get(&4).unwrap();
        assert_eq!(chat.unread_count, 3);
        assert_eq!(chat.last_read_inbox_message_id.0, 10);
        assert_eq!(chat.last_read_outbox_message_id.0, 0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":13,"unread_count":0}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":12}"#,
        );
        let chat = session.chats.get(&4).unwrap();
        assert_eq!(chat.unread_count, 0);
        assert_eq!(unread_badge_text(chat.unread_count), None);
        assert_eq!(chat.last_read_inbox_message_id.0, 13);
        assert_eq!(chat.last_read_outbox_message_id.0, 12);
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn outbox_receipt_is_honest_when_cursor_is_zero() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":1,"title":"dm","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0,"last_read_outbox_message_id":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":20,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_OUTBOX_hi","entities":[]}}}}"#,
        );
        let chat = session.chats.get(&1).unwrap();
        let message = session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&20)
            .unwrap();
        assert_eq!(chat.outbox_receipt(message), OutboxReceipt::Sent);
        assert_eq!(
            outgoing_status_label(false, chat.outbox_receipt(message)),
            "You · sent"
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadOutbox","chat_id":1,"last_read_outbox_message_id":20}"#,
        );
        let chat = session.chats.get(&1).unwrap();
        let message = session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&20)
            .unwrap();
        assert_eq!(chat.outbox_receipt(message), OutboxReceipt::Read);
        assert_eq!(
            outgoing_status_label(false, chat.outbox_receipt(message)),
            "You · read"
        );
        assert_eq!(
            outgoing_status_label(true, OutboxReceipt::None),
            "You (sending)"
        );
        assert!(!sink.rendered().contains("CANARY_OUTBOX"));
    }

    #[test]
    fn opening_a_chat_clears_viewed_ids_for_that_generation() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        session.mark_viewed(ChatId(1), &[MessageId(5)]);
        assert!(session.histories.get(&1).unwrap().viewed.contains(&5));
        session.open_chat(ChatId(1));
        assert!(session.histories.get(&1).unwrap().viewed.is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
        session.mark_viewed(ChatId(1), &[MessageId(5)]);
        assert!(session.message_ids_to_view(ChatId(1)).is_empty());
    }

    #[test]
    fn view_messages_tdlib_error_releases_in_flight_ids() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
        session.begin_viewing(ChatId(1), &[MessageId(5)]);
        assert!(session.message_ids_to_view(ChatId(1)).is_empty());
        assert!(
            session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(1))
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_FAIL","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(!session.requests.has_purpose(RequestPurpose::ViewMessages));
        assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
        assert!(session.histories.get(&1).unwrap().viewing.is_empty());
        assert!(!session.histories.get(&1).unwrap().viewed.contains(&5));
        assert!(!sink.rendered().contains("CANARY_VIEW_FAIL"));

        let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
        session.begin_viewing(ChatId(1), &[MessageId(5)]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(session.message_ids_to_view(ChatId(1)).is_empty());
        assert!(session.histories.get(&1).unwrap().viewed.contains(&5));
        assert!(session.histories.get(&1).unwrap().viewing.is_empty());
    }

    fn media_file_json(id: i32, path: &str, completed: bool) -> String {
        format!(
            r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#,
            path = serde_json::to_string(path).unwrap(),
            completed = completed,
        )
    }

    #[test]
    fn photo_and_document_are_stored_and_update_file_completes_path() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let thumb = media_file_json(1, "", false);
        let full = media_file_json(2, "", false);
        let doc = media_file_json(9, "", false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":10,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_body","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":11,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{doc}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
            ),
        );
        assert_eq!(
            session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&10)
                .unwrap()
                .content
                .preview(),
            "CANARY_PHOTO_body"
        );
        assert_eq!(
            session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&11)
                .unwrap()
                .content
                .preview(),
            "notes.txt"
        );
        assert!(session.file(FileId(1)).unwrap().needs_download());
        assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(1)]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateFile","file":{}}}"#,
                media_file_json(1, "/tmp/quill-thumb.jpg", true)
            ),
        );
        assert_eq!(
            session.file(FileId(1)).unwrap().usable_path(),
            Some("/tmp/quill-thumb.jpg")
        );
        assert!(session.thumb_file_ids_to_download().is_empty());
        assert!(!sink.rendered().contains("CANARY_PHOTO"));
        assert!(!sink.rendered().contains("CANARY_REMOTE"));
        assert!(!sink.rendered().contains("/tmp/quill-thumb"));
    }

    #[test]
    fn secret_photo_is_not_auto_thumbed() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let file = media_file_json(3, "", false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":12,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":true}}}}}}"#
            ),
        );
        assert!(session.thumb_file_ids_to_download().is_empty());
        assert!(session.should_download(FileId(3)));
    }

    #[test]
    fn video_thumb_auto_downloads_secret_video_does_not() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let thumb = media_file_json(41, "", false);
        let clip = media_file_json(42, "", false);
        let secret_thumb = media_file_json(43, "", false);
        let secret_clip = media_file_json(44, "", false);
        let open = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":8,"width":320,"height":180,"file_name":"a.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":68,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
        );
        let secret = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":21,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":100,"height":100,"file_name":"s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{secret_thumb}}},"video":{secret_clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":true}}}}}}"#
        );
        apply_json(&mut session, &seq, &sink, &open);
        apply_json(&mut session, &seq, &sink, &secret);
        assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(41)]);
        assert!(session.should_download(FileId(42)));
        assert!(session.should_download(FileId(43)));
        assert_eq!(
            session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&20)
                .unwrap()
                .content
                .preview(),
            "clip"
        );
    }

    #[test]
    fn video_note_thumb_auto_downloads_secret_note_does_not() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let thumb = media_file_json(51, "", false);
        let clip = media_file_json(52, "", false);
        let secret_thumb = media_file_json(53, "", false);
        let secret_clip = media_file_json(54, "", false);
        let open = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":30,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":6,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
        );
        let secret = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":31,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":1,"waveform":"","length":200,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{secret_thumb}}},"speech_recognition_result":null,"video":{secret_clip}}},"is_viewed":false,"is_secret":true}}}}}}"#
        );
        apply_json(&mut session, &seq, &sink, &open);
        apply_json(&mut session, &seq, &sink, &secret);
        assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(51)]);
        assert!(session.should_download(FileId(52)));
        assert!(session.should_download(FileId(53)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageContentOpened","chat_id":1,"message_id":30}"#,
        );
        let content = &session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&30)
            .unwrap()
            .content;
        let crate::telegram::envelope::MessageContent::VideoNote(note) = content else {
            panic!("{content:?}");
        };
        assert!(note.is_viewed);
        assert_eq!(note.length, 240);
        assert_eq!(content.preview(), "Video note");
    }

    #[test]
    fn audio_cover_auto_downloads_track_does_not() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let cover = media_file_json(61, "", false);
        let track = media_file_json(62, "", false);
        let external = media_file_json(63, "", false);
        let open = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":40,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":90,"title":"Night Drive","performer":"Ada","file_name":"night.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":90,"file":{cover}}},"external_album_covers":[{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{external}}}],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
        );
        apply_json(&mut session, &seq, &sink, &open);
        assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(61)]);
        assert!(session.should_download(FileId(62)));
        let fallback = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":41,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":10,"title":"","performer":"","file_name":"b.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":40,"height":40,"file":{external}}}],"audio":{track}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
        );
        apply_json(&mut session, &seq, &sink, &fallback);
        let ids = session.thumb_file_ids_to_download();
        assert!(ids.contains(&FileId(61)));
        assert!(ids.contains(&FileId(63)));
        assert!(!ids.contains(&FileId(62)));
        assert_eq!(
            session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&40)
                .unwrap()
                .content
                .preview(),
            "Night Drive"
        );
    }

    #[test]
    fn download_error_unsticks_in_flight_file() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_download(FileId(4));
        session.begin_download(FileId(4));
        assert!(!session.should_download(FileId(4)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_FILE_ERR","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.should_download(FileId(4)));
        assert!(!session.downloading.contains(&4));
        assert!(!sink.rendered().contains("CANARY_FILE_ERR"));
    }

    fn file_reply_json(extra: u64, id: i32, active: bool) -> String {
        format!(
            r#"{{"@type":"file","@extra":"{extra}","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#
        )
    }

    #[test]
    fn download_unsticks_after_file_extra_then_idle_update_or_error() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        let extra = session.request_download(FileId(4));
        session.begin_download(FileId(4));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &file_reply_json(extra.0, 4, true),
        );
        assert!(session.requests.take(extra).is_none());
        assert!(!session.should_download(FileId(4)));
        assert!(session.downloading.contains(&4));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateFile","file":{}}}"#,
                media_file_json(4, "", false)
            ),
        );
        assert!(session.should_download(FileId(4)));
        assert!(!session.downloading.contains(&4));

        let extra = session.request_download(FileId(5));
        session.begin_download(FileId(5));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &file_reply_json(extra.0, 5, true),
        );
        assert!(!session.should_download(FileId(5)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_FILE_ERR2","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.should_download(FileId(5)));
        assert!(!session.downloading.contains(&5));
        assert!(!sink.rendered().contains("CANARY_FILE_ERR2"));
    }

    #[test]
    fn nested_idle_message_file_does_not_unstick_in_flight_download() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_download(FileId(6));
        session.begin_download(FileId(6));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &file_reply_json(extra.0, 6, true),
        );
        let file = media_file_json(6, "", false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":13,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
            ),
        );
        assert!(session.downloading.contains(&6));
        assert!(!session.should_download(FileId(6)));
    }

    #[test]
    fn search_chats_and_messages_happy_path() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":false}}"#,
        );
        session.open_search();
        let search_gen = session.search.begin_query("hello");
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
                chats_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Searching);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[42]}}"#,
                public_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Searching);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_SEARCH_hi","entities":[]}}}}}}]}}"#,
                messages_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Ready);
        assert_eq!(session.search.chat_ids, vec![ChatId(11)]);
        assert_eq!(session.search.public_chat_ids, vec![ChatId(42)]);
        assert_eq!(session.search.messages.len(), 1);
        assert_eq!(session.search.messages[0].preview, "CANARY_SEARCH_hi");
        session.promote_search_message(ChatId(11), MessageId(101));
        assert!(
            session
                .histories
                .get(&11)
                .unwrap()
                .messages
                .contains_key(&101)
        );
        session.close_search();
        assert_eq!(session.search.status, SearchStatus::Closed);
        assert!(session.search.query.is_empty());
        assert!(!sink.rendered().contains("CANARY_SEARCH"));
    }

    #[test]
    fn search_empty_and_error_and_stale_generation() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let search_gen = session.search.begin_query("zzz");
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                chats_extra.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                messages_extra.0
            ),
        );
        // Phase 7.2: still waiting on the public leg.
        assert_eq!(session.search.status, SearchStatus::Searching);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                public_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Empty);

        let stale = session.search.begin_query("old");
        let stale_chats = session.request_search(RequestPurpose::SearchChats, stale);
        let stale_messages = session.request_search(RequestPurpose::SearchMessages, stale);
        let fresh = session.search.begin_query("new");
        let _fresh_chats = session.request_search(RequestPurpose::SearchChats, fresh);
        let fresh_messages = session.request_search(RequestPurpose::SearchMessages, fresh);
        let fresh_public = session.request_search(RequestPurpose::SearchPublicChats, fresh);
        let _ = fresh_public;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[99]}}"#,
                stale_chats.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":1,"chat_id":99,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
                stale_messages.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Searching);
        assert!(session.search.chat_ids.is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR","@extra":"{}"}}"#,
                fresh_messages.0
            ),
        );
        let search_gen = session.search.generation;
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR2","@extra":"{}"}}"#,
                chats_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Searching);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR3","@extra":"{}"}}"#,
                public_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Failed);
        assert!(!sink.rendered().contains("CANARY_SEARCH_ERR"));
    }

    #[test]
    fn search_recently_found_chats_empty_query() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":false}}"#,
        );
        let search_gen = session.search.begin_recents();
        assert!(session.search.recents);
        let extra = session.request_search(RequestPurpose::SearchRecentlyFoundChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
                extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Ready);
        assert_eq!(session.search.chat_ids, vec![ChatId(11)]);
        assert!(session.search.messages.is_empty());
        let empty_gen = session.search.begin_recents();
        let empty_extra =
            session.request_search(RequestPurpose::SearchRecentlyFoundChats, empty_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                empty_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Idle);
        assert!(session.search.recents);
    }

    #[test]
    fn chat_search_happy_empty_stale_and_jump() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello already loaded.","entities":[]}}}}"#,
        );
        session.open_chat(ChatId(11));
        assert!(session.open_chat_search());
        let search_gen = session.chat_search.begin_query("hello");
        let extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_CHAT_hi","entities":[]}}}}}},{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Ready);
        assert_eq!(session.chat_search.hits.len(), 2);
        assert_eq!(session.chat_search.selected, Some(0));
        assert_eq!(session.chat_search.hits[0].preview, "CANARY_CHAT_hi");
        assert_eq!(
            session.begin_chat_search_jump(MessageId(101)),
            ChatSearchJumpNeed::AlreadyReady
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(101)
            }
        );
        assert_eq!(
            session.begin_chat_search_jump(MessageId(90)),
            ChatSearchJumpNeed::LoadAround
        );
        let around = session.request_history_around(ChatId(11), MessageId(90));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":89,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
                around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(90)
            }
        );
        assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
        assert!(session.histories.get(&11).unwrap().contains(MessageId(89)));
        assert!(session.histories.get(&11).unwrap().contains(MessageId(101)));

        let empty_gen = session.chat_search.begin_query("zzz");
        let empty_extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), empty_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                empty_extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Empty);

        let stale = session.chat_search.begin_query("old");
        let stale_extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), stale);
        let fresh = session.chat_search.begin_query("new");
        let fresh_extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), fresh);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":1,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
                stale_extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Searching);
        assert!(session.chat_search.hits.is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_CHAT_ERR","@extra":"{}"}}"#,
                fresh_extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Failed);
        session.close_chat_search();
        assert_eq!(session.chat_search.status, SearchStatus::Closed);
        assert!(session.histories.get(&11).unwrap().contains(MessageId(101)));
        assert!(!sink.rendered().contains("CANARY_CHAT"));
    }

    #[test]
    fn chat_search_jump_missing_deleted_and_inaccessible() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello","entities":[]}}}}"#,
        );
        session.open_chat(ChatId(11));
        assert!(session.open_chat_search());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[70],"is_permanent":true,"from_cache":false}"#,
        );
        session.chat_search.hits.push(SearchMessageHit {
            chat_id: ChatId(11),
            message_id: MessageId(70),
            preview: "gone".into(),
            is_outgoing: false,
            content: crate::telegram::envelope::MessageContent::Text("gone".into()),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
        });
        assert_eq!(
            session.begin_chat_search_jump(MessageId(70)),
            ChatSearchJumpNeed::Missing
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Missing {
                message_id: MessageId(70)
            }
        );

        session.chat_search.hits.push(SearchMessageHit {
            chat_id: ChatId(11),
            message_id: MessageId(80),
            preview: "ghost".into(),
            is_outgoing: false,
            content: crate::telegram::envelope::MessageContent::Text("ghost".into()),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
        });
        assert_eq!(
            session.begin_chat_search_jump(MessageId(80)),
            ChatSearchJumpNeed::LoadAround
        );
        let around = session.request_history_around(ChatId(11), MessageId(80));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":79,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor only","entities":[]}}}}}}]}}"#,
                around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Missing {
                message_id: MessageId(80)
            }
        );
        assert!(!session.histories.get(&11).unwrap().contains(MessageId(80)));

        let stale_around = session.request_history_around(ChatId(11), MessageId(80));
        session.chat_search.generation = session.chat_search.generation.saturating_add(1);
        assert_eq!(
            session.begin_chat_search_jump(MessageId(101)),
            ChatSearchJumpNeed::AlreadyReady
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"late","entities":[]}}}}}}]}}"#,
                stale_around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(101)
            }
        );
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn reply_to_message_preview_and_jump() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"sounds good","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""}}}"#,
        );
        session.open_chat(ChatId(11));
        let reply = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&104)
            .unwrap();
        assert_eq!(
            reply.reply_to.as_ref().map(|r| r.message_id),
            Some(MessageId(101))
        );
        assert_eq!(
            session.reply_quote_preview(reply).as_deref(),
            Some("Hello from injected JSON.")
        );
        assert_eq!(
            session.begin_chat_search_jump(reply.reply_to.as_ref().unwrap().message_id),
            ChatSearchJumpNeed::AlreadyReady
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(101)
            }
        );

        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"quoted","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":90,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"manual quote","entities":[]},"position":0,"is_manual":true},"checklist_task_id":0,"poll_option_id":""}}}"#,
        );
        let quoted = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&105)
            .unwrap();
        assert_eq!(
            session.reply_quote_preview(quoted).as_deref(),
            Some("manual quote")
        );
        assert_eq!(
            session.begin_chat_search_jump(MessageId(90)),
            ChatSearchJumpNeed::LoadAround
        );
        let around = session.request_history_around(ChatId(11), MessageId(90));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older original","entities":[]}}}}}}]}}"#,
                around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(90)
            }
        );
        assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
    }

    #[test]
    fn update_message_content_rewrites_own_text() {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let mut session = Session::new(AccountKey::primary(), dyn_sink);
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":102,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageContent","chat_id":11,"message_id":102,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDITED_own","entities":[]}}}"#,
        );
        let text = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&102)
            .unwrap()
            .content
            .preview();
        assert_eq!(text, "CANARY_EDITED_own");
        assert!(
            !session
                .histories
                .get(&11)
                .unwrap()
                .is_tombstone(MessageId(102))
        );
        assert!(!sink.rendered().contains("CANARY_EDITED"));
    }

    #[test]
    fn forward_messages_result_upserts_dest_and_labels_origin() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"2","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":12,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"1","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"already forwarded","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1}}}"#,
        );
        let dests: Vec<_> = session
            .forward_destinations("bo")
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(dests, vec![ChatId(12)]);
        assert!(
            session
                .forward_destinations("")
                .iter()
                .all(|c| c.supported())
        );
        let extra = session.request(RequestPurpose::ForwardMessages, Some(ChatId(12)));
        session.in_flight_forward = Some(ForwardFlight {
            extra,
            dest_chat_id: ChatId(12),
            from_chat_id: ChatId(11),
            requested: 1,
        });
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}}]}}"#,
                extra.0
            ),
        );
        let result = session.last_forward.as_ref().expect("forward result");
        assert_eq!(result.dest_chat_id, ChatId(12));
        assert_eq!(result.dest_title, "Bob");
        assert_eq!(result.forwarded_ids, vec![MessageId(80)]);
        assert_eq!(result.success_label(), "Forwarded to Bob");
        let dest = session
            .histories
            .get(&12)
            .unwrap()
            .messages
            .get(&80)
            .unwrap();
        assert_eq!(
            session.forward_from_label(dest.forward_info.as_ref().unwrap()),
            "Forwarded from Alice"
        );
        let incoming = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&105)
            .unwrap();
        assert_eq!(
            session.forward_from_label(incoming.forward_info.as_ref().unwrap()),
            "Forwarded from Ada Lovelace"
        );
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn interaction_info_update_sets_chips_and_own_highlight() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}}"#,
        );
        let incoming = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        assert!(incoming.can_react());
        assert_eq!(incoming.emoji_reaction_chips().len(), 1);
        assert!(!incoming.chosen_emoji("❤"));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        );
        let updated = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        let chips = updated.emoji_reaction_chips();
        assert_eq!(chips[0].chip_label().as_deref(), Some("❤ 3"));
        assert!(chips[0].is_chosen);
        assert_eq!(chips[1].chip_label().as_deref(), Some("👍 2"));
        assert!(!chips[1].is_chosen);
        assert!(updated.chosen_emoji("❤"));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":null}"#,
        );
        let cleared = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        assert!(cleared.emoji_reaction_chips().is_empty());
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn message_is_pinned_update_and_newest_pinned() {
        let sink = Arc::new(MemorySink::new());
        let mut session = Session::new(AccountKey::primary(), sink.clone());
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":100,"chat_id":11,"is_outgoing":false,"is_pinned":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"older","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"is_pinned":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
        );
        session.open_chat = Some(ChatId(11));
        let pinned = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        assert!(pinned.is_pinned);
        assert!(pinned.can_pin());
        assert_eq!(
            session.open_chat_pinned_message().map(|m| m.id),
            Some(MessageId(101))
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":false}"#,
        );
        assert!(
            !session
                .histories
                .get(&11)
                .unwrap()
                .messages
                .get(&101)
                .unwrap()
                .is_pinned
        );
        assert!(session.open_chat_pinned_message().is_none());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":100,"is_pinned":true}"#,
        );
        assert_eq!(
            session.open_chat_pinned_message().map(|m| m.id),
            Some(MessageId(100))
        );
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn private_draft_restores_and_dirty_update_is_ignored() {
        let sink = Arc::new(MemorySink::new());
        let seq = AtomicU64::new(0);
        let mut session = Session::new(AccountKey::primary(), sink.clone());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":{"@type":"inputMessageReplyToMessage","message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""},"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"meet at 6","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
        );
        assert!(session.accepts_composer_draft(ChatId(11)));
        let draft = session.chats.get(&11).unwrap().draft.clone().unwrap();
        assert_eq!(draft.text, "meet at 6");
        assert_eq!(draft.reply_to_message_id, Some(MessageId(101)));
        assert!(
            session
                .chats
                .get(&11)
                .unwrap()
                .sidebar_preview()
                .starts_with("Draft:")
        );
        session.mark_draft_dirty(ChatId(11));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatDraftMessage","chat_id":11,"draft_message":{"@type":"draftMessage","reply_to":null,"date":2,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"stale","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null},"positions":[]}"#,
        );
        assert_eq!(
            session.chats.get(&11).unwrap().draft.as_ref().unwrap().text,
            "meet at 6"
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUser","user":{"id":11,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
        );
        assert!(!session.accepts_composer_draft(ChatId(11)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0,"draft_message":{"@type":"draftMessage","reply_to":null,"date":1,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"nope","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}}"#,
        );
        assert!(!session.accepts_composer_draft(ChatId(13)));
    }

    #[test]
    fn update_poll_refreshes_counts_and_chosen_marks() {
        // Phase 4.2: `updatePoll` carries no chat/message id — the reducer
        // scans loaded histories and replaces the matching `poll.id` in
        // place (vote counts, percentages, chosen marks).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":106,"chat_id":15,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":12,"vote_percentage":55,"is_chosen":true},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":7,"vote_percentage":32,"is_chosen":false}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":13,"vote_percentage":56,"is_chosen":false},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":10,"vote_percentage":43,"is_chosen":true}],"total_voter_count":23,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":true,"type":{"@type":"pollTypeRegular"}}}"#,
        );
        let history = session.histories.get(&15).unwrap();
        let message = history.messages.get(&106).unwrap();
        let MessageContent::Poll(poll_content) = &message.content else {
            panic!("{:?}", message.content);
        };
        let poll = &poll_content.poll;
        assert_eq!(poll.total_voter_count, 23);
        assert_eq!(poll.options[1].voter_count, 10);
        assert_eq!(poll.options[1].vote_percentage, 43);
        assert!(!poll.options[0].is_chosen);
        assert!(poll.options[1].is_chosen);
        assert!(poll.is_closed);
    }

    #[test]
    fn update_poll_with_unknown_id_updates_nothing() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updatePoll","poll":{"@type":"poll","id":9999,"question":{"@type":"formattedText","text":"Ghost","entities":[]},"options":[],"total_voter_count":0,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":false,"is_closed":false,"type":{"@type":"pollTypeRegular"}}}"#,
        );
        assert!(session.histories.values().all(|h| h.messages.is_empty()));
    }

    /// Phase 5.1: `updateSupergroup` / `getSupergroup` responses resolve
    /// `ChatSummary::is_forum` for the matching supergroup chat.
    #[test]
    fn update_supergroup_resolves_forum_flag() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        );
        assert_eq!(session.chats.get(&16).unwrap().is_forum, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
        );
        assert!(session.chats.get(&16).unwrap().is_forum_chat());
        // The getSupergroup response path is gated on the pending purpose.
        let extra = session.request(RequestPurpose::GetSupergroup, Some(ChatId(16)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"supergroup","@extra":"{}","id":16,"is_forum":false}}"#,
                extra.0
            ),
        );
        assert!(!session.chats.get(&16).unwrap().is_forum_chat());
        // Same payload without the pending purpose is ignored (it is not an update).
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"supergroup","id":16,"is_forum":true}"#,
        );
        assert!(!session.chats.get(&16).unwrap().is_forum_chat());
    }

    /// Parity slice: `updateNewChat` keeps `chat.photo.small` (`chatPhotoInfo`,
    /// schema 1.8.67 lines 762/3627) on `ChatSummary::photo_file_id` and
    /// remembers the file so the driver can download it.
    #[test]
    fn chat_photo_remembered_from_update_new_chat() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0,"photo":{"@type":"chatPhotoInfo","small":{"@type":"file","id":91,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"big":{"@type":"file","id":92,"size":0,"expected_size":0,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}},"minithumbnail":null,"has_animation":false,"is_personal":false}}}"#,
        );
        assert_eq!(session.chats.get(&11).unwrap().photo_file_id, Some(91));
        assert!(session.files.contains_key(&91));
        // `updateNewChat` without a photo leaves no avatar.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":12,"title":"No photo","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0}}"#,
        );
        assert_eq!(session.chats.get(&12).unwrap().photo_file_id, None);
    }

    /// Parity slice: `updateChatPhoto` (schema 1.8.67 line 10488) swaps the
    /// cached photo; a null photo clears it.
    #[test]
    fn update_chat_photo_swaps_and_clears() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let photo = |id: i32| {
            format!(
                r#""photo":{{"@type":"chatPhotoInfo","small":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}}},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}"#
            )
        };
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"updateChatPhoto\",\"chat_id\":11,{}}}",
                photo(91)
            ),
        );
        assert_eq!(session.chats.get(&11).unwrap().photo_file_id, Some(91));
        assert!(session.files.contains_key(&91));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"updateChatPhoto\",\"chat_id\":11,{}}}",
                photo(95)
            ),
        );
        assert_eq!(session.chats.get(&11).unwrap().photo_file_id, Some(95));
        // Photo removed → fallback avatar.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPhoto","chat_id":11,"photo":null}"#,
        );
        assert_eq!(session.chats.get(&11).unwrap().photo_file_id, None);
    }

    /// Parity slice: `updateSupergroup` caches the first active username and
    /// `updateSupergroupFullInfo` lands the description/count/linked chat
    /// without a pending request; `discussion_chat_id` resolves the
    /// channel's discussion group.
    #[test]
    fn supergroup_username_and_linked_chat_cached() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"usernames":{"@type":"usernames","active_usernames":["demochannel"],"disabled_usernames":[],"editable_username":"demochannel","collectible_usernames":[]},"is_forum":false,"is_channel":true}}"#,
        );
        assert_eq!(session.supergroup_username(13), Some("demochannel"));
        assert_eq!(session.discussion_chat_id(ChatId(13)), None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"@type":"supergroupFullInfo","description":"CANARY channel","member_count":12345,"linked_chat_id":14}}"#,
        );
        let info = session.supergroup_full_info(13).unwrap();
        assert_eq!(info.description, "CANARY channel");
        assert_eq!(info.member_count, 12345);
        assert_eq!(info.linked_chat_id, 14);
        // The linked discussion group resolves to its chat id.
        assert_eq!(session.discussion_chat_id(ChatId(13)), Some(14));
        // Non-channels never get a "Discuss" affordance, even with a link.
        assert_eq!(session.discussion_chat_id(ChatId(14)), None);
        // Empty username stores an empty sentinel (renders as no username,
        // keeps the dedupe cache filled so re-opens don't refetch).
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":13,"usernames":null,"is_forum":false,"is_channel":true}}"#,
        );
        assert_eq!(session.supergroup_username(13), Some(""));
        assert!(session.supergroup_usernames.contains_key(&13));
    }

    /// Parity slice: `chat_list_photo_file_ids` only returns photos that
    /// still need a download (dedupes in-flight and completed files).
    #[test]
    fn chat_list_photo_file_ids_dedupes() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        for (chat_id, file_id) in [(11, 91), (12, 92), (13, 93)] {
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    "{{\"@type\":\"updateNewChat\",\"chat\":{{\"id\":{chat_id},\"title\":\"c{chat_id}\",\"type\":{{\"@type\":\"chatTypePrivate\",\"user_id\":{chat_id}}},\"unread_count\":0,\"photo\":{{\"@type\":\"chatPhotoInfo\",\"small\":{{\"@type\":\"file\",\"id\":{file_id},\"size\":24,\"expected_size\":24,\"local\":{{\"@type\":\"localFile\",\"path\":\"\",\"can_be_downloaded\":true,\"can_be_deleted\":false,\"is_downloading_active\":false,\"is_downloading_completed\":false,\"download_offset\":0,\"downloaded_prefix_size\":0,\"downloaded_size\":0}},\"remote\":{{\"@type\":\"remoteFile\",\"id\":\"x\",\"unique_id\":\"u\",\"is_uploading_active\":false,\"is_uploading_completed\":false,\"uploaded_size\":0}}}},\"big\":null,\"minithumbnail\":null,\"has_animation\":false,\"is_personal\":false}}}}}}",
                ),
            );
        }
        // 92 completes locally; 93 is already in flight.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateFile","file":{"@type":"file","id":92,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"/tmp/x.png","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":true,"download_offset":0,"downloaded_prefix_size":24,"downloaded_size":24},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}"#,
        );
        session.begin_download(FileId(93));
        let ids: Vec<i32> = session
            .chat_list_photo_file_ids()
            .into_iter()
            .map(|id| id.0)
            .collect();
        assert_eq!(ids, vec![91]);
        // A completed file resolves its display path.
        session.open_chat(ChatId(12));
        assert_eq!(session.chat_photo_path(ChatId(12)), Some("/tmp/x.png"));
        assert_eq!(session.chat_photo_path(ChatId(11)), None);
    }

    /// Phase 5.1: `forumTopics` responses land in the requesting chat's
    /// topic cache; `ordered_forum_topics` sorts by order descending.
    #[test]
    fn forum_topics_response_is_cached_per_chat() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        );
        let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
        let json = format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":2,"name":"Random","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":false,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":null,"order":"100","is_pinned":false,"unread_count":5,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}},{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":1,"name":"General","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":5}},"is_general":true,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":null,"order":"900","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0
        );
        apply_json(&mut session, &seq, &sink, &json);
        let ordered = session.ordered_forum_topics(ChatId(16));
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].forum_topic_id, 1); // order 900 first
        assert_eq!(ordered[1].forum_topic_id, 2);
        assert_eq!(ordered[1].unread_count, 5);
        // A response for a different purpose must not populate the cache.
        let extra2 = session.request(RequestPurpose::GetHistory, Some(ChatId(16)));
        let json2 = json.replace(
            &format!("\"@extra\":\"{}\"", extra.0),
            &format!("\"@extra\":\"{}\"", extra2.0),
        );
        session.forum_topics.clear();
        apply_json(&mut session, &seq, &sink, &json2);
        assert!(!session.forum_topics.contains_key(&16));
    }

    /// Phase 5.1: topic selection state — selecting sets `open_topic`,
    /// deselecting clears it, switching chats resets it.
    #[test]
    fn topic_selection_state() {
        let (mut session, _sink) = session();
        session.open_chat(ChatId(16));
        assert_eq!(session.open_topic, None);
        session.select_topic(ChatId(16), 2);
        assert_eq!(session.open_topic, Some(2));
        session.deselect_topic();
        assert_eq!(session.open_topic, None);
        session.select_topic(ChatId(16), 2);
        session.open_chat(ChatId(17));
        assert_eq!(session.open_topic, None);
    }

    /// Phase 5.1: replay — a `foundChatMessages` answer to
    /// `GetTopicHistory` populates the topic history keyed by
    /// `(chat_id, forum_topic_id)` and pages via `next_from_message_id`.
    #[test]
    fn topic_history_response_is_stored_per_topic() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_TOPIC_page1","entities":[]}}}}}},{{"id":40,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        let history = session.topic_histories.get(&(16, 2)).unwrap();
        assert_eq!(history.messages.len(), 2);
        assert_eq!(history.next_from_message_id, MessageId(40));
        assert!(!history.loaded_complete);
        // A response for another topic does not mix in.
        let extra_other =
            session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 3);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                extra_other.0
            ),
        );
        assert_eq!(
            session
                .topic_histories
                .get(&(16, 2))
                .unwrap()
                .messages
                .len(),
            2
        );
        let other = session.topic_histories.get(&(16, 3)).unwrap();
        assert!(other.loaded_complete);
        assert!(other.messages.is_empty());
        // next_from_message_id 0 completes the first topic's history.
        let extra2 =
            session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":30,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"oldest","entities":[]}}}}}}]}}"#,
                extra2.0
            ),
        );
        let history = session.topic_histories.get(&(16, 2)).unwrap();
        assert!(history.loaded_complete);
        assert_eq!(history.messages.len(), 3);
    }

    /// Parity slice 4: replay — an `updateNewMessage` carrying
    /// `topic_id = messageTopicForum` lands in the loaded topic's history
    /// (and still in the chat's main history). A topic with no loaded
    /// history gets no entry — the paging cursor stays fetch-owned.
    #[test]
    fn topic_message_update_lands_in_loaded_topic_history() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(16));
        let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"foundChatMessages\",\"@extra\":\"{}\",{}}}",
                extra.0,
                r#""total_count":1,"next_from_message_id":0,"messages":[{"id":50,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"seed","entities":[]}}}]"#
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":51,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_live","entities":[]}}}}"#,
        );
        let topic = session.topic_histories.get(&(16, 2)).unwrap();
        assert!(topic.messages.values().any(|m| m.id == MessageId(51)));
        // Still in the main history (unchanged behavior).
        assert!(session.histories.get(&16).unwrap().contains(MessageId(51)));
        // Unloaded topic: no entry is created.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":52,"chat_id":16,"is_outgoing":false,"topic_id":{"@type":"messageTopicForum","forum_topic_id":9},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"unloaded","entities":[]}}}}"#,
        );
        assert!(!session.topic_histories.contains_key(&(16, 9)));
        assert!(session.histories.get(&16).unwrap().contains(MessageId(52)));
        // A message with no topic stays a plain chat message.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":53,"chat_id":16,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"plain","entities":[]}}}}"#,
        );
        assert!(session.histories.get(&16).unwrap().contains(MessageId(53)));
    }

    /// Parity slice 4: replay — a topic send's pending row (from the
    /// `sendMessage` response) resolves in the topic history on
    /// `updateMessageSendSucceeded`, mirroring the main history.
    #[test]
    fn topic_send_succeeded_replaces_pending_row_in_topic_history() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(16));
        let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"foundChatMessages\",\"@extra\":\"{}\",{}}}",
                extra.0, r#""total_count":0,"next_from_message_id":0,"messages":[]"#
            ),
        );
        // The `sendMessage` response: pending outgoing message with a
        // temporary (negative) id and the forum topic attached.
        let send_extra = session.request(RequestPurpose::SendMessage, Some(ChatId(16)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"message\",\"@extra\":\"{}\",{}}}",
                send_extra.0,
                r#""id":-1,"chat_id":16,"is_outgoing":true,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_send","entities":[]}}"#
            ),
        );
        assert!(
            session
                .topic_histories
                .get(&(16, 2))
                .unwrap()
                .messages
                .contains_key(&-1)
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageSendSucceeded","message":{"id":60,"chat_id":16,"is_outgoing":true,"topic_id":{"@type":"messageTopicForum","forum_topic_id":2},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_TOPIC_send","entities":[]}}},"old_message_id":-1}"#,
        );
        let topic = session.topic_histories.get(&(16, 2)).unwrap();
        assert!(!topic.messages.contains_key(&-1));
        assert!(topic.messages.contains_key(&60));
    }

    /// Parity slice 4: replay — `chat.permissions.can_send_basic_messages`
    /// (schema 1.8.67, line 1070) feeds the topic-composer gate, and
    /// `updateChatPermissions` (line 10500) refreshes it.
    #[test]
    fn chat_permissions_gate_topic_composer() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let permissions = r#""permissions":{"@type":"chatPermissions","can_send_basic_messages":false,"can_send_audios":true,"can_send_documents":true,"can_send_photos":true,"can_send_videos":true,"can_send_video_notes":true,"can_send_voice_notes":true,"can_send_polls":true,"can_send_other_messages":true,"can_add_link_previews":true,"can_react_to_messages":true,"can_edit_tag":false,"can_change_info":false,"can_invite_users":true,"can_pin_messages":false,"can_create_topics":false}"#;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"updateNewChat\",\"chat\":{{\"id\":16,\"title\":\"Demo forum\",\"type\":{{\"@type\":\"chatTypeSupergroup\",\"supergroup_id\":16,\"is_channel\":false}},{permissions},\"unread_count\":0}}}}",
                permissions = permissions
            ),
        );
        assert!(!session.chats.get(&16).unwrap().can_send_basic_messages);
        let permissions_on = permissions.replace(
            "\"can_send_basic_messages\":false",
            "\"can_send_basic_messages\":true",
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                "{{\"@type\":\"updateChatPermissions\",\"chat_id\":16,{}}}",
                permissions_on
            ),
        );
        assert!(session.chats.get(&16).unwrap().can_send_basic_messages);
    }

    // Phase 6: `updateUser` upserts the user directory (contacts list /
    // info panel source of names + status).
    #[test]
    fn update_user_populates_user_directory() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"+15550131","status":{"@type":"userStatusOnline","expires":1},"type":{"@type":"userTypeRegular"},"is_contact":true}}"#,
        );
        let user = session.user(31).expect("user cached");
        assert_eq!(user.display_name(), "Ada Lovelace");
        assert!(user.is_contact);
        assert!(!user.is_bot);
        assert!(user.status.is_online());
    }

    // Phase 6: `updateUserStatus` refreshes the cached status.
    #[test]
    fn update_user_status_refreshes_cached_status() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#,
        );
        assert!(session.user(31).unwrap().status.is_online());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUserStatus","user_id":31,"status":{"@type":"userStatusLastWeek","by_my_privacy_settings":false}}"#,
        );
        let user = session.user(31).unwrap();
        assert!(!user.status.is_online());
        assert_eq!(user.status.display(), "last seen within a week");
    }

    // Phase 6: `getContacts` → `users` lands the id list only when it
    // answers our own fetch; `contact_rows` sorts by name and skips users
    // not yet seen via `updateUser`.
    #[test]
    fn get_contacts_accepts_matching_response() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        assert!(!session.contacts_settled());
        let extra = session.request(RequestPurpose::GetContacts, None);
        // A stray `users` payload without our `@extra` is ignored.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"users","total_count":1,"user_ids":[99]}"#,
        );
        assert!(!session.contacts_settled());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUser","user":{"id":32,"first_name":"Zed","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusRecently","by_my_privacy_settings":false}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":1}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"users","@extra":"{}","total_count":3,"user_ids":[31,32,33]}}"#,
                extra.0
            ),
        );
        assert!(session.contacts_settled());
        assert_eq!(session.contacts.as_deref(), Some([31, 32, 33].as_slice()));
        // User 33 never arrived via `updateUser` → no row yet.
        let rows = session.contact_rows();
        assert_eq!(rows.len(), 2);
        // Sorted by name: Ada before Zed, regardless of server order.
        assert_eq!(rows[0].user_id, 31);
        assert_eq!(rows[0].name, "Ada");
        assert!(rows[0].is_online);
        assert_eq!(rows[1].user_id, 32);
        assert_eq!(rows[1].status_text, "last seen recently");
        assert!(!rows[1].is_online);
    }

    // Phase 6: a failed `getContacts` marks `contacts_error` so the tab can
    // offer a retry instead of a stuck spinner.
    #[test]
    fn get_contacts_error_surfaces_retry() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetContacts, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
                extra.0
            ),
        );
        assert!(session.contacts_error);
        assert!(session.contacts_settled());
    }

    // Phase 6: `userFullInfo` bio lands on the right user both for a
    // user-scoped fetch (contacts panel) and a chat-scoped fetch (private
    // chat header).
    #[test]
    fn user_full_info_bio_resolves_user() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // User-scoped fetch (no chat).
        let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"CANARY bio","entities":[]}},"bot_info":null}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.user_full_info(31).map(|i| i.bio.as_str()),
            Some("CANARY bio")
        );
        // Chat-scoped fetch for a private chat.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":32},"unread_count":0}}"#,
        );
        let chat_extra = session.request(RequestPurpose::GetUserFullInfo, Some(ChatId(41)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"chat bio","entities":[]}},"bot_info":null}}"#,
                chat_extra.0
            ),
        );
        assert_eq!(
            session.user_full_info(32).map(|i| i.bio.as_str()),
            Some("chat bio")
        );
        // `updateUserFullInfo` refreshes the same cache.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUserFullInfo","user_id":31,"user_full_info":{"@type":"userFullInfo","bio":{"@type":"formattedText","text":"refreshed","entities":[]},"bot_info":null}}"#,
        );
        assert_eq!(
            session.user_full_info(31).map(|i| i.bio.as_str()),
            Some("refreshed")
        );
    }

    // Phase 6: `getSupergroupFullInfo` → `supergroupFullInfo` correlates via
    // the pending request's `supergroup_id` (the response has no id).
    #[test]
    fn supergroup_full_info_resolves_supergroup() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, 77);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"CANARY desc","member_count":4321}}"#,
                extra.0
            ),
        );
        let info = session.supergroup_full_info(77).expect("cached");
        assert_eq!(info.description, "CANARY desc");
        assert_eq!(info.member_count, 4321);
        assert!(session.supergroup_full_info(78).is_none());
    }

    // Phase 6: `addContact` ok invalidates the contacts list for refetch.
    #[test]
    fn add_contact_ok_invalidates_contacts() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let list_extra = session.request(RequestPurpose::GetContacts, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"users","@extra":"{}","total_count":0,"user_ids":[]}}"#,
                list_extra.0
            ),
        );
        assert!(session.contacts.is_some());
        let add_extra = session.request_for_user(RequestPurpose::AddContact, 55);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, add_extra.0),
        );
        assert!(session.contacts.is_none());
    }

    // Phase 6: info-panel open/close state.
    #[test]
    fn info_panel_open_close() {
        let (mut session, _sink) = session();
        assert!(session.open_info_panel.is_none());
        session.open_info_panel = Some(InfoPanelTarget::User(31));
        assert_eq!(session.open_info_panel, Some(InfoPanelTarget::User(31)));
        session.open_info_panel = Some(InfoPanelTarget::Supergroup(77));
        assert_eq!(
            session.open_info_panel,
            Some(InfoPanelTarget::Supergroup(77))
        );
        session.open_info_panel = None;
        assert!(session.open_info_panel.is_none());
    }

    // Phase 7.1: `updateChatFolders` replaces the folder list.
    #[test]
    fn chat_folders_update_replaces_list() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":3,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#,
        );
        assert_eq!(session.chat_folders.len(), 1);
        assert_eq!(session.folder_name(3), Some("Work"));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatFolders","chat_folders":[],"main_chat_list_position":0,"are_tags_enabled":false}"#,
        );
        assert!(session.chat_folders.is_empty());
        assert_eq!(session.folder_name(3), None);
    }

    // Phase 7.1: folder positions track membership and sort order, and a
    // full positions set drops stale folder ids.
    #[test]
    fn folder_position_membership_and_order() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        for (chat_id, title, order) in [(5, "alpha", "60"), (6, "beta", "50")] {
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0}}}}"#
                ),
            );
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListFolder","chat_folder_id":3}},"order":"{order}","is_pinned":false}}}}"#
                ),
            );
        }
        let folders = session.ordered_folder_chats(3);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].id.0, 5);
        assert_eq!(folders[1].id.0, 6);
        // Full positions set without the folder evicts it.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":5,"last_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"6","is_pinned":false}]}"#,
        );
        assert!(session.ordered_folder_chats(3).iter().all(|c| c.id.0 != 5));
        assert_eq!(session.ordered_folder_chats(3).len(), 1);
    }

    // Phase 7.1: order 0 removes folder membership; add/remove-from-list
    // track membership even before the position arrives.
    #[test]
    fn folder_membership_add_remove() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":5,"title":"alpha","type":{"@type":"chatTypePrivate","user_id":5},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAddedToList","chat_id":5,"chat_list":{"@type":"chatListFolder","chat_folder_id":3}}"#,
        );
        assert!(session.ordered_folder_chats(3).iter().any(|c| c.id.0 == 5));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":3},"order":"0","is_pinned":false}}"#,
        );
        assert!(session.ordered_folder_chats(3).is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListFolder","chat_folder_id":3},"order":"9","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatRemovedFromList","chat_id":5,"chat_list":{"@type":"chatListFolder","chat_folder_id":3}}"#,
        );
        assert!(session.ordered_folder_chats(3).is_empty());
    }

    // Parity slice: a `chats` response to `GetChatFolderChatsToLeave` is
    // cached per folder id for the delete-confirm dialog; a `chats`
    // response for another purpose must not touch the cache.
    #[test]
    fn folder_chats_to_leave_cached_per_folder() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_folder(RequestPurpose::GetChatFolderChatsToLeave, 3);
        let json = format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[5,6]}}"#,
            extra.0
        );
        apply_json(&mut session, &seq, &sink, &json);
        assert_eq!(session.folder_chats_to_leave.get(&3), Some(&vec![5, 6]));
        // Same payload shape, different purpose: cache untouched.
        let extra2 = session.request(RequestPurpose::SearchChats, None);
        let json2 = json.replace(
            &format!("\"@extra\":\"{}\"", extra.0),
            &format!("\"@extra\":\"{}\"", extra2.0),
        );
        session.folder_chats_to_leave.clear();
        apply_json(&mut session, &seq, &sink, &json2);
        assert!(!session.folder_chats_to_leave.contains_key(&3));
    }

    // Phase 7.2: `searchPublicChats` results land in `public_chat_ids` and
    // the search status waits for all three requests.
    #[test]
    fn public_search_results_accepted() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_search();
        let search_gen = session.search.begin_query("quill");
        let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[42]}}"#,
                public_extra.0
            ),
        );
        assert_eq!(session.search.public_chat_ids, vec![ChatId(42)]);
        // Still waiting on `searchChats` / `searchMessages` → still Searching.
        assert_eq!(session.search.status, SearchStatus::Searching);
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                chats_extra.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"messages":[],"next_offset":""}}"#,
                messages_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Ready);
    }

    // Phase 7.2: an errored public search does not strand the query in
    // `Searching` — the status resolves once every request settles.
    #[test]
    fn public_search_error_resolves_status() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_search();
        let search_gen = session.search.begin_query("zzz");
        let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"QUERY_TOO_SHORT"}}"#,
                public_extra.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                chats_extra.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"messages":[],"next_offset":""}}"#,
                messages_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Failed);
        assert!(session.search.public_chat_ids.is_empty());
    }

    fn tray_json(chat_id: i64, list: &str, order: i64, max_read: i32, story_ids: &[i32]) -> String {
        let stories = story_ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"storyInfo","story_id":{id},"date":1,"is_for_close_friends":false,"is_live":false}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat_id},"list":{list},"order":"{order}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{stories}]}}}}"#
        )
    }

    #[test]
    fn story_tray_keeps_main_entries_sorted_by_order() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &tray_json(11, r#"{"@type":"storyListMain"}"#, 10, 0, &[5]),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &tray_json(12, r#"{"@type":"storyListMain"}"#, 30, 5, &[6]),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &tray_json(13, r#"{"@type":"storyListArchive"}"#, 50, 0, &[7]),
        );
        let tray = session.ordered_story_tray();
        // Archived entries drop out of the tray.
        assert_eq!(tray.len(), 2);
        // Sorted by (order, chat_id) descending (schema line 6781).
        assert_eq!(tray[0].chat_id, 12);
        assert_eq!(tray[1].chat_id, 11);
        assert!(tray[0].has_unread());
        assert!(tray[1].has_unread());
    }

    #[test]
    fn story_tray_update_replaces_and_hides_entries() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &tray_json(11, r#"{"@type":"storyListMain"}"#, 10, 0, &[5]),
        );
        assert_eq!(session.ordered_story_tray().len(), 1);
        // Later update moves the chat to the archive list → tray hides it.
        apply_json(
            &mut session,
            &seq,
            &sink,
            &tray_json(11, r#"{"@type":"storyListArchive"}"#, 10, 5, &[]),
        );
        assert!(session.ordered_story_tray().is_empty());
        assert!(!session.story_tray.contains_key(&11));
    }

    #[test]
    fn update_story_deleted_removes_cache_and_tray() {
        // Phase 9.2: `updateStoryDeleted` (schema 1.8.67 line 10898).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &tray_json(11, r#"{"@type":"storyListMain"}"#, 10, 0, &[5]),
        );
        session.stories.insert(
            (11, 5),
            crate::telegram::envelope::ParsedStory {
                id: 5,
                poster_chat_id: 11,
                date: 1,
                content: crate::telegram::envelope::StoryContentView::Unsupported,
                caption: String::new(),
                caption_entities: Vec::new(),
                chosen_reaction_emoji: None,
                interaction_info: None,
                can_be_deleted: false,
                can_be_replied: false,
                can_get_interactions: false,
            },
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateStoryDeleted","story_poster_chat_id":11,"story_id":5}"#,
        );
        assert!(!session.stories.contains_key(&(11, 5)));
        // Tray no longer references the deleted story; without an unread
        // story left, the entry is dropped.
        assert!(session.ordered_story_tray().is_empty());
    }

    #[test]
    fn update_story_post_succeeded_upserts_and_queues_tray_refresh() {
        // Phase 9.2: `updateStoryPostSucceeded` (schema 1.8.67 line 10901).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateStoryPostSucceeded","story":{"@type":"story","id":9,"poster_chat_id":11,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"old_story_id":8}"#,
        );
        let story = session.stories.get(&(11, 9)).expect("story cached");
        assert_eq!(story.poster_chat_id, 11);
        // The driver's `tick` drains this into a `getChatActiveStories`
        // refresh for the poster's tray entry.
        assert!(session.story_tray_refresh.contains(&11));
    }

    #[test]
    fn get_story_response_lands_in_story_cache() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(11), 5);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"story","@extra":"{}","id":5,"poster_chat_id":11,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"CANARY_STORY","entities":[]}}}}"#,
                extra.0
            ),
        );
        let story = session.stories.get(&(11, 5)).expect("story cached");
        assert_eq!(story.caption, "CANARY_STORY");
        assert!(matches!(
            story.content,
            crate::telegram::envelope::StoryContentView::Unsupported
        ));
    }

    #[test]
    fn get_story_dedupes_in_flight_per_story() {
        let (mut session, _sink) = session();
        let extra1 = session.request_for_story(RequestPurpose::GetStory, ChatId(11), 5);
        assert!(
            session
                .requests
                .has_purpose_for_story(RequestPurpose::GetStory, ChatId(11), 5)
        );
        // Same chat, different story → not suppressed.
        assert!(
            !session
                .requests
                .has_purpose_for_story(RequestPurpose::GetStory, ChatId(11), 6)
        );
        session.requests.take(extra1);
        assert!(
            !session
                .requests
                .has_purpose_for_story(RequestPurpose::GetStory, ChatId(11), 5)
        );
    }

    // Phase A1: slow-mode gate (`Session::slow_mode_wait_secs`).
    // `fetched_at_ms` is stamped from the real clock at apply time, so the
    // helper reads it back and tests pass explicit `now_ms` — fully
    // deterministic, no sleeps.
    #[allow(clippy::too_many_arguments)]
    fn seed_slow_mode_group(
        session: &mut Session,
        seq: &AtomicU64,
        sink: &Arc<MemorySink>,
        chat_id: i64,
        status_json: Option<&str>,
        full_info_fields: &str,
        is_channel: bool,
    ) -> u64 {
        apply_json(
            session,
            seq,
            sink,
            &format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Slow group","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":{is_channel}}},"unread_count":0}}}}"#
            ),
        );
        if let Some(status) = status_json {
            apply_json(
                session,
                seq,
                sink,
                &format!(
                    r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{chat_id},"is_forum":false,"status":{status}}}}}"#
                ),
            );
        }
        let extra = session.request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, chat_id);
        apply_json(
            session,
            seq,
            sink,
            &format!(
                r#"{{"@type":"supergroupFullInfo","@extra":"{}","description":"d","member_count":10,{}}}"#,
                extra.0, full_info_fields
            ),
        );
        session.supergroup_full_infos[&chat_id].fetched_at_ms
    }

    const SLOW_MODE_FIELDS: &str = r#""slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":0,"unrestrict_boost_count":0"#;
    const MEMBER_STATUS: &str = r#"{"@type":"chatMemberStatusMember"}"#;

    #[test]
    fn slow_mode_member_wait_countdown_and_expiry() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetched = seed_slow_mode_group(
            &mut session,
            &seq,
            &sink,
            17,
            Some(MEMBER_STATUS),
            SLOW_MODE_FIELDS,
            false,
        );
        let chat = ChatId(17);
        // 3s elapsed → ceil(22.0) = 22.
        assert_eq!(session.slow_mode_wait_secs(chat, fetched + 3_000), Some(22));
        // Countdown rounds up: 24.4s remaining → 25.
        assert_eq!(session.slow_mode_wait_secs(chat, fetched + 600), Some(25));
        // Expiry boundary: 25.0s elapsed → remaining 0 → free to send.
        assert_eq!(session.slow_mode_wait_secs(chat, fetched + 25_000), None);
        assert_eq!(session.slow_mode_wait_secs(chat, fetched + 60_000), None);
    }

    #[test]
    fn slow_mode_creator_and_admin_bypass() {
        for (status, label) in [
            (r#"{"@type":"chatMemberStatusCreator"}"#, "creator"),
            (
                r#"{"@type":"chatMemberStatusAdministrator"}"#,
                "administrator",
            ),
        ] {
            let (mut session, sink) = session();
            let seq = AtomicU64::new(0);
            let fetched = seed_slow_mode_group(
                &mut session,
                &seq,
                &sink,
                18,
                Some(status),
                SLOW_MODE_FIELDS,
                false,
            );
            assert_eq!(
                session.slow_mode_wait_secs(ChatId(18), fetched + 1_000),
                None,
                "{label} bypasses slow mode"
            );
        }
    }

    #[test]
    fn slow_mode_boost_bypass() {
        // `my_boost_count >= unrestrict_boost_count > 0` → exempt.
        let (mut session_a, sink_a) = session();
        let seq_a = AtomicU64::new(0);
        let fetched = seed_slow_mode_group(
            &mut session_a,
            &seq_a,
            &sink_a,
            19,
            Some(MEMBER_STATUS),
            r#""slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":5,"unrestrict_boost_count":5"#,
            false,
        );
        assert_eq!(
            session_a.slow_mode_wait_secs(ChatId(19), fetched + 1_000),
            None
        );

        // `unrestrict_boost_count` 0 = unspecified → still gated even with boosts.
        let (mut session2, sink2) = session();
        let seq2 = AtomicU64::new(0);
        let fetched = seed_slow_mode_group(
            &mut session2,
            &seq2,
            &sink2,
            20,
            Some(MEMBER_STATUS),
            r#""slow_mode_delay":30,"slow_mode_delay_expires_in":25.0,"my_boost_count":5,"unrestrict_boost_count":0"#,
            false,
        );
        assert_eq!(
            session2.slow_mode_wait_secs(ChatId(20), fetched + 1_000),
            Some(24)
        );
    }

    #[test]
    fn slow_mode_channel_and_zero_delay_are_ungated() {
        // Broadcast channels ignore slow mode even when the fields are set.
        let (mut session_a, sink_a) = session();
        let seq_a = AtomicU64::new(0);
        let fetched = seed_slow_mode_group(
            &mut session_a,
            &seq_a,
            &sink_a,
            21,
            Some(MEMBER_STATUS),
            SLOW_MODE_FIELDS,
            true,
        );
        assert_eq!(
            session_a.slow_mode_wait_secs(ChatId(21), fetched + 1_000),
            None
        );

        // `slow_mode_delay` 0 = slow mode off.
        let (mut session2, sink2) = session();
        let seq2 = AtomicU64::new(0);
        let fetched = seed_slow_mode_group(
            &mut session2,
            &seq2,
            &sink2,
            22,
            Some(MEMBER_STATUS),
            r#""slow_mode_delay":0,"slow_mode_delay_expires_in":0.0,"my_boost_count":0,"unrestrict_boost_count":0"#,
            false,
        );
        assert_eq!(
            session2.slow_mode_wait_secs(ChatId(22), fetched + 1_000),
            None
        );
    }

    #[test]
    fn slow_mode_unknown_status_is_conservatively_gated() {
        // No `updateSupergroup` seen yet → no bypass, the gate applies.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetched =
            seed_slow_mode_group(&mut session, &seq, &sink, 23, None, SLOW_MODE_FIELDS, false);
        assert_eq!(
            session.slow_mode_wait_secs(ChatId(23), fetched + 1_000),
            Some(24)
        );
    }

    #[test]
    fn slow_mode_restrict_right_tracks_admin_rights() {
        // Phase A1: `setChatSlowModeDelay` requires `can_restrict_members`
        // (schema 1.8.67, line 13551); the reducer records it per
        // supergroup from own `chatMemberStatusAdministrator.rights`.
        let (mut session_a, sink_a) = session();
        let seq_a = AtomicU64::new(0);
        let with_right = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":true}}"#;
        seed_slow_mode_group(
            &mut session_a,
            &seq_a,
            &sink_a,
            30,
            Some(with_right),
            SLOW_MODE_FIELDS,
            false,
        );
        assert_eq!(
            session_a.supergroup_own_status(30),
            Some(ChannelMemberStatus::Administrator)
        );
        assert!(session_a.supergroup_can_restrict_members(30));

        let (mut session_b, sink_b) = session();
        let seq_b = AtomicU64::new(0);
        let without_right = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":false}}"#;
        seed_slow_mode_group(
            &mut session_b,
            &seq_b,
            &sink_b,
            31,
            Some(without_right),
            SLOW_MODE_FIELDS,
            false,
        );
        assert!(!session_b.supergroup_can_restrict_members(31));
        // Unknown supergroup → treated as lacking the right.
        assert!(!session_b.supergroup_can_restrict_members(999));
    }

    #[test]
    fn slow_mode_ungated_without_full_info() {
        // No cached full info → no gate (can't know the delay).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":24,"title":"Slow group","type":{"@type":"chatTypeSupergroup","supergroup_id":24,"is_channel":false},"unread_count":0}}"#,
        );
        assert_eq!(session.slow_mode_wait_secs(ChatId(24), unix_ms_now()), None);
    }
}
