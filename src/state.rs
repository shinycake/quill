use crate::auth::{AuthView, view_for};
use crate::calls::engine::{RemoteVideoState, TransportState};
use crate::composer::{CommandMenuItem, merge_command_menu_items};
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::{
    AccountGeneration, AccountKey, ChatId, FileId, MessageId, RequestId, ViewGeneration,
};
use crate::notify::{self, OsNotification, QueuedNotification};
use crate::settings::{
    AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_GIF, AUTO_DOWNLOAD_MAX_BYTES, AUTO_DOWNLOAD_MUSIC,
    AUTO_DOWNLOAD_PHOTO, AUTO_DOWNLOAD_VIDEO, AUTO_DOWNLOAD_VIDEO_NOTE, AUTO_DOWNLOAD_VOICE,
    CallPrefs, ContactPrefs, MediaPrefs,
};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AnimationItem, AuthorizationState, BotCommand, BotInfo, CallbackQueryAnswer,
    CanPostStoryResult, ChannelMemberStatus, ChatAction, ChatActiveStoriesView, ChatAdminRights,
    ChatAdministratorEntry, ChatDraft, ChatFolderInfo, ChatFolderSpec, ChatJoinResult, ChatKind,
    ChatList, ChatNotificationSettings, ChatPermissions, ChatPositionUpdate, ChatStatistics,
    ConnectionState, EnvelopePayload, EphemeralMessageContent, ErrorClass, ForumTopic,
    InlineQueryResultSummary, InlineQueryResultsButton, InviteGroupCallParticipantResult,
    LinkPreview, LoginUrlInfo, MessageAutoDelete, MessageContent, MessageForwardInfo,
    MessageInteractionInfo, MessageOrigin, MessageReaction, MessageReplyTo, MessageSelfDestruct,
    MessageSender, NotificationSettingsScope, NotificationSound, OptionValue, ParsedCall,
    ParsedChatEvent, ParsedChatInviteLink, ParsedChatJoinRequest, ParsedChatMember, ParsedFile,
    ParsedGroupCall, ParsedGroupCallMessage, ParsedGroupCallParticipant, ParsedMessage,
    ParsedSecretChat, ParsedSession, ParsedStory, ParsedUser, ParsedVideoChat, ParsedWebsite,
    ParsedWelcomeMessage, PasswordState, PaymentFormData, PaymentReceiptData, Poll, ReplyKeyboard,
    ReplyMarkup, ReportChatOutcome, ReportOption, ReportSponsoredResult, ReportStoryResult,
    RichMessageContent, ScopeNotificationSettings, SecretChatState, SponsoredMessage,
    StickerFormat, StickerItem, StickerSetInfo, StorageStats, StoryAvailableReactionView,
    StoryInteractionView, StoryInteractionsView, StoryListView, TdError, UsernameCheckResult,
    ValidatedOrderInfoData, effective_content, reply_markup_demands_reply,
};
use crate::telegram::envelope::{CallState, ReadyParams};
use crate::telegram::requests::{
    ArchiveChatListSettings, CallPrivacySetting, ChatEventLogFilterSet, PrivacyWho,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

/// Phase D3b: which admin-management operation a `setChatMemberStatus`
/// request performs. Correlated on the `SetChatMemberStatus` purpose so
/// the response handler knows how to refresh the admin list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatusChange {
    /// Member → administrator with a fresh rights block.
    Promote,
    /// Administrator → administrator with an edited rights block.
    EditRights,
    /// Administrator → plain member (`chatMemberStatusMember`).
    Demote,
    /// Member → restricted (`chatMemberStatusRestricted`, schema 1.8.67
    /// line 2510). Not supported in basic groups and channels.
    Restrict,
    /// Member → banned (`chatMemberStatusBanned`, schema 1.8.67 line
    /// 2517). Works in supergroups and channels.
    Ban,
    /// Restricted/banned member → plain member
    /// (`chatMemberStatusMember`).
    Unban,
}

/// Slice G1: which `getSupergroupMembers` filter backs one cached member
/// page. The `Search` page's query is tracked by the caller (promote
/// picker / member dialog) rather than the cache — one page per
/// (chat, filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberListFilter {
    /// `supergroupMembersFilterRecent` (schema 1.8.67, line 2559).
    Recent,
    /// `supergroupMembersFilterSearch` (line 2568).
    Search,
    /// `supergroupMembersFilterAdministrators` (line 2565).
    Administrators,
    /// `supergroupMembersFilterRestricted` (line 2571); admins only.
    Restricted,
    /// `supergroupMembersFilterBanned` (line 2574); admins only.
    Banned,
}

impl MemberListFilter {
    /// Slice G1: whether `getSupergroupMembers` with this filter requires
    /// the `can_restrict_members` administrator right (schema 1.8.67,
    /// lines 2570/2574: restricted/banned filters are admin-only).
    pub fn requires_restrict_right(self) -> bool {
        matches!(
            self,
            MemberListFilter::Restricted | MemberListFilter::Banned
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPurpose {
    GetAuthorizationState,
    SetParameters,
    SetPhoneNumber,
    CheckAuthenticationCode,
    CheckAuthenticationPassword,
    /// Slice A1: `resendAuthenticationCode` from the code-entry screen.
    ResendAuthenticationCode,
    /// Slice A1: `requestQrCodeAuthentication` from the phone screen.
    RequestQrCodeAuthentication,
    LoadChats,
    /// Phase 7.1: single-shot `loadChats(chatListFolder(id))` when a folder
    /// tab is selected. Separate from `LoadChats` so the ok-response does
    /// not re-trigger main-list paging.
    LoadFolderChats,
    GetHistory,
    /// Slice CL: one-shot `getChatHistory` for the chat-list peek preview
    /// (`parity:chatlist-chat-preview`). The `messages` answer lands in
    /// `Session::chat_preview_fetch` — it must NOT merge into the open
    /// chat's history (the `GetHistory` branch drops answers for non-open
    /// chats, and the preview never calls `openChat`).
    GetChatPreview,
    /// Any `sendMessage` (text / photo / document). Response `message` is pending.
    SendMessage,
    /// M2: `getFullRichMessage`. Response `richMessage` replaces the
    /// partial blocks of the history message.
    GetFullRichMessage {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `sendMessageAlbum`. Response `messages` are pending until send-succeeded.
    SendMessageAlbum,
    OpenChat,
    CloseChat,
    ViewMessages,
    DownloadFile,
    /// MED3: `cancelDownloadFile`. Response is `Ok`.
    CancelDownloadFile,
    SearchChats,
    SearchMessages,
    SearchRecentlyFoundChats,
    /// Phase 7.2: `searchPublicChats` — public username/title lookup across
    /// all public chats (not just known ones). Sent alongside `searchChats`.
    SearchPublicChats,
    AddRecentlyFoundChat,
    SearchChatMessages,
    /// Slice media-shared-gallery: one `searchChatMessages` page for a
    /// gallery tab. `generation` is the `SharedMediaState` generation at
    /// send time — late answers drop on mismatch.
    GetSharedMedia {
        tab: SharedMediaTab,
        generation: u64,
    },
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
    /// B4: `getPollVoters` (schema 1.8.67 line 12941). Response is
    /// `pollVoters`; one page per (chat, message, option) cached in
    /// `Session::poll_voters`, keyed with `offset` for append-merging.
    GetPollVoters {
        chat_id: ChatId,
        message_id: MessageId,
        option_id: i32,
        offset: i32,
    },
    /// Bots slice: `getInlineQueryResults` (schema 1.8.67, line 13019).
    /// Response is `inlineQueryResults`; the single active fetch lives in
    /// `Session::inline_query` (the composer has one active query).
    GetInlineQueryResults {
        chat_id: ChatId,
        bot_user_id: i64,
        first_page: bool,
    },
    /// B4: `stopPoll` (schema 1.8.67 line 12953). Response is `ok`; the
    /// poll closes via `updatePoll`.
    StopPoll,
    /// `unpinChatMessage`. Response is `ok`; pin via `updateMessageIsPinned`.
    UnpinChatMessage,
    /// M1: `unpinAllChatMessages`. Response is `ok`; pins clear via
    /// `updateChatPinnedMessages`.
    UnpinAllChatMessages,
    /// M1: `getMessageLink`. Response is `messageLink`; the parsed link is
    /// stored in `Session::message_link_result` for the UI to copy.
    GetMessageLink,
    /// M1 fix-up: `getMessageProperties`, sent first by "Share link" so
    /// the driver can gate `getMessageLink` on
    /// `messageProperties.can_get_link` (schema 1.8.67 line 12056).
    GetMessageLinkProperties {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// MED4: `getWebPageInstantView` (TDLib 1.8.67, `schema/td_api.tl:14794`).
    /// The URL rides `Session::instant_view_urls` keyed by `RequestId`
    /// (the purpose stays `Copy`). Success lands in
    /// `Session::instant_view`; the 404 error falls back to the browser
    /// via `Session::instant_view_fallback_url` — a refusal is never
    /// shown as success.
    GetWebPageInstantView,
    /// MED4b: `getLinkPreview` (TDLib 1.8.67, `schema/td_api.tl:14792`) —
    /// the composer prefetch. Unit variant to keep the enum `Copy`; the
    /// URL rides `Session::composer_preview_urls`, the result lands in
    /// `Session::composer_preview`.
    GetLinkPreview,
    /// M1: `resendMessages`. Response is `messages` (the retried sends).
    ResendMessages,
    /// M1: `getChatScheduledMessages`. Response is `messages`, stored in
    /// `Session::scheduled_messages` instead of merged into history.
    GetChatScheduledMessages,
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
    /// Slice A3: `getActiveSessions`. Response is `sessions`; the list is
    /// replaced from the authoritative answer (never optimistic).
    GetActiveSessions,
    /// Slice A3: `terminateSession`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    TerminateSession {
        session_id: i64,
    },
    /// Slice A3: `terminateAllOtherSessions`. Response is `ok`; the list
    /// is refetched from the authoritative answer (never optimistic).
    TerminateAllOtherSessions,
    /// Slice A4: `toggleSessionCanAcceptSecretChats`. Response is `ok`;
    /// the sessions list is refetched from the authoritative answer
    /// (never optimistic) — same as a terminate.
    ToggleSessionSecretChats {
        session_id: i64,
    },
    /// Slice A4: `toggleSessionCanAcceptCalls`. Response is `ok`; the
    /// sessions list is refetched from the authoritative answer (never
    /// optimistic).
    ToggleSessionCalls {
        session_id: i64,
    },
    /// Slice A4: `getConnectedWebsites`. Response is
    /// `connectedWebsites`; the list is replaced from the authoritative
    /// answer (never optimistic).
    GetConnectedWebsites,
    /// Slice A4: `disconnectWebsite`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    DisconnectWebsite {
        website_id: i64,
    },
    /// Slice A4: `disconnectAllWebsites`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    DisconnectAllWebsites,
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
    /// MED2: `recognizeSpeech`. Response is `ok`; the transcript arrives
    /// later via `updateMessageContent` on the message's
    /// `speech_recognition_result`.
    RecognizeSpeech,
    /// `getInstalledStickerSets` (`stickerTypeRegular`). Response is `stickerSets`.
    GetInstalledStickerSets,
    /// `getStickerSet`. Response is `stickerSet`.
    GetStickerSet,
    /// Slice S8: `getTrendingStickerSets` (regular). Response is
    /// `trendingStickerSets`.
    GetTrendingStickerSets,
    /// Slice S8: `viewTrendingStickerSets`. Response is `ok`.
    ViewTrendingStickerSets,
    /// Slice S8: `searchStickerSets` (regular). Response is `stickerSets`.
    SearchStickerSets,
    /// Slice S8: `searchStickers` (regular). Response is `stickers`.
    SearchStickers,
    /// Slice S8: `getFavoriteStickers`. Response is `stickers`.
    GetFavoriteStickers,
    /// Slice S8: `addFavoriteSticker`. Response is `ok`; the favorites
    /// cache is cleared so it refetches.
    AddFavoriteSticker,
    /// Slice S8: `removeFavoriteSticker`. Response is `ok`; same
    /// invalidation as add.
    RemoveFavoriteSticker,
    /// Slice S8: `getRecentStickers`. Response is `stickers`.
    GetRecentStickers,
    /// Slice S8: `clearRecentStickers`. Response is `ok`; the recent
    /// cache is cleared.
    ClearRecentStickers,
    /// Slice S8: `changeStickerSet` (install / archive / remove).
    /// Response is `ok`; the installed-sets cache is cleared so the
    /// panel refetches the authoritative list.
    ChangeStickerSet,
    /// Slice S8: `reorderInstalledStickerSets`. Response is `ok`; same
    /// installed-sets invalidation as change.
    ReorderInstalledStickerSets,
    /// `getSavedAnimations`. Response is `animations`.
    GetSavedAnimations,
    /// Slice S9: `getInlineQueryResults` against the animation search
    /// bot for the GIF panel search. The bot is resolved via
    /// `getOption("animation_search_bot_username")` + `searchPublicChat`
    /// (schema 1.8.67, lines 6483, 11063); the driver slice will carry the
    /// resolved id when it issues searches.
    /// ponytail: no query identity — two concurrent searches can race and
    /// a stale first page can clobber newer results; the UI slice must
    /// debounce/serialize searches.
    GetGifSearchResults {
        first_page: bool,
    },
    /// Slice S9: `addSavedAnimation` (schema 1.8.67, line 14769).
    /// Response is `ok`; the saved-GIF cache is cleared so it refetches.
    AddSavedAnimation,
    /// Slice S9: `removeSavedAnimation` (schema 1.8.67, line 14772).
    /// Response is `ok`; same invalidation as add.
    RemoveSavedAnimation,
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
    /// B1: `getCallbackQueryAnswer` with
    /// `callbackQueryPayloadDataWithPassword` (password-protected button).
    /// A 400 answer surfaces as "wrong 2-step password" instead of the
    /// generic callback note.
    GetCallbackQueryAnswerWithPassword,
    /// B1: `getCallbackQueryAnswer` with `callbackQueryPayloadGame` (game
    /// button); the answer URL (if any) opens the game in the OS browser.
    GetCallbackQueryAnswerGame,
    /// B1: `getLoginUrlInfo` for a login-URL button press. Response is
    /// `loginUrlInfo*`; on error the button degrades to a plain URL button
    /// (schema 1.8.67 doc on `getLoginUrl`).
    GetLoginUrlInfo,
    /// Slice P1: `getPaymentForm` after a Buy button press. Response is
    /// `paymentForm` (schema 1.8.67, line 15262).
    GetPaymentForm,
    /// Slice P1: `validateOrderInfo` after the order-info form validates.
    /// Response is `validatedOrderInfo` (schema 1.8.67, line 15268).
    ValidateOrderInfo,
    /// Slice P1: `sendPaymentForm` from the checkout dialog. Response is
    /// `paymentResult` (schema 1.8.67, line 15277).
    SendPaymentForm,
    /// Slice P1: `getPaymentReceipt` for an invoice's
    /// `receipt_message_id`. Response is `paymentReceipt` (schema 1.8.67,
    /// line 15280).
    GetPaymentReceipt,
    /// B1: `getLoginUrl` after the user consented to a
    /// `loginUrlInfoRequestConfirmation`. Response is `httpUrl`; on error
    /// the button degrades to a plain URL button (schema 1.8.67 doc on
    /// `getLoginUrl`).
    GetLoginUrl,
    /// B1: `deleteChatReplyMarkup` after a one-time custom keyboard is used
    /// (schema 1.8.67, line 13183).
    DeleteChatReplyMarkup,
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
    /// A5: `setName` (schema 1.8.67, line 14823). Response is `ok`; the
    /// new name arrives via `updateUser`.
    SetName,
    /// A5: `setBio` (schema 1.8.67, line 14826). Response is `ok`; the
    /// new bio arrives via `updateUserFullInfo`.
    SetBio,
    /// A5: `setUsername` (schema 1.8.67, line 14830). Response is `ok`;
    /// the new usernames arrive via `updateUser`.
    SetUsername,
    /// A5: `checkChatUsername` with the private chat with self (schema
    /// 1.8.67, line 11677) — the documented availability check for the
    /// current user's own username (TGX `EditUsernameController` sends it
    /// with `tdlib.selfChatId()`). Response is `checkChatUsernameResult*`.
    CheckUsername,
    /// A5: `reorderActiveUsernames` (schema 1.8.67, line 14838).
    /// Response is `ok`; the new order arrives via `updateUser`.
    ReorderActiveUsernames,
    /// A5: `toggleUsernameIsActive` (schema 1.8.67, line 14835).
    /// Response is `ok`; the new lists arrive via `updateUser`.
    ToggleUsernameIsActive,
    /// A5: `setProfilePhoto` (schema 1.8.67, line 14803). Response is
    /// `ok`; the new photo arrives via `updateUserFullInfo`.
    SetProfilePhoto,
    /// A5: `deleteProfilePhoto` (schema 1.8.67, line 14806). Response is
    /// `ok`; the removal arrives via `updateUserFullInfo`.
    DeleteProfilePhoto,
    /// Slice A6: `removeContacts`. Response is `ok`; the contacts list is
    /// invalidated for refetch (same as `AddContact`).
    RemoveContact,
    /// Slice A6: `importContacts`. Response is `importedContacts`; the
    /// contacts list is invalidated for refetch.
    ImportContacts,
    /// Slice A6: `clearImportedContacts`. Response is `ok`; the contacts
    /// list is invalidated for refetch.
    ClearImportedContacts,
    /// Phase 6: `getSupergroupFullInfo`. Response is `supergroupFullInfo`;
    /// correlated via `PendingRequest::supergroup_id`.
    GetSupergroupFullInfo,
    /// Phase D2: `getChatStatistics`. Response is
    /// `chatStatisticsChannel` / `chatStatisticsSupergroup`; correlated
    /// via `PendingRequest::chat_id` (the response carries no chat id).
    GetChatStatistics,
    /// Phase D3a: `getChatInviteLinks`. Response is `chatInviteLinks`;
    /// correlated via `PendingRequest::chat_id`.
    GetChatInviteLinks,
    /// Phase D3a: `createChatInviteLink`. Response is the created
    /// `chatInviteLink`; correlated via `PendingRequest::chat_id`.
    CreateChatInviteLink,
    /// Phase D3a: `editChatInviteLink`. Response is the updated
    /// `chatInviteLink`; correlated via `PendingRequest::chat_id`.
    EditChatInviteLink,
    /// Phase D3a: `revokeChatInviteLink`. Response is the updated
    /// `chatInviteLinks` list; correlated via `PendingRequest::chat_id`.
    RevokeChatInviteLink,
    /// Phase D3a: `getChatJoinRequests`. Response is `chatJoinRequests`;
    /// correlated via `PendingRequest::chat_id`.
    GetChatJoinRequests,
    /// Phase D3a: `processChatJoinRequest`. Response is `ok`; `user_id`
    /// identifies the join request that was approved/declined.
    ProcessChatJoinRequest {
        user_id: i64,
    },
    /// Phase D3b: `getChatAdministrators`. Response is
    /// `chatAdministrators`; correlated via `PendingRequest::chat_id`.
    GetChatAdministrators,
    /// Phase D3b: `setChatMemberStatus`. Response is `ok`; the member
    /// change itself arrives as `updateChatMember`. `user_id` + `kind`
    /// identify which admin-management operation was confirmed.
    SetChatMemberStatus {
        user_id: i64,
        kind: MemberStatusChange,
    },
    /// Phase D3b: `getChatMember` for one administrator's rights (edit
    /// dialog). Response is `chatMember`; `user_id` identifies the admin,
    /// correlated to the chat via `PendingRequest::chat_id`.
    GetAdminRights {
        user_id: i64,
    },
    /// Phase D3b / slice G1: `getSupergroupMembers` for the promote
    /// member picker and the member-management dialog. Response is
    /// `chatMembers`; correlated via `PendingRequest::chat_id`, cached
    /// per (`chat_id`, `filter`).
    GetSupergroupMembers {
        filter: MemberListFilter,
    },
    /// Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507).
    /// Response is `basicGroupFullInfo`; correlated via
    /// `PendingRequest::chat_id`.
    GetBasicGroupFullInfo,
    /// Phase D3c: `getChatEventLog`. Response is `chatEvents`; correlated
    /// via `PendingRequest::chat_id`. `from_event_id` is the paging
    /// cursor: 0 replaces the cached page, a nonzero id appends the
    /// older page to it.
    GetChatEventLog {
        from_event_id: i64,
    },
    /// Phase A1: `setChatSlowModeDelay`. Response is `ok`; the new delay
    /// arrives via `updateSupergroupFullInfo`.
    SetChatSlowModeDelay,
    /// Slice G1: `createNewBasicGroupChat` (schema 1.8.67, line 13327).
    /// Response is `createdBasicGroupChat`; the chat itself arrives as
    /// `updateNewChat`.
    CreateBasicGroup,
    /// Slice G1: `createNewSupergroupChat` (schema 1.8.67, line 13337).
    /// Response is the new `chat`; `updateNewChat` follows as well.
    CreateSupergroupChannel {
        is_channel: bool,
    },
    /// Slice G1: `toggleSupergroupIsBroadcastGroup` (schema 1.8.67, line
    /// 15221). Response is `ok`; `updateSupergroup` carries the new
    /// `is_broadcast_group`. One-way: supergroup → broadcast group.
    ToggleBroadcastGroup,
    /// Slice G1: `addChatMembers` (schema 1.8.67, line 13584). Response is
    /// `failedToAddMembers`; added members arrive as `updateChatMember`.
    /// The single bulk response replaces the failure count.
    AddChatMembers,
    /// Slice G1 fix-up: one `addChatMember` per user for basic groups
    /// (schema 1.8.67, line 13578 — also answers `failedToAddMembers`,
    /// 0 or 1 failures each). Per-user responses accumulate into the
    /// failure count instead of replacing it.
    AddChatMember,
    /// Slice G1: `setChatPermissions` (schema 1.8.67, line 13464).
    /// Response is `ok`; `updateChatPermissions` carries the new block.
    /// Applied optimistically by the driver at send time; a TDLib error
    /// restores the previous block via `PendingRequest::rollback`.
    SetChatPermissions,
    /// Slice G1: `replacePrimaryChatInviteLink` (schema 1.8.67, line
    /// 14089). Response is the new `chatInviteLink`; correlated via
    /// `PendingRequest::chat_id`.
    ReplacePrimaryChatInviteLink,
    /// Slice G1: `toggleSupergroupJoinByRequest` (schema 1.8.67, line
    /// 15188). Response is `ok`; `updateSupergroup` carries the new
    /// `join_by_request`. Applied optimistically by the driver at send
    /// time; a TDLib error restores the previous flag via
    /// `PendingRequest::rollback`.
    ToggleSupergroupJoinByRequest,
    /// Slice G1: `setSupergroupUsername` (schema 1.8.67, line 15136).
    /// Response is `ok`; `updateSupergroup` carries the new username.
    /// Applied optimistically by the driver at send time; a TDLib error
    /// restores the previous username via `PendingRequest::rollback`.
    SetSupergroupUsername,
    /// Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) — the
    /// admin custom-title setter (Telegram X `EditRightsController`
    /// drives the "Custom title" field through it). Response is `ok`;
    /// member caches are invalidated so the new tag is refetched.
    SetChatMemberTag {
        user_id: i64,
    },
    /// Slice G2: `toggleSupergroupSignMessages` (schema 1.8.67, line
    /// 15175). Response is `ok`; `updateSupergroup` carries the new
    /// `sign_messages` / `show_message_sender`. Applied optimistically;
    /// a TDLib error restores the previous flags via
    /// `PendingRequest::rollback`.
    ToggleSupergroupSignMessages,
    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled` (schema
    /// 1.8.67, line 15212). Response is `ok`;
    /// `updateSupergroupFullInfo` carries the new
    /// `has_aggressive_anti_spam_enabled`. Applied optimistically; a
    /// TDLib error restores the previous flag via
    /// `PendingRequest::rollback`.
    ToggleSupergroupAggressiveAntiSpam,
    /// Slice G2: forum topic management (schema 1.8.67, lines
    /// 12665/12674/12713/12725/12736/12718). `createForumTopic`
    /// answers `forumTopicInfo`; the rest answer `ok`. The topic list is
    /// refetched on success; `updateForumTopicInfo` keeps it fresh
    /// otherwise.
    CreateForumTopic,
    EditForumTopic {
        forum_topic_id: i32,
    },
    ToggleForumTopicClosed {
        forum_topic_id: i32,
    },
    ToggleForumTopicPinned {
        forum_topic_id: i32,
    },
    DeleteForumTopic {
        forum_topic_id: i32,
    },
    ToggleGeneralForumTopicHidden,
    /// Slice G2: `getMessageThreadHistory` (schema 1.8.67, line 11839)
    /// — the channel-comments viewer. Response is `messages`;
    /// `message_id` identifies the channel post, correlated to the chat
    /// via `PendingRequest::chat_id`.
    GetMessageThreadHistory {
        message_id: i64,
    },
    /// Slice G2: `getChatBoostStatus` (schema 1.8.67, line 13917).
    /// Response is `chatBoostStatus`; correlated via
    /// `PendingRequest::chat_id`.
    GetChatBoostStatus,
    /// Slice G2: `getAvailableChatBoostSlots` as the first half of the
    /// boost action (schema 1.8.67, line 13914). Response is
    /// `chatBoostSlots`; the driver chains `boostChat` with the first
    /// slot id. Correlated via `PendingRequest::chat_id`.
    GetBoostSlotsForBoost,
    /// Slice G2: `boostChat` (schema 1.8.67, line 13922). Response is
    /// `chatBoostSlots`; the boost status is refetched afterwards.
    BoostChat,
    /// Slice G2: `loadChatWelcomeMessages` (schema 1.8.67, line 12630).
    /// The pack arrives as `updateChatWelcomeMessages`.
    LoadChatWelcomeMessages,
    /// Slice G2: `addChatWelcomeMessage` / `editChatWelcomeMessage` /
    /// `deleteChatWelcomeMessage` (schema 1.8.67, lines 12639/12646/
    /// 12651). Responses are `ok`; the pack is reloaded on success.
    AddChatWelcomeMessage,
    EditChatWelcomeMessage {
        welcome_message_id: i32,
    },
    DeleteChatWelcomeMessage {
        welcome_message_id: i32,
    },
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
    /// Response is `ok`; the row drops via `updateChatRemovedFromList`.
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
    /// Slice CL3: `reportChat` (schema 1.8.67, line 15693). Response is
    /// `ReportChatResult`; `Ok` reports the chat, any other variant
    /// surfaces as "more info required" (never success).
    ReportChat,
    /// Slice CL3: `setMessageSenderBlockList` (schema 1.8.67, line
    /// 14492). Response is `ok`; the new state arrives via
    /// `updateChatBlockList`. Slice A6: carries the requested `block`
    /// value so the `ok` arm can update the cached
    /// `UserFullInfoData.blocked` authoritatively (the `ok` response
    /// itself carries no state).
    SetMessageSenderBlockList {
        block: bool,
    },
    /// Slice B2: `sendBotStartMessage` (schema 1.8.67, line 12216) from
    /// the START button / "Restart bot". Response is the sent `message`.
    SendBotStartMessage,
    /// Slice B2: `getBotSimilarBots` (schema 1.8.67, line 11640) for the
    /// similar-bots section of the bot profile. Response is `users`;
    /// the bot ids land in `Session::similar_bots` (keyed by the pending
    /// request's `user_id`).
    GetBotSimilarBots,
    /// Slice CL2: `getArchiveChatListSettings` (schema 1.8.67, line
    /// 13421). Response is `archiveChatListSettings`; stored in
    /// `Session::archive_chat_list_settings`.
    GetArchiveChatListSettings,
    /// Slice CL2: `setArchiveChatListSettings` (schema 1.8.67, line
    /// 13424). Response is `ok`; the settings were flipped
    /// optimistically, rollback on refusal.
    SetArchiveChatListSettings,
    /// Slice CL2: `createPrivateChat` for Saved Messages (schema
    /// 1.8.67, line 9590 — "Call createPrivateChat with
    /// getOption("my_id") and open the chat"). The `chat` answer opens
    /// the chat.
    CreatePrivateChat,
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
    /// Phase 9.5: `getStoryInteractions` — an own story's viewers list.
    /// Response is `storyInteractions`; pages accumulate in
    /// `Session::story_viewers` (the previous page's `next_offset`
    /// starts the next request).
    GetStoryInteractions,
    /// Phase 9.5: `reportStory`. Response is `ReportStoryResult`
    /// (`Ok` / `OptionRequired` / `TextRequired`); driven by
    /// `Session::story_report`.
    ReportStory,
    /// Phase 9.5: `activateStoryStealthMode`. Response is `ok`; the new
    /// state lands as `updateStoryStealthMode` in
    /// `Session::story_stealth`.
    ActivateStoryStealthMode,
    /// Phase 9.3: `canPostStory`. Response is a `canPostStoryResult*`;
    /// the reducer stores it in `Session::story_post.eligibility`.
    CheckCanPostStory,
    /// Phase 9.3: `postStory`. Response is a `story` (the pending
    /// story, id = temporary); success/failure lands via
    /// `updateStoryPostSucceeded` / `updateStoryPostFailed`.
    PostStory,
    /// Phase 9.5: `editStory`. Response is `ok`; the edited story
    /// arrives via `updateStory`.
    EditStory,
    /// Phase 9.5: `editStoryCover`. Response is `ok`.
    EditStoryCover,
    /// Phase 9.5: `setStoryPrivacySettings`. Response is `ok`.
    SetStoryPrivacySettings,
    /// Phase 9.5: `getChatsToPostStories`. Response is `chats`;
    /// stored in `Session::story_post_as_chats`.
    GetChatsToPostStories,
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
    /// Phase B1: `createNewSecretChat`. Response is `chat` (the new
    /// secret chat); the canonical state arrives as `updateNewChat` /
    /// `updateSecretChat`.
    CreateNewSecretChat,
    /// Phase B1: `getSecretChat`. Response is `secretChat`; resolves the
    /// initial state of a secret chat whose `updateSecretChat` was never
    /// seen (e.g. loaded from the local DB). Correlated via
    /// `PendingRequest::secret_chat_id`.
    GetSecretChat,
    /// Phase B1: `closeSecretChat`. Response is `ok`; the state change to
    /// `secretChatStateClosed` arrives as `updateSecretChat`.
    CloseSecretChat,
    /// Phase C1: `createCall`. Response is `callId`; correlated via
    /// `PendingRequest::user_id`. The call's states arrive as
    /// `updateCall`. Phase C1b: `is_video` rides along so the `callId`
    /// answer can start tracking with the right call kind — the answer
    /// itself carries no `is_video` (schema 1.8.67, :7034).
    CreateCall {
        is_video: bool,
    },
    /// Phase C1: `acceptCall`. Response is `ok`; the answered state
    /// arrives as `updateCall`.
    AcceptCall,
    /// Phase C2b: `sendCallSignalingData`. Response is `ok`; this is a
    /// fire-and-forget bridge from the call engine to TDLib.
    SendCallSignalingData,
    /// Phase C1: `discardCall`. Response is `ok`; the hangup states
    /// (`callStateHangingUp` → `callStateDiscarded`) arrive as
    /// `updateCall`.
    DiscardCall,
    /// Phase C1: `sendCallRating`. Response is `ok`; sent from the
    /// call-end rating card when `callStateDiscarded.need_rating`.
    SendCallRating,
    /// Phase C2d: `sendCallDebugInformation` for the ended call.
    SendCallDebugInformation,
    /// Phase C2i: `searchCallMessages`. Response is `foundMessages`;
    /// drives the Recent-calls tab.
    SearchCallMessages,
    /// Phase C2i: `getUserPrivacySettingRules`. Response is
    /// `userPrivacySettingRules`; `setting` selects which of the two
    /// call privacy settings is fetched.
    GetCallPrivacyRules {
        setting: CallPrivacySetting,
    },
    /// Phase C2i: `setUserPrivacySettingRules`. Response is `ok`; the
    /// new value is applied optimistically at send time.
    SetCallPrivacyRules {
        setting: CallPrivacySetting,
    },
    /// Phase C2i: `sendCallLog` for the ended call. Response is `ok`.
    SendCallLog,
    /// Phase C3a: `createVideoChat`. Response is `groupCallId`; the
    /// chat-bound voice chat's states arrive as `updateGroupCall`.
    CreateVideoChat {
        chat_id: i64,
    },
    /// Phase C3a: `joinVideoChat`. Response is `text` (join payload
    /// for tgcalls) — stored on the tracked call; Phase C2g consumes it
    /// in the driver pump to finish the native group handshake.
    JoinVideoChat {
        group_call_id: i32,
    },
    /// Phase C3a: `leaveGroupCall`. Response is `ok`.
    LeaveGroupCall {
        group_call_id: i32,
    },
    /// Phase C3a: `endGroupCall`. Response is `ok`.
    EndGroupCall {
        group_call_id: i32,
    },
    /// Phase C2g: `startGroupCallScreenSharing` (schema 1.8.67, :14303).
    /// Response is `text` — the presentation answer for
    /// `ntg_connect(..., is_presentation=true)`.
    StartGroupCallScreenSharing {
        group_call_id: i32,
    },
    /// Phase C2g: `endGroupCallScreenSharing` (schema 1.8.67, :14309).
    /// Response is `ok`.
    EndGroupCallScreenSharing {
        group_call_id: i32,
    },
    /// Phase C3a: `getGroupCall`. Response is `groupCall`; refreshes
    /// the tracked call via `updateGroupCall`-equivalent handling.
    GetGroupCall {
        group_call_id: i32,
    },
    /// Phase C3a: `loadGroupCallParticipants`. Response is `ok`;
    /// participants arrive as updates.
    LoadGroupCallParticipants {
        group_call_id: i32,
    },
    /// Phase C3a: `getVideoChatInviteLink`. Response is `httpUrl`.
    GetVideoChatInviteLink {
        group_call_id: i32,
    },
    /// Phase C3a: `setVideoChatTitle`. Response is `ok`; the new title
    /// arrives as `updateGroupCall`.
    SetVideoChatTitle {
        group_call_id: i32,
    },
    /// Phase C2h: `revokeGroupCallInviteLink`. Response is `ok`;
    /// clears the cached invite link.
    RevokeVideoChatInviteLink {
        group_call_id: i32,
    },
    /// Phase C2h: `startGroupCallRecording`. Response is `ok`;
    /// recording state arrives as `updateGroupCall`
    /// (`record_duration` / `is_video_recorded`).
    StartGroupCallRecording {
        group_call_id: i32,
    },
    /// Phase C2h: `endGroupCallRecording`. Response is `ok`;
    /// recording state arrives as `updateGroupCall`.
    EndGroupCallRecording {
        group_call_id: i32,
    },
    /// Phase C2h: `startScheduledVideoChat`. Response is `ok`;
    /// the call goes live via `updateGroupCall` /
    /// `updateNewVideoChat`.
    StartScheduledVideoChat {
        group_call_id: i32,
    },
    /// `toggleVideoChatEnabledStartNotification` (schema 1.8.67,
    /// :14282). Response is `ok`; the new
    /// `groupCall.enabled_start_notification` arrives as
    /// `updateGroupCall`.
    ToggleVideoChatEnabledStartNotification {
        group_call_id: i32,
        enabled: bool,
    },
    /// Phase C2h: `getVideoChatRtmpUrl`. Response is `rtmpUrl`.
    GetVideoChatRtmpUrl {
        chat_id: i64,
    },
    /// Phase C2h: `replaceVideoChatRtmpUrl`. Response is `rtmpUrl`.
    ReplaceVideoChatRtmpUrl {
        chat_id: i64,
    },
    /// Phase C2h: `sendGroupCallMessage`. Response is `ok`; the
    /// message arrives back as `updateNewGroupCallMessage` (echo),
    /// or `updateGroupCallMessageSendFailed` on failure.
    SendGroupCallMessage {
        group_call_id: i32,
    },
    /// Phase C2h: `toggleGroupCallAreMessagesAllowed`. Response is
    /// `ok`; the new flag arrives as `updateGroupCall`.
    ToggleGroupCallAreMessagesAllowed {
        group_call_id: i32,
    },
    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` /
    /// `toggleGroupCallIsMyVideoPaused`. Response is `ok`; state
    /// refreshes via `updateGroupCall`.
    ToggleGroupCallVideo {
        group_call_id: i32,
    },
    /// Phase C3a: `toggleGroupCallParticipantIsMuted`. Response is
    /// `ok`; state refreshes via updates.
    ToggleGroupCallParticipantMute {
        group_call_id: i32,
    },
    /// Phase C3a: `toggleGroupCallParticipantIsHandRaised`. Response is
    /// `ok`; state refreshes via updates.
    ToggleGroupCallParticipantHand {
        group_call_id: i32,
    },
    /// Phase C3a: `toggleVideoChatMuteNewParticipants`. Response is
    /// `ok`; state refreshes via `updateGroupCall`.
    ToggleVideoChatMuteNew {
        group_call_id: i32,
    },
    /// Phase C2f: `inviteGroupCallParticipant`. Response is
    /// `inviteGroupCallParticipantResult*`; non-success results
    /// surface via `group_call_error`.
    InviteGroupCallParticipant {
        group_call_id: i32,
    },
    /// Phase C2f: `banGroupCallParticipants`. Response is `ok`;
    /// the roster refreshes via participant updates.
    BanGroupCallParticipants {
        group_call_id: i32,
    },
    /// Phase C2f: `setGroupCallParticipantVolumeLevel`. Response is
    /// `ok`; the new level arrives via `updateGroupCallParticipant`.
    SetGroupCallParticipantVolumeLevel {
        group_call_id: i32,
    },
    /// Phase C2f: `joinGroupCall` to accept a `messageGroupCall`
    /// invitation (schema 1.8.67, line 5288: "Use joinGroupCall to
    /// accept the call"). The joined call is tracked via
    /// `updateGroupCall` like any other join.
    JoinGroupCallInvitation,
    /// Phase C2f: `declineGroupCallInvitation`. Response is `ok`.
    DeclineGroupCallInvitation {
        chat_id: i64,
        message_id: i64,
    },
    /// Phase B4: `setChatMessageAutoDeleteTime`. Response is `ok`; the
    /// new timer arrives as `updateChatMessageAutoDeleteTime` (plus a
    /// `messageChatSetMessageAutoDeleteTime` service message in history).
    SetChatMessageAutoDeleteTime,
    /// Phase S2: `getStorageStatistics`. Response is `storageStatistics`;
    /// aggregated by file type into `Session::storage_stats` (TGX
    /// `SettingsCacheController` / `TGStorageStats` style, including the
    /// "Secret media and files" category for `fileTypeSecret`).
    GetStorageStatistics,
    /// Slice A2: a 2FA management request (`getPasswordState` /
    /// `setPassword` / `setRecoveryEmailAddress` /
    /// `resendRecoveryEmailAddressCode` /
    /// `cancelRecoveryEmailAddressVerification`). All answer
    /// `passwordState`, stored in `Session::password_state`; `op`
    /// classifies the honest error line.
    PasswordStateOp {
        op: PasswordOp,
    },
    Close,
    LogOut,
    Other,
}

/// Slice A2: which 2FA management request a `PasswordStateOp` purpose
/// carries. Used for honest per-action error lines and for the driver's
/// single-in-flight gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordOp {
    /// `getPasswordState` — initial / refresh fetch.
    Fetch,
    /// `setPassword` with a new password (empty old = first-time enable).
    SetPassword,
    /// `setPassword` with an empty new password (disable).
    DisablePassword,
    /// `setRecoveryEmailAddress`.
    SetRecoveryEmail,
    /// `resendRecoveryEmailAddressCode`.
    ResendCode,
    /// `cancelRecoveryEmailAddressVerification`.
    AbortEmailSetup,
}

impl PasswordOp {
    /// Past-tense action label for honest error lines, e.g.
    /// "Could not change the two-step password (error 400)".
    fn action_label(self) -> &'static str {
        match self {
            PasswordOp::Fetch => "load two-step verification settings",
            PasswordOp::SetPassword => "change the two-step password",
            PasswordOp::DisablePassword => "turn off two-step verification",
            PasswordOp::SetRecoveryEmail => "set the recovery email",
            PasswordOp::ResendCode => "resend the confirmation code",
            PasswordOp::AbortEmailSetup => "abort the email setup",
        }
    }
}

/// Slice G1: the pre-request value an optimistic mutation restores when
/// TDLib rejects it. Stored on `PendingRequest::rollback` at send time;
/// the error arm below restores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestRollback {
    /// `setChatPermissions`: the chat's previous permission block and
    /// `can_send_basic_messages`.
    ChatPermissions {
        previous: Option<ChatPermissions>,
        previous_can_send: bool,
    },
    /// `toggleSupergroupJoinByRequest`: the previous join-by-request flag
    /// (`None` = unknown, treated as disabled).
    JoinByRequest {
        supergroup_id: i64,
        previous: Option<bool>,
    },
    /// `setSupergroupUsername`: the previous username (`None` = none set).
    SupergroupUsername {
        supergroup_id: i64,
        previous: Option<String>,
    },
    /// Slice G2: `toggleSupergroupSignMessages`: the previous
    /// `sign_messages` / `show_message_sender` flags (`None` = unknown).
    SignMessages {
        supergroup_id: i64,
        previous_sign: Option<bool>,
        previous_show: Option<bool>,
    },
    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled`: the
    /// previous `has_aggressive_anti_spam_enabled` flag (`None` =
    /// unknown).
    AntiSpam {
        supergroup_id: i64,
        previous: Option<bool>,
    },
    /// Slice CL1: `toggleChatIsPinned` — the previous pinned flag and
    /// which list it belonged to (`archived` = archive list). The
    /// authoritative state arrives via `updateChatPosition`.
    ChatPin { previous: bool, archived: bool },
    /// Slice CL1: `toggleChatIsMarkedAsUnread` — the previous
    /// marked-as-unread flag.
    ChatMarkedAsUnread { previous: bool },
    /// Slice CL2: `setPinnedChats` — the previous `(chat_id, order)`
    /// pairs of the pinned chats in the list (`archived` = archive
    /// list). The reorder swaps `order` values among the pinned chats
    /// so `rebuild_main_order` keeps the new arrangement until the
    /// authoritative `updateChatPosition` orders arrive.
    ChatPinOrder {
        previous: Vec<(i64, i64)>,
        archived: bool,
    },
    /// Slice CL2: `setArchiveChatListSettings` — the previous settings
    /// (`None` = never fetched; the optimistic value is dropped and
    /// the panel re-fetches).
    ArchiveChatListSettings {
        previous: Option<ArchiveChatListSettings>,
    },
}

fn is_auth_submit(purpose: RequestPurpose) -> bool {
    matches!(
        purpose,
        RequestPurpose::SetPhoneNumber
            | RequestPurpose::CheckAuthenticationCode
            | RequestPurpose::CheckAuthenticationPassword
            | RequestPurpose::ResendAuthenticationCode
            | RequestPurpose::RequestQrCodeAuthentication
    )
}

/// Phase C1: classified error line for a failed call request. Only the
/// numeric code is shown — TDLib's `message` is never stored (it can
/// contain secrets). Code 4005000 (outgoing call missed because the
/// timeout expired) gets a plain-language line instead.
fn call_request_error_line(err: &TdError, action: &str) -> String {
    if err.code == 4005000 {
        format!("{action}: no answer — the call timed out")
    } else {
        format!("{action} (error {})", err.code)
    }
}

/// Slice A2: honest one-line failure for a 2FA management request. The
/// classification comes from TDLib's actual numeric error code (400 =
/// wrong password / invalid input, 429 = flood-wait); the native message
/// is never stored (it can contain secrets — see `TdError`).
fn password_op_error_line(op: PasswordOp, err: &TdError) -> String {
    let detail = match err.class {
        ErrorClass::Flood => "too many attempts — wait and try again",
        ErrorClass::Unauthorized => "session is no longer authorized",
        ErrorClass::Invalid => match op {
            PasswordOp::SetPassword | PasswordOp::DisablePassword => {
                "wrong password or invalid input"
            }
            PasswordOp::SetRecoveryEmail => "wrong password, or the email was rejected",
            PasswordOp::ResendCode => "the code can't be resent yet",
            PasswordOp::AbortEmailSetup => "the pending setup can't be aborted",
            PasswordOp::Fetch => "try again",
        },
        _ => "Telegram rejected the request",
    };
    format!("Could not {}: {detail}", op.action_label())
}

/// Slice A3: honest one-line failure for a sessions fetch or terminate.
/// The classification comes from TDLib's actual numeric error code
/// (400 = invalid/refused — e.g. terminating a session TDLib won't let
/// go, 429 = flood-wait); the native message is never stored (it can
/// contain secrets — see `TdError`).
fn sessions_error_line(action: &str, err: &TdError) -> String {
    let detail = match err.class {
        ErrorClass::Flood => "too many requests — wait and try again",
        ErrorClass::Unauthorized => "session is no longer authorized",
        ErrorClass::Invalid => "Telegram refused the request",
        ErrorClass::NotFound => "no longer exists",
        ErrorClass::Other => return format!("Could not {action} (error {})", err.code),
    };
    format!("Could not {action}: {detail}")
}

/// Phase 9.3: one-line `TdError` reason for the story composer. Never
/// includes the native TDLib message (it can contain secrets — see
/// `TdError`).
fn error_reason(err: &TdError) -> String {
    match err.class {
        ErrorClass::NotFound => "not found".to_string(),
        ErrorClass::Unauthorized => "not authorized".to_string(),
        ErrorClass::Flood => "too many requests — try again later".to_string(),
        ErrorClass::Invalid => "invalid request".to_string(),
        ErrorClass::Other => format!("error {}", err.code),
    }
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
            (RequestPurpose::ResendAuthenticationCode, ErrorClass::Invalid) => {
                "couldn't resend the code"
            }
            // No invented local cooldown: a too-early resend fails
            // server-side (429), and this is the honest surface for it.
            (RequestPurpose::ResendAuthenticationCode, ErrorClass::Flood) => {
                "too many resends — wait and try again"
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

/// B1: an incoming message whose markup demands a reply
/// (`replyMarkupForceReply`, or `force_reply` on an inline / show-keyboard
/// markup). Drained by the UI into a composer reply-to + focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceReplyTarget {
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

/// B1: context of an in-flight login-URL button press. Kept while
/// `getLoginUrlInfo` (and then `getLoginUrl`, after consent) is in flight,
/// so an error degrades the button to a plain URL-button press (schema
/// 1.8.67 doc on `getLoginUrl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginUrlRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub button_id: i64,
    /// The button's raw URL — the plain-URL fallback.
    pub raw_url: String,
}

/// Slice P1: context of an in-flight payment request (`getPaymentForm`,
/// `validateOrderInfo`, `sendPaymentForm`, `getPaymentReceipt`), kept so
/// the answers / errors correlate to the right invoice message. Set when a
/// payment request is sent (connect.rs), cleared when the checkout dialog
/// closes — the answer reducers never clear it, because `validateOrderInfo`
/// and `sendPaymentForm` need it after the form answer is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRequest {
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

/// B1: pick the custom keyboard to show for a chat — the newest message
/// carrying `replyMarkupShowKeyboard`, invalidated by a newer
/// `replyMarkupRemoveKeyboard` or by a one-time keyboard already tapped.
/// Pure logic: unit-tested.
pub fn active_custom_keyboard(
    messages: &std::collections::BTreeMap<i64, HistoryMessage>,
    dismissed: &std::collections::HashSet<(i64, i64)>,
) -> Option<(ChatId, MessageId, ReplyKeyboard)> {
    let mut active: Option<(ChatId, MessageId, ReplyKeyboard)> = None;
    for message in messages.values() {
        match &message.reply_markup {
            Some(ReplyMarkup::ShowKeyboard(keyboard)) => {
                active = Some((message.chat_id, message.id, keyboard.clone()));
            }
            Some(ReplyMarkup::RemoveKeyboard) => active = None,
            _ => {}
        }
    }
    active.filter(|(chat_id, message_id, _)| !dismissed.contains(&(chat_id.0, message_id.0)))
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

/// Slice B2: fetch state of `getBotSimilarBots` for a bot profile. The
/// `users` payload carries only ids (names resolve through
/// `Session::users`); presence records "fetched" so the driver never
/// retries, and `Loading` dedupes the request.
#[derive(Debug, Clone, Default)]
pub enum SimilarBotsFetch {
    #[default]
    Loading,
    Loaded(Vec<i64>),
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
    /// Phase B1: `secret_chat_id` for `GetSecretChat` /
    /// `CloseSecretChat` so the id-less `secretChat` response and
    /// purpose-gating correlate to the right secret chat.
    pub secret_chat_id: Option<i32>,
    /// Slice G1: rollback for requests the driver applies optimistically
    /// at send time (`setChatPermissions`,
    /// `toggleSupergroupJoinByRequest`, `setSupergroupUsername`). On a
    /// TDLib error the pre-request value is restored so the UI never
    /// keeps showing a change the server rejected.
    pub rollback: Option<RequestRollback>,
}

#[derive(Debug, Default)]
pub struct RequestRegistry {
    next: u64,
    pending: HashMap<u64, PendingRequest>,
}

impl RequestRegistry {
    /// Phase B1: whether a request with the given purpose is already in
    /// flight for this secret chat id. Used to avoid duplicate offline
    /// `getSecretChat` fetches while the state resolves.
    pub fn has_pending_for_secret_chat(
        &self,
        purpose: RequestPurpose,
        secret_chat_id: i32,
    ) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.secret_chat_id == Some(secret_chat_id))
    }

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
                secret_chat_id: None,
                rollback: None,
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
                secret_chat_id: None,
                rollback: None,
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
                secret_chat_id: None,
                rollback: None,
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
                secret_chat_id: None,
                rollback: None,
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
                secret_chat_id: None,
                rollback: None,
            },
        );
        id
    }

    pub fn take(&mut self, id: RequestId) -> Option<PendingRequest> {
        self.pending.remove(&id.0)
    }

    /// Drop the in-flight request with the given purpose, if any (Phase
    /// S2: storage-stats Refresh must drop the stale in-flight purpose,
    /// or the immediate refetch no-ops and the overlay shows "No storage
    /// data yet." until the old answer lands).
    pub fn take_purpose(&mut self, purpose: RequestPurpose) -> Option<PendingRequest> {
        let key = self
            .pending
            .iter()
            .find(|(_, req)| req.purpose == purpose)
            .map(|(key, _)| *key)?;
        self.pending.remove(&key)
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

    /// Slice G2: non-destructive view of a pending request, so the
    /// driver can capture mutation context before `apply` takes it.
    pub fn get(&self, id: RequestId) -> Option<&PendingRequest> {
        self.pending.get(&id.0)
    }

    pub fn has_purpose(&self, purpose: RequestPurpose) -> bool {
        self.pending.values().any(|p| p.purpose == purpose)
    }

    pub fn has_purpose_for_chat(&self, purpose: RequestPurpose, chat_id: ChatId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.chat_id == Some(chat_id))
    }

    /// Phase D3c: whether any `getChatEventLog` request is in flight for
    /// the chat, whatever its paging cursor. `has_purpose_for_chat`
    /// compares the full purpose (including `from_event_id`), so it
    /// cannot dedup across pages.
    pub fn has_event_log_in_flight(&self, chat_id: ChatId) -> bool {
        self.pending.values().any(|p| {
            matches!(p.purpose, RequestPurpose::GetChatEventLog { .. })
                && p.chat_id == Some(chat_id)
        })
    }

    /// Bots slice: whether a `getInlineQueryResults` request is in flight
    /// for the (chat, bot) pair, whatever the page. `has_purpose_for_chat`
    /// compares the full purpose (including `first_page`), so it cannot
    /// dedup across pages.
    pub fn has_inline_query_in_flight(&self, chat_id: ChatId, bot_user_id: i64) -> bool {
        self.pending.values().any(|p| {
            matches!(
                p.purpose,
                RequestPurpose::GetInlineQueryResults {
                    chat_id: c,
                    bot_user_id: b,
                    ..
                } if c == chat_id && b == bot_user_id
            )
        })
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

/// Slice B2: parse a bot deep link of the form
/// `t.me/<bot_username>?start=<start_parameter>` (with or without the
/// `https://` scheme) into `(bot_username, start_parameter)` — the two
/// pieces of `internalLinkTypeBotStart` (schema 1.8.67, line 9399) the
/// START button needs. Returns `None` for anything that isn't that shape
/// (different host, multi-segment path, missing/empty `start`).
/// URL-decoding of the parameter is deliberately skipped: the parameter
/// goes to `sendBotStartMessage` verbatim (schema line 12216), and
/// inventing a decode here would corrupt parameters the bot generated.
pub fn parse_bot_start_link(link: &str) -> Option<(String, String)> {
    let rest = link
        .strip_prefix("https://t.me/")
        .or_else(|| link.strip_prefix("http://t.me/"))
        .or_else(|| link.strip_prefix("https://telegram.me/"))
        .or_else(|| link.strip_prefix("t.me/"))?;
    let (path, query) = rest.split_once('?')?;
    if path.is_empty() || path.contains('/') {
        return None;
    }
    let parameter = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "start")
        .map(|(_, value)| value.to_string())?;
    if parameter.is_empty() {
        return None;
    }
    Some((path.to_string(), parameter))
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

/// kit Phase 4: compact `HH:MM` in-bubble timestamp for a TDLib `date`
/// (unix seconds). UTC rather than local time: the sandbox/CI clock's
/// local zone is not the user's, and the existing `format_unix_date_time`
/// UI helper is UTC too. Returns `None` for `0`/negative (date absent).
pub fn message_time_hhmm(unix: i32) -> Option<String> {
    if unix <= 0 {
        return None;
    }
    let mins = unix as i64 / 60;
    let day_minutes = mins.rem_euclid(24 * 60);
    let hh = day_minutes / 60;
    let mm = day_minutes % 60;
    Some(format!("{hh:02}:{mm:02}"))
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
    /// Phase D3a: `rights.can_invite_users` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates the invite-link
    /// / join-request management UI.
    pub my_admin_can_invite_users: Option<bool>,
    /// Slice G2: `rights.can_change_info` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates
    /// `toggleSupergroupSignMessages` in channels (schema line 15175).
    pub my_admin_can_change_info: Option<bool>,
    /// Slice G2: `rights.can_send_welcome_messages` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1090). `Some` only when the
    /// status is Administrator and the rights block parsed. Gates
    /// welcome-message management in channels.
    pub my_admin_can_send_welcome_messages: Option<bool>,
    /// Phase D3b: `rights.can_promote_members` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates admin
    /// management (promote / demote / edit rights).
    pub my_admin_can_promote_members: Option<bool>,
    /// Slice G1: `rights.can_restrict_members` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates member
    /// restriction / banning (`setChatMemberStatus` with a restricted or
    /// banned status requires this right, schema lines 13586-13587).
    pub my_admin_can_restrict_members: Option<bool>,
    /// MED1: `rights.can_pin_messages` from
    /// `chatMemberStatusAdministrator` (TDLib 1.8.67,
    /// `chatAdministratorRights`, schema line 1092). `Some` only when the
    /// status is Administrator and the rights block parsed; `None` for
    /// every other status or an absent rights block. Gates album pin /
    /// unpin (`pinChatMessage`, schema line 13559).
    pub my_admin_can_pin_messages: Option<bool>,
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
    /// Slice G1: the full default `chat.permissions` block
    /// (`chatPermissions`, schema 1.8.67 line 1070), refreshed by
    /// `updateChatPermissions` (line 10500). Drives the default chat
    /// permissions editor; `None` until a full block parses.
    pub permissions: Option<ChatPermissions>,
    /// Slice G1: `chat.can_be_deleted_for_all_users` (schema 1.8.67, line
    /// 3616), refreshed by `updateNewChat`. Gates `deleteChat` (schema
    /// line 11850).
    pub can_be_deleted_for_all_users: bool,
    /// Slice CL1: `chat.can_be_deleted_only_for_self` (schema 1.8.67,
    /// line 3604), refreshed by `updateNewChat`. Together with
    /// `can_be_deleted_for_all_users` gates `deleteChatHistory` (schema
    /// line 11845).
    pub can_be_deleted_only_for_self: bool,
    /// Slice CL1: `chat.is_marked_as_unread` (schema 1.8.67, lines
    /// 3600 / 3627), refreshed by `updateNewChat` and
    /// `updateChatIsMarkedAsUnread` (schema line 10588).
    pub is_marked_as_unread: bool,
    /// Slice CL3: `chat.unread_mention_count` (schema 1.8.67, lines
    /// 3611 / 3627), refreshed by `updateChatUnreadMentionCount`
    /// (schema line 10567). Drives the @ mention badge on the row.
    pub unread_mention_count: i32,
    /// Slice CL3: `chat.unread_reaction_count` (schema 1.8.67, lines
    /// 3612 / 3627), refreshed by `updateChatUnreadReactionCount`
    /// (schema line 10570). Drives the ♥ reaction badge on the row.
    pub unread_reaction_count: i32,
    /// Slice CL3: `chat.can_be_reported` (schema 1.8.67, lines 3606 /
    /// 3627). Gates the row-menu Report item (`reportChat`, schema
    /// line 15693).
    pub can_be_reported: bool,
    /// Slice CL3: the peer is on `blockListMain` (`chat.block_list`,
    /// schema 1.8.67 lines 3627 / 9692), refreshed by
    /// `updateChatBlockList` (schema line 10594). Drives the
    /// row-menu Block/Unblock label.
    pub blocked: bool,
    /// Phase B1: latest known `SecretChatState` for `ChatKind::Secret`
    /// chats (from `updateSecretChat` / `getSecretChat`, schema 1.8.67
    /// lines 10741 / 2816). `None` for other chat kinds and until the
    /// state resolves. Only `Ready` chats can send.
    pub secret_state: Option<SecretChatState>,
    /// Phase B4: `chat.message_auto_delete_time` (schema 1.8.67, lines
    /// 3616 / 3627) — the chat-level auto-delete or self-destruct
    /// (secret chats) timer, in seconds; 0 when disabled. Set by
    /// `updateNewChat`, refreshed by `updateChatMessageAutoDeleteTime`.
    pub message_auto_delete_time: i32,
    /// Phase C3a: `chat.video_chat` (`videoChat`, schema 1.8.67, lines
    /// 3576 / 3579 / 3627). `None` when the chat has no active video
    /// chat. Set by `updateNewChat`, refreshed by `updateChatVideoChat`.
    pub video_chat: Option<VideoChatInfo>,
}

/// Phase C3a: a chat's active video chat (`videoChat`, schema 1.8.67,
/// line 3579).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoChatInfo {
    pub group_call_id: i32,
    pub has_participants: bool,
}

impl ChatSummary {
    pub fn supported(&self) -> bool {
        self.kind.is_supported_chat()
    }

    /// `chatTypeSupergroup` with `is_channel: true`.
    pub fn is_channel(&self) -> bool {
        self.kind.is_channel()
    }

    /// Whether the composer is shown for this chat. In 2.3 admins get the
    /// composer in broadcast channels (derived from own membership, see
    /// `channel_admin_can_post`); everyone else in a channel keeps it hidden.
    /// Phase B1: secret chats keep the composer only while their state is
    /// `Ready` — a Pending chat is still handshaking and a Closed chat can
    /// never send again. All other supported chats keep the composer.
    pub fn can_post(&self) -> bool {
        if self.is_channel() {
            return self.channel_admin_can_post();
        }
        if matches!(self.kind, ChatKind::Secret { .. }) {
            return self.secret_state == Some(SecretChatState::Ready);
        }
        self.supported()
    }

    /// Phase B1: the secret-chat id behind a secret chat, if any.
    pub fn secret_chat_id(&self) -> Option<i32> {
        match self.kind {
            ChatKind::Secret { secret_chat_id, .. } => Some(secret_chat_id),
            _ => None,
        }
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

    /// Phase D3a: whether the current user may manage this chat's invite
    /// links and join requests. The creator always can; an administrator
    /// needs the explicit `can_invite_users` right. An absent rights block
    /// keeps the gate closed rather than fabricating a right.
    pub fn can_invite_users(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_invite_users.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Phase D3a: record `rights.can_invite_users` (`None` for non-admin
    /// statuses or an absent rights block).
    pub fn set_admin_can_invite_users(&mut self, can_invite_users: Option<bool>) {
        self.my_admin_can_invite_users = can_invite_users;
    }

    /// Slice G2: record `rights.can_change_info` (`None` for non-admin
    /// statuses or an absent rights block).
    pub fn set_admin_can_change_info(&mut self, can_change_info: Option<bool>) {
        self.my_admin_can_change_info = can_change_info;
    }

    /// Slice G2: whether the current user may change this chat's info.
    /// The creator always can; an administrator needs the explicit
    /// `can_change_info` right.
    pub fn can_change_info(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_change_info.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: record `rights.can_send_welcome_messages` (`None` for
    /// non-admin statuses or an absent rights block).
    pub fn set_admin_can_send_welcome_messages(&mut self, can_send: Option<bool>) {
        self.my_admin_can_send_welcome_messages = can_send;
    }

    /// Slice G2: whether the current user may manage this chat's welcome
    /// messages. The creator always can; an administrator needs the
    /// explicit `can_send_welcome_messages` right.
    pub fn can_send_welcome_messages(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_send_welcome_messages.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Phase D3b: whether the current user may manage this chat's
    /// administrators (view the admin list, promote/demote members, edit
    /// admin rights). The creator always can; an administrator needs the
    /// explicit `can_promote_members` right. An absent rights block keeps
    /// the gate closed rather than fabricating a right.
    pub fn can_manage_admins(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_promote_members.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Phase D3c: whether the current user is the creator or an
    /// administrator of this chat (own membership, probed via
    /// `getChatMember`). Unlike `can_manage_admins` (D3b, which needs the
    /// explicit `can_promote_members` right) and `can_invite_users`
    /// (D3a, `can_invite_users`), the event log requires only
    /// administrator rights (schema 1.8.67, line 15252), so any
    /// administrator qualifies. Unknown status keeps the gate closed.
    pub fn is_admin_or_creator(&self) -> bool {
        matches!(
            self.my_member_status,
            Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
        )
    }

    /// Phase D3b: record `rights.can_promote_members` (`None` for
    /// non-admin statuses or an absent rights block).
    pub fn set_admin_can_promote_members(&mut self, can_promote_members: Option<bool>) {
        self.my_admin_can_promote_members = can_promote_members;
    }

    /// Slice G1: whether the current user may restrict or ban this chat's
    /// members (`setChatMemberStatus` with a restricted or banned status
    /// requires the `can_restrict_members` administrator right, schema
    /// 1.8.67 lines 13586-13587). The creator always can; an administrator
    /// needs the explicit right. Unknown status or an absent rights block
    /// keeps the gate closed.
    pub fn can_restrict_members(&self) -> bool {
        match self.my_member_status {
            Some(ChannelMemberStatus::Creator) => true,
            Some(ChannelMemberStatus::Administrator) => {
                self.my_admin_can_restrict_members.unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G1: record `rights.can_restrict_members` (`None` for
    /// non-admin statuses or an absent rights block).
    pub fn set_admin_can_restrict_members(&mut self, can_restrict_members: Option<bool>) {
        self.my_admin_can_restrict_members = can_restrict_members;
    }

    /// MED1: whether the current user may pin messages in this chat
    /// (`pinChatMessage`, schema 1.8.67 line 13559). Private and secret
    /// chats allow own-side pins; the creator always can; an
    /// administrator needs the explicit `can_pin_messages` right
    /// (`chatAdministratorRights`, schema line 1092); basic-group and
    /// supergroup members need the `can_pin_messages` member right
    /// (`chatPermissions`, schema line 1070). Unknown status keeps the
    /// gate closed rather than fabricating a right.
    pub fn can_pin_messages(&self) -> bool {
        use crate::telegram::envelope::ChatKind;
        match &self.kind {
            ChatKind::Private { .. } | ChatKind::Secret { .. } => true,
            _ => match self.my_member_status {
                Some(ChannelMemberStatus::Creator) => true,
                Some(ChannelMemberStatus::Administrator) => {
                    self.my_admin_can_pin_messages.unwrap_or(false)
                }
                _ => self
                    .permissions
                    .as_ref()
                    .is_some_and(|p| p.can_pin_messages),
            },
        }
    }

    /// MED1: record `rights.can_pin_messages` (`None` for non-admin
    /// statuses or an absent rights block).
    pub fn set_admin_can_pin_messages(&mut self, can_pin_messages: Option<bool>) {
        self.my_admin_can_pin_messages = can_pin_messages;
    }

    pub fn is_forum_chat(&self) -> bool {
        self.is_forum == Some(true)
    }

    pub fn is_muted(&self) -> bool {
        self.notification_settings.is_muted()
    }

    /// Slice CL2: the TGX unread-filter predicate
    /// (`ChatFilter.unreadFilter.accept`: `unreadCount > 0 ||
    /// isMarkedAsUnread`) — drives the Unread category chip.
    pub fn is_unread(&self) -> bool {
        self.unread_count > 0 || self.is_marked_as_unread
    }

    /// Phase B4: the chat-level timer status line — "Self-destruct: 1h"
    /// for secret chats, "Auto-delete: 7d" for other chats
    /// (`chat.message_auto_delete_time`, schema 1.8.67 lines 3616 /
    /// 3627). `None` when the timer is disabled.
    pub fn ttl_status_line(&self) -> Option<String> {
        if self.message_auto_delete_time <= 0 {
            return None;
        }
        let label = crate::telegram::envelope::format_ttl_setting(self.message_auto_delete_time);
        let noun = if matches!(self.kind, ChatKind::Secret { .. }) {
            "Self-destruct"
        } else {
            "Auto-delete"
        };
        Some(format!("{noun}: {label}"))
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
        if matches!(self.kind, ChatKind::Secret { .. }) {
            return "Secret chat".into();
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
        my_admin_can_invite_users: None,
        my_admin_can_change_info: None,
        my_admin_can_send_welcome_messages: None,
        my_admin_can_promote_members: None,
        my_admin_can_restrict_members: None,
        my_admin_can_pin_messages: None,
        is_forum: None,
        photo_file_id: None,
        // Parity slice 4: lenient default true — the real `chat` object
        // always carries `permissions`; only `updateNewChat` /
        // `updateChatPermissions` ever set it to false.
        can_send_basic_messages: true,
        permissions: None,
        can_be_deleted_for_all_users: false,
        can_be_deleted_only_for_self: false,
        is_marked_as_unread: false,
        unread_mention_count: 0,
        unread_reaction_count: 0,
        can_be_reported: false,
        blocked: false,
        // Phase B1: unknown until `updateSecretChat` / `getSecretChat`
        // resolves it.
        secret_state: None,
        // Phase B4: 0 = disabled (`chat.message_auto_delete_time`,
        // schema 1.8.67, lines 3616 / 3627).
        message_auto_delete_time: 0,
        // Phase C3a: unknown until `updateNewChat` / `updateChatVideoChat`
        // resolves it.
        video_chat: None,
    }
}

/// M2 fix-up: the preview text of a history row — the content it actually
/// shows, so `ephemeral_content` wins over the regular content (schema
/// 1.8.67, line 3161: "must be shown instead of the regular content").
pub fn effective_preview(message: &HistoryMessage) -> String {
    effective_content(&message.content, message.ephemeral.as_ref()).preview()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    /// Schema `message.date` (unix seconds, server time) — feeds the
    /// in-bubble timestamp. `0` when the source didn't carry one.
    pub date: i32,
    pub content: MessageContent,
    pub pending: bool,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.is_pinned` / `updateMessageIsPinned`.
    pub is_pinned: bool,
    /// Schema `message.media_album_id`. `0` is not an album.
    pub media_album_id: i64,
    /// Schema `message.reply_markup` — all `replyMarkup*` constructors
    /// (B1). Rendered as the button grid under the message (inline) or the
    /// custom keyboard above the composer (showKeyboard).
    pub reply_markup: Option<ReplyMarkup>,
    /// Phase B3: schema `message.self_destruct_type` /
    /// Phase B3: schema `message.self_destruct_type` /
    /// `message.self_destruct_in` (TDLib 1.8.67 lines 3146–3147 / 3165).
    /// `None` for ordinary messages. Self-destructed rows leave via
    /// `updateDeleteMessages` (the normal delete path).
    pub self_destruct: Option<MessageSelfDestruct>,
    /// Phase B4: schema `message.auto_delete_in` (TDLib 1.8.67 lines
    /// 3148 / 3165) — locally decayed countdown until the chat's
    /// `message_auto_delete_time` setting deletes this message; `None`
    /// when never. Renders as a countdown chip on the row; the row
    /// itself leaves via `updateDeleteMessages`.
    pub auto_delete: Option<MessageAutoDelete>,
    /// Phase D2: schema `message.author_signature` (TDLib 1.8.67, lines
    /// 3155/3165) — author signature on channel posts and anonymous group
    /// messages. Rendered as a small signature line under the post, except
    /// under forwarded-message headers (which already attribute it).
    pub author_signature: Option<String>,
    /// M1: the send failed (`updateMessageSendFailed`). `true` means the
    /// row renders a failed state. A "Retry send" affordance is offered
    /// only when `can_retry` is also true — TDLib does not allow every
    /// failed send to be retried.
    pub failed: bool,
    /// M1 fix-up: `message.sending_state.can_retry` (TDLib 1.8.67 line
    /// 3038) — whether the failed send may be retried via
    /// `resendMessages`. Gates the retry affordance and the driver's
    /// `resend_failed_message`.
    pub can_retry: bool,
    /// M2: parsed `message.ephemeral_content` (TDLib 1.8.67 lines
    /// 3161/3165). When present the row renders it instead of `content`
    /// (use `effective_content`); it carries its own `reply_markup`.
    pub ephemeral: Option<EphemeralMessageContent>,
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

    /// Phase B3: timer badge for self-destructing media rows (`None` for
    /// ordinary messages). The `self_destruct_in` countdown decays locally
    /// against `now_ms` (`slow_mode_delay_expires_in` pattern, Phase A1);
    /// TDLib removes the row via `updateDeleteMessages` when it fires.
    pub fn self_destruct_badge(&self, now_ms: u64) -> Option<String> {
        self.self_destruct.as_ref().map(|sd| sd.badge_label(now_ms))
    }

    /// Phase B3: whether the row still has a live (not yet expired)
    /// `self_destruct_in` countdown — drives the 1-second render tick.
    pub fn has_live_self_destruct(&self, now_ms: u64) -> bool {
        self.self_destruct
            .as_ref()
            .and_then(|sd| sd.remaining_secs(now_ms))
            .is_some_and(|left| left > 0)
    }

    /// Phase B4: countdown chip label for `message.auto_delete_in`
    /// (schema 1.8.67 line 3148) — "🗑 59m left".
    pub fn auto_delete_chip(&self, now_ms: u64) -> Option<String> {
        self.auto_delete.as_ref().map(|ad| ad.chip_label(now_ms))
    }

    /// Phase B4: whether the row has a live auto-delete countdown —
    /// joins the same 1-second render tick as the self-destruct badge.
    pub fn has_live_auto_delete(&self, now_ms: u64) -> bool {
        self.auto_delete
            .as_ref()
            .is_some_and(|ad| ad.remaining_secs(now_ms) > 0)
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
    fn update_reply_markup(&mut self, id: MessageId, reply_markup: Option<ReplyMarkup>) -> bool {
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
    pub reply_markup: Option<ReplyMarkup>,
    /// Phase B3: carried through from `ParsedMessage` so search hits can
    /// become history rows without losing the timer badge.
    pub self_destruct: Option<MessageSelfDestruct>,
    /// Phase B4: same carry-through for the auto-delete countdown chip.
    pub auto_delete: Option<MessageAutoDelete>,
    /// Phase D2: same carry-through for the author signature line.
    pub author_signature: Option<String>,
    /// kit Phase 4: same carry-through for the in-bubble timestamp.
    pub date: i32,
}

impl SearchMessageHit {
    fn from_parsed(message: &ParsedMessage) -> Self {
        Self {
            chat_id: message.chat_id,
            message_id: message.id,
            preview: effective_content(&message.content, message.ephemeral.as_ref()).preview(),
            is_outgoing: message.is_outgoing,
            content: message.content.clone(),
            reply_to: message.reply_to.clone(),
            forward_info: message.forward_info.clone(),
            interaction_info: message.interaction_info.clone(),
            is_pinned: message.is_pinned,
            media_album_id: message.media_album_id,
            reply_markup: message.reply_markup.clone(),
            self_destruct: message.self_destruct,
            auto_delete: message.auto_delete,
            author_signature: message.author_signature.clone(),
            date: message.date,
        }
    }

    fn into_history(self) -> HistoryMessage {
        HistoryMessage {
            id: self.message_id,
            chat_id: self.chat_id,
            is_outgoing: self.is_outgoing,
            date: self.date,
            content: self.content,
            pending: false,
            reply_to: self.reply_to,
            forward_info: self.forward_info,
            interaction_info: self.interaction_info,
            is_pinned: self.is_pinned,
            media_album_id: self.media_album_id,
            reply_markup: self.reply_markup,
            self_destruct: self.self_destruct,
            auto_delete: self.auto_delete,
            author_signature: self.author_signature,
            failed: false,
            can_retry: false,
            ephemeral: None,
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

/// Slice media-shared-gallery: the per-chat shared-media gallery tabs (README
/// tab order: Media / Files / Music / Links / Voice / GIFs). Each tab maps to
/// one TDLib `searchChatMessages` filter constructor — verified concept-level
/// against every `searchMessagesFilter*` constructor in `schema/td_api.tl`
/// (lines 6275-6326), never a single-name grep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SharedMediaTab {
    #[default]
    Media,
    Files,
    Music,
    Links,
    Voice,
    Gifs,
}

impl SharedMediaTab {
    pub const ALL: [SharedMediaTab; 6] = [
        SharedMediaTab::Media,
        SharedMediaTab::Files,
        SharedMediaTab::Music,
        SharedMediaTab::Links,
        SharedMediaTab::Voice,
        SharedMediaTab::Gifs,
    ];

    /// Index into `SharedMediaState::tabs` (discriminant order = `ALL` order).
    pub fn index(self) -> usize {
        match self {
            SharedMediaTab::Media => 0,
            SharedMediaTab::Files => 1,
            SharedMediaTab::Music => 2,
            SharedMediaTab::Links => 3,
            SharedMediaTab::Voice => 4,
            SharedMediaTab::Gifs => 5,
        }
    }

    /// Tab label in the gallery tab bar.
    pub fn label(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "Media",
            SharedMediaTab::Files => "Files",
            SharedMediaTab::Music => "Music",
            SharedMediaTab::Links => "Links",
            SharedMediaTab::Voice => "Voice",
            SharedMediaTab::Gifs => "GIFs",
        }
    }

    /// `searchChatMessages` `filter` constructor (`schema/td_api.tl:11864`).
    pub fn filter_constructor(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "searchMessagesFilterPhotoAndVideo",
            SharedMediaTab::Files => "searchMessagesFilterDocument",
            SharedMediaTab::Music => "searchMessagesFilterAudio",
            SharedMediaTab::Links => "searchMessagesFilterUrl",
            SharedMediaTab::Voice => "searchMessagesFilterVoiceNote",
            SharedMediaTab::Gifs => "searchMessagesFilterAnimation",
        }
    }

    /// Empty-state glyph (TGX's `EmptySmartView` uses 96dp drawables; a
    /// single glyph keeps the one-shared-renderer rule).
    pub fn glyph(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "🖼",
            SharedMediaTab::Files => "📄",
            SharedMediaTab::Music => "🎵",
            SharedMediaTab::Links => "🔗",
            SharedMediaTab::Voice => "🎙",
            SharedMediaTab::Gifs => "▶",
        }
    }

    /// TGX empty-state title (`NoMediaToShow`, `NoDocumentsToShow`, … —
    /// `~/workspace/telegram-x/app/src/main/res/values/strings.xml:1601-1609`).
    pub fn empty_title(self) -> &'static str {
        match self {
            SharedMediaTab::Media => "No media to show",
            SharedMediaTab::Files => "No documents to show",
            SharedMediaTab::Music => "No music to show",
            SharedMediaTab::Links => "No links to show",
            SharedMediaTab::Voice => "No voice messages to show",
            SharedMediaTab::Gifs => "No GIFs to show",
        }
    }

    /// TGX empty-state description (`No*ToShowInChat` / `No*ToShowInChannel`,
    /// strings.xml:1610-1627) — `is_channel` selects the channel wording,
    /// exactly as `EmptySmartView.setMode(mode, isChannel, …)` does.
    pub fn empty_hint(self, is_channel: bool) -> &'static str {
        if is_channel {
            return match self {
                SharedMediaTab::Media => "Published photos and videos\nwill be shown here.",
                SharedMediaTab::Files => "Published documents and files\nwill be shown here.",
                SharedMediaTab::Music => "Published music and audio files\nwill be shown here.",
                SharedMediaTab::Links => "Published links and articles\nwill be shown here.",
                SharedMediaTab::Voice => "Published voice messages\nwill be shown here.",
                SharedMediaTab::Gifs => "Published GIFs will be shown here.",
            };
        }
        match self {
            SharedMediaTab::Media => {
                "Share photos and videos in this chat and\naccess them on any of your devices."
            }
            SharedMediaTab::Files => {
                "Share files and documents in this chat and\naccess them on any of your devices."
            }
            SharedMediaTab::Music => {
                "Share music and audio files in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Links => {
                "Share links in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Voice => {
                "Share voice messages in this chat and\naccess them on any device you have."
            }
            SharedMediaTab::Gifs => {
                "Share GIFs in this chat and\naccess them on any device you have."
            }
        }
    }
}

/// Slice media-shared-gallery: per-tab fetch state. The renderer keys "still
/// loading" / "empty" / "failed" off this enum — it never shows the empty
/// state while a fetch is in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SharedMediaTabStatus {
    #[default]
    Idle,
    Loading,
    Ready,
    Empty,
    Failed,
}

/// One gallery row: enough to label the item and jump to its message.
#[derive(Debug, Clone)]
pub struct SharedMediaItem {
    pub message_id: MessageId,
    pub glyph: &'static str,
    pub label: String,
}

impl SharedMediaItem {
    /// Row label from the parsed message: caption first, then file name /
    /// URL, then a kind fallback — never empty.
    pub fn from_parsed(tab: SharedMediaTab, message: &ParsedMessage) -> Self {
        let glyph = tab.glyph();
        let label = match &message.content {
            MessageContent::Photo(c) => caption_or(&[&c.caption], "Photo"),
            MessageContent::Video(c) => caption_or(&[&c.caption, &c.file_name], "Video"),
            MessageContent::Document(c) => caption_or(&[&c.caption, &c.file_name], "File"),
            MessageContent::Audio(c) => {
                let performer_title = if !c.performer.is_empty() || !c.title.is_empty() {
                    format!(
                        "{} — {}",
                        non_empty_or(&c.performer, "Unknown artist"),
                        non_empty_or(&c.title, "Unknown track"),
                    )
                } else {
                    String::new()
                };
                caption_or(&[&performer_title, &c.file_name], "Music")
            }
            MessageContent::VoiceNote(c) => caption_or(&[&c.caption], "Voice message"),
            MessageContent::Animation(c) => caption_or(&[&c.caption, &c.file_name], "GIF"),
            MessageContent::Text(c) => {
                let url = c
                    .entities
                    .iter()
                    .find_map(|entity| entity.open_href(&c.text))
                    .unwrap_or_default();
                let preview: String = c.text.chars().take(60).collect();
                caption_or(&[url, &preview], "Link")
            }
            _ => tab.label().to_string(),
        };
        SharedMediaItem {
            message_id: message.id,
            glyph,
            label,
        }
    }
}

fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn caption_or(candidates: &[&str], fallback: &str) -> String {
    candidates
        .iter()
        .find(|candidate| !candidate.is_empty())
        .map(|candidate| candidate.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Debug, Clone, Default)]
pub struct SharedMediaTabState {
    pub status: SharedMediaTabStatus,
    pub items: Vec<SharedMediaItem>,
    pub total_count: i32,
    pub error: String,
}

impl SharedMediaTabState {
    fn clear(&mut self) {
        self.status = SharedMediaTabStatus::Idle;
        self.items.clear();
        self.total_count = 0;
        self.error.clear();
    }
}

/// Slice media-shared-gallery: the gallery panel state. Switching chats or
/// closing bumps `generation`, so late `foundChatMessages` answers drop.
#[derive(Debug, Clone, Default)]
pub struct SharedMediaState {
    pub open: bool,
    pub chat_id: Option<ChatId>,
    pub active_tab: SharedMediaTab,
    pub tabs: [SharedMediaTabState; 6],
    pub generation: u64,
}

impl SharedMediaState {
    fn tab_state(&mut self, tab: SharedMediaTab) -> &mut SharedMediaTabState {
        &mut self.tabs[tab.index()]
    }

    /// Open the gallery for `chat_id`; returns the tab to fetch. Reopening
    /// for the same chat keeps already-fetched tabs.
    pub fn open_for(&mut self, chat_id: ChatId) -> SharedMediaTab {
        if self.open && self.chat_id == Some(chat_id) {
            return self.active_tab;
        }
        self.open = true;
        self.chat_id = Some(chat_id);
        self.active_tab = SharedMediaTab::Media;
        self.generation = self.generation.saturating_add(1);
        for tab in self.tabs.iter_mut() {
            tab.clear();
        }
        self.active_tab
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chat_id = None;
        self.generation = self.generation.saturating_add(1);
        for tab in self.tabs.iter_mut() {
            tab.clear();
        }
    }

    /// Select a tab; returns `true` when it was never fetched and the caller
    /// must send `searchChatMessages`.
    pub fn select_tab(&mut self, tab: SharedMediaTab) -> bool {
        self.active_tab = tab;
        self.tabs[tab.index()].status == SharedMediaTabStatus::Idle
    }

    /// Mark the tab loading and bump the generation; the caller stamps the
    /// returned generation on the request purpose so late answers drop.
    pub fn begin_fetch(&mut self, tab: SharedMediaTab) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.tab_state(tab).status = SharedMediaTabStatus::Loading;
        self.generation
    }

    /// A `foundChatMessages` answer: applied only when the chat, tab, and
    /// generation all match the open gallery.
    pub fn accept(
        &mut self,
        chat_id: ChatId,
        tab: SharedMediaTab,
        generation: u64,
        items: Vec<SharedMediaItem>,
        total_count: i32,
    ) {
        if !self.open || self.chat_id != Some(chat_id) || self.generation != generation {
            return;
        }
        let state = self.tab_state(tab);
        state.items = items;
        state.total_count = total_count;
        state.error.clear();
        state.status = if state.items.is_empty() {
            SharedMediaTabStatus::Empty
        } else {
            SharedMediaTabStatus::Ready
        };
    }

    pub fn fail(&mut self, chat_id: ChatId, tab: SharedMediaTab, generation: u64, error: String) {
        if !self.open || self.chat_id != Some(chat_id) || self.generation != generation {
            return;
        }
        let state = self.tab_state(tab);
        state.items.clear();
        state.total_count = 0;
        state.error = error;
        state.status = SharedMediaTabStatus::Failed;
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
    /// Slice S8: trending sets (`getTrendingStickerSets`) + premium-row flag.
    pub trending: Vec<StickerSetInfo>,
    pub trending_is_premium: bool,
    /// Slice S8: favorite stickers (`getFavoriteStickers`).
    pub favorites: Vec<StickerItem>,
    /// Slice S8: recent stickers (`getRecentStickers`).
    pub recent: Vec<StickerItem>,
    /// Slice S8: `searchStickerSets` / `searchStickers` results.
    pub found_sets: Vec<StickerSetInfo>,
    pub found_stickers: Vec<StickerItem>,
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
    /// Slice S9: GIF search via the animation search bot
    /// (`getInlineQueryResults`; schema 1.8.67, lines 6483, 13019).
    /// First page replaces; later pages append, deduped by file id.
    pub search_results: Vec<AnimationItem>,
    /// Slice S9: `next_offset` of the last search page ("" = exhausted).
    pub search_next_offset: String,
    /// Slice S9: `updateAnimationSearchParameters` (schema 1.8.67, line
    /// 11064) — the upstream animation-search provider name and its
    /// suggested search emojis.
    pub search_provider: String,
    pub provider_emojis: Vec<String>,
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

/// Phase C1: the tracked live call (signaling only — no media
/// transport; real audio/video is the C2 libtgvoip spike). States
/// follow TDLib's `CallState` (schema 1.8.67, lines 7054–7086):
/// Pending → ExchangingKeys → Ready → HangingUp →
/// Discarded / Error.
#[derive(Debug, Clone)]
pub struct ActiveCall {
    pub id: i32,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    pub state: CallState,
    /// When the call was created (outgoing) or first rang (incoming) —
    /// drives the "ringing" time on the overlay.
    pub started_at: Instant,
    /// When `callStateReady` arrived — the call-duration clock starts
    /// here.
    pub ready_at: Option<Instant>,
    pub ready: Option<ReadyParams>,
    pub transport: Option<TransportState>,
    pub transport_error: Option<String>,
    /// Phase C2b: chunks from `updateNewCallSignalingData`; the engine now
    /// consumes them too, while this queue remains the honest diagnostic
    /// record. Capped at
    /// `MAX_QUEUED_SIGNALING_CHUNKS`; overflow is counted, not kept.
    pub signaling_queue: Vec<Vec<u8>>,
    pub signaling_dropped: usize,
    /// Phase C1b: local-only mute toggle state. Tracked but a no-op
    /// without media transport (C2) — the UI labels it honestly.
    pub muted: bool,
    /// Phase C2e: local camera intent (UI toggle). Initialized from
    /// `is_video` — a video call starts with the camera on, a voice
    /// call with it off. The engine picks it up through
    /// `set_camera_enabled`.
    pub camera_on: bool,
    /// Phase C2i: local screen-share send intent (UI toggle). The
    /// engine picks it up through `set_screen_share_enabled`; screen
    /// share replaces the camera (ntgcalls forbids camera+screen in
    /// Capture mode).
    pub screen_sharing: bool,
    /// Phase C2e: peer camera state from the engine hook; `Inactive`
    /// until the first state callback arrives.
    pub remote_video: RemoteVideoState,
}

/// Phase C1: summary of the most recently ended call, driving the
/// call-end screen and the optional 1–5 rating card
/// (`callStateDiscarded.need_rating`, schema 1.8.67, line 7081).
#[derive(Debug, Clone)]
pub struct CallSummary {
    pub call_id: i32,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    /// Seconds between `callStateReady` and the end (0 when the call
    /// never connected).
    pub duration_secs: i64,
    pub had_audio: bool,
    /// Human-readable end line (reason-aware).
    pub end_line: String,
    pub need_rating: bool,
    /// `need_debug_information` / `need_log` are out of this slice
    /// (no media log exists; debug-info upload is C2) — kept so the
    /// end screen can say so honestly.
    pub need_debug_information: bool,
    pub need_log: bool,
    pub rating_sent: bool,
    pub debug_information_sent: bool,
    pub debug_information_error: Option<String>,
    /// Phase C2i: `sendCallLog` upload state (`need_log` from
    /// `callStateDiscarded`, schema 1.8.67 :7080).
    pub log_sent: bool,
    pub log_error: Option<String>,
    pub final_transport: Option<TransportState>,
    pub reconnect_attempts: usize,
    pub muted: bool,
}

impl ActiveCall {
    /// Seconds since `callStateReady` (the billable call duration).
    pub fn connected_secs(&self) -> i64 {
        self.ready_at
            .map(|t| t.elapsed().as_secs() as i64)
            .unwrap_or(0)
    }
}

/// Phase C3a: E2E verification state of a group call
/// (`updateGroupCallVerificationState`, schema 1.8.67, line 10836).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallVerificationState {
    pub generation: i32,
    pub emojis: Vec<String>,
}

/// Phase C3a: the tracked group call / voice chat (**signaling
/// only** — no media transport; real group audio/video is the C2
/// program). State follows TDLib's `groupCall` (schema 1.8.67, line
/// 7154) and its participant updates (`updateGroupCall` 10819,
/// `updateGroupCallParticipant` 10824, `updateGroupCallParticipants`
/// 10830, `updateGroupCallVerificationState` 10836).
#[derive(Debug, Clone)]
pub struct ActiveGroupCall {
    pub id: i32,
    pub title: String,
    pub is_video_chat: bool,
    pub is_joined: bool,
    /// `need_rejoin` arrived (kicked by network loss).
    pub need_rejoin: bool,
    /// UI "reconnecting" banner; set on `need_rejoin`, cleared when a
    /// rejoin is issued or a fresh joined `updateGroupCall` arrives.
    pub reconnecting: bool,
    /// Phase C2f: group-call rejoin attempts after `need_rejoin` —
    /// the C2d 1:1 reconnect discipline (max 3 attempts). Reset on a
    /// fresh joined `updateGroupCall` or when a new call takes the
    /// slot (`fresh()`).
    pub rejoin_attempts: usize,
    pub can_be_managed: bool,
    pub is_owned: bool,
    pub participant_count: i32,
    pub loaded_all_participants: bool,
    /// Sorted: recent speakers first (in `recent_speakers` order),
    /// then by `order` descending (lexicographic — schema: "The
    /// bigger is order, the higher is user in the list").
    pub participants: Vec<ParsedGroupCallParticipant>,
    /// Recent speakers as last reported by `updateGroupCall` (drives
    /// the participant ordering above).
    pub recent_speaker_order: Vec<MessageSender>,
    /// Phase C3a: local-only self mute. TDLib has no "mute self"
    /// request for group calls outside the join parameters, and there
    /// is no audio path yet (C2) — the UI labels this honestly as
    /// local-only. Sent as `is_muted` on (re)join.
    pub is_muted_self: bool,
    pub is_my_video_enabled: bool,
    pub is_my_video_paused: bool,
    pub can_enable_video: bool,
    pub mute_new_participants: bool,
    pub can_toggle_mute_new_participants: bool,
    pub verification: Option<GroupCallVerificationState>,
    /// The `Text` join payload returned by `joinVideoChat`; Phase C2g
    /// consumes it in the driver pump to finish the native group
    /// handshake.
    pub join_payload: String,
    /// Phase C2g: whether the native group transport handshake has
    /// completed (`ntg_connect` with the `joinVideoChat` answer). False
    /// when the join went out with the honest no-device params (no
    /// engine) or while the answer is still in flight.
    pub transport_ready: bool,
    /// Phase C2g: last native group-transport error (offer/connect/
    /// subscribe failures), mirroring the 1:1 call's `transport_error`.
    /// The join itself is never blocked by these.
    pub transport_error: Option<String>,
    /// Phase C2g: screen-share presentation state. `screen_share_pending`
    /// while the `startGroupCallScreenSharing` answer is in flight;
    /// `screen_sharing` once the presentation transport connected.
    pub screen_share_pending: bool,
    pub screen_sharing: bool,
    /// The `Text` presentation answer from
    /// `startGroupCallScreenSharing`; consumed by the driver pump to
    /// finish the presentation handshake.
    pub screen_share_answer: String,
    /// `HttpUrl` from `getVideoChatInviteLink`, fetched on demand.
    pub invite_link: Option<String>,
    /// Phase C2h: `scheduled_start_date` of a not-yet-started video
    /// chat (0 = live or unknown). Drives the "starts in …" card.
    pub scheduled_start_date: i32,
    /// `enabled_start_notification` from `updateGroupCall` (schema
    /// 1.8.67, :7154) — "notify me when this scheduled chat starts".
    pub enabled_start_notification: bool,
    /// Phase C2h: `rtmpUrl` from `getVideoChatRtmpUrl` /
    /// `replaceVideoChatRtmpUrl`, fetched on demand by an admin.
    pub rtmp_url: Option<String>,
    pub rtmp_stream_key: Option<String>,
    /// Phase C2h: in-call chat flags (schema 1.8.67, lines
    /// 7147-7150).
    pub can_send_messages: bool,
    pub are_messages_allowed: bool,
    pub can_toggle_are_messages_allowed: bool,
    pub can_delete_messages: bool,
    /// Phase C2h: in-call chat messages — append-only live feed from
    /// `updateNewGroupCallMessage` (TDLib has no history getter for
    /// group-call messages, so only messages seen while joined are
    /// shown; capped).
    pub messages: Vec<ParsedGroupCallMessage>,
    /// Phase C2h: recording state from `updateGroupCall`
    /// (`record_duration` seconds, 0 = not recording).
    pub record_duration: i32,
    pub is_video_recorded: bool,
}

impl ActiveGroupCall {
    /// Blank tracked call for a newly seen call id. Participant state
    /// repopulates from updates.
    pub(crate) fn fresh(id: i32) -> Self {
        ActiveGroupCall {
            id,
            title: String::new(),
            is_video_chat: false,
            is_joined: false,
            need_rejoin: false,
            reconnecting: false,
            rejoin_attempts: 0,
            can_be_managed: false,
            is_owned: false,
            participant_count: 0,
            loaded_all_participants: false,
            participants: Vec::new(),
            recent_speaker_order: Vec::new(),
            is_muted_self: false,
            is_my_video_enabled: false,
            is_my_video_paused: false,
            can_enable_video: false,
            mute_new_participants: false,
            can_toggle_mute_new_participants: false,
            verification: None,
            join_payload: String::new(),
            transport_ready: false,
            transport_error: None,
            screen_share_pending: false,
            screen_sharing: false,
            screen_share_answer: String::new(),
            invite_link: None,
            scheduled_start_date: 0,
            enabled_start_notification: false,
            rtmp_url: None,
            rtmp_stream_key: None,
            can_send_messages: false,
            are_messages_allowed: false,
            can_toggle_are_messages_allowed: false,
            can_delete_messages: false,
            messages: Vec::new(),
            record_duration: 0,
            is_video_recorded: false,
        }
    }

    /// Re-sort participants: recent speakers first (in reported
    /// order), then by `order` descending (lexicographic).
    fn sort_participants(&mut self) {
        let recent = self.recent_speaker_order.clone();
        self.participants.sort_by(|a, b| {
            let ra = recent.iter().position(|s| s == &a.participant_id);
            let rb = recent.iter().position(|s| s == &b.participant_id);
            match (ra, rb) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b.order.cmp(&a.order),
            }
        });
    }
}

impl CallSummary {
    /// Build the end screen from a terminal `updateCall`. `duration_secs`
    /// is the connected time (0 when the call never reached `Ready`).
    fn from_terminal(call: &ParsedCall, duration_secs: i64, had_audio: bool) -> Self {
        let (end_line, need_rating, need_debug_information, need_log) = match &call.state {
            CallState::Discarded {
                reason,
                need_rating,
                need_debug_information,
                need_log,
            } => (
                reason.summary(call.is_outgoing),
                *need_rating,
                *need_debug_information,
                *need_log,
            ),
            CallState::Error { code } => {
                // TDLib reports a missed outgoing call whose timeout
                // expired with error code 4005000; the raw message text
                // is never stored (it can contain secrets), so any other
                // code renders as the code only.
                let line = if *code == 4005000 {
                    "No answer — the call timed out".to_string()
                } else {
                    format!("Call failed (error {code})")
                };
                (line, false, false, false)
            }
            _ => ("Call ended".to_string(), false, false, false),
        };
        CallSummary {
            call_id: call.id,
            user_id: call.user_id,
            is_outgoing: call.is_outgoing,
            is_video: call.is_video,
            duration_secs,
            had_audio,
            end_line,
            need_rating,
            need_debug_information,
            need_log,
            rating_sent: false,
            debug_information_sent: false,
            debug_information_error: None,
            log_sent: false,
            log_error: None,
            final_transport: None,
            reconnect_attempts: 0,
            muted: false,
        }
    }
}

/// Cap for the honest signaling queue (C1: no consumer yet).
const MAX_QUEUED_SIGNALING_CHUNKS: usize = 32;

/// MED4: one-shot `getWebPageInstantView` answer — the URL plus the M2
/// `pageBlock*` content the IV reader renders.
#[derive(Debug, Clone)]
pub struct InstantViewPage {
    pub url: String,
    pub rich: RichMessageContent,
}

/// MED4b: composer `getLinkPreview` prefetch state (TGX `LinkPreview`).
#[derive(Debug, Clone, Default)]
pub struct ComposerLinkPreview {
    /// URL the prefetch was requested for.
    pub url: String,
    /// `None` while the request is in flight; `Some(None)` when TDLib
    /// 404s (no preview for this URL) — a refusal is never rendered as
    /// a card.
    pub preview: Option<Option<LinkPreview>>,
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
    /// M1: parsed `messageLink.link` from the last `getMessageLink` response
    /// (one-shot; the UI copies it to the clipboard and clears it).
    pub message_link_result: Option<String>,
    /// M1 fix-up: one-shot; set when "Share link" is gated off by
    /// `messageProperties.can_get_link == false` or the `getMessageLink`
    /// request errors. The UI drains it into the status note so the
    /// click never silently does nothing.
    pub message_link_error: Option<String>,
    /// MED4: `getOption("message_caption_length_max")` via `updateOption`
    /// (TDLib 1.8.67, `schema/td_api.tl:10926`); default 1024 is TDLib's
    /// compiled default. Guards caption edits and media-send captions.
    pub message_caption_length_max: i32,
    /// Slice CL1: `getOption("pinned_chat_count_max")` /
    /// `getOption("pinned_archived_chat_count_max")` via `updateOption`
    /// (schema 1.8.67, line 13674). Defaults 5 / 100 are TDLib's
    /// compiled defaults; the server raises them for Premium. Used for
    /// the client-side pin-limit pre-check (TGX `ChatsController`
    /// `PinTooMuchWarn` / `ErrorPinnedChatsLimit` behavior).
    pub pinned_chat_count_max: i32,
    pub pinned_archived_chat_count_max: i32,
    /// Slice CL1: one-shot error from a refused chat-list action
    /// (`toggleChatIsPinned`, `toggleChatIsMarkedAsUnread`,
    /// `deleteChatHistory`). The UI drains it into the status note so a
    /// refused action never looks like it worked.
    pub chat_action_error: Option<String>,
    /// Slice CL3: one-shot `reportChat` outcome (`reportChatResultOk`
    /// vs option/text/messages required). The UI drains it into the
    /// status note next to `chat_action_error`.
    pub report_chat_outcome: Option<String>,
    /// MED4: one-shot `getWebPageInstantView` answer for the IV reader.
    /// The UI drains it (opens the reader) and clears it.
    pub instant_view: Option<InstantViewPage>,
    /// MED4: one-shot fallback URL when `getWebPageInstantView` errors
    /// (TDLib 404s when the page has no Instant View). The UI drains it
    /// into the browser — TGX behaves the same.
    pub instant_view_fallback_url: Option<String>,
    /// MED4: pending `getWebPageInstantView` URLs by `RequestId`
    /// (`RequestPurpose` stays `Copy`, so the URL rides here).
    pub instant_view_urls: HashMap<RequestId, String>,
    /// MED4b: composer `getLinkPreview` prefetch state — the chip reads
    /// this. Replaced on every new request; cleared when the composer's
    /// detected URL changes away or the composer is submitted.
    pub composer_preview: Option<ComposerLinkPreview>,
    /// MED4b: pending `getLinkPreview` URLs by `RequestId` (same
    /// `Copy`-purpose pattern as `instant_view_urls`).
    pub composer_preview_urls: HashMap<RequestId, String>,
    /// MED2 fix-up: one-shot; set when TDLib refuses a `recognizeSpeech`
    /// request. The UI drains it into the status note — previously the
    /// error fell into the `_ => {}` swallower and the user saw
    /// "transcription requested" followed by silence.
    pub recognize_speech_error: Option<String>,
    /// M1 fix-up: one-shot; set when a `resendMessages` request errors.
    /// The UI drains it into the status note — previously the error fell
    /// into the `_ => {}` swallower and the user saw "retrying send…"
    /// followed by silence.
    pub resend_error: Option<String>,
    /// Slice G1 fix-up: one-shot; set when an invite-link mutation
    /// (create/edit/revoke/replace-primary) errors. The UI drains it into
    /// the status note — the previously loaded list is kept, not wiped.
    pub invite_link_error: Option<String>,
    /// M1: `getChatScheduledMessages` results — the chat's scheduled sends,
    /// with `scheduling_state` showing the planned send time.
    pub scheduled_messages: Vec<ParsedMessage>,
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
    /// Phase S2: cached `getStorageStatistics` answer (aggregated by file
    /// type, TGX `TGStorageStats` style); drives the storage-usage overlay,
    /// including the "Secret media and files" category.
    pub storage_stats: Option<StorageStats>,
    /// Phase S2: a `getStorageStatistics` round trip is in flight.
    pub storage_stats_loading: bool,
    /// Slice A2: cached `getPasswordState` / `setPassword` /
    /// `setRecoveryEmailAddress` answer; drives the two-step
    /// verification overlay. Replaced only by our own
    /// `PasswordStateOp` answers — never mutated optimistically.
    pub password_state: Option<PasswordState>,
    /// Slice A2: a 2FA management round trip is in flight (fetch or
    /// mutation); the overlay shows progress and disables submits.
    pub password_state_loading: bool,
    /// Slice A2: honest one-line failure of the last 2FA management
    /// request (TDLib's actual error, classified — never a fake
    /// success). Cleared on the next attempt and on success.
    pub password_op_error: Option<String>,
    /// Slice A3: cached `getActiveSessions` answer (TGX `SessionsInfo`
    /// style); drives the Active Sessions overlay. Incomplete login
    /// attempts (`is_password_pending`) render in their own section.
    pub sessions: Option<Vec<ParsedSession>>,
    /// Slice A3: a `getActiveSessions` round trip is in flight.
    pub sessions_loading: bool,
    /// Slice A3: a `terminateSession` / `terminateAllOtherSessions` round
    /// trip is in flight — terminate buttons stay disabled meanwhile.
    pub sessions_mutating: bool,
    /// Slice A3: honest one-line failure of the last sessions fetch or
    /// terminate (classified from the TDLib error code, never the native
    /// message). Cleared on the next successful fetch.
    pub sessions_error: Option<String>,
    /// Slice A3: a terminate succeeded — the old cache stays visible and
    /// is refetched from the authoritative answer on the next ingest
    /// (the `saved_sounds_stale` pattern); never an optimistic delete.
    pub sessions_stale: bool,
    /// Slice A4: cached `getConnectedWebsites` answer (TGX
    /// `SettingsWebsitesController` style); drives the Connected Websites
    /// overlay.
    pub connected_websites: Option<Vec<ParsedWebsite>>,
    /// Slice A4: a `getConnectedWebsites` round trip is in flight.
    pub connected_websites_loading: bool,
    /// Slice A4: a `disconnectWebsite` / `disconnectAllWebsites` round trip
    /// is in flight — disconnect buttons stay disabled meanwhile.
    pub websites_mutating: bool,
    /// Slice A4: honest one-line failure of the last websites fetch or
    /// disconnect (classified from the TDLib error code, never the native
    /// message). Cleared on the next successful fetch.
    pub websites_error: Option<String>,
    /// Slice A4: a disconnect succeeded — the old cache stays visible and
    /// is refetched from the authoritative answer on the next ingest
    /// (the `saved_sounds_stale` pattern); never an optimistic delete.
    pub websites_stale: bool,
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
    /// Phase B1: secret-chat records keyed by `secret_chat_id`
    /// (`updateSecretChat` / `getSecretChat` answers). Kept at the
    /// session level because `updateSecretChat` is guaranteed to arrive
    /// *before* the chat identifier is returned (schema 1.8.67, line
    /// 10740), so a secret chat's chat summary may not exist yet when
    /// its state does. The full record (including `key_hash`) is kept
    /// for the B2 key-verification UI.
    pub secret_chat_states: HashMap<i32, ParsedSecretChat>,
    /// Phase B1: secret_chat_ids whose state still needs a `getSecretChat`
    /// fetch (e.g. a secret chat loaded from the local DB with no state
    /// seen yet). Drained by the driver's `maybe_fetch_secret_chat_states`.
    pub secret_chat_fetch_queue: Vec<i32>,
    /// Phase C1: the tracked live call, if any. **Signaling only** —
    /// TDLib transports no audio/video (official clients use
    /// libtgvoip); real media transport is the C2 spike.
    pub active_call: Option<ActiveCall>,
    /// Phase C1: summary of the most recently ended call, driving the
    /// call-end screen and the optional 1–5 rating card
    /// (`callStateDiscarded.need_rating`).
    pub call_summary: Option<CallSummary>,
    /// Phase C1: last async call-request error (e.g. `createCall`
    /// rejected), shown on the call overlay and cleared when
    /// dismissed. Never a secret.
    pub call_error: Option<String>,
    /// Phase C3a: last async group-call request error (e.g.
    /// `joinVideoChat` rejected), shown on the group-call overlay and
    /// cleared when dismissed. Never a secret.
    pub group_call_error: Option<String>,
    /// Phase C1: incoming calls that arrived while another call was
    /// active — the driver discards them (busy) via `discardCall`.
    /// Entries are `(call_id, user_id, is_video)` so the decline
    /// reports the actual call kind rather than a hardcoded one.
    pub call_busy_decline_queue: Vec<(i32, i64, bool)>,
    /// Phase C2i: incoming calls auto-declined while busy, kept as
    /// `(user_id, is_video)` so the UI can say so honestly instead of
    /// declining silently. Drained by the UI banner.
    pub call_busy_declined: Vec<(i64, bool)>,
    /// Phase C2i: recent calls from `searchCallMessages` (server-side
    /// history, schema 1.8.67 :11903) for the Recent-calls tab, newest
    /// first.
    pub recent_calls: Vec<ParsedMessage>,
    /// `next_offset` from the last `foundMessages` page; empty starts
    /// (or restarts) the list.
    pub recent_calls_offset: String,
    /// A `searchCallMessages` page is in flight.
    pub recent_calls_loading: bool,
    /// The last `searchCallMessages` request failed.
    pub recent_calls_error: bool,
    /// Phase C2i: "who can call me"
    /// (`userPrivacySettingAllowCalls`, schema 1.8.67 :9006).
    pub call_privacy_allow_calls: Option<PrivacyWho>,
    /// Phase C2i: peer-to-peer calls
    /// (`userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67 :9009).
    pub call_privacy_p2p: Option<PrivacyWho>,
    /// A privacy get/set round-trip is in flight (see
    /// `call_privacy_pending` — fetch sends two gets, so this clears
    /// only when the last response lands).
    pub call_privacy_loading: bool,
    /// Outstanding call-privacy get/set round-trips.
    pub call_privacy_pending: u8,
    /// The last privacy get/set failed.
    pub call_privacy_error: bool,
    /// Phase C2i: local call preferences (confirm-before-calling,
    /// less-data), persisted via `settings::CallPrefs`. The driver
    /// loads them at startup; the UI saves on toggle.
    pub call_prefs: CallPrefs,
    /// MED1: local media preferences (remember-media-grouping),
    /// persisted via `settings::MediaPrefs`. Loaded at startup like
    /// `call_prefs`; the UI saves on toggle.
    pub media_prefs: MediaPrefs,
    /// Slice A6: local contacts preferences (sync toggle), persisted
    /// via `settings::ContactPrefs`. Loaded at startup like
    /// `call_prefs`; the UI saves on toggle. Client-side only — TDLib
    /// 1.8.67 has no contact-sync switch (verified concept-level; TGX
    /// implements sync client-side in `TdlibContactManager`).
    pub contact_prefs: ContactPrefs,
    /// Phase C3a: the tracked group call / voice chat, if any.
    /// **Signaling only** — TDLib transports no audio/video; the
    /// `joinVideoChat` response payload is stored (`join_payload`) and
    /// never consumed (real media transport is the C2 program).
    pub active_group_call: Option<ActiveGroupCall>,
    /// Phase C3a: group-call ids whose full `groupCall` still needs a
    /// `getGroupCall` fetch (queued from the `createVideoChat`
    /// `groupCallId` answer). Drained by the driver.
    pub group_call_fetch_queue: Vec<i32>,
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
    /// B1: last `loginUrlInfo*` / `httpUrl` answer for a login-URL button
    /// press. The UI takes it on the next poll: `Open` opens the URL in the
    /// OS browser, `RequestConfirmation` asks for consent, `Failed` opens
    /// the button's raw URL in the browser.
    pub last_login_url_info: Option<LoginUrlInfo>,
    /// B1: the login button's request context, kept while `getLoginUrlInfo`
    /// (then `getLoginUrl`) is in flight so an error can degrade to a plain
    /// URL button press.
    pub login_url_request: Option<LoginUrlRequest>,
    /// Slice P1: the in-flight payment request context (`getPaymentForm`,
    /// `validateOrderInfo`, `sendPaymentForm`, `getPaymentReceipt`).
    pub payment_request: Option<PaymentRequest>,
    /// Slice P1: the fetched `paymentForm`, shown in the checkout dialog.
    pub payment_form: Option<PaymentFormData>,
    /// Slice P1: `getPaymentForm` is in flight (dialog shows a spinner).
    pub payment_form_loading: bool,
    /// Slice P1: the validated order info + shipping options from
    /// `validateOrderInfo`.
    pub payment_validated: Option<ValidatedOrderInfoData>,
    /// Slice P1: the chosen shipping option id (default: the first).
    pub payment_shipping_id: Option<String>,
    /// Slice P1: the fetched `paymentReceipt`, shown in the receipt dialog.
    pub payment_receipt: Option<PaymentReceiptData>,
    /// Slice P1: receipt dialog visibility.
    pub payment_receipt_open: bool,
    /// Slice P1: latest payment error / outcome note, shown in the
    /// checkout dialog (never a secret — order fields and credentials are
    /// never echoed here).
    pub payment_note: Option<String>,
    /// Slice P1: `sendPaymentForm` is in flight — the Pay button shows
    /// "Processing…" and is disabled until the `paymentResult` answer (or
    /// error) lands, so a double-click can't submit twice.
    pub payment_sending: bool,
    /// Slice P1: a failed `getPaymentReceipt`, drained into the status
    /// note by `poll_live` — the checkout dialog (which renders
    /// `payment_note`) may be closed when the receipt fetch fails.
    pub payment_receipt_error: Option<String>,
    /// Slice P1: `paymentResult.verification_url` from a non-successful
    /// `sendPaymentForm` — the UI takes it on the next poll and opens it
    /// in the OS browser (3-D Secure and similar).
    pub payment_verification_url: Option<String>,
    /// B1: force-reply target set when an incoming message carrying
    /// force-reply markup (`replyMarkupForceReply`, or `force_reply` on an
    /// inline / show-keyboard markup) arrives. The UI drains it on the
    /// next render: composer gets the reply-to and focus.
    pub pending_force_reply: Option<ForceReplyTarget>,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: HashMap<i32, ParsedFile>,
    /// `downloadFile` in flight (until completed, undownloadable, idle, or error).
    pub downloading: HashSet<i32>,
    /// Subset of `downloading` the user explicitly started (history rows,
    /// viewer) — these surface in the downloads manager. Automatic
    /// thumbs/avatars/sounds are not tracked here.
    pub user_downloads: HashSet<i32>,
    /// `downloadFile` requests TDLib answered with an error (file id → still
    /// in `downloading` until unstuck; the UI shows "failed — retry").
    /// Cleared when a new download starts or the file completes.
    pub failed_downloads: HashSet<i32>,
    /// Recently completed downloads (file ids, most recent last, capped) for
    /// the downloads manager's "recent" list. Recorded only when a file was
    /// in `downloading` and its `updateFile` shows completion — pre-existing
    /// local files don't count.
    pub completed_downloads: VecDeque<i32>,
    /// Downloads manager panel open (right side, next to the info panel).
    pub downloads_panel_open: bool,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    download_extras: HashMap<u64, i32>,
    pub search: SearchState,
    pub chat_search: ChatSearchState,
    /// Slice media-shared-gallery: per-chat shared-media gallery state
    /// (Media / Files / Music / Links / Voice / GIFs tabs).
    pub shared_media: SharedMediaState,
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
    /// Slice B2: pending bot `start_parameter` per chat
    /// (`internalLinkTypeBotStart`, schema 1.8.67 line 9399) — from a
    /// `t.me/<bot>?start=<param>` deep link. The UI shows the START
    /// button while set; pressing it sends `sendBotStartMessage` (line
    /// 12216) with the parameter and clears the entry.
    pub bot_start_params: HashMap<i64, String>,
    /// Slice B2: `getBotSimilarBots` results (schema 1.8.67, line 11640)
    /// for the similar-bots section of the bot profile, keyed by bot
    /// user id. The `users` ids resolve to names via `Session::users`.
    pub similar_bots: HashMap<i64, SimilarBotsFetch>,
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
    /// Slice CL2: archive auto-settings from `getArchiveChatListSettings`
    /// (schema 1.8.67, line 13421). `None` until the first fetch; the
    /// archive-settings panel fetches on open (TGX
    /// `SettingsArchiveChatListController` does the same).
    pub archive_chat_list_settings: Option<ArchiveChatListSettings>,
    /// Slice CL2: the archive-settings panel is fetching its truth.
    pub archive_settings_loading: bool,
    /// Slice CL2: the archive-settings panel is open.
    pub archive_settings_open: bool,
    /// Slice CL2: the Archived section is collapsed to a single summary
    /// row (TGX `archiveCollapsed`). Client-side only, per session —
    /// not persisted.
    pub archive_collapsed: bool,
    /// Phase 6: user directory from `updateUser`, keyed by user id. Feeds
    /// the contacts list and the user info panel.
    pub users: HashMap<i64, ParsedUser>,
    /// Phase 6: `getContacts` result — user ids, in server order. `None`
    /// until the first `users` response; `contacts_error` records a failed
    /// fetch so the UI can offer a retry.
    pub contacts: Option<Vec<i64>>,
    pub contacts_error: bool,
    /// Slice A6: outcome line for contacts mutations (`removeContacts`,
    /// `importContacts`, `clearImportedContacts`) — set on ok and on
    /// error, cleared by the next mutation; shown in the contacts
    /// settings section.
    pub contacts_notice: Option<String>,
    /// Phase 6: cached `getUserFullInfo` bios, keyed by user id. Presence
    /// records "fetched" so the driver never refetches.
    pub user_full_infos: HashMap<i64, UserFullInfoData>,
    /// Phase 6: cached `getSupergroupFullInfo`, keyed by supergroup id.
    /// Presence records "fetched".
    pub supergroup_full_infos: HashMap<i64, SupergroupFullInfoData>,
    /// Phase D2: `getChatStatistics` fetch state, keyed by chat id.
    pub chat_statistics: HashMap<i64, ChatStatisticsFetch>,
    /// Phase D3a: `getChatInviteLinks` fetch state, keyed by chat id.
    pub invite_links: HashMap<i64, InviteLinkFetch>,
    /// Phase D3a: `getChatJoinRequests` fetch state, keyed by chat id.
    pub join_requests: HashMap<i64, JoinRequestFetch>,
    /// Phase D3a: latest `updateChatPendingJoinRequests` total per chat
    /// (schema 1.8.67, line 10555). The full request list still needs
    /// `getChatJoinRequests`; this is only the badge count.
    pub pending_join_request_counts: HashMap<i64, i32>,
    /// Phase D3c: `getChatEventLog` fetch state, keyed by chat id.
    pub event_logs: HashMap<i64, ChatEventLogFetch>,
    /// Slice G2: per-chat event-log filters (`chatEventLogFilters`,
    /// schema 1.8.67, line 7956). Absent = all event types (the schema's
    /// `null`).
    pub event_log_filters: HashMap<i64, ChatEventLogFilterSet>,
    /// Slice G2: per-chat event-log text search (the `query` parameter of
    /// `getChatEventLog`, schema 1.8.67, line 15252). Absent = no search.
    pub event_log_queries: HashMap<i64, String>,
    /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746),
    /// keyed by supergroup id. Drives the channel "Sign messages" toggle.
    pub supergroup_sign_messages: HashMap<i64, bool>,
    /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
    /// 2746), keyed by supergroup id. Drives the "Show authors" toggle.
    pub supergroup_show_message_sender: HashMap<i64, bool>,
    /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
    /// (schema 1.8.67, line 2792), keyed by supergroup id.
    pub supergroup_anti_spam_enabled: HashMap<i64, bool>,
    /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
    /// (schema 1.8.67, line 2792), keyed by supergroup id. Gates the
    /// anti-spam toggle.
    pub supergroup_can_toggle_anti_spam: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_manage_topics` per supergroup
    /// (schema 1.8.67, line 1092). Gates forum topic management.
    pub supergroup_manage_topics_right: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_change_info` per supergroup
    /// (schema 1.8.67, line 1092). `toggleSupergroupSignMessages`
    /// requires this right.
    pub supergroup_change_info_right: HashMap<i64, bool>,
    /// Slice G2: the viewer's `rights.can_send_welcome_messages` per
    /// supergroup (schema 1.8.67, line 1090). Gates welcome-message
    /// management.
    pub supergroup_send_welcome_right: HashMap<i64, bool>,
    /// Slice G2: `chat.has_welcome_messages` (schema 1.8.67, line 3627)
    /// / `updateChatHasWelcomeMessages` (line 10600), keyed by chat id.
    pub chat_has_welcome_messages: HashMap<i64, bool>,
    /// Slice G2: the welcome-message pack per chat
    /// (`updateChatWelcomeMessages`, schema 1.8.67, line 10649).
    pub welcome_messages: HashMap<i64, Vec<ParsedWelcomeMessage>>,
    /// Slice G2: `loadChatWelcomeMessages` fetch state, keyed by chat id.
    pub welcome_message_fetches: HashMap<i64, WelcomeMessagesFetch>,
    /// Slice G2: `(level, boost_count)` from `getChatBoostStatus`
    /// (schema 1.8.67, lines 13917/6943), keyed by chat id.
    pub chat_boost_status: HashMap<i64, (i32, i32)>,
    /// Slice G2: available boost slot ids from `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914), keyed by chat id. The driver consumes
    /// them to chain `boostChat` once per boost intent.
    pub boost_slots_by_chat: HashMap<i64, Vec<i32>>,
    /// Slice G2: the chat id of a pending user boost intent — set by
    /// `request_chat_boost`, consumed by the driver's `boostChat` chain.
    pub boost_intent: Option<i64>,
    /// Slice G2: channel-comments viewer — the latest
    /// `getMessageThreadHistory` result (channel post → comment thread).
    pub comment_thread: Option<CommentThreadFetch>,
    /// Slice CL: chat-list peek preview — the latest `getChatHistory`
    /// result for one unopened chat (`parity:chatlist-chat-preview`).
    pub chat_preview_fetch: Option<PreviewHistoryFetch>,
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
    /// 2500/`chatAdministratorRights` 1092). `setChatSlowModeDelay` requires
    /// 13551). Absent = unknown, treated as lacking the right.
    pub supergroup_restrict_right: HashMap<i64, bool>,
    /// Phase D3a: the viewer's `rights.can_invite_users` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, line
    /// 1092). Invite-link management requires this right (or creator
    /// status). Absent = unknown, treated as lacking the right.
    pub supergroup_invite_right: HashMap<i64, bool>,
    /// Phase D3b: `getChatAdministrators` fetch state, keyed by chat id.
    pub admin_lists: HashMap<i64, AdminListFetch>,
    /// Phase D3b / slice G1: `getSupergroupMembers` fetch state for the
    /// promote member picker and the member-management dialog, keyed by
    /// (chat id, filter). One page per filter.
    pub supergroup_members: HashMap<(i64, MemberListFilter), SupergroupMembersFetch>,
    /// B4: `getPollVoters` fetch state for the poll-voters dialog, keyed
    /// by (chat id, message id, 0-based option index). One page per key.
    pub poll_voters: HashMap<(i64, i64, i32), PollVotersFetch>,
    /// Bots slice: the single active `getInlineQueryResults` fetch (the
    /// composer has one active inline query, so a slot — not a map).
    pub inline_query: Option<InlineQuerySlot>,
    /// Slice G1: `getBasicGroupFullInfo` fetch state (the member list for
    /// basic groups), keyed by chat id. Reuses `SupergroupMembersFetch`
    /// (Loading / Loaded / Failed).
    pub basic_group_members: HashMap<i64, SupergroupMembersFetch>,
    /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
    /// 2733/2746), keyed by supergroup id. Drives the "Approve new
    /// members" toggle.
    pub supergroup_join_by_request: HashMap<i64, bool>,
    /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67, lines
    /// 2736/2746), keyed by supergroup id. Set by
    /// `toggleSupergroupIsBroadcastGroup` (one-way upgrade).
    pub supergroup_is_broadcast: HashMap<i64, bool>,
    /// Slice G1: `addChatMembers` failure count from the last add
    /// attempt — `failedToAddMembers.failed_to_add_members.len()` for the
    /// bulk path (schema 1.8.67, line 3640), or the accumulated per-user
    /// `addChatMember` `failedToAddMembers` counts plus error responses
    /// for basic groups — keyed by chat id.
    /// Reset when a new add starts; cleared when the dialog closes.
    pub add_members_failed: HashMap<i64, i32>,
    /// Slice G1 fix-up: chat ids whose member-list caches were dropped by
    /// an `updateChatMember` while a member dialog may be open. One-shot;
    /// the UI drains it and refetches the open dialog's page.
    pub member_list_stale: Vec<i64>,
    /// Slice G1: last member-action failure for the member-management
    /// dialog (`setChatMemberTag` / `setChatMemberStatus`), keyed by
    /// chat id. The dialog reads the member-list fetch states, not
    /// `admin_lists`, so action failures need their own slot to be
    /// visible where the action was taken. Cleared when the dialog
    /// opens.
    pub member_action_error: HashMap<i64, String>,
    /// Phase D3b: one administrator's parsed `chatAdministratorRights`
    /// fetch state, keyed by (chat_id, user_id). Filled by `getChatMember`
    /// (purpose `GetAdminRights`); drives the edit-rights dialog.
    pub admin_rights: HashMap<(i64, i64), AdminRightsFetch>,
    /// Phase D3b: the viewer's `rights.can_promote_members` per supergroup
    /// from own `chatMemberStatusAdministrator` (schema 1.8.67, line
    /// 1092). Admin management requires this right (or creator status).
    /// Absent = unknown, treated as lacking the right.
    pub supergroup_promote_right: HashMap<i64, bool>,
    /// Slice G1: own `rights.can_manage_tags` per supergroup (schema
    /// 1.8.67, line 1092) — gates custom-title changes for others.
    pub supergroup_manage_tags_right: HashMap<i64, bool>,
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
    /// Phase 9.3: story-posting round-trip state — `canPostStory`
    /// eligibility plus the `postStory` pending/succeeded/failed outcome
    /// the composer renders.
    pub story_post: StoryPostState,
    /// Phase 9.5: paginated viewers list for the story currently open in
    /// the viewer (`getStoryInteractions` pages, `Session::story_viewers`
    /// accumulates them). `None` when the panel is closed or the viewer
    /// moved to a different story.
    pub story_viewers: Option<StoryViewersState>,
    /// Phase 9.5: the in-progress `reportStory` flow for the story open in
    /// the viewer — the reason picker and the optional details step.
    /// `None` when no report is in flight.
    pub story_report: Option<StoryReportFlow>,
    /// Phase 9.5: story stealth-mode state from `updateStoryStealthMode`
    /// (TDLib 1.8.67, `schema/td_api.tl:10919`); 0/0 = disabled, no
    /// cooldown — the schema exposes no getter, so this only ever
    /// reflects updates TDLib has pushed.
    pub story_stealth: StoryStealthMode,
    /// Phase 9.5: last `activateStoryStealthMode` error (e.g. Premium
    /// required), cleared when a new activation is sent.
    pub story_stealth_error: Option<String>,
    /// A5: latest `checkChatUsername` verdict for the edit-profile
    /// dialog: (checked username text, result). Written by the driver
    /// before `apply` takes the pending request; the dialog only shows it
    /// when it matches the current input text (stale verdicts ignored).
    pub username_check: Option<(String, UsernameCheckResult)>,
    /// A5: username text of the in-flight `checkChatUsername`.
    pub username_check_pending: Option<String>,
    /// A5: last profile-edit request failure
    /// (`setName`/`setBio`/`setUsername`/`checkChatUsername`/
    /// `reorderActiveUsernames`/`toggleUsernameIsActive`/
    /// `setProfilePhoto`/`deleteProfilePhoto`), shown in the
    /// edit-profile dialog. Cleared when the dialog opens.
    pub profile_edit_error: Option<String>,
    /// Phase 9.5: chat ids from the last `getChatsToPostStories` answer —
    /// the composer's "post as" picker (channels/supergroups where the
    /// user has the `can_post_stories` admin right).
    pub story_post_as_chats: Vec<i64>,
    /// Phase 9.5: posted-story management round-trip state (edit /
    /// cover / privacy) rendered as one status line.
    pub story_manage: StoryManageState,
    /// Phase 9.1: `loadActiveStories(storyListMain)` was issued. A retry is
    /// allowed (the flag is reset) if the attempt failed.
    pub stories_active_loaded: bool,
    diagnostics: Arc<dyn DiagnosticSink>,
}

/// Phase 9.3: honest `postStory` UI states — pending while TDLib uploads,
/// succeeded / failed when `updateStoryPostSucceeded` /
/// `updateStoryPostFailed` arrive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StoryPostOutcome {
    #[default]
    None,
    /// `postStory` answered with a `story`; `story_id` is the temporary
    /// id the succeeded/failed updates correlate against
    /// (`old_story_id` / `story.id`).
    Posting {
        story_id: i32,
    },
    Succeeded,
    Failed(String),
}

/// Phase 9.3: the composer's server-side state — latest `canPostStory`
/// answer (checked before every post), its last error, and the
/// `postStory` outcome.
#[derive(Debug, Clone, Default)]
pub struct StoryPostState {
    pub eligibility: Option<CanPostStoryResult>,
    pub check_error: Option<String>,
    pub outcome: StoryPostOutcome,
}

/// Phase 9.5: honest pending/failed states for `editStory` /
/// Phase 9.5: the viewers panel's accumulated `getStoryInteractions`
/// pages for one story. `next_offset` empty = no more pages.
#[derive(Debug, Clone, Default)]
pub struct StoryViewersState {
    pub chat_id: i64,
    pub story_id: i32,
    pub total_count: i32,
    pub rows: Vec<StoryInteractionView>,
    pub next_offset: String,
    pub loading: bool,
    pub error: Option<String>,
}

/// Phase 9.5: honest `reportStory` UI states. The initial request carries
/// an empty option id; TDLib answers `OptionRequired` (reason picker),
/// then `TextRequired` (optional details), then `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryReportStage {
    /// The initial `reportStory` is in flight.
    Checking,
    /// The user must pick a reason before the flow can continue.
    PickOption {
        title: String,
        options: Vec<ReportOption>,
    },
    /// A follow-up `reportStory` (option picked, or details submitted)
    /// is in flight.
    Sending,
    /// The server wants extra text details for `option_id`.
    TextRequired {
        option_id: String,
        is_optional: bool,
    },
    Reported,
    Failed(String),
}

/// Phase 9.5: the in-progress `reportStory` flow for one story.
#[derive(Debug, Clone)]
pub struct StoryReportFlow {
    pub chat_id: i64,
    pub story_id: i32,
    pub stage: StoryReportStage,
}

/// Phase 9.5: story stealth-mode state (`updateStoryStealthMode`,
/// TDLib 1.8.67, `schema/td_api.tl:10919`). Unix timestamps; 0 = the
/// corresponding state is off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoryStealthMode {
    pub active_until_date: i32,
    pub cooldown_until_date: i32,
}

impl StoryStealthMode {
    /// `true` while stealth hides the user's story views (`now` is a
    /// Unix timestamp).
    pub fn is_active(&self, now: i64) -> bool {
        i64::from(self.active_until_date) > now
    }

    /// `true` while stealth cannot be re-enabled (and is not active).
    pub fn is_cooling_down(&self, now: i64) -> bool {
        !self.is_active(now) && i64::from(self.cooldown_until_date) > now
    }
}

/// `editStoryCover` / `setStoryPrivacySettings` — all `= Ok` calls, so
/// success only clears the spinner (the edited story itself arrives via
/// `updateStory`); failures surface the sanitized TDLib error. One
/// shared slot: the viewer disables its management buttons while
/// `pending`, so only one call is ever in flight.
#[derive(Debug, Clone, Default)]
pub struct StoryManageState {
    pub pending: bool,
    pub error: Option<String>,
}

/// Phase 6: which info panel is open in the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoPanelTarget {
    User(i64),
    Supergroup(i64),
    /// Slice G1: basic-group info panel, keyed by basic group id.
    BasicGroup(i64),
    /// Phase D2: channel/group statistics view, keyed by chat id. The
    /// `getChatStatistics` fetch is gated on
    /// `supergroupFullInfo.can_get_statistics` before opening.
    Statistics(i64),
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
    /// A5: `chatPhoto.id` (schema 1.8.67, line 1030) — the
    /// `profile_photo_id` for `deleteProfilePhoto`.
    pub photo_id: Option<i64>,
    /// Slice A6: `userFullInfo.block_list` is `blockListMain` — drives
    /// the Block/Unblock label in the user info panel.
    pub blocked: bool,
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
    /// Phase D2: `supergroupFullInfo.can_get_statistics` (schema 1.8.67,
    /// line 2792). Gates the channel statistics entry point in the info
    /// panel; `getChatStatistics` errors when false.
    pub can_get_statistics: bool,
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
            can_get_statistics: false,
        }
    }
}

/// Phase D2: fetch state for one chat's `getChatStatistics` result
/// (schema 1.8.67, line 15760). Keyed by chat id. `Loading` is also the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatStatisticsFetch {
    Loading,
    Loaded(Box<ChatStatistics>),
    Failed(String),
}

/// Phase D3a: fetch state for one chat's `getChatInviteLinks` result
/// (schema 1.8.67, line 14138). Keyed by chat id. `Loading` is the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum InviteLinkFetch {
    Loading,
    Loaded(InviteLinkList),
    Failed(String),
}

/// Phase D3a: one chat's invite-link list (`chatInviteLinks`, schema
/// 1.8.67, line 2630).
#[derive(Debug, Clone, PartialEq)]
pub struct InviteLinkList {
    pub total_count: i32,
    pub links: Vec<ParsedChatInviteLink>,
}

/// Phase D3a: fetch state for one chat's `getChatJoinRequests` result
/// (schema 1.8.67, line 14174). Same Loading-guard convention.
#[derive(Debug, Clone, PartialEq)]
pub enum JoinRequestFetch {
    Loading,
    Loaded(JoinRequestList),
    Failed(String),
}

/// Phase D3a: one chat's join-request list (`chatJoinRequests`, schema
/// 1.8.67, line 2691).
#[derive(Debug, Clone, PartialEq)]
pub struct JoinRequestList {
    pub total_count: i32,
    pub requests: Vec<ParsedChatJoinRequest>,
}

/// Phase D3b: fetch state for one administrator's `getChatMember` rights
/// lookup (schema 1.8.67, line 13622), keyed by (chat_id, user_id).
/// Drives the edit-rights dialog's loading / error states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminRightsFetch {
    Loading,
    Loaded(ChatAdminRights),
    Failed(String),
}

/// Phase D3b: fetch state for one chat's `getChatAdministrators` result
/// (schema 1.8.67, line 13632). Keyed by chat id. `Loading` is the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum AdminListFetch {
    Loading,
    Loaded(Vec<ChatAdministratorEntry>),
    Failed(String),
}

/// Phase D3b: fetch state for one chat's `getSupergroupMembers` result
/// (schema 1.8.67, line 15238) backing the promote member picker. Same
/// Loading-guard convention.
#[derive(Debug, Clone, PartialEq)]
pub enum SupergroupMembersFetch {
    Loading,
    Loaded {
        members: Vec<ParsedChatMember>,
        total_count: i32,
    },
    Failed(String),
}

/// B4: one `getPollVoters` page (schema 1.8.67, line 12941) backing the
/// poll-voters dialog. Same Loading-guard convention; a failed first page
/// lands in `Failed`, a failed "load more" keeps the loaded page.
#[derive(Debug, Clone, PartialEq)]
pub enum PollVotersFetch {
    Loading,
    Loaded {
        voters: Vec<MessageSender>,
        total_count: i32,
    },
    Failed(String),
}

/// Bots slice: fetch state for the single active inline query
/// (`getInlineQueryResults`, schema 1.8.67, line 13019). Mirrors
/// `PollVotersFetch`: a failed first page lands in `Failed`, a failed
/// page-next keeps the loaded page.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineQueryFetch {
    Loading,
    Loaded {
        inline_query_id: i64,
        button: Option<InlineQueryResultsButton>,
        results: Vec<InlineQueryResultSummary>,
        next_offset: String,
    },
    Failed(String),
}

/// Bots slice: the single active inline-query slot — which (chat, bot)
/// the results belong to, the query text that produced them, and the
/// current fetch state.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineQuerySlot {
    pub chat_id: ChatId,
    pub bot_user_id: i64,
    pub query: String,
    pub fetch: InlineQueryFetch,
}

/// Phase D3c: `getChatEventLog` page size (schema 1.8.67, line 15252:
/// "up to 100"). Shared by the driver and the `has_more` heuristic in
/// `Session::apply` — a short page means the log is exhausted.
pub const CHAT_EVENT_LOG_PAGE_SIZE: i32 = 100;

/// Phase D3c: fetch state for one chat's `getChatEventLog` result
/// (schema 1.8.67, line 15252). Keyed by chat id. `Loading` is the
/// in-flight guard — the driver never sends a second request while one
/// is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEventLogFetch {
    Loading,
    Loaded(ChatEventLogPage),
    Failed(String),
}

/// Phase D3c: one cached `getChatEventLog` result. Events arrive in
/// reverse chronological order (decreasing event `id`, schema 1.8.67,
/// line 15252); pages append older events, deduped by event id.
/// `has_more` is true when the last page was full — an older page is
/// worth requesting.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatEventLogPage {
    pub events: Vec<ParsedChatEvent>,
    pub has_more: bool,
}

impl ChatEventLogPage {
    /// Slice G2: distinct admin user ids (first-seen order) from a loaded
    /// page; drives the per-admin filter chips in the info panel. Chat
    /// senders (`MessageSender::Chat`) are not admins and are skipped.
    pub fn admin_user_ids(&self) -> Vec<i64> {
        let mut admins = Vec::new();
        for event in &self.events {
            if let MessageSender::User { user_id } = event.member_id
                && !admins.contains(&user_id)
            {
                admins.push(user_id);
            }
        }
        admins
    }
}

/// Slice G2: fetch state for one chat's welcome-message pack
/// (`loadChatWelcomeMessages`, schema 1.8.67, line 12630). `Loading` is
/// the in-flight guard — the driver never sends a second request while
/// one is outstanding.
#[derive(Debug, Clone, PartialEq)]
pub enum WelcomeMessagesFetch {
    Loading,
    Loaded,
    Failed(String),
}

/// Slice G2: the channel-comments viewer result — the latest
/// `getMessageThreadHistory` answer for one channel post.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentThreadFetch {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub messages: Vec<ParsedMessage>,
    pub failed: Option<String>,
}

/// Slice CL: the chat-list peek preview result — the latest
/// `getChatHistory` answer for one unopened chat.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewHistoryFetch {
    pub chat_id: ChatId,
    pub messages: Vec<ParsedMessage>,
    pub failed: Option<String>,
}

/// Phase D3c: relative timestamp for admin-log rows. The log only
/// covers the last 48 hours (schema 1.8.67, line 15252), so relative
/// forms are always meaningful; no date crate is pulled in for this.
/// Pure in `now_unix` for tests. `pub` (not `pub(crate)`) because the
/// `ui` module is built against the lib as an external crate.
pub fn event_log_relative_time_for(date_unix: i32, now_unix: i64) -> String {
    let age = now_unix.saturating_sub(i64::from(date_unix));
    if age < 60 {
        "just now".to_owned()
    } else if age < 3600 {
        format!("{}m ago", age / 60)
    } else if age < 86_400 {
        format!("{}h ago", age / 3600)
    } else {
        format!("{}d ago", age / 86_400)
    }
}

/// Phase D3c: relative timestamp for admin-log rows, against the
/// current wall clock. `pub` (not `pub(crate)`) because the `ui` module
/// is built against the lib as an external crate.
pub fn event_log_relative_time(date_unix: i32) -> String {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    event_log_relative_time_for(date_unix, now_unix)
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
    /// Phase C2i: one call-privacy round-trip landed. The spinner and
    /// error flag only reset when no round-trip is outstanding —
    /// `fetch_call_privacy` sends two gets, and clearing on the first
    /// would briefly render the radios with nothing selected.
    fn privacy_roundtrip_done(&mut self) {
        self.call_privacy_pending = self.call_privacy_pending.saturating_sub(1);
        if self.call_privacy_pending == 0 {
            self.call_privacy_loading = false;
            self.call_privacy_error = false;
        }
    }

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
            message_link_result: None,
            message_caption_length_max: 1024,
            // Slice CL1: TDLib's compiled defaults for the pin limits
            // (schema 1.8.67, line 13674); `updateOption` overrides.
            pinned_chat_count_max: 5,
            pinned_archived_chat_count_max: 100,
            chat_action_error: None,
            report_chat_outcome: None,
            instant_view: None,
            instant_view_fallback_url: None,
            instant_view_urls: HashMap::new(),
            composer_preview: None,
            composer_preview_urls: HashMap::new(),
            message_link_error: None,
            recognize_speech_error: None,
            resend_error: None,
            invite_link_error: None,
            scheduled_messages: Vec::new(),
            open_chat: None,
            app_active: true,
            hide_notification_previews: true,
            pending_notifications: Vec::new(),
            saved_notification_sounds: Vec::new(),
            saved_sounds_loaded: false,
            saved_sounds_stale: false,
            scope_notification_settings: HashMap::new(),
            scope_settings_loading: HashSet::new(),
            storage_stats: None,
            storage_stats_loading: false,
            password_state: None,
            password_state_loading: false,
            password_op_error: None,
            sessions: None,
            sessions_loading: false,
            sessions_mutating: false,
            sessions_error: None,
            sessions_stale: false,
            connected_websites: None,
            connected_websites_loading: false,
            websites_mutating: false,
            websites_error: None,
            websites_stale: false,
            sound_file_ids: HashMap::new(),
            pending_sound_downloads: HashSet::new(),
            pending_sound_plays: Vec::new(),
            secret_chat_states: HashMap::new(),
            secret_chat_fetch_queue: Vec::new(),
            active_call: None,
            call_summary: None,
            call_error: None,
            group_call_error: None,
            call_busy_decline_queue: Vec::new(),
            call_busy_declined: Vec::new(),
            recent_calls: Vec::new(),
            recent_calls_offset: String::new(),
            recent_calls_loading: false,
            recent_calls_error: false,
            call_privacy_allow_calls: None,
            call_privacy_p2p: None,
            call_privacy_loading: false,
            call_privacy_pending: 0,
            call_privacy_error: false,
            call_prefs: CallPrefs::default(),
            media_prefs: MediaPrefs::default(),
            contact_prefs: ContactPrefs::default(),
            active_group_call: None,
            group_call_fetch_queue: Vec::new(),
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
            last_login_url_info: None,
            login_url_request: None,
            payment_request: None,
            payment_form: None,
            payment_form_loading: false,
            payment_validated: None,
            payment_shipping_id: None,
            payment_receipt: None,
            payment_receipt_open: false,
            payment_note: None,
            payment_sending: false,
            payment_receipt_error: None,
            payment_verification_url: None,
            pending_force_reply: None,
            files: HashMap::new(),
            downloading: HashSet::new(),
            user_downloads: HashSet::new(),
            failed_downloads: HashSet::new(),
            completed_downloads: VecDeque::new(),
            downloads_panel_open: false,
            download_extras: HashMap::new(),
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            shared_media: SharedMediaState::default(),
            stickers: StickerPanel::default(),
            gifs: GifPanel::default(),
            bot_user_ids: HashSet::new(),
            bot_info: HashMap::new(),
            bot_start_params: HashMap::new(),
            similar_bots: HashMap::new(),
            bot_commands: HashMap::new(),
            draft_dirty: HashSet::new(),
            draft_clears: Vec::new(),
            sponsored: HashMap::new(),
            sponsored_report: None,
            sponsored_report_target: None,
            last_sponsored_report: None,
            my_user_id: None,
            archive_chat_list_settings: None,
            archive_settings_loading: false,
            archive_settings_open: false,
            archive_collapsed: false,
            users: HashMap::new(),
            contacts: None,
            contacts_error: false,
            contacts_notice: None,
            user_full_infos: HashMap::new(),
            supergroup_full_infos: HashMap::new(),
            chat_statistics: HashMap::new(),
            invite_links: HashMap::new(),
            join_requests: HashMap::new(),
            pending_join_request_counts: HashMap::new(),
            event_logs: HashMap::new(),
            event_log_filters: HashMap::new(),
            event_log_queries: HashMap::new(),
            supergroup_sign_messages: HashMap::new(),
            supergroup_show_message_sender: HashMap::new(),
            supergroup_anti_spam_enabled: HashMap::new(),
            supergroup_can_toggle_anti_spam: HashMap::new(),
            supergroup_manage_topics_right: HashMap::new(),
            supergroup_change_info_right: HashMap::new(),
            supergroup_send_welcome_right: HashMap::new(),
            chat_has_welcome_messages: HashMap::new(),
            welcome_messages: HashMap::new(),
            welcome_message_fetches: HashMap::new(),
            chat_boost_status: HashMap::new(),
            boost_slots_by_chat: HashMap::new(),
            boost_intent: None,
            comment_thread: None,
            chat_preview_fetch: None,
            supergroup_usernames: HashMap::new(),
            supergroup_member_status: HashMap::new(),
            supergroup_restrict_right: HashMap::new(),
            supergroup_invite_right: HashMap::new(),
            poll_voters: HashMap::new(),
            inline_query: None,
            supergroup_join_by_request: HashMap::new(),
            supergroup_is_broadcast: HashMap::new(),
            add_members_failed: HashMap::new(),
            member_list_stale: Vec::new(),
            member_action_error: HashMap::new(),
            admin_lists: HashMap::new(),
            supergroup_members: HashMap::new(),
            basic_group_members: HashMap::new(),
            admin_rights: HashMap::new(),
            supergroup_promote_right: HashMap::new(),
            supergroup_manage_tags_right: HashMap::new(),
            open_info_panel: None,
            story_tray: HashMap::new(),
            stories: HashMap::new(),
            stories_active_loaded: false,
            story_available_reactions: None,
            story_tray_refresh: HashSet::new(),
            story_post: StoryPostState::default(),
            story_viewers: None,
            story_report: None,
            story_stealth: StoryStealthMode::default(),
            story_stealth_error: None,
            username_check: None,
            username_check_pending: None,
            profile_edit_error: None,
            story_post_as_chats: Vec::new(),
            story_manage: StoryManageState::default(),
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
    /// (`is_supported_chat` / `can_post`) — no special gate. This
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
    /// private chat, the supergroup for a group/channel chat, the chat
    /// partner for a secret chat (Phase B2: their panel hosts the
    /// encryption-key section), `None` for basic groups and unknown
    /// kinds.
    pub fn info_panel_target_for_chat(&self, chat_id: ChatId) -> Option<InfoPanelTarget> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } => Some(InfoPanelTarget::User(user_id.0)),
            ChatKind::Secret { user_id, .. } => Some(InfoPanelTarget::User(user_id.0)),
            ChatKind::Supergroup { supergroup_id, .. } => {
                Some(InfoPanelTarget::Supergroup(supergroup_id))
            }
            ChatKind::BasicGroup { basic_group_id } => {
                Some(InfoPanelTarget::BasicGroup(basic_group_id))
            }
            _ => None,
        }
    }

    /// Phase B2: the full `ParsedSecretChat` record behind the *open*
    /// chat, when the open chat is a **Ready** secret chat whose partner is
    /// `user_id`. Drives the encryption-key section of the partner's info
    /// panel. `None` for non-Ready chats (the key is only meaningful once
    /// the session is established) and while the record hasn't arrived.
    ///
    /// Security: the returned record borrows session memory; callers pass
    /// the bytes through `key_fingerprint::key_hash_pixels` and render
    /// pixel colors only — raw bytes never leave `Session`.
    pub fn open_ready_secret_chat_for_user(&self, user_id: i64) -> Option<&ParsedSecretChat> {
        let open = self.open_chat?;
        let chat = self.chats.get(&open.0)?;
        let ChatKind::Secret {
            secret_chat_id,
            user_id: partner,
        } = &chat.kind
        else {
            return None;
        };
        if partner.0 != user_id {
            return None;
        }
        if chat.secret_state != Some(SecretChatState::Ready) {
            return None;
        }
        self.secret_chat_states.get(secret_chat_id)
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

    /// Phase D3a: whether the viewer's own administrator rights in a
    /// supergroup include `can_invite_users` (schema 1.8.67, line 1092),
    /// which invite-link management requires. Creators hold all rights
    /// implicitly — check `supergroup_own_status` for that. Absent =
    /// unknown, treated as lacking the right.
    pub fn supergroup_can_invite_users(&self, supergroup_id: i64) -> bool {
        self.supergroup_invite_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Phase D3b: whether the viewer's own administrator rights in a
    /// supergroup include `can_promote_members` (schema 1.8.67, line
    /// 1092), which admin management requires. Creators hold all rights
    /// implicitly — check `supergroup_own_status` for that. Absent =
    /// unknown, treated as lacking the right.
    pub fn supergroup_can_promote_members(&self, supergroup_id: i64) -> bool {
        self.supergroup_promote_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Slice G1: whether the viewer's own administrator rights in a
    /// supergroup include `can_manage_tags` (schema 1.8.67, line 1092),
    /// which changing another member's custom title requires. Creators
    /// hold all rights implicitly — check `supergroup_own_status` for
    /// that. Absent = unknown, treated as lacking the right.
    pub fn supergroup_can_manage_tags(&self, supergroup_id: i64) -> bool {
        self.supergroup_manage_tags_right
            .get(&supergroup_id)
            .copied()
            .unwrap_or(false)
    }

    /// Slice G1: whether the viewer may set a member's custom title in
    /// this chat — creator, or an admin with `can_manage_tags`.
    /// Basic groups: creators only (basic groups expose no per-admin
    /// rights; the server rejects anything else).
    pub fn chat_can_manage_tags(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_manage_tags(supergroup_id)
            }
            ChatKind::BasicGroup { .. } => self.chat_is_owner(chat_id),
            _ => false,
        }
    }

    /// Slice G2: whether the viewer holds `can_manage_topics` in a
    /// supergroup — creator, or an admin with the right (schema 1.8.67,
    /// line 1092). Gates forum topic management.
    pub fn chat_can_manage_topics(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .supergroup_manage_topics_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: whether the viewer may change a channel's info —
    /// creator, or an admin with `can_change_info` (schema 1.8.67, line
    /// 15175: `toggleSupergroupSignMessages` requires it). The
    /// `ChatSummary` path covers channels (own membership probed via
    /// `getChatMember`); supergroups carry the right on the
    /// `updateSupergroup` / `getSupergroup` status block.
    pub fn chat_can_change_info(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_change_info() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .supergroup_change_info_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: whether the viewer may manage welcome messages —
    /// creator, or an admin with `can_send_welcome_messages` (schema
    /// 1.8.67, line 1090). Same two paths as `chat_can_change_info`.
    pub fn chat_can_send_welcome_messages(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_send_welcome_messages() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .supergroup_send_welcome_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Slice G2: cached `supergroup.sign_messages` (schema 1.8.67, line
    /// 2746). Absent = unknown → shown off.
    pub fn chat_sign_messages(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.supergroup_sign_messages
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached `supergroup.show_message_sender` (schema 1.8.67,
    /// line 2746).
    pub fn chat_show_message_sender(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.supergroup_show_message_sender
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached
    /// `supergroupFullInfo.has_aggressive_anti_spam_enabled` (schema
    /// 1.8.67, line 2792).
    pub fn chat_anti_spam_enabled(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.supergroup_anti_spam_enabled
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached
    /// `supergroupFullInfo.can_toggle_aggressive_anti_spam` (schema
    /// 1.8.67, line 2792) — the only gate for the anti-spam toggle.
    pub fn chat_can_toggle_anti_spam(&self, chat_id: ChatId) -> bool {
        self.chat_supergroup(chat_id).is_some_and(|id| {
            self.supergroup_can_toggle_anti_spam
                .get(&id)
                .copied()
                .unwrap_or(false)
        })
    }

    /// Slice G2: cached `chat.has_welcome_messages` (schema 1.8.67, line
    /// 3627).
    pub fn chat_has_welcome_messages_flag(&self, chat_id: ChatId) -> bool {
        self.chat_has_welcome_messages
            .get(&chat_id.0)
            .copied()
            .unwrap_or(false)
    }

    /// Slice G2: supergroup id for any supergroup-kind chat (channels
    /// included); `None` for basic groups and other kinds.
    fn chat_supergroup(&self, chat_id: ChatId) -> Option<i64> {
        self.chats.get(&chat_id.0).and_then(|chat| match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
            _ => None,
        })
    }

    /// Phase D3a: invite-link / join-request gate for a chat. The
    /// `ChatSummary` path covers channels (own membership probed via
    /// `getChatMember`); non-channel supergroups carry own admin rights
    /// on the `updateSupergroup` / `getSupergroup` status block instead.
    pub fn chat_can_invite_users(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_invite_users() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_invite_users(supergroup_id)
            }
            _ => false,
        }
    }

    /// Slice G1: whether the viewer may add members to a chat.
    /// `addChatMember` / `addChatMembers` require the `can_invite_users`
    /// *member* right (schema 1.8.67, lines 13574/13580) — a plain member
    /// with the default permission qualifies, so the default
    /// `chat.permissions` block governs; the admin invite right (or
    /// creator status) is a blanket override. Unknown permissions keep
    /// the gate closed.
    pub fn chat_can_add_members(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if self.chat_can_invite_users(chat_id) {
            return true;
        }
        chat.permissions
            .as_ref()
            .is_some_and(|p| p.can_invite_users)
    }
    /// Phase D3b: admin-management gate for a chat. The `ChatSummary`
    /// path covers channels (own membership probed via `getChatMember`);
    /// non-channel supergroups carry own admin rights on the
    /// `updateSupergroup` / `getSupergroup` status block instead.
    pub fn chat_can_manage_admins(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_manage_admins() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_promote_members(supergroup_id)
            }
            _ => false,
        }
    }

    /// Slice G1: restrict/ban gate for a chat. `setChatMemberStatus`
    /// requires the `can_restrict_members` administrator right "to change
    /// restrictions of a user" (schema 1.8.67, lines 13586-13587).
    /// Deny-by-default, following the D3b `chat_can_manage_admins`
    /// pattern: the `ChatSummary` path covers channels (own membership
    /// probed via `getChatMember`); non-channel supergroups carry own
    /// admin status on the `updateSupergroup` / `getSupergroup` status
    /// block instead. Note `chatMemberStatusRestricted` is "not supported
    /// in basic groups and channels" (schema line 2510) — restrict applies
    /// to non-channel supergroups only; ban works in supergroups and
    /// channels.
    pub fn chat_can_restrict_members(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.can_restrict_members() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self.supergroup_can_restrict_members(supergroup_id)
            }
            _ => false,
        }
    }

    /// Slice G1: whether the viewer owns the chat (creator status).
    /// `toggleSupergroupIsBroadcastGroup` and `setSupergroupUsername`
    /// require owner privileges (schema 1.8.67, lines 15220/15133).
    pub fn chat_is_owner(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.my_member_status == Some(ChannelMemberStatus::Creator) {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
            }
            _ => false,
        }
    }

    /// Phase D3c: admin-log gate for a chat. `getChatEventLog` "requires
    /// administrator rights" and is "available only in supergroups and
    /// channels" (schema 1.8.67, line 15252). Deny-by-default, following
    /// the D3b `chat_can_manage_admins` pattern: the `ChatSummary` path
    /// covers channels (own membership probed via `getChatMember`);
    /// supergroups carry own admin status on the `updateSupergroup` /
    /// `getSupergroup` status block instead. Unlike admin management
    /// (which needs the explicit `can_promote_members` right), any
    /// administrator or the creator may view the log. Unknown/absent
    /// status keeps the section hidden and the request unsent rather
    /// than fabricating a right.
    pub fn chat_can_view_event_log(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if chat.is_admin_or_creator() {
            return true;
        }
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => matches!(
                self.supergroup_own_status(supergroup_id),
                Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
            ),
            _ => false,
        }
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

    /// Phase B3: whether the currently open chat's loaded history contains
    /// any message with a still-live `self_destruct_in` countdown — drives
    /// the 1-second render tick that keeps the timer badges fresh.
    pub fn open_chat_has_live_self_destruct(&self, now_ms: u64) -> bool {
        let Some(open) = self.open_chat else {
            return false;
        };
        self.histories.get(&open.0).is_some_and(|history| {
            history.messages.values().any(|message| {
                // Phase B4: auto-delete countdowns share the 1-second
                // render tick with self-destruct badges.
                message.has_live_self_destruct(now_ms) || message.has_live_auto_delete(now_ms)
            })
        })
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
            // MED4: `updateOption` (schema:10926). Only
            // `message_caption_length_max` is consumed (caption edits /
            // media-send captions); every other option parses but is
            // ignored, never an error.
            EnvelopePayload::UpdateOption { name, value } => {
                if name == "message_caption_length_max"
                    && let OptionValue::Integer(limit) = value
                {
                    self.message_caption_length_max = limit.max(0).min(i64::from(i32::MAX)) as i32;
                }
                // Slice CL1: pin-limit options (schema:13674) for the
                // client-side pin pre-check.
                if (name == "pinned_chat_count_max" || name == "pinned_archived_chat_count_max")
                    && let OptionValue::Integer(limit) = value
                {
                    let limit = limit.max(0).min(i64::from(i32::MAX)) as i32;
                    if name == "pinned_chat_count_max" {
                        self.pinned_chat_count_max = limit;
                    } else {
                        self.pinned_archived_chat_count_max = limit;
                    }
                }
            }
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
                permissions,
                can_be_deleted_for_all_users,
                can_be_deleted_only_for_self,
                is_marked_as_unread,
                message_auto_delete_time,
                video_chat,
                has_welcome_messages,
                unread_mention_count,
                unread_reaction_count,
                can_be_reported,
                blocked,
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
                // Slice G1: full default permissions block for the editor.
                chat.permissions = permissions;
                chat.can_be_deleted_for_all_users = can_be_deleted_for_all_users;
                // Slice CL1: clear-history gate + marked-as-unread flag.
                chat.can_be_deleted_only_for_self = can_be_deleted_only_for_self;
                chat.is_marked_as_unread = is_marked_as_unread;
                // Slice CL3: mention / reaction badge counts, report gate,
                // block-list state.
                chat.unread_mention_count = unread_mention_count;
                chat.unread_reaction_count = unread_reaction_count;
                chat.can_be_reported = can_be_reported;
                chat.blocked = blocked;
                // Phase B4: chat-level auto-delete / self-destruct timer
                // (`chat.message_auto_delete_time`, schema 1.8.67, lines
                // 3616 / 3627).
                chat.message_auto_delete_time = message_auto_delete_time;
                // Phase C3a: the chat's active video chat (`videoChat`,
                // schema 1.8.67, lines 3576 / 3579).
                chat.video_chat = video_chat.map(|v| VideoChatInfo {
                    group_call_id: v.group_call_id,
                    has_participants: v.has_participants,
                });
                // Slice G2: `chat.has_welcome_messages` (schema 1.8.67,
                // line 3627).
                self.chat_has_welcome_messages
                    .insert(chat_id.0, has_welcome_messages);
                // Phase B1: secret chats — `updateSecretChat` arrives before
                // `updateNewChat` (schema 1.8.67, line 10740), so a state
                // may already be recorded; otherwise the driver fetches it
                // via `getSecretChat` (an offline method).
                if let ChatKind::Secret { secret_chat_id, .. } = &chat.kind {
                    let secret_chat_id = *secret_chat_id;
                    match self.secret_chat_states.get(&secret_chat_id) {
                        Some(secret_chat) => chat.secret_state = Some(secret_chat.state.clone()),
                        None if !self.secret_chat_fetch_queue.contains(&secret_chat_id) => {
                            self.secret_chat_fetch_queue.push(secret_chat_id);
                        }
                        None => {}
                    }
                }
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
                permissions,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.can_send_basic_messages = can_send_basic_messages;
                // Slice G1: keep the full default permissions block for
                // the editor.
                chat.permissions = permissions;
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
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBotSimilarBots)
                    && let Some(pending) = pending
                    && let Some(bot_user_id) = pending.user_id
                {
                    // Slice B2: `getBotSimilarBots` answer — the user
                    // objects arrive via `updateUser`; ids alone drive the
                    // list (Telegram X `SharedChatsController` similarly
                    // resolves users from its cache).
                    self.similar_bots
                        .insert(bot_user_id, SimilarBotsFetch::Loaded(user_ids));
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
                can_get_statistics,
                has_aggressive_anti_spam_enabled,
                can_toggle_aggressive_anti_spam,
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
                            can_get_statistics,
                        },
                    );
                    // Slice G2: anti-spam state for the manage-dialog
                    // toggle.
                    self.supergroup_anti_spam_enabled
                        .insert(supergroup_id, has_aggressive_anti_spam_enabled);
                    self.supergroup_can_toggle_anti_spam
                        .insert(supergroup_id, can_toggle_aggressive_anti_spam);
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
                can_get_statistics,
                has_aggressive_anti_spam_enabled,
                can_toggle_aggressive_anti_spam,
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
                        can_get_statistics,
                    },
                );
                // Slice G2: anti-spam state for the manage-dialog toggle.
                self.supergroup_anti_spam_enabled
                    .insert(supergroup_id, has_aggressive_anti_spam_enabled);
                self.supergroup_can_toggle_anti_spam
                    .insert(supergroup_id, can_toggle_aggressive_anti_spam);
            }
            // Slice G2: welcome-message pack (`updateChatWelcomeMessages`,
            // schema 1.8.67, line 10649) — the full pack replaces the
            // cache; the welcome dialog renders it.
            EnvelopePayload::UpdateChatWelcomeMessages { chat_id, messages } => {
                self.welcome_messages.insert(chat_id, messages);
                self.welcome_message_fetches
                    .insert(chat_id, WelcomeMessagesFetch::Loaded);
            }
            // Slice G2: `updateChatHasWelcomeMessages` (schema 1.8.67,
            // line 10600).
            EnvelopePayload::UpdateChatHasWelcomeMessages {
                chat_id,
                has_welcome_messages,
            } => {
                self.chat_has_welcome_messages
                    .insert(chat_id, has_welcome_messages);
            }
            // Slice G2: `getChatBoostStatus` answer (schema 1.8.67, line
            // 13917) — correlated via the pending request's `chat_id`.
            EnvelopePayload::ChatBoostStatus { level, boost_count } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_boost_status
                        .insert(chat_id.0, (level, boost_count));
                }
            }
            // Slice G2: `chatBoostSlots` (schema 1.8.67, line 6968) — the
            // `getAvailableChatBoostSlots` answer. Stashed per chat so the
            // driver's `boostChat` chain can consume it (see
            // `maybe_continue_boost` in connect.rs).
            EnvelopePayload::ChatBoostSlots { slots } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBoostSlotsForBoost)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.boost_slots_by_chat.insert(chat_id.0, slots);
                }
                // Slice G2: `boostChat` answers `chatBoostSlots` as well
                // (schema 1.8.67, line 13922) — drop the cached status so
                // the dialog refetches it.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::BoostChat)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chat_boost_status.remove(&chat_id.0);
                }
            }
            // Phase D2: `getChatStatistics` answer — the response carries
            // no chat id, so it is correlated via the pending request's
            // `chat_id`.
            EnvelopePayload::ChatStatistics { statistics } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatStatistics)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.chat_statistics
                        .insert(chat_id.0, ChatStatisticsFetch::Loaded(Box::new(statistics)));
                }
            }
            // Phase D3a: `createChatInviteLink` / `editChatInviteLink`
            // answer — the created/updated link, correlated via the
            // pending request's `chat_id`. Upserts into the cached list;
            // a genuinely new link (create purpose, not already present)
            // also bumps `total_count`.
            EnvelopePayload::ChatInviteLink { link } => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::CreateChatInviteLink
                            | RequestPurpose::EditChatInviteLink
                            | RequestPurpose::ReplacePrimaryChatInviteLink,
                    )
                ) && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    let is_create = pending.purpose == RequestPurpose::CreateChatInviteLink;
                    match self.invite_links.entry(chat_id.0) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            match entry.get_mut() {
                                InviteLinkFetch::Loaded(list) => {
                                    if pending.purpose
                                        == RequestPurpose::ReplacePrimaryChatInviteLink
                                    {
                                        // Slice G1: the old primary was
                                        // revoked server-side; drop it so
                                        // the list shows only the new one.
                                        list.links.retain(|e| {
                                            !e.is_primary || e.invite_link == link.invite_link
                                        });
                                    }
                                    if let Some(existing) = list
                                        .links
                                        .iter_mut()
                                        .find(|e| e.invite_link == link.invite_link)
                                    {
                                        *existing = link;
                                    } else {
                                        list.links.push(link);
                                        if is_create {
                                            list.total_count = list.total_count.saturating_add(1);
                                        }
                                    }
                                }
                                fetch => {
                                    *fetch = InviteLinkFetch::Loaded(InviteLinkList {
                                        total_count: 1,
                                        links: vec![link],
                                    });
                                }
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(InviteLinkFetch::Loaded(InviteLinkList {
                                total_count: 1,
                                links: vec![link],
                            }));
                        }
                    }
                }
            }
            // Phase D3a: `getChatInviteLinks` / `revokeChatInviteLink`
            // answer — replaces the cached list.
            EnvelopePayload::ChatInviteLinks { total_count, links } => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::GetChatInviteLinks | RequestPurpose::RevokeChatInviteLink)
                ) && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.invite_links.insert(
                        chat_id.0,
                        InviteLinkFetch::Loaded(InviteLinkList { total_count, links }),
                    );
                }
            }
            // Phase D3a: `getChatJoinRequests` answer.
            EnvelopePayload::ChatJoinRequests {
                total_count,
                requests,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatJoinRequests)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Loaded(JoinRequestList {
                            total_count,
                            requests,
                        }),
                    );
                }
            }
            // Phase D3b: `getChatAdministrators` answer — replaces the
            // cached admin list.
            EnvelopePayload::ChatAdministrators { administrators } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatAdministrators)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.admin_lists
                        .insert(chat_id.0, AdminListFetch::Loaded(administrators));
                }
            }
            // Phase D3b / slice G1: `getSupergroupMembers` answer —
            // replaces the cached page for this (chat, filter).
            EnvelopePayload::SupergroupMembers {
                members,
                total_count,
            } => {
                if let Some(RequestPurpose::GetSupergroupMembers { filter }) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.supergroup_members.insert(
                        (chat_id.0, filter),
                        SupergroupMembersFetch::Loaded {
                            members,
                            total_count,
                        },
                    );
                }
            }
            // B4: `getPollVoters` answer — a first page (offset 0)
            // replaces the cached list for this (chat, message, option);
            // a later page appends, deduped by sender, keeping server
            // order (the list is per-option; `option_id` is part of the
            // key so switching options refetches).
            EnvelopePayload::PollVoters {
                total_count,
                voters,
            } => {
                if let Some(RequestPurpose::GetPollVoters {
                    chat_id,
                    message_id,
                    option_id,
                    offset,
                }) = pending.map(|p| p.purpose)
                {
                    let key = (chat_id.0, message_id.0, option_id);
                    let page_len = voters.len();
                    let merged = if offset == 0 {
                        voters
                    } else {
                        match self.poll_voters.get(&key) {
                            Some(PollVotersFetch::Loaded { voters: old, .. }) => {
                                let mut merged = old.clone();
                                for voter in voters {
                                    if !merged.contains(&voter) {
                                        merged.push(voter);
                                    }
                                }
                                merged
                            }
                            _ => voters,
                        }
                    };
                    // B4: a short page is the honest exhaustion signal —
                    // `total_count` is approximate per the schema, so on a
                    // short page (limit is 50, schema line 12941) clamp it
                    // to what we actually hold; the UI hides "Load more"
                    // when `voters.len() >= total_count`.
                    let total_count = if page_len < 50 {
                        merged.len() as i32
                    } else {
                        total_count
                    };
                    self.poll_voters.insert(
                        key,
                        PollVotersFetch::Loaded {
                            voters: merged,
                            total_count,
                        },
                    );
                }
            }
            // Bots slice: `inlineQueryResults` — the `getInlineQueryResults`
            // answer (schema 1.8.67, line 7716). A first page replaces the
            // slot; a later page appends, deduped by result id, keeping
            // the new page's id/offset.
            // ponytail: rapid re-queries can let an older response land on
            // a newer slot — the response never echoes the query text, so
            // the slot keys on (chat, bot) only; the UI slice debounces
            // queries anyway.
            EnvelopePayload::InlineQueryResults(page) => {
                if let Some(RequestPurpose::GetInlineQueryResults {
                    chat_id,
                    bot_user_id,
                    first_page,
                }) = pending.map(|p| p.purpose)
                {
                    let slot_ok = match (first_page, self.inline_query.as_ref()) {
                        (true, Some(slot)) => {
                            slot.chat_id == chat_id
                                && slot.bot_user_id == bot_user_id
                                && matches!(slot.fetch, InlineQueryFetch::Loading)
                        }
                        (false, Some(slot)) => {
                            slot.chat_id == chat_id
                                && slot.bot_user_id == bot_user_id
                                && matches!(slot.fetch, InlineQueryFetch::Loaded { .. })
                        }
                        _ => false,
                    };
                    if slot_ok {
                        let fetch = match (first_page, &self.inline_query) {
                            (
                                false,
                                Some(InlineQuerySlot {
                                    fetch: InlineQueryFetch::Loaded { results: old, .. },
                                    ..
                                }),
                            ) => {
                                let mut results = old.clone();
                                for result in page.results {
                                    if !results.iter().any(|r| r.id == result.id) {
                                        results.push(result);
                                    }
                                }
                                InlineQueryFetch::Loaded {
                                    inline_query_id: page.inline_query_id,
                                    button: page.button,
                                    results,
                                    next_offset: page.next_offset,
                                }
                            }
                            _ => InlineQueryFetch::Loaded {
                                inline_query_id: page.inline_query_id,
                                button: page.button,
                                results: page.results,
                                next_offset: page.next_offset,
                            },
                        };
                        if let Some(slot) = self.inline_query.as_mut() {
                            slot.fetch = fetch;
                        }
                    }
                } else if let Some(RequestPurpose::GetGifSearchResults { first_page, .. }) =
                    pending.map(|p| p.purpose)
                {
                    // Slice S9: GIF-panel search — the
                    // `inlineQueryResultAnimation` entries land in `GifPanel`,
                    // never the composer's `inline_query` slot. First page
                    // replaces; later pages append (deduped); `next_offset`
                    // pages the bot's result list.
                    self.remember_files(&page.files);
                    self.accept_gif_search_results(page.animations, page.next_offset, first_page);
                }
            }
            // Slice G1: `createNewBasicGroupChat` answer
            // (`createdBasicGroupChat`, schema 1.8.67, line 3644). The new
            // chat itself arrives as `updateNewChat`; nothing to cache.
            EnvelopePayload::CreatedBasicGroupChat { chat_id: _ } => {}
            // Slice G1: `addChatMembers` answer (`failedToAddMembers`,
            // schema 1.8.67, line 3640). Added members arrive as
            // `updateChatMember`; the failure count drives the notice in
            // the add-members dialog.
            // Slice G1: `getBasicGroupFullInfo` answer — replaces the
            // cached basic-group member list.
            EnvelopePayload::BasicGroupFullInfo { members } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBasicGroupFullInfo)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    let total_count = members.len() as i32;
                    self.basic_group_members.insert(
                        chat_id.0,
                        SupergroupMembersFetch::Loaded {
                            members,
                            total_count,
                        },
                    );
                }
            }
            EnvelopePayload::FailedToAddMembers { failed_count } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    match pending.map(|p| p.purpose) {
                        // Slice G1: the bulk `addChatMembers` answer is a
                        // single response — it replaces the count.
                        Some(RequestPurpose::AddChatMembers) => {
                            self.add_members_failed.insert(chat_id.0, failed_count);
                        }
                        // Slice G1 fix-up: basic groups send one
                        // `addChatMember` per user and each answers
                        // `failedToAddMembers` — accumulate, or the last
                        // response would overwrite the earlier ones.
                        Some(RequestPurpose::AddChatMember) => {
                            *self.add_members_failed.entry(chat_id.0).or_insert(0) += failed_count;
                        }
                        _ => {}
                    }
                }
            }
            // Phase D3c: `getChatEventLog` answer — a first page (cursor
            // 0) replaces the cache; an older page appends, deduped by
            // event id, keeping reverse-chronological order (decreasing
            // event id, schema 1.8.67 line 15252). A full page sets
            // `has_more`; a short page exhausts the log.
            EnvelopePayload::ChatEvents { events } => {
                if let Some(pending) = pending
                    && let RequestPurpose::GetChatEventLog { from_event_id } = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                {
                    let has_more = events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE;
                    let mut merged = match (from_event_id, self.event_logs.get(&chat_id.0)) {
                        (0, _) => events,
                        (_, Some(ChatEventLogFetch::Loaded(page))) => {
                            let mut merged = page.events.clone();
                            for event in events {
                                if !merged.iter().any(|old| old.id == event.id) {
                                    merged.push(event);
                                }
                            }
                            merged
                        }
                        _ => events,
                    };
                    merged.sort_by_key(|event| std::cmp::Reverse(event.id));
                    self.event_logs.insert(
                        chat_id.0,
                        ChatEventLogFetch::Loaded(ChatEventLogPage {
                            events: merged,
                            has_more,
                        }),
                    );
                }
            }
            // Phase D3a: `updateNewChatJoinRequest` (schema 1.8.67, line
            // 11210) — a new join request arrived. Prepend it to the cached
            // list when one is loaded; otherwise the next fetch picks it up.
            // The total is bumped: the update announces a genuinely new
            // pending request.
            EnvelopePayload::UpdateNewChatJoinRequest {
                chat_id, request, ..
            } => {
                if let Some(JoinRequestFetch::Loaded(mut list)) =
                    self.join_requests.get(&chat_id).cloned()
                    && !list
                        .requests
                        .iter()
                        .any(|existing| existing.user_id == request.user_id)
                {
                    list.requests.insert(0, request);
                    list.total_count = list.total_count.saturating_add(1);
                    self.join_requests
                        .insert(chat_id, JoinRequestFetch::Loaded(list));
                }
            }
            // Phase D3a: `updateChatPendingJoinRequests` (schema 1.8.67,
            // line 10555) — the badge count. The full list still needs
            // `getChatJoinRequests`.
            EnvelopePayload::UpdateChatPendingJoinRequests {
                chat_id,
                total_count,
                ..
            } => {
                self.pending_join_request_counts
                    .insert(chat_id, total_count);
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
            // Slice CL1: `updateChatIsMarkedAsUnread` (schema 1.8.67,
            // line 10588) — the authoritative marked-as-unread flag; the
            // row shows the unread badge while set.
            EnvelopePayload::UpdateChatIsMarkedAsUnread {
                chat_id,
                is_marked_as_unread,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .is_marked_as_unread = is_marked_as_unread;
            }
            // Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
            // line 10549) — keep the chat-level timer fresh. The same
            // change also lands in history as a
            // `messageChatSetMessageAutoDeleteTime` service row.
            EnvelopePayload::UpdateChatMessageAutoDeleteTime {
                chat_id,
                message_auto_delete_time,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .message_auto_delete_time = message_auto_delete_time;
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
            // Phase B1: secret chat lifecycle (schema 1.8.67, lines
            // 10741 / 2816). `updateSecretChat` may arrive before any
            // `updateNewChat`; `secretChat` is the `getSecretChat` answer.
            // Both funnel into `accept_secret_chat`.
            EnvelopePayload::UpdateSecretChat { secret_chat } => {
                self.accept_secret_chat(&secret_chat);
            }
            EnvelopePayload::SecretChat { secret_chat } => {
                self.accept_secret_chat(&secret_chat);
            }
            // Phase C1: call signaling (schema 1.8.67, lines 10816 /
            // 10862). `updateCall` drives the single-call state machine;
            // signaling data is queued as a diagnostic record and also fed
            // to the engine by the C2b driver bridge; `callId` is the answer
            // that starts tracking the outgoing call.
            EnvelopePayload::UpdateCall { call } => {
                self.accept_call_update(&call);
            }
            EnvelopePayload::UpdateNewCallSignalingData { call_id, data } => {
                self.accept_call_signaling_data(call_id, data);
            }
            EnvelopePayload::CallId { id } => {
                if let Some(pending) = pending
                    && let RequestPurpose::CreateCall { is_video } = pending.purpose
                    && let Some(user_id) = pending.user_id
                    && self.active_call.is_none()
                {
                    self.active_call = Some(ActiveCall {
                        id,
                        user_id,
                        is_outgoing: true,
                        // Phase C1b: the `callId` answer carries no
                        // `is_video` (schema 1.8.67, :7034), so it is
                        // derived from the `createCall` request args
                        // stashed in the request purpose.
                        is_video,
                        muted: false,
                        camera_on: is_video,
                        screen_sharing: false,
                        remote_video: RemoteVideoState::Inactive,
                        state: CallState::Pending {
                            is_created: true,
                            is_received: false,
                        },
                        started_at: Instant::now(),
                        ready_at: None,
                        ready: None,
                        transport: None,
                        transport_error: None,
                        signaling_queue: Vec::new(),
                        signaling_dropped: 0,
                    });
                    self.call_summary = None;
                    self.call_error = None;
                }
            }
            // Phase C3a: `groupCallId` — the `createVideoChat` answer.
            // Queue a `getGroupCall` fetch so tracking starts even if
            // the `updateGroupCall` is delayed; the update remains the
            // source of truth.
            EnvelopePayload::GroupCallId { id } => {
                if let Some(RequestPurpose::CreateVideoChat { .. }) = pending.map(|p| p.purpose)
                    && !self.group_call_fetch_queue.contains(&id)
                {
                    self.group_call_fetch_queue.push(id);
                }
                self.group_call_error = None;
            }
            // Phase C2f: `groupCallInfo` — the `joinGroupCall`
            // answer to invitation acceptance. Queue a `getGroupCall`
            // fetch so tracking starts even if the `updateGroupCall`
            // is delayed; the update remains the source of truth for
            // `is_joined`. Store the tgcalls join payload like the
            // `joinVideoChat` Text arm does.
            EnvelopePayload::GroupCallInfo {
                group_call_id,
                join_payload,
            } => {
                if let Some(RequestPurpose::JoinGroupCallInvitation) = pending.map(|p| p.purpose) {
                    if !self.group_call_fetch_queue.contains(&group_call_id) {
                        self.group_call_fetch_queue.push(group_call_id);
                    }
                    self.set_group_call_join_payload(group_call_id, join_payload);
                }
                self.group_call_error = None;
            }
            // Phase C3a: group-call signaling (schema 1.8.67, lines
            // 10819 / 10824 / 10830 / 10836 / 10576). `updateGroupCall`
            // drives the tracked-call state; participant updates feed
            // the grid; the verification state feeds the E2E emoji UI;
            // `updateChatVideoChat` refreshes the chat's join affordance.
            // All signaling-only — no media transport until Phase C2.
            EnvelopePayload::UpdateGroupCall { group_call } => {
                self.accept_group_call_update(&group_call);
            }
            EnvelopePayload::UpdateGroupCallParticipant {
                group_call_id,
                participant,
            } => {
                self.accept_group_call_participant_update(group_call_id, &participant);
            }
            EnvelopePayload::UpdateGroupCallParticipants {
                group_call_id,
                participant_user_ids,
            } => {
                self.accept_group_call_participants_update(group_call_id, &participant_user_ids);
            }
            EnvelopePayload::UpdateGroupCallVerificationState {
                group_call_id,
                generation,
                emojis,
            } => {
                self.accept_group_call_verification_state(group_call_id, generation, &emojis);
            }
            EnvelopePayload::UpdateChatVideoChat {
                chat_id,
                video_chat,
            } => {
                self.accept_chat_video_chat(ChatId(chat_id), &video_chat);
            }
            // Phase C2h: in-call chat message updates.
            EnvelopePayload::UpdateNewGroupCallMessage {
                group_call_id,
                message,
            } => {
                self.accept_new_group_call_message(group_call_id, &message);
            }
            EnvelopePayload::UpdateGroupCallMessageSendFailed {
                group_call_id,
                message_id: _,
                error,
            } => {
                if self
                    .active_group_call
                    .as_ref()
                    .is_some_and(|c| c.id == group_call_id)
                {
                    self.group_call_error = Some(call_request_error_line(
                        &error,
                        "Could not send the message",
                    ));
                }
            }
            EnvelopePayload::UpdateGroupCallMessagesDeleted {
                group_call_id,
                message_ids,
            } => {
                self.accept_group_call_messages_deleted(group_call_id, &message_ids);
            }
            // Phase C2h: `rtmpUrl` — the `getVideoChatRtmpUrl` /
            // `replaceVideoChatRtmpUrl` answer. Stored on the tracked
            // call whose chat the request targeted.
            EnvelopePayload::RtmpUrl { url, stream_key } => {
                if let Some(
                    RequestPurpose::GetVideoChatRtmpUrl { chat_id }
                    | RequestPurpose::ReplaceVideoChatRtmpUrl { chat_id },
                ) = pending.map(|p| p.purpose)
                {
                    let call_id = self
                        .chats
                        .get(&chat_id)
                        .and_then(|c| c.video_chat.as_ref())
                        .map(|vc| vc.group_call_id);
                    if let (Some(call_id), Some(tracked)) =
                        (call_id, self.active_group_call.as_mut())
                        && tracked.id == call_id
                    {
                        tracked.rtmp_url = Some(url);
                        tracked.rtmp_stream_key = Some(stream_key);
                    }
                }
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
            // Slice CL3: mention / reaction badge counts (schema 1.8.67,
            // lines 10567/10570).
            EnvelopePayload::UpdateChatUnreadMentionCount {
                chat_id,
                unread_mention_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_mention_count = unread_mention_count;
            }
            EnvelopePayload::UpdateChatUnreadReactionCount {
                chat_id,
                unread_reaction_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_reaction_count = unread_reaction_count;
            }
            // Slice CL3: `updateChatBlockList` (schema 1.8.67, line
            // 10594).
            EnvelopePayload::UpdateChatBlockList { chat_id, blocked } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .blocked = blocked;
            }
            // Slice CL3: `reportChat` result — surfaced as a status note
            // via the same drain as `chat_action_error`; a refusal or a
            // "more info required" is never shown as success.
            EnvelopePayload::ReportChatResult(outcome) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChat) {
                    self.report_chat_outcome = Some(match outcome {
                        ReportChatOutcome::Ok => "chat reported".to_string(),
                        ReportChatOutcome::MoreInfoRequired => "report needs a reason or messages — the chat list only sends simple spam reports".to_string(),
                    });
                }
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
                    .map(|message| {
                        effective_content(&message.content, message.ephemeral.as_ref()).preview()
                    })
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
                // Phase 9.3: a `postStory` answer is the pending story —
                // its id is the temporary id the succeeded/failed updates
                // correlate against.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::PostStory) {
                    self.story_post.outcome = StoryPostOutcome::Posting { story_id: story.id };
                }
                self.stories.insert((story.poster_chat_id, story.id), story);
            }
            EnvelopePayload::CanPostStoryResult { result } => {
                // Phase 9.3: `canPostStory` answer — honored only for the
                // composer's own check (purpose-gated, so a stray result
                // never flips the UI).
                if pending.is_some_and(|p| p.purpose == RequestPurpose::CheckCanPostStory) {
                    self.story_post.eligibility = Some(result);
                    self.story_post.check_error = None;
                }
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
                old_story_id,
            } => {
                // Phase 9.2: a story posted from another client is live —
                // upsert it and refresh the poster's tray row so an own
                // story appears in the tray.
                self.remember_files(&files);
                let poster_chat_id = story.poster_chat_id;
                // Phase 9.3: our own pending post went live — the composer
                // shows "Posted".
                if matches!(
                    self.story_post.outcome,
                    StoryPostOutcome::Posting { story_id } if story_id == old_story_id
                ) {
                    self.story_post.outcome = StoryPostOutcome::Succeeded;
                }
                self.stories.insert((story.poster_chat_id, story.id), story);
                self.story_tray_refresh.insert(poster_chat_id);
            }
            EnvelopePayload::UpdateStoryPostFailed { story, error } => {
                // Phase 9.2: a story failed to post — drop it like a delete
                // (it never went live).
                // Phase 9.3: the failure reaches our own pending post (the
                // `sendStory` blocker was retracted — the constructor is
                // `postStory`, schema `td_api.tl:13715`) and the composer
                // shows it.
                if matches!(
                    self.story_post.outcome,
                    StoryPostOutcome::Posting { story_id } if story_id == story.id
                ) {
                    self.story_post.outcome = StoryPostOutcome::Failed(format!(
                        "Posting failed: {}",
                        error_reason(&error)
                    ));
                }
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
            EnvelopePayload::StoryInteractions { interactions } => {
                // Phase 9.5: a `getStoryInteractions` page — honored only
                // for the viewer's own fetch (purpose-gated, and the page
                // is dropped when the viewer moved to another story).
                if pending.is_some_and(|p| p.purpose == RequestPurpose::GetStoryInteractions)
                    && let Some(pending) = pending
                {
                    self.accept_story_interactions(pending, interactions);
                }
            }
            EnvelopePayload::ReportStoryResult(result) => {
                // Phase 9.5: a `reportStory` answer — honored only for the
                // viewer's own report flow.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::ReportStory)
                    && let Some(pending) = pending
                {
                    self.accept_story_report(pending, result);
                }
            }
            EnvelopePayload::UpdateStoryStealthMode {
                active_until_date,
                cooldown_until_date,
            } => {
                // Phase 9.5: stealth-mode state changed (Telegram X keeps
                // the same two timestamps; there is no getter, so updates
                // are the only source).
                self.apply_update_story_stealth_mode(active_until_date, cooldown_until_date);
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
                // B1: an incoming message demanding a reply (force-reply
                // markup) arms the composer's reply-to; the UI drains
                // `pending_force_reply` on the next render. Computed before
                // `upsert_message` moves `message`.
                let force_reply = (!message.is_outgoing
                    && message
                        .reply_markup
                        .as_ref()
                        .is_some_and(reply_markup_demands_reply))
                .then_some(ForceReplyTarget {
                    chat_id: message.chat_id,
                    message_id: message.id,
                });
                self.upsert_message(message, false);
                if let Some(target) = force_reply {
                    self.pending_force_reply = Some(target);
                }
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
                // M1: mark the row failed. The retry affordance is gated
                // separately on `can_retry` (`resendMessages` via
                // `driver.resend_failed_message`) — not every failed send
                // may be retried.
                let mut row = history_message(message, true);
                row.failed = true;
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
                // M1 fix-up: the edited message may be a scheduled send —
                // refresh the scheduled-list entry too, not just history.
                if let Some(slot) = self
                    .scheduled_messages
                    .iter_mut()
                    .find(|m| m.chat_id == chat_id && m.id == message_id)
                {
                    slot.content = content.clone();
                }
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
                // Phase 9.5: `getChatsToPostStories` answer — the
                // composer's "post as" picker options. Runs before the
                // search branch below consumes `chat_ids`.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatsToPostStories) {
                    self.story_post_as_chats = chat_ids.iter().map(|id| id.0).collect();
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
            EnvelopePayload::FoundMessages {
                messages,
                next_offset,
                ..
            } => {
                if self.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchMessages)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.accept_messages(hits, false);
                }
                // Phase C2i: `searchCallMessages` pages for the
                // Recent-calls tab. `searchCallMessages` returns call and
                // group-call messages newest-first; the rows render both.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchCallMessages) {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    self.recent_calls.extend(messages);
                    self.recent_calls_offset = next_offset;
                    self.recent_calls_loading = false;
                    self.recent_calls_error = false;
                }
            }
            // Phase C2i: `getUserPrivacySettingRules` answer — map the
            // rule list to the simple Everybody / Contacts / Nobody
            // choice (`None` when the account has custom rules the UI
            // cannot represent; the radios then show nothing selected).
            EnvelopePayload::UserPrivacySettingRules { rules } => {
                if let Some(RequestPurpose::GetCallPrivacyRules { setting }) =
                    pending.map(|p| p.purpose)
                {
                    let who = PrivacyWho::from_rule_names(&rules);
                    match setting {
                        CallPrivacySetting::AllowCalls => self.call_privacy_allow_calls = who,
                        CallPrivacySetting::PeerToPeer => self.call_privacy_p2p = who,
                    }
                    self.privacy_roundtrip_done();
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
                // Slice media-shared-gallery: one gallery-tab page.
                // Correlated by chat + tab + generation stamped on the
                // request purpose; late answers drop in `accept`.
                if let Some(RequestPurpose::GetSharedMedia { tab, generation }) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let items = messages
                        .iter()
                        .map(|message| SharedMediaItem::from_parsed(tab, message))
                        .collect();
                    self.shared_media
                        .accept(chat_id, tab, generation, items, total_count);
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
                can_invite_users,
                can_promote_members,
                can_manage_tags,
                can_manage_topics,
                can_change_info,
                can_send_welcome_messages,
                join_by_request,
                is_broadcast_group,
                sign_messages,
                show_message_sender,
            } => {
                self.set_supergroup_forum(supergroup_id, is_forum);
                self.set_supergroup_username(supergroup_id, username);
                // Phase A1: own member status drives the slow-mode bypass.
                self.supergroup_member_status.insert(supergroup_id, status);
                // Phase A1: `can_restrict_members` gates the slow-mode
                // admin control; absent = unknown → treated as lacking.
                self.supergroup_restrict_right
                    .insert(supergroup_id, can_restrict_members.unwrap_or(false));
                // Phase D3a: `can_invite_users` gates invite-link /
                // join-request management; absent = unknown → lacking.
                self.supergroup_invite_right
                    .insert(supergroup_id, can_invite_users.unwrap_or(false));
                // Phase D3b: `can_promote_members` gates admin
                // management; absent = unknown → lacking.
                self.supergroup_promote_right
                    .insert(supergroup_id, can_promote_members.unwrap_or(false));
                // Slice G1: `can_manage_tags` gates custom-title
                // changes for other members.
                self.supergroup_manage_tags_right
                    .insert(supergroup_id, can_manage_tags.unwrap_or(false));
                // Slice G2: forum-topic / sign-messages / welcome-message
                // rights; absent = unknown → treated as lacking.
                self.supergroup_manage_topics_right
                    .insert(supergroup_id, can_manage_topics.unwrap_or(false));
                self.supergroup_change_info_right
                    .insert(supergroup_id, can_change_info.unwrap_or(false));
                self.supergroup_send_welcome_right
                    .insert(supergroup_id, can_send_welcome_messages.unwrap_or(false));
                // Slice G1: `supergroup.join_by_request` (schema 1.8.67,
                // lines 2733/2746) drives the "Approve new members"
                // toggle; `supergroup.is_broadcast_group` (lines
                // 2736/2746) drives the broadcast-group toggle.
                self.supergroup_join_by_request
                    .insert(supergroup_id, join_by_request);
                self.supergroup_is_broadcast
                    .insert(supergroup_id, is_broadcast_group);
                // Slice G2: `supergroup.sign_messages` /
                // `show_message_sender` (schema 1.8.67, lines 2731/2746).
                self.supergroup_sign_messages
                    .insert(supergroup_id, sign_messages);
                self.supergroup_show_message_sender
                    .insert(supergroup_id, show_message_sender);
            }
            EnvelopePayload::Supergroup {
                supergroup_id,
                is_forum,
                username,
                status,
                can_restrict_members,
                can_invite_users,
                can_promote_members,
                can_manage_tags,
                can_manage_topics,
                can_change_info,
                can_send_welcome_messages,
                join_by_request,
                is_broadcast_group,
                sign_messages,
                show_message_sender,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupergroup) {
                    self.set_supergroup_forum(supergroup_id, is_forum);
                    self.set_supergroup_username(supergroup_id, username);
                    // Phase A1: own member status drives the slow-mode bypass.
                    self.supergroup_member_status.insert(supergroup_id, status);
                    self.supergroup_restrict_right
                        .insert(supergroup_id, can_restrict_members.unwrap_or(false));
                    // Phase D3a: `can_invite_users` gates invite-link /
                    // join-request management.
                    self.supergroup_invite_right
                        .insert(supergroup_id, can_invite_users.unwrap_or(false));
                    // Phase D3b: `can_promote_members` gates admin management.
                    self.supergroup_promote_right
                        .insert(supergroup_id, can_promote_members.unwrap_or(false));
                    // Slice G1: `can_manage_tags` gates custom-title
                    // changes for other members.
                    self.supergroup_manage_tags_right
                        .insert(supergroup_id, can_manage_tags.unwrap_or(false));
                    // Slice G2: forum-topic / sign-messages /
                    // welcome-message rights.
                    self.supergroup_manage_topics_right
                        .insert(supergroup_id, can_manage_topics.unwrap_or(false));
                    self.supergroup_change_info_right
                        .insert(supergroup_id, can_change_info.unwrap_or(false));
                    self.supergroup_send_welcome_right
                        .insert(supergroup_id, can_send_welcome_messages.unwrap_or(false));
                    // Slice G1: join-by-request + broadcast flags (schema
                    // 1.8.67, lines 2733/2736/2746).
                    self.supergroup_join_by_request
                        .insert(supergroup_id, join_by_request);
                    self.supergroup_is_broadcast
                        .insert(supergroup_id, is_broadcast_group);
                    // Slice G2: sign/show flags (schema 1.8.67, lines
                    // 2731/2746).
                    self.supergroup_sign_messages
                        .insert(supergroup_id, sign_messages);
                    self.supergroup_show_message_sender
                        .insert(supergroup_id, show_message_sender);
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
            // Slice G2: `createForumTopic` answers `forumTopicInfo` —
            // drop the cached topic list so the UI refetches it.
            EnvelopePayload::ForumTopic { chat_id } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::CreateForumTopic) {
                    self.forum_topics.remove(&chat_id);
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
                // M1: scheduled sends go to the scheduled list, not history.
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetChatScheduledMessages
                {
                    self.scheduled_messages = messages.to_vec();
                    return;
                }
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ForwardMessages
                {
                    self.finish_forward(pending, &messages, false);
                    return;
                }
                // Slice G2: channel-comments viewer — cache the thread
                // history for the requesting channel post.
                if let Some(pending) = pending
                    && let RequestPurpose::GetMessageThreadHistory { message_id } = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                {
                    self.comment_thread = Some(CommentThreadFetch {
                        chat_id,
                        message_id: MessageId(message_id),
                        messages: messages.to_vec(),
                        failed: None,
                    });
                    return;
                }
                // Slice CL: chat-list peek preview — cache the latest
                // messages for the previewed (unopened) chat.
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetChatPreview
                    && let Some(chat_id) = pending.chat_id
                {
                    self.chat_preview_fetch = Some(PreviewHistoryFetch {
                        chat_id,
                        messages: messages.to_vec(),
                        failed: None,
                    });
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
                // M1 fix-up: editing a scheduled send returns the edited
                // `message` with `scheduling_state` set — refresh the
                // scheduled-list entry instead of inserting a phantom row
                // into chat history (which also left the scheduled list
                // showing the stale pre-edit text).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::EditMessage)
                    && message.scheduling_state.is_some()
                {
                    self.remember_files(&message.files);
                    if let Some(slot) = self
                        .scheduled_messages
                        .iter_mut()
                        .find(|m| m.id == message.id)
                    {
                        *slot = message;
                    } else {
                        self.scheduled_messages.push(message);
                    }
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMessage) {
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
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchStickerSets) {
                    // Slice S8: `searchStickerSets` answers with `stickerSets`.
                    self.accept_found_sticker_sets(sets);
                }
            }
            // Slice S8: `getTrendingStickerSets` answers with
            // `trendingStickerSets`.
            EnvelopePayload::TrendingStickerSets {
                sets, is_premium, ..
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTrendingStickerSets) {
                    self.accept_trending_sticker_sets(sets, is_premium);
                }
            }
            // Slice S8: `searchStickers` / `getFavoriteStickers` /
            // `getRecentStickers` answer with bare `stickers`.
            EnvelopePayload::Stickers { stickers, files } => {
                self.remember_files(&files);
                let purpose = pending.map(|p| p.purpose);
                if purpose == Some(RequestPurpose::SearchStickers) {
                    self.accept_found_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetFavoriteStickers) {
                    self.accept_favorite_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetRecentStickers) {
                    self.accept_recent_stickers(stickers);
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
                    self.accept_own_chat_member(chat_id, member.clone());
                }
                // Phase D3b: `getChatMember` for one administrator's rights
                // (edit dialog). Only an administrator status carries a
                // rights block worth caching.
                if let Some(RequestPurpose::GetAdminRights { user_id }) = pending.map(|p| p.purpose)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(rights) = member.admin_rights
                {
                    self.admin_rights
                        .insert((chat_id.0, user_id), AdminRightsFetch::Loaded(rights));
                }
            }
            EnvelopePayload::UserFullInfo {
                bot_info,
                bio,
                photo,
                photo_id,
                blocked,
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
                        self.user_full_infos.insert(
                            user_id,
                            UserFullInfoData {
                                bio,
                                photo_file_id,
                                photo_id,
                                blocked,
                            },
                        );
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
                photo_id,
                blocked,
            } => {
                self.bot_info.insert(user_id.0, bot_info);
                let photo_file_id = photo.map(|file| {
                    let id = file.id.0;
                    self.upsert_file(file, false);
                    id
                });
                self.user_full_infos.insert(
                    user_id.0,
                    UserFullInfoData {
                        bio,
                        photo_file_id,
                        photo_id,
                        blocked,
                    },
                );
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
                // Phase D3b: any membership change may alter the admin
                // list — drop the cached list so the info panel refetches
                // instead of showing stale data. `accept_own_chat_member`
                // also refreshes the viewer's own rights below.
                self.admin_lists.remove(&chat_id.0);
                // Phase D3b: per-admin rights for the changed member are
                // stale too (e.g. after an edit-rights save) — drop them
                // so the editor refetches instead of showing old rights.
                if let MessageSender::User { user_id } = member.member_id {
                    self.admin_rights.remove(&(chat_id.0, user_id));
                }
                // Slice G1 fix-up: membership changes also stale the
                // member-list caches (e.g. our own add, or someone else
                // joining). Drop both so the dialog refetches instead of
                // showing the pre-change list; `member_list_stale` tells
                // the UI an open dialog needs a refetch.
                self.basic_group_members.remove(&chat_id.0);
                self.supergroup_members
                    .retain(|(id, _), _| *id != chat_id.0);
                if !self.member_list_stale.contains(&chat_id.0) {
                    self.member_list_stale.push(chat_id.0);
                }
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
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::GetCallbackQueryAnswer
                            | RequestPurpose::GetCallbackQueryAnswerWithPassword
                            | RequestPurpose::GetCallbackQueryAnswerGame
                    )
                ) {
                    self.last_callback_answer = Some(answer);
                }
            }
            EnvelopePayload::LoginUrlInfo(info) => {
                // B1: `getLoginUrlInfo` response to our own login-button
                // press (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrlInfo) {
                    self.last_login_url_info = Some(info);
                }
            }
            EnvelopePayload::PaymentForm(form) => {
                // Slice P1: `getPaymentForm` answer to our own Buy press
                // (matched by `@extra`). Opens the checkout dialog.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentForm) {
                    self.payment_form = Some(form);
                    self.payment_form_loading = false;
                    self.payment_validated = None;
                    self.payment_shipping_id = None;
                    self.payment_note = None;
                }
            }
            EnvelopePayload::ValidatedOrderInfo(validated) => {
                // Slice P1: `validateOrderInfo` answer (matched by `@extra`).
                // The first shipping option is pre-selected, like the
                // official clients.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ValidateOrderInfo) {
                    self.payment_shipping_id =
                        validated.shipping_options.first().map(|o| o.id.clone());
                    self.payment_validated = Some(validated);
                    self.payment_note = None;
                }
            }
            EnvelopePayload::PaymentResult(result) => {
                // Slice P1: `sendPaymentForm` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendPaymentForm) {
                    self.payment_sending = false;
                    if result.success {
                        self.payment_note = Some("✅ Payment successful".to_string());
                    } else if !result.verification_url.is_empty() {
                        // Schema: the URL is for additional payment
                        // credentials verification (e.g. 3-D Secure) — the
                        // UI opens it in the OS browser.
                        self.payment_verification_url = Some(result.verification_url);
                    } else {
                        self.payment_note = Some("Payment failed".to_string());
                    }
                }
            }
            EnvelopePayload::PaymentReceipt(receipt) => {
                // Slice P1: `getPaymentReceipt` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentReceipt) {
                    self.payment_receipt = Some(receipt);
                    self.payment_receipt_open = true;
                }
            }
            EnvelopePayload::UpdateSavedAnimations { .. } => {
                if self.gifs.open {
                    self.gifs.stale = true;
                }
            }
            // Slice S9: `updateAnimationSearchParameters` (schema 1.8.67,
            // line 11064) — server-pushed; store the provider name and the
            // new suggested search emojis for the GIF search surface.
            EnvelopePayload::UpdateAnimationSearchParameters { provider, emojis } => {
                self.gifs.search_provider = provider;
                self.gifs.provider_emojis = emojis;
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
            EnvelopePayload::StorageStatistics {
                total_size,
                by_file_type,
            } => {
                // Phase S2: `getStorageStatistics` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStorageStatistics) {
                    self.storage_stats = Some(StorageStats {
                        total_size,
                        by_file_type,
                    });
                    self.storage_stats_loading = false;
                }
            }
            EnvelopePayload::PasswordState { state } => {
                // Slice A2: `passwordState` answer — only our own
                // in-flight `PasswordStateOp` writes the cache (matched by
                // `@extra`). The response is authoritative: it replaces
                // the cached state and clears any stale error. No
                // optimistic mutation ever happens client-side.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::PasswordStateOp { .. })
                ) {
                    self.password_state = Some(state);
                    self.password_state_loading = false;
                    self.password_op_error = None;
                }
            }
            EnvelopePayload::Sessions { sessions } => {
                // Slice A3: `getActiveSessions` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetActiveSessions) {
                    self.sessions = Some(sessions);
                    self.sessions_loading = false;
                    self.sessions_error = None;
                    self.sessions_stale = false;
                }
            }
            EnvelopePayload::ConnectedWebsites { websites } => {
                // Slice A4: `getConnectedWebsites` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetConnectedWebsites) {
                    self.connected_websites = Some(websites);
                    self.connected_websites_loading = false;
                    self.websites_error = None;
                    self.websites_stale = false;
                }
            }
            EnvelopePayload::ArchiveChatListSettings { settings } => {
                // Slice CL2: `getArchiveChatListSettings` answer — only
                // our own in-flight request writes the cache.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetArchiveChatListSettings) {
                    self.archive_chat_list_settings = Some(settings);
                    self.archive_settings_loading = false;
                }
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
            // Phase C2g: `joinVideoChat` returns `text` — the tgcalls
            // join answer, stored on the tracked call and consumed by
            // the driver pump (`ntg_connect`).
            // `startGroupCallScreenSharing` returns `text` — the
            // presentation answer, consumed by the driver pump.
            EnvelopePayload::Text { text } => match pending.map(|p| p.purpose) {
                Some(RequestPurpose::JoinVideoChat { group_call_id }) => {
                    self.set_group_call_join_payload(group_call_id, text);
                }
                Some(RequestPurpose::StartGroupCallScreenSharing { group_call_id }) => {
                    self.set_group_call_screen_share_answer(group_call_id, text);
                }
                _ => {}
            },
            // Phase C3a: `getVideoChatInviteLink` returns `httpUrl`.
            EnvelopePayload::HttpUrl { url } => {
                if let Some(RequestPurpose::GetVideoChatInviteLink { group_call_id }) =
                    pending.map(|p| p.purpose)
                {
                    self.set_group_call_invite_link(group_call_id, url);
                // B1: `getLoginUrl` returns `httpUrl` too (schema 1.8.67,
                // line 7458) — the authorized URL after consent.
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrl) {
                    self.last_login_url_info = Some(LoginUrlInfo::Open { url });
                }
            }
            // M1: `getMessageLink` returns `messageLink`. The driver
            // stashes the link in `Session::message_link_result` before
            // `apply` takes the pending request; nothing to reduce here.
            EnvelopePayload::MessageLink { .. } => {}
            // A5: `checkChatUsername` answer — the driver stashes the
            // verdict in `Session::username_check` before `apply` takes
            // the pending request; nothing to reduce here.
            EnvelopePayload::CheckChatUsernameResult(_) => {}
            // M2: handled by the driver before `apply` (blocks land in
            // history there); nothing to reduce here.
            EnvelopePayload::RichMessage { .. } => {}
            // MED4: `webPageInstantView` — captured by the driver before
            // `apply` into `Session::instant_view` (success) or
            // `Session::instant_view_fallback_url` (error); nothing to
            // reduce here.
            EnvelopePayload::WebPageInstantView { .. } => {}
            // MED4b: `linkPreview` (`getLinkPreview` answer) — captured
            // by the driver before `apply` into
            // `Session::composer_preview`; nothing to reduce here.
            EnvelopePayload::LinkPreview { .. } => {}
            // M1 fix-up: `getMessageProperties` returns
            // `messageProperties`. The driver gates the chained
            // `getMessageLink` on `can_get_link` before `apply` takes
            // the pending request; nothing to reduce here.
            EnvelopePayload::MessageProperties { .. } => {}
            // Phase C2f: `inviteGroupCallParticipant` answer. A success
            // clears any earlier invite error; the three failure
            // variants surface honestly via `group_call_error` (shown
            // on the group-call overlay).
            EnvelopePayload::InviteGroupCallParticipantResult(result) => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::InviteGroupCallParticipant { .. })
                ) {
                    self.group_call_error = match result {
                        InviteGroupCallParticipantResult::Success { .. } => None,
                        InviteGroupCallParticipantResult::UserPrivacyRestricted => Some(
                            "Couldn't invite: that user restricts group-call invitations."
                                .to_string(),
                        ),
                        InviteGroupCallParticipantResult::UserAlreadyParticipant => {
                            Some("That user is already in the voice chat.".to_string())
                        }
                        InviteGroupCallParticipantResult::UserWasBanned => {
                            Some("That user was banned from the voice chat.".to_string())
                        }
                    };
                }
            }
            EnvelopePayload::Ok => {
                // Slice A3: a `terminateSession` /
                // `terminateAllOtherSessions` succeeded — keep the old
                // cache visible and mark it stale so the driver refetches
                // the authoritative answer on this same ingest (the
                // `saved_sounds_stale` pattern). No optimistic deletion:
                // the terminated row stays until the server-confirmed
                // list replaces it.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::TerminateSession { .. }
                            | RequestPurpose::TerminateAllOtherSessions
                    )
                ) {
                    self.sessions_stale = true;
                    self.sessions_mutating = false;
                    self.sessions_error = None;
                }
                // Slice A4: a `toggleSessionCanAcceptSecretChats` /
                // `toggleSessionCanAcceptCalls` succeeded — same stale
                // pattern: the toggled value comes back in the
                // authoritative refetch, never from an optimistic flip.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::ToggleSessionSecretChats { .. }
                            | RequestPurpose::ToggleSessionCalls { .. }
                    )
                ) {
                    self.sessions_stale = true;
                    self.sessions_mutating = false;
                    self.sessions_error = None;
                }
                // Slice A4: a `disconnectWebsite` /
                // `disconnectAllWebsites` succeeded — same stale pattern
                // on the websites list.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::DisconnectWebsite { .. }
                            | RequestPurpose::DisconnectAllWebsites
                    )
                ) {
                    self.websites_stale = true;
                    self.websites_mutating = false;
                    self.websites_error = None;
                }
                // Slice S8: a sticker-set mutation succeeded — invalidate
                // the affected cache so the next fetch shows the
                // server-confirmed list instead of a stale one.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::AddFavoriteSticker | RequestPurpose::RemoveFavoriteSticker
                    )
                ) {
                    self.stickers.favorites.clear();
                }
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::ClearRecentStickers)
                ) {
                    self.stickers.recent.clear();
                }
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::ChangeStickerSet
                            | RequestPurpose::ReorderInstalledStickerSets
                    )
                ) {
                    self.invalidate_installed_sticker_sets();
                }
                // Slice S9: a saved-GIF mutation (`addSavedAnimation` /
                // `removeSavedAnimation`) succeeded — drop the saved list so
                // the panel refetches the server-confirmed list instead of
                // a stale one.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
                ) {
                    self.gifs.animations.clear();
                }
                // Phase C3a: a successful `leaveGroupCall` /
                // `endGroupCall` drops the tracked call (the `ok`
                // confirms the server side; `updateGroupCall`
                // `!is_active` is the backstop).
                match pending.map(|p| p.purpose) {
                    Some(
                        RequestPurpose::LeaveGroupCall { group_call_id }
                        | RequestPurpose::EndGroupCall { group_call_id },
                    ) if self
                        .active_group_call
                        .as_ref()
                        .is_some_and(|c| c.id == group_call_id) =>
                    {
                        self.leave_group_call_local();
                    }
                    // Phase C2h: the server confirmed revocation — drop
                    // the cached link so the UI stops showing it.
                    Some(RequestPurpose::RevokeVideoChatInviteLink { group_call_id })
                        if self
                            .active_group_call
                            .as_ref()
                            .is_some_and(|c| c.id == group_call_id) =>
                    {
                        if let Some(tracked) = self.active_group_call.as_mut() {
                            tracked.invite_link = None;
                        }
                    }
                    // Slice G2: forum-topic mutations confirmed — drop
                    // the cached topic list so the UI refetches it.
                    Some(
                        RequestPurpose::EditForumTopic { .. }
                        | RequestPurpose::ToggleForumTopicClosed { .. }
                        | RequestPurpose::ToggleForumTopicPinned { .. }
                        | RequestPurpose::DeleteForumTopic { .. }
                        | RequestPurpose::ToggleGeneralForumTopicHidden,
                    ) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.forum_topics.remove(&chat_id.0);
                        }
                    }
                    // Slice G2: welcome-message mutations confirmed —
                    // drop the cached pack so the dialog refetches it.
                    Some(
                        RequestPurpose::AddChatWelcomeMessage
                        | RequestPurpose::EditChatWelcomeMessage { .. }
                        | RequestPurpose::DeleteChatWelcomeMessage { .. },
                    ) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.welcome_messages.remove(&chat_id.0);
                            self.welcome_message_fetches.remove(&chat_id.0);
                        }
                    }
                    // Phase 9.5: a posted-story management call landed —
                    // clear the spinner; the edited story itself arrives
                    // via `updateStory`.
                    Some(
                        RequestPurpose::EditStory
                        | RequestPurpose::EditStoryCover
                        | RequestPurpose::SetStoryPrivacySettings,
                    ) => {
                        self.story_manage.pending = false;
                    }
                    _ => {}
                }
                // Phase C2i: `sendCallLog` confirmed — the log upload for
                // the ended call succeeded.
                if let Some(RequestPurpose::SendCallLog) = pending.map(|p| p.purpose)
                    && let Some(summary) = self.call_summary.as_mut()
                {
                    summary.log_sent = true;
                    summary.log_error = None;
                }
                // Phase C2i: `setUserPrivacySettingRules` confirmed (the
                // new value was applied optimistically at send time).
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::SetCallPrivacyRules { .. })
                ) {
                    self.privacy_roundtrip_done();
                }
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
                // Phase D3b: `setChatMemberStatus` confirmed — the member
                // change itself arrives as `updateChatMember`. Invalidate
                // the cached admin list so the panel refetches instead of
                // showing stale data.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::SetChatMemberStatus { .. })
                ) && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.admin_lists.remove(&chat_id.0);
                    // Slice G1: restrict/ban/unban change the member
                    // lists too — drop all cached pages for this chat.
                    if matches!(
                        pending.map(|p| p.purpose),
                        Some(RequestPurpose::SetChatMemberStatus {
                            kind: MemberStatusChange::Restrict
                                | MemberStatusChange::Ban
                                | MemberStatusChange::Unban,
                            ..
                        })
                    ) {
                        self.supergroup_members
                            .retain(|(id, _), _| *id != chat_id.0);
                    }
                }
                // Slice G1: `setChatMemberTag` confirmed — the custom
                // title itself arrives via `updateChatMember`; drop
                // cached member pages so the new tag is refetched
                // instead of showing stale data.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::SetChatMemberTag { .. })
                ) && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.supergroup_members
                        .retain(|(id, _), _| *id != chat_id.0);
                    self.basic_group_members.remove(&chat_id.0);
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LeaveChat)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    // Optimistic: `updateChatMember` confirms. TDLib errors
                    // keep the old status (Error arm below does not touch it).
                    chat.set_member_status(ChannelMemberStatus::Left, None);
                }
                // Slice G1: `deleteChat` confirmed — drop the chat locally.
                // The schema (1.8.67, line 11850) deletes the chat for all
                // members and releases the username; no update announces
                // it, so the client removes it itself.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::DeleteChat)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chats.remove(&chat_id.0);
                    self.supergroup_members
                        .retain(|(id, _), _| *id != chat_id.0);
                    self.admin_lists.remove(&chat_id.0);
                    self.add_members_failed.remove(&chat_id.0);
                }
                // Phase D3a: `processChatJoinRequest` confirmed — drop the
                // processed request from the cached list. The count is
                // approximate per the schema; `updateChatPendingJoinRequests`
                // is the authoritative badge source.
                if let Some(RequestPurpose::ProcessChatJoinRequest { user_id }) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(JoinRequestFetch::Loaded(list)) =
                        self.join_requests.get(&chat_id.0).cloned()
                {
                    let requests: Vec<ParsedChatJoinRequest> = list
                        .requests
                        .into_iter()
                        .filter(|r| r.user_id != user_id)
                        .collect();
                    self.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Loaded(JoinRequestList {
                            total_count: list.total_count.saturating_sub(1),
                            requests,
                        }),
                    );
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::AddContact) {
                    // Phase 6: the new contact arrives via `updateUser`
                    // (`is_contact` flips); invalidate the list so the
                    // contacts tab refetches it.
                    self.contacts = None;
                    self.contacts_error = false;
                }
                // Slice A6: a contacts mutation landed — never optimistic:
                // the new list arrives via the `getContacts` refetch the
                // tab triggers.
                match pending.map(|p| p.purpose) {
                    Some(RequestPurpose::RemoveContact) => {
                        self.contacts = None;
                        self.contacts_error = false;
                        // The delete-synced-contacts batch remove shares
                        // this purpose but carries no user_id — its notice
                        // must not read as a single delete (or overwrite
                        // the synced flow's own notice on arrival order).
                        self.contacts_notice = Some(if pending.and_then(|p| p.user_id).is_some() {
                            "Contact deleted.".to_string()
                        } else {
                            "Synced contacts deleted from the servers.".to_string()
                        });
                        // Slice A6: the server confirmed the deletion —
                        // drop the contact flag on the cached user too so
                        // the info panel stops offering "Delete contact"
                        // before the refetched list arrives.
                        if let Some(user_id) = pending.and_then(|p| p.user_id)
                            && let Some(user) = self.users.get_mut(&user_id)
                        {
                            user.is_contact = false;
                        }
                    }
                    Some(RequestPurpose::ImportContacts) => {
                        self.contacts = None;
                        self.contacts_error = false;
                        self.contacts_notice = Some("Contacts imported.".to_string());
                    }
                    Some(RequestPurpose::ClearImportedContacts) => {
                        self.contacts = None;
                        self.contacts_error = false;
                        self.contacts_notice =
                            Some("Synced contacts deleted from the servers.".to_string());
                    }
                    _ => {}
                }
                // Slice A6: a user-scoped `setMessageSenderBlockList`
                // succeeded — the `ok` carries no state, but the request
                // we just confirmed does, so the cached
                // `UserFullInfoData.blocked` is updated authoritatively
                // (never flipped optimistically). Chat-scoped (CL3)
                // requests carry no user_id and keep flowing through
                // `updateChatBlockList`.
                if let Some(p) = pending
                    && let RequestPurpose::SetMessageSenderBlockList { block } = p.purpose
                    && let Some(user_id) = p.user_id
                    && let Some(info) = self.user_full_infos.get_mut(&user_id)
                {
                    info.blocked = block;
                }
                if pending.is_some_and(|p| is_auth_submit(p.purpose)) {
                    self.last_auth_error = None;
                }
            }
            // Slice A6: `importedContacts` (schema 1.8.67, line 14517) —
            // the `importContacts` answer. Same invalidate + notice as
            // the `ok` of the other contact mutations; the user ids are
            // not merged into the cache (the tab refetches the
            // authoritative list).
            EnvelopePayload::ImportedContacts { .. } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ImportContacts) {
                    self.contacts = None;
                    self.contacts_error = false;
                    self.contacts_notice = Some("Contacts imported.".to_string());
                }
            }
            EnvelopePayload::Error(err) => {
                // Phase 9.3: a `postStory` / `canPostStory` error — the
                // composer shows it instead of spinning forever.
                match pending.map(|p| p.purpose) {
                    Some(RequestPurpose::PostStory) => {
                        self.story_post.outcome = StoryPostOutcome::Failed(format!(
                            "Posting failed: {}",
                            error_reason(&err)
                        ));
                    }
                    Some(RequestPurpose::CheckCanPostStory) => {
                        self.story_post.check_error =
                            Some(format!("Eligibility check failed: {}", error_reason(&err)));
                    }
                    // Phase 9.5: a `getStoryInteractions` / `reportStory` /
                    // `activateStoryStealthMode` error — the viewer panel /
                    // report flow / stealth button shows it instead of
                    // spinning forever.
                    Some(RequestPurpose::GetStoryInteractions) => {
                        if let Some(pending) = pending {
                            self.fail_story_viewers(
                                pending,
                                format!("Could not load viewers: {}", error_reason(&err)),
                            );
                        }
                    }
                    Some(RequestPurpose::ReportStory) => {
                        if let Some(pending) = pending {
                            self.fail_story_report(
                                pending,
                                format!("Reporting failed: {}", error_reason(&err)),
                            );
                        }
                    }
                    Some(RequestPurpose::ActivateStoryStealthMode) => {
                        self.story_stealth_error =
                            Some(format!("Stealth mode failed: {}", error_reason(&err)));
                    }
                    // Phase 9.5: a posted-story management call failed —
                    // clear the spinner and surface the sanitized error.
                    Some(
                        RequestPurpose::EditStory
                        | RequestPurpose::EditStoryCover
                        | RequestPurpose::SetStoryPrivacySettings,
                    ) => {
                        self.story_manage.pending = false;
                        self.story_manage.error =
                            Some(format!("Story update failed: {}", error_reason(&err)));
                    }
                    // A5: profile-edit failures surface in the
                    // edit-profile dialog. The message is classified by
                    // `error_reason`, never the raw TDLib text.
                    Some(
                        RequestPurpose::SetName
                        | RequestPurpose::SetBio
                        | RequestPurpose::SetUsername
                        | RequestPurpose::CheckUsername
                        | RequestPurpose::ReorderActiveUsernames
                        | RequestPurpose::ToggleUsernameIsActive
                        | RequestPurpose::SetProfilePhoto
                        | RequestPurpose::DeleteProfilePhoto,
                    ) => {
                        self.profile_edit_error =
                            Some(format!("Profile update failed: {}", error_reason(&err)));
                    }
                    // Phase 9.5 (review fix-up): `getChatsToPostStories`
                    // failed — surface a transient error so the "Post as"
                    // picker doesn't silently show only "Myself".
                    Some(RequestPurpose::GetChatsToPostStories) => {
                        self.story_post.check_error = Some(format!(
                            "Could not load \"Post as\" chats: {}",
                            error_reason(&err)
                        ));
                    }
                    // Slice P1: a payment request failed — surface the
                    // reason in the checkout dialog instead of spinning
                    // forever. The receipt fetch has its own error field:
                    // the checkout dialog may be closed, so `payment_note`
                    // (rendered only there) would stay invisible.
                    Some(
                        RequestPurpose::GetPaymentForm
                        | RequestPurpose::ValidateOrderInfo
                        | RequestPurpose::SendPaymentForm,
                    ) => {
                        self.payment_form_loading = false;
                        self.payment_sending = false;
                        self.payment_note = Some(format!("Payment failed: {}", error_reason(&err)));
                    }
                    Some(RequestPurpose::GetPaymentReceipt) => {
                        self.payment_receipt_error =
                            Some(format!("Receipt failed: {}", error_reason(&err)));
                    }
                    _ => {}
                }
                // Slice G1: roll back optimistic mutations the server
                // rejected — the pre-request value rides on
                // `PendingRequest::rollback`.
                match pending.and_then(|p| p.rollback.clone()) {
                    Some(RequestRollback::ChatPermissions {
                        previous,
                        previous_can_send,
                    }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                            && let Some(chat) = self.chats.get_mut(&chat_id.0)
                        {
                            chat.permissions = previous;
                            chat.can_send_basic_messages = previous_can_send;
                        }
                    }
                    Some(RequestRollback::JoinByRequest {
                        supergroup_id,
                        previous,
                    }) => match previous {
                        Some(flag) => {
                            self.supergroup_join_by_request.insert(supergroup_id, flag);
                        }
                        None => {
                            self.supergroup_join_by_request.remove(&supergroup_id);
                        }
                    },
                    Some(RequestRollback::SupergroupUsername {
                        supergroup_id,
                        previous,
                    }) => match previous {
                        Some(username) => {
                            self.supergroup_usernames.insert(supergroup_id, username);
                        }
                        None => {
                            self.supergroup_usernames.remove(&supergroup_id);
                        }
                    },
                    // Slice G2: restore the pre-toggle sign/show flags.
                    Some(RequestRollback::SignMessages {
                        supergroup_id,
                        previous_sign,
                        previous_show,
                    }) => {
                        match previous_sign {
                            Some(flag) => {
                                self.supergroup_sign_messages.insert(supergroup_id, flag);
                            }
                            None => {
                                self.supergroup_sign_messages.remove(&supergroup_id);
                            }
                        }
                        match previous_show {
                            Some(flag) => {
                                self.supergroup_show_message_sender
                                    .insert(supergroup_id, flag);
                            }
                            None => {
                                self.supergroup_show_message_sender.remove(&supergroup_id);
                            }
                        }
                    }
                    // Slice G2: restore the pre-toggle anti-spam flag.
                    Some(RequestRollback::AntiSpam {
                        supergroup_id,
                        previous,
                    }) => match previous {
                        Some(flag) => {
                            self.supergroup_anti_spam_enabled
                                .insert(supergroup_id, flag);
                        }
                        None => {
                            self.supergroup_anti_spam_enabled.remove(&supergroup_id);
                        }
                    },
                    // Slice CL1: restore the pre-toggle pinned /
                    // marked-as-unread flags the server refused.
                    Some(RequestRollback::ChatPin { previous, archived }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                            && let Some(chat) = self.chats.get_mut(&chat_id.0)
                        {
                            if archived {
                                chat.archive_is_pinned = previous;
                            } else {
                                chat.is_pinned = previous;
                            }
                        }
                    }
                    Some(RequestRollback::ChatMarkedAsUnread { previous }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                            && let Some(chat) = self.chats.get_mut(&chat_id.0)
                        {
                            chat.is_marked_as_unread = previous;
                        }
                    }
                    // Slice CL2: restore the pre-reorder `order` values
                    // the server refused, then rebuild the list order.
                    Some(RequestRollback::ChatPinOrder { previous, archived }) => {
                        for (chat_id, order) in previous {
                            if let Some(chat) = self.chats.get_mut(&chat_id) {
                                if archived {
                                    chat.archive_order = order;
                                } else {
                                    chat.order = order;
                                }
                            }
                        }
                        self.rebuild_main_order();
                    }
                    // Slice CL2: drop the refused archive-settings flip;
                    // the panel re-fetches the truth on next open.
                    Some(RequestRollback::ArchiveChatListSettings { previous }) => {
                        self.archive_chat_list_settings = previous;
                    }
                    None => {}
                }
                // Phase C1: a failed call request surfaces on the call
                // overlay (shown and cleared by the UI). A failed
                // `createCall` also drops the half-tracked outgoing call.
                match pending.map(|p| p.purpose) {
                    Some(RequestPurpose::CreateCall { .. }) => {
                        // Note: the tracked outgoing call is *not*
                        // cleared here — a failed `createCall` never
                        // produced a `callId`, so any tracked call came
                        // from elsewhere and must survive. The driver
                        // also refuses a second `createCall` while one
                        // is active.
                        self.call_error =
                            Some(call_request_error_line(&err, "Could not start the call"));
                    }
                    Some(RequestPurpose::AcceptCall) => {
                        self.call_error =
                            Some(call_request_error_line(&err, "Could not answer the call"));
                    }
                    Some(RequestPurpose::DiscardCall) => {
                        self.call_error =
                            Some(call_request_error_line(&err, "Could not hang up the call"));
                    }
                    Some(RequestPurpose::SendCallRating) => {
                        self.call_error =
                            Some(call_request_error_line(&err, "Could not send the rating"));
                    }
                    Some(RequestPurpose::SendCallDebugInformation) => {
                        if let Some(summary) = self.call_summary.as_mut() {
                            summary.debug_information_sent = false;
                            summary.debug_information_error = Some(call_request_error_line(
                                &err,
                                "Could not upload diagnostics",
                            ));
                        }
                    }
                    // Phase C2i: call history / privacy / log failures
                    // surface on the Recent-calls tab (the UI reads the
                    // flags), not the call overlay.
                    Some(RequestPurpose::SearchCallMessages) => {
                        self.recent_calls_loading = false;
                        self.recent_calls_error = true;
                    }
                    Some(RequestPurpose::GetCallPrivacyRules { .. }) => {
                        self.privacy_roundtrip_done();
                        self.call_privacy_error = true;
                    }
                    // Phase C2i: a failed `setUserPrivacySettingRules`
                    // clears the optimistic value (the next fetch
                    // restores the truth) and flags the error.
                    Some(RequestPurpose::SetCallPrivacyRules { setting }) => {
                        match setting {
                            CallPrivacySetting::AllowCalls => self.call_privacy_allow_calls = None,
                            CallPrivacySetting::PeerToPeer => self.call_privacy_p2p = None,
                        }
                        self.privacy_roundtrip_done();
                        self.call_privacy_error = true;
                    }
                    Some(RequestPurpose::SendCallLog) => {
                        if let Some(summary) = self.call_summary.as_mut() {
                            summary.log_sent = false;
                            summary.log_error = Some(call_request_error_line(
                                &err,
                                "Could not upload the call log",
                            ));
                        }
                    }
                    // Slice CL1: refused chat-list actions surface in the
                    // status note (the UI drains `chat_action_error`) —
                    // the optimistic state was already rolled back above.
                    // Only the numeric code is shown; TDLib's message is
                    // never stored.
                    Some(RequestPurpose::ToggleChatIsPinned) => {
                        self.chat_action_error =
                            Some(format!("could not pin the chat (error {})", err.code));
                    }
                    Some(RequestPurpose::ToggleChatIsMarkedAsUnread) => {
                        self.chat_action_error =
                            Some(format!("could not change read state (error {})", err.code));
                    }
                    Some(RequestPurpose::DeleteChatHistory) => {
                        self.chat_action_error =
                            Some(format!("could not clear history (error {})", err.code));
                    }
                    // Slice B2: refused `sendBotStartMessage` / `getBotSimilarBots` —
                    // a refusal is never shown as success.
                    Some(RequestPurpose::SendBotStartMessage) => {
                        self.chat_action_error =
                            Some(format!("could not start the bot (error {})", err.code));
                    }
                    Some(RequestPurpose::GetBotSimilarBots) => {
                        self.chat_action_error =
                            Some(format!("could not load similar bots (error {})", err.code));
                    }
                    Some(RequestPurpose::RemoveChatFromList) => {
                        self.chat_action_error =
                            Some(format!("could not delete the chat (error {})", err.code));
                    }
                    // Slice CL2: refused chat-list actions surface in the
                    // status note like the CL1 ones — a refusal is never
                    // shown as success.
                    Some(RequestPurpose::SetPinnedChats) => {
                        self.chat_action_error = Some(format!(
                            "could not reorder pinned chats (error {})",
                            err.code
                        ));
                    }
                    Some(RequestPurpose::ReadChatList) => {
                        self.chat_action_error = Some(format!(
                            "could not mark all chats as read (error {})",
                            err.code
                        ));
                    }
                    Some(RequestPurpose::ClearRecentlyFoundChats) => {
                        self.chat_action_error = Some(format!(
                            "could not clear recent searches (error {})",
                            err.code
                        ));
                    }
                    // Slice CL3: refused report / block surfaces in the
                    // status note — never shown as success.
                    Some(RequestPurpose::ReportChat) => {
                        self.chat_action_error =
                            Some(format!("could not report the chat (error {})", err.code));
                    }
                    Some(RequestPurpose::SetMessageSenderBlockList { .. }) => {
                        self.chat_action_error = Some(format!(
                            "could not change the block state (error {})",
                            err.code
                        ));
                    }
                    // Slice A6: contacts mutations — the notice surfaces
                    // in the contacts settings section.
                    Some(RequestPurpose::RemoveContact) => {
                        let what = if pending.and_then(|p| p.user_id).is_some() {
                            "the contact"
                        } else {
                            "synced contacts"
                        };
                        self.contacts_notice =
                            Some(format!("could not delete {what} (error {})", err.code));
                    }
                    Some(RequestPurpose::ImportContacts) => {
                        self.contacts_notice =
                            Some(format!("could not import contacts (error {})", err.code));
                    }
                    Some(RequestPurpose::ClearImportedContacts) => {
                        self.contacts_notice = Some(format!(
                            "could not delete synced contacts (error {})",
                            err.code
                        ));
                    }
                    Some(RequestPurpose::GetArchiveChatListSettings) => {
                        self.archive_settings_loading = false;
                        self.chat_action_error = Some(format!(
                            "could not load archive settings (error {})",
                            err.code
                        ));
                    }
                    Some(RequestPurpose::SetArchiveChatListSettings) => {
                        self.chat_action_error = Some(format!(
                            "could not save archive settings (error {})",
                            err.code
                        ));
                    }
                    Some(RequestPurpose::CreatePrivateChat) => {
                        self.chat_action_error = Some(format!(
                            "could not open Saved Messages (error {})",
                            err.code
                        ));
                    }
                    // Phase C3a: group-call request failures surface on
                    // the group-call overlay (shown and cleared by the
                    // UI). A failed `joinVideoChat` leaves any tracked
                    // call in place — `updateGroupCall` is the source
                    // of truth for join state.
                    Some(RequestPurpose::CreateVideoChat { .. }) => {
                        self.group_call_error = Some(call_request_error_line(
                            &err,
                            "Could not start the voice chat",
                        ));
                    }
                    Some(RequestPurpose::JoinVideoChat { .. }) => {
                        // Phase C2f: a failed rejoin re-arms
                        // `reconnecting` so the driver's auto-rejoin
                        // retries (max 3 attempts, the C2d discipline);
                        // a plain initial-join failure just reports.
                        // `rejoin_attempts > 0` marks the failed join
                        // as a rejoin (only `rejoin_group_call`
                        // increments the counter).
                        let rejoin_attempt = self
                            .active_group_call
                            .as_ref()
                            .map(|call| call.rejoin_attempts)
                            .unwrap_or(0);
                        if rejoin_attempt > 0 {
                            if let Some(call) = self.active_group_call.as_mut() {
                                // Keep the banner + manual Rejoin
                                // available even after exhaustion.
                                call.reconnecting = true;
                                if call.rejoin_attempts >= 3 {
                                    self.group_call_error =
                                        Some("Reconnect attempts exhausted.".to_string());
                                }
                            }
                        } else {
                            self.group_call_error = Some(call_request_error_line(
                                &err,
                                "Could not join the voice chat",
                            ));
                        }
                    }
                    // Phase C2g: a failed screen-sharing handshake must
                    // not leave the call stuck "sharing" — clear the
                    // pending/active flags and surface an honest error.
                    Some(RequestPurpose::StartGroupCallScreenSharing { group_call_id }) => {
                        if let Some(tracked) = self.active_group_call.as_mut()
                            && tracked.id == group_call_id
                        {
                            tracked.screen_share_pending = false;
                            tracked.screen_sharing = false;
                            tracked.screen_share_answer.clear();
                        }
                        self.group_call_error = Some(call_request_error_line(
                            &err,
                            "Could not start screen sharing",
                        ));
                    }
                    Some(RequestPurpose::EndGroupCallScreenSharing { group_call_id }) => {
                        if let Some(tracked) = self.active_group_call.as_mut()
                            && tracked.id == group_call_id
                        {
                            tracked.screen_share_pending = false;
                            tracked.screen_sharing = false;
                            tracked.screen_share_answer.clear();
                        }
                        self.group_call_error = Some(call_request_error_line(
                            &err,
                            "Could not stop screen sharing",
                        ));
                    }
                    Some(
                        RequestPurpose::LeaveGroupCall { .. }
                        | RequestPurpose::EndGroupCall { .. }
                        | RequestPurpose::GetGroupCall { .. }
                        | RequestPurpose::LoadGroupCallParticipants { .. }
                        | RequestPurpose::GetVideoChatInviteLink { .. }
                        | RequestPurpose::SetVideoChatTitle { .. }
                        | RequestPurpose::RevokeVideoChatInviteLink { .. }
                        | RequestPurpose::StartGroupCallRecording { .. }
                        | RequestPurpose::EndGroupCallRecording { .. }
                        | RequestPurpose::StartScheduledVideoChat { .. }
                        | RequestPurpose::ToggleVideoChatEnabledStartNotification { .. }
                        | RequestPurpose::GetVideoChatRtmpUrl { .. }
                        | RequestPurpose::ReplaceVideoChatRtmpUrl { .. }
                        | RequestPurpose::SendGroupCallMessage { .. }
                        | RequestPurpose::ToggleGroupCallAreMessagesAllowed { .. }
                        | RequestPurpose::ToggleGroupCallVideo { .. }
                        | RequestPurpose::ToggleGroupCallParticipantMute { .. }
                        | RequestPurpose::ToggleGroupCallParticipantHand { .. }
                        | RequestPurpose::ToggleVideoChatMuteNew { .. }
                        | RequestPurpose::InviteGroupCallParticipant { .. }
                        | RequestPurpose::BanGroupCallParticipants { .. }
                        | RequestPurpose::SetGroupCallParticipantVolumeLevel { .. }
                        | RequestPurpose::JoinGroupCallInvitation
                        | RequestPurpose::DeclineGroupCallInvitation { .. },
                    ) => {
                        self.group_call_error =
                            Some(call_request_error_line(&err, "Voice chat request failed"));
                    }
                    // Phase D2: a failed `getChatStatistics` lands in the
                    // fetch state so the statistics panel shows an honest
                    // error instead of spinning forever.
                    Some(RequestPurpose::GetChatStatistics) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.chat_statistics.insert(
                                chat_id.0,
                                ChatStatisticsFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load statistics",
                                )),
                            );
                        }
                    }
                    // Phase D3a: failed invite-link / join-request requests
                    // land in the fetch state so the panel shows an honest
                    // error instead of spinning forever.
                    Some(RequestPurpose::GetChatInviteLinks) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.invite_links.insert(
                                chat_id.0,
                                InviteLinkFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load invite links",
                                )),
                            );
                        }
                    }
                    Some(RequestPurpose::CreateChatInviteLink) => {
                        // Slice G1 fix-up: a failed mutation must not wipe
                        // the previously loaded list — surface the error in
                        // the status note and keep the last good data.
                        self.invite_link_error = Some(call_request_error_line(
                            &err,
                            "Could not create invite link",
                        ));
                    }
                    Some(RequestPurpose::EditChatInviteLink) => {
                        self.invite_link_error =
                            Some(call_request_error_line(&err, "Could not edit invite link"));
                    }
                    Some(RequestPurpose::RevokeChatInviteLink) => {
                        self.invite_link_error = Some(call_request_error_line(
                            &err,
                            "Could not revoke invite link",
                        ));
                    }
                    // Slice G1: failed primary-link replacement — keep the
                    // last good list, surface the error in the note.
                    Some(RequestPurpose::ReplacePrimaryChatInviteLink) => {
                        self.invite_link_error = Some(call_request_error_line(
                            &err,
                            "Could not replace primary invite link",
                        ));
                    }
                    // Slice G1: roll back the optimistic broadcast-group
                    // upgrade so the panel doesn't lie.
                    Some(RequestPurpose::ToggleBroadcastGroup) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                            && let Some(chat) = self.chats.get(&chat_id.0)
                            && let ChatKind::Supergroup { supergroup_id, .. } = chat.kind
                        {
                            self.supergroup_is_broadcast.remove(&supergroup_id);
                        }
                    }
                    Some(RequestPurpose::GetChatJoinRequests) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.join_requests.insert(
                                chat_id.0,
                                JoinRequestFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load join requests",
                                )),
                            );
                        }
                    }
                    Some(RequestPurpose::ProcessChatJoinRequest { .. }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.join_requests.insert(
                                chat_id.0,
                                JoinRequestFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not process join request",
                                )),
                            );
                        }
                    }
                    // Phase D3b: failed admin-management requests land in
                    // the fetch state so the panel shows an honest error
                    // instead of spinning forever.
                    Some(RequestPurpose::GetChatAdministrators) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.admin_lists.insert(
                                chat_id.0,
                                AdminListFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load administrators",
                                )),
                            );
                        }
                    }
                    Some(RequestPurpose::SetChatMemberStatus { .. }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            let line =
                                call_request_error_line(&err, "Could not update member status");
                            // Slice G1: the member-management dialog reads
                            // the member-list fetch states, not
                            // `admin_lists`, so the failure is also parked
                            // where the action was taken.
                            self.member_action_error.insert(chat_id.0, line.clone());
                            self.admin_lists
                                .insert(chat_id.0, AdminListFetch::Failed(line));
                        }
                    }
                    // Slice G1: failed `setChatMemberTag` (custom title)
                    // surfaces as an admin-list error so the info panel
                    // shows it, and in `member_action_error` so the
                    // member-management dialog (which launched the
                    // action) shows it too.
                    Some(RequestPurpose::SetChatMemberTag { .. }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            let line = call_request_error_line(&err, "Could not set custom title");
                            self.member_action_error.insert(chat_id.0, line.clone());
                            self.admin_lists
                                .insert(chat_id.0, AdminListFetch::Failed(line));
                        }
                    }
                    Some(RequestPurpose::GetBasicGroupFullInfo) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.basic_group_members.insert(
                                chat_id.0,
                                SupergroupMembersFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load members",
                                )),
                            );
                        }
                    }
                    // Slice G1: a basic-group `addChatMember` answers per
                    // user with `failedToAddMembers` (schema 1.8.67, line
                    // 13578) — but a request-level TDLib error has no
                    // such body. Count per-user errors in the same slot
                    // the dialog already renders so partial adds stay
                    // honest.
                    Some(RequestPurpose::AddChatMembers | RequestPurpose::AddChatMember) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            *self.add_members_failed.entry(chat_id.0).or_insert(0) += 1;
                        }
                    }
                    Some(RequestPurpose::GetSupergroupMembers { filter }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.supergroup_members.insert(
                                (chat_id.0, filter),
                                SupergroupMembersFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load members",
                                )),
                            );
                        }
                    }
                    // B4: a failed `getPollVoters` first page lands in the
                    // fetch state so the dialog shows an honest error; a
                    // failed "load more" keeps the loaded page retryable.
                    Some(RequestPurpose::GetPollVoters {
                        chat_id,
                        message_id,
                        option_id,
                        offset,
                    }) => {
                        if offset == 0
                            || !matches!(
                                self.poll_voters.get(&(chat_id.0, message_id.0, option_id)),
                                Some(PollVotersFetch::Loaded { .. })
                            )
                        {
                            self.poll_voters.insert(
                                (chat_id.0, message_id.0, option_id),
                                PollVotersFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load voters",
                                )),
                            );
                        }
                    }
                    // Bots slice: a failed first page lands in the slot so
                    // the picker shows an honest error; a failed "load
                    // more" keeps the loaded page retryable.
                    Some(RequestPurpose::GetInlineQueryResults {
                        chat_id,
                        bot_user_id,
                        first_page,
                    }) => {
                        let failed_first_page = first_page
                            && matches!(
                                &self.inline_query,
                                Some(slot)
                                    if slot.chat_id == chat_id
                                        && slot.bot_user_id == bot_user_id
                            );
                        if failed_first_page && let Some(slot) = self.inline_query.as_mut() {
                            slot.fetch = InlineQueryFetch::Failed(call_request_error_line(
                                &err,
                                "Could not load inline results",
                            ));
                        }
                    }
                    // Phase D3c: a failed first page lands in the fetch
                    // state so the panel shows an honest error instead of
                    // spinning forever. A failed "load more" keeps the
                    // already-loaded page so the button stays retryable.
                    Some(RequestPurpose::GetChatEventLog { from_event_id }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                            && (from_event_id == 0
                                || !matches!(
                                    self.event_logs.get(&chat_id.0),
                                    Some(ChatEventLogFetch::Loaded(_))
                                ))
                        {
                            self.event_logs.insert(
                                chat_id.0,
                                ChatEventLogFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load recent actions",
                                )),
                            );
                        }
                    }
                    Some(RequestPurpose::GetAdminRights { user_id }) => {
                        if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                            self.admin_rights.insert(
                                (chat_id.0, user_id),
                                AdminRightsFetch::Failed(call_request_error_line(
                                    &err,
                                    "Could not load admin rights",
                                )),
                            );
                        }
                    }
                    // Phase S2: a failed `getStorageStatistics` clears the
                    // in-flight flag so the overlay shows "No storage data
                    // yet." instead of spinning forever.
                    Some(RequestPurpose::GetStorageStatistics) => {
                        self.storage_stats_loading = false;
                    }
                    // Slice A2: a failed 2FA management request clears the
                    // in-flight flag and parks the honest, classified
                    // error line on the overlay — never a fake success,
                    // never an optimistic state change.
                    Some(RequestPurpose::PasswordStateOp { op }) => {
                        self.password_state_loading = false;
                        self.password_op_error = Some(password_op_error_line(op, &err));
                    }
                    // Slice A3: a failed sessions fetch or terminate
                    // clears the in-flight flags and parks the honest,
                    // classified error line on the overlay — never a fake
                    // success, never an optimistic list change.
                    // A failed stale-refetch also clears `sessions_stale`
                    // so the next ingest does not retry the fetch and
                    // flood state worsens; retry is user-driven via the
                    // Refresh button. The old cache stays visible.
                    Some(RequestPurpose::GetActiveSessions) => {
                        self.sessions_loading = false;
                        self.sessions_stale = false;
                        self.sessions_error =
                            Some(sessions_error_line("load the sessions list", &err));
                    }
                    Some(
                        RequestPurpose::TerminateSession { .. }
                        | RequestPurpose::TerminateAllOtherSessions,
                    ) => {
                        self.sessions_mutating = false;
                        self.sessions_error =
                            Some(sessions_error_line("terminate the session", &err));
                    }
                    // Slice A4: a refused session toggle surfaces an honest
                    // classified error and leaves the list untouched (the
                    // toggled value is never applied optimistically).
                    Some(
                        RequestPurpose::ToggleSessionSecretChats { .. }
                        | RequestPurpose::ToggleSessionCalls { .. },
                    ) => {
                        self.sessions_mutating = false;
                        self.sessions_error =
                            Some(sessions_error_line("change the session setting", &err));
                    }
                    // Slice A4: a failed websites fetch or disconnect
                    // clears the in-flight flags and parks the honest,
                    // classified error line on the overlay.
                    // A failed stale-refetch also clears `websites_stale`
                    // (mirroring A3's sessions arm): otherwise the next
                    // ingest retries the fetch and flood state worsens;
                    // retry is user-driven via the Refresh button. The old
                    // cache stays visible.
                    Some(RequestPurpose::GetConnectedWebsites) => {
                        self.connected_websites_loading = false;
                        self.websites_stale = false;
                        self.websites_error =
                            Some(sessions_error_line("load the websites list", &err));
                    }
                    Some(
                        RequestPurpose::DisconnectWebsite { .. }
                        | RequestPurpose::DisconnectAllWebsites,
                    ) => {
                        self.websites_mutating = false;
                        self.websites_error =
                            Some(sessions_error_line("disconnect the website", &err));
                    }
                    // M1 fix-up: a failed `resendMessages` surfaces in the
                    // status note instead of vanishing into `_ => {}` —
                    // the menu item says "retrying send…" and the user
                    // deserves an answer either way.
                    Some(RequestPurpose::ResendMessages) => {
                        self.resend_error =
                            Some(call_request_error_line(&err, "Could not retry the send"));
                    }
                    // M1 fix-up: a failed "Share link" surfaces in the
                    // status note instead of silently doing nothing.
                    Some(
                        RequestPurpose::GetMessageLink
                        | RequestPurpose::GetMessageLinkProperties { .. },
                    ) => {
                        self.message_link_error =
                            Some(call_request_error_line(&err, "Could not get message link"));
                    }
                    // MED2 fix-up: a refused `recognizeSpeech` surfaces in
                    // the status note instead of vanishing into `_ => {}` —
                    // the row says "transcription requested" and the user
                    // deserves an answer either way.
                    Some(RequestPurpose::RecognizeSpeech) => {
                        self.recognize_speech_error = Some(call_request_error_line(
                            &err,
                            "Could not transcribe this message",
                        ));
                    }
                    _ => {}
                }
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
                // Slice media-shared-gallery: failed gallery-tab fetch — the
                // tab shows the failed state with Retry, never the spinner
                // or the empty state.
                if let Some(RequestPurpose::GetSharedMedia { tab, generation }) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.shared_media.fail(
                        chat_id,
                        tab,
                        generation,
                        call_request_error_line(&err, "Could not load shared media"),
                    );
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
                // Slice G2: failed welcome-message pack fetch — mark it so
                // the dialog shows an error, not a spinner.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChatWelcomeMessages)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.welcome_message_fetches.insert(
                        chat_id.0,
                        WelcomeMessagesFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load welcome messages",
                        )),
                    );
                }
                // Slice G2: failed thread-history fetch — mark the comment
                // viewer so it shows an error.
                if let Some(RequestPurpose::GetMessageThreadHistory { message_id }) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.comment_thread = Some(CommentThreadFetch {
                        chat_id,
                        message_id: MessageId(message_id),
                        messages: Vec::new(),
                        failed: Some(call_request_error_line(&err, "Could not load comments")),
                    });
                }
                // Slice CL: failed preview-history fetch — mark the peek
                // preview so it shows an error instead of a spinner.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatPreview)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chat_preview_fetch = Some(PreviewHistoryFetch {
                        chat_id,
                        messages: Vec::new(),
                        failed: Some(call_request_error_line(&err, "Could not load preview")),
                    });
                }
                // Slice G2: the slots half of a boost failed — the chain
                // cannot continue; drop the intent.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBoostSlotsForBoost) {
                    self.boost_intent = None;
                }
                // Slice G2: `boostChat` failed — the status is refetched on
                // success only, so nothing to roll back.
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
                // B1: a refused password-protected callback surfaces
                // honestly — 400 is the wrong-password case, anything else
                // is the generic bot-timeout note.
                if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::GetCallbackQueryAnswerWithPassword)
                {
                    let text = if err.code == 400 {
                        "wrong 2-step verification password"
                    } else {
                        "bot did not answer"
                    };
                    self.last_callback_answer = Some(CallbackQueryAnswer {
                        text: text.to_string(),
                        show_alert: false,
                        url: String::new(),
                    });
                }
                // B1: a refused `getLoginUrlInfo` / `getLoginUrl` degrades
                // the login button to an ordinary URL button (schema 1.8.67
                // doc on `getLoginUrl`).
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::GetLoginUrlInfo) | Some(RequestPurpose::GetLoginUrl)
                ) {
                    let fallback_url = self
                        .login_url_request
                        .take()
                        .map(|request| request.raw_url)
                        .unwrap_or_default();
                    self.last_login_url_info = Some(LoginUrlInfo::Failed { fallback_url });
                }
                let download_id = pending
                    .filter(|p| p.purpose == RequestPurpose::DownloadFile)
                    .and_then(|p| p.file_id)
                    .or_else(|| extra.and_then(|id| self.download_extras.get(&id.0).copied()));
                if let Some(file_id) = download_id {
                    // MED3 review: only user-initiated downloads enter the
                    // Failed section; automatic downloads never started by
                    // the user must not show rows here.
                    if self.user_downloads.contains(&file_id) {
                        self.failed_downloads.insert(file_id);
                    }
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
        if file.local.is_downloading_completed {
            self.failed_downloads.remove(&file.id.0);
            if self.user_downloads.contains(&file.id.0) {
                // A user-initiated download that finished: remember for the
                // downloads manager's recent list (deduped, capped).
                self.completed_downloads.retain(|id| *id != file.id.0);
                self.completed_downloads.push_back(file.id.0);
                while self.completed_downloads.len() > 50 {
                    self.completed_downloads.pop_front();
                }
            }
        }
        if from_file_update && idle_incomplete && self.user_downloads.contains(&file.id.0) {
            // MED3: a user-initiated download that went active → idle without
            // completing stalled (or errored without failing the request) —
            // surface it as failed so the row offers Retry. Explicit cancels
            // are excluded: `abort_download` already dropped them from
            // `user_downloads`.
            self.failed_downloads.insert(file.id.0);
        }
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
        self.user_downloads.remove(&file_id);
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
            self.failed_downloads.remove(&file_id.0);
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

    /// MED3: full media files in the open chat eligible for automatic
    /// download under the user's per-chat-kind × media-type prefs (TGX
    /// `settings_autodownload`). Unlike the thumbnail pass, each media type
    /// is gated on its own flag; secret and spoiler content is never
    /// auto-downloaded (same safeguard as the thumb hook), and files known
    /// to exceed `AUTO_DOWNLOAD_MAX_BYTES` are skipped (TGX
    /// `canAutomaticallyDownload` download limit).
    pub fn auto_download_media_file_ids(&self) -> Vec<FileId> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        if self.media_prefs.data_saver {
            return Vec::new();
        }
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        let push = |flag: u8, file_id: FileId, ids: &mut Vec<FileId>| {
            // MED3 review: skip files known to exceed the auto-download cap
            // (TGX `canAutomaticallyDownload` download limit, WiFi default
            // 50 MiB); an unknown size (`display_size() == 0`) is not a
            // reason to block.
            let oversized = self
                .files
                .get(&file_id.0)
                .is_some_and(|file| file.display_size() > AUTO_DOWNLOAD_MAX_BYTES);
            if !oversized
                && self.auto_download_allowed(chat_id, flag)
                && self.should_download(file_id)
            {
                ids.push(file_id);
            }
        };
        for message in history.messages.values() {
            match &message.content {
                MessageContent::Photo(photo) => {
                    if photo.is_secret || photo.has_spoiler {
                        continue;
                    }
                    if let Some(size) = photo.largest_size() {
                        push(AUTO_DOWNLOAD_PHOTO, size.file_id, &mut ids);
                    }
                }
                MessageContent::Document(doc) => {
                    push(AUTO_DOWNLOAD_FILE, doc.file_id, &mut ids);
                }
                MessageContent::Animation(animation) => {
                    if animation.is_secret || animation.has_spoiler {
                        continue;
                    }
                    push(AUTO_DOWNLOAD_GIF, animation.file_id, &mut ids);
                }
                MessageContent::Video(video) => {
                    if video.is_secret || video.has_spoiler {
                        continue;
                    }
                    push(AUTO_DOWNLOAD_VIDEO, video.file_id, &mut ids);
                }
                MessageContent::VideoNote(note) => {
                    if note.is_secret {
                        continue;
                    }
                    push(AUTO_DOWNLOAD_VIDEO_NOTE, note.file_id, &mut ids);
                }
                MessageContent::VoiceNote(voice) => {
                    push(AUTO_DOWNLOAD_VOICE, voice.file_id, &mut ids);
                }
                MessageContent::Audio(audio) => {
                    push(AUTO_DOWNLOAD_MUSIC, audio.file_id, &mut ids);
                }
                _ => {}
            }
        }
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }

    /// MED3: auto-download gate for automatic (non-user-initiated) downloads
    /// in a chat. `flag` is one of the `settings::AUTO_DOWNLOAD_*` media-type
    /// bits. Data saver pauses every automatic download (TGX
    /// `settings_datasaver`); otherwise the chat kind selects the per-kind
    /// bitfield (TGX `settings_autodownload` private/group/channel shifts).
    /// Secret chats use the private bucket.
    pub fn auto_download_allowed(&self, chat_id: ChatId, flag: u8) -> bool {
        if self.media_prefs.data_saver {
            return false;
        }
        let bits = match self.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => self.media_prefs.auto_download_channels,
            Some(ChatKind::BasicGroup { .. }) | Some(ChatKind::Supergroup { .. }) => {
                self.media_prefs.auto_download_groups
            }
            _ => self.media_prefs.auto_download_private,
        };
        bits & flag != 0
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

    /// Slice S8: store a `trendingStickerSets` page. Single-page replace
    /// semantics: a paged second call overwrites page one. Append-before-
    /// needed is speculative — the tab UI will own paging when it lands.
    pub fn accept_trending_sticker_sets(&mut self, sets: Vec<StickerSetInfo>, is_premium: bool) {
        self.stickers.trending = sets;
        self.stickers.trending_is_premium = is_premium;
    }

    /// Slice S8: store a `getFavoriteStickers` answer.
    pub fn accept_favorite_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.favorites = stickers;
    }

    /// Slice S8: store a `getRecentStickers` answer.
    pub fn accept_recent_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.recent = stickers;
    }

    /// Slice S8: store a `searchStickerSets` answer.
    pub fn accept_found_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.found_sets = sets;
    }

    /// Slice S8: store a `searchStickers` answer.
    pub fn accept_found_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.found_stickers = stickers;
    }

    /// Slice S8: a sticker-set mutation (`changeStickerSet` /
    /// `reorderInstalledStickerSets`) succeeded — drop the installed-sets
    /// cache so the panel refetches the authoritative list instead of
    /// showing a stale order.
    pub fn invalidate_installed_sticker_sets(&mut self) {
        self.stickers.sets.clear();
        self.stickers.selected_set_id = None;
        self.stickers.loaded_set_id = None;
        self.stickers.stickers.clear();
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

    /// Slice S9: store a GIF-search `inlineQueryResults` page. A first page
    /// replaces; a later page appends, deduped by animation file id, and
    /// keeps the page's `next_offset` — mirrors the composer's inline-query
    /// paging (Loop 3) without its slot machinery, since the purpose carries
    /// `first_page`.
    pub fn accept_gif_search_results(
        &mut self,
        animations: Vec<AnimationItem>,
        next_offset: String,
        first_page: bool,
    ) {
        if first_page {
            self.gifs.search_results = animations;
        } else {
            for item in animations {
                if !self
                    .gifs
                    .search_results
                    .iter()
                    .any(|r| r.file_id == item.file_id)
                {
                    self.gifs.search_results.push(item);
                }
            }
        }
        self.gifs.search_next_offset = next_offset;
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
            chat.set_admin_can_invite_users(member.admin_can_invite_users);
            chat.set_admin_can_promote_members(member.admin_rights.map(|r| r.can_promote_members));
            chat.set_admin_can_restrict_members(
                member.admin_rights.map(|r| r.can_restrict_members),
            );
            chat.set_admin_can_pin_messages(member.admin_rights.map(|r| r.can_pin_messages));
            // Slice G2: sign-messages + welcome-message rights for the
            // channel path.
            chat.set_admin_can_change_info(member.admin_rights.map(|r| r.can_change_info));
            chat.set_admin_can_send_welcome_messages(
                member.admin_rights.map(|r| r.can_send_welcome_messages),
            );
        }
    }

    /// Phase B1: record a secret chat (`updateSecretChat` or a
    /// `getSecretChat` answer), fanning the state out to the chat
    /// summary when the chat is already known. `updateSecretChat` is
    /// guaranteed to arrive *before* the chat identifier is returned
    /// (schema 1.8.67, line 10740), hence the session-level map that
    /// `updateNewChat` hydrates from; a satisfied fetch leaves
    /// `secret_chat_fetch_queue`.
    fn accept_secret_chat(&mut self, secret_chat: &ParsedSecretChat) {
        let state = secret_chat.state.clone();
        self.secret_chat_states
            .insert(secret_chat.id, secret_chat.clone());
        self.secret_chat_fetch_queue
            .retain(|id| *id != secret_chat.id);
        for chat in self.chats.values_mut() {
            if let ChatKind::Secret { secret_chat_id, .. } = &chat.kind
                && *secret_chat_id == secret_chat.id
            {
                chat.secret_state = Some(state.clone());
            }
        }
    }

    /// Phase C1: `updateCall` state machine. Quill tracks at most one
    /// call at a time (TDLib / official clients allow a single active
    /// 1:1 call):
    /// - same call id → advance the state; terminal states
    ///   (`callStateDiscarded` / `callStateError`) end the call and
    ///   record a summary for the end screen / rating card;
    /// - a *different*, non-terminal, incoming `Pending` call while one
    ///   is active → queued in `call_busy_decline_queue` for the driver
    ///   to `discardCall` (busy);
    /// - a terminal update for an untracked call id (e.g. a missed call
    ///   we never saw pending) still records a summary so the end
    ///   screen can show "Missed call".
    fn accept_call_update(&mut self, call: &ParsedCall) {
        if let Some(active) = self.active_call.as_mut()
            && active.id == call.id
        {
            if call.state.is_terminal() {
                self.end_active_call(call);
            } else {
                if matches!(call.state, CallState::Ready) && active.ready_at.is_none() {
                    active.ready_at = Some(Instant::now());
                }
                if matches!(call.state, CallState::Ready) {
                    active.ready = call.ready.clone();
                }
                active.state = call.state.clone();
                active.is_video = call.is_video;
            }
            return;
        }
        if self.active_call.is_some() {
            if !call.is_outgoing
                && matches!(call.state, CallState::Pending { .. })
                && !self
                    .call_busy_decline_queue
                    .iter()
                    .any(|(id, _, _)| *id == call.id)
            {
                self.call_busy_decline_queue
                    .push((call.id, call.user_id, call.is_video));
                self.diagnostics.record(Diagnostic {
                    category: "call",
                    type_name: Some("updateCall".to_string()),
                    extra: None,
                    seq: Some(self.last_seq),
                    note: "incoming-while-active-busy-decline",
                });
            }
            return;
        }
        if call.state.is_terminal() {
            self.call_summary = Some(CallSummary::from_terminal(call, 0, false));
            return;
        }
        self.active_call = Some(ActiveCall {
            id: call.id,
            user_id: call.user_id,
            is_outgoing: call.is_outgoing,
            is_video: call.is_video,
            state: call.state.clone(),
            started_at: Instant::now(),
            ready_at: None,
            ready: call.ready.clone(),
            transport: None,
            transport_error: None,
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: call.is_video,
            screen_sharing: false,
            remote_video: RemoteVideoState::Inactive,
        });
        self.call_summary = None;
        self.call_error = None;
    }

    /// Phase C2b: `updateNewCallSignalingData`. The driver also feeds these
    /// bytes into the engine; this bounded queue remains the honest
    /// diagnostic record. Data for an unknown call id is dropped (never
    /// buffered without a tracked call).
    fn accept_call_signaling_data(&mut self, call_id: i32, data: Vec<u8>) {
        let Some(active) = self.active_call.as_mut() else {
            return;
        };
        if active.id != call_id {
            return;
        }
        if active.signaling_queue.len() >= MAX_QUEUED_SIGNALING_CHUNKS {
            active.signaling_dropped += 1;
            self.diagnostics.record(Diagnostic {
                category: "call",
                type_name: Some("updateNewCallSignalingData".to_string()),
                extra: None,
                seq: Some(self.last_seq),
                note: "signaling-queue-overflow-dropped",
            });
            return;
        }
        active.signaling_queue.push(data);
    }

    /// Phase C1: end the tracked call on a terminal `updateCall` and
    /// record the summary shown on the call-end screen. Any queued
    /// signaling diagnostic data is dropped with the call.
    fn end_active_call(&mut self, call: &ParsedCall) {
        let Some(active) = self.active_call.take() else {
            return;
        };
        let duration_secs = active
            .ready_at
            .map(|t| t.elapsed().as_secs() as i64)
            .unwrap_or(0);
        let mut summary = CallSummary::from_terminal(
            call,
            duration_secs,
            active.transport == Some(TransportState::Connected),
        );
        summary.final_transport = active.transport;
        summary.muted = active.muted;
        self.call_summary = Some(summary);
        self.call_busy_decline_queue
            .retain(|(id, _, _)| *id != call.id);
    }

    /// Phase C3a: `updateGroupCall` state machine. Quill tracks at most
    /// one group call at a time (like the single 1:1 call):
    /// - a different call id replaces the tracked call (participants
    ///   reload via updates);
    /// - `need_rejoin` sets the `reconnecting` flag so the UI shows
    ///   "Reconnecting…" and the driver re-issues the join;
    /// - a fresh joined, non-`need_rejoin` update clears `reconnecting`;
    /// - `!is_active` (ended) drops the tracked call.
    ///
    /// Everything here is signaling — `join_payload` is stored, never
    /// consumed (no media transport until Phase C2).
    fn accept_group_call_update(&mut self, group_call: &ParsedGroupCall) {
        if !group_call.is_active {
            // Phase C2h: a scheduled (not yet started) video chat is
            // still tracked — the overlay shows "starts in …" plus an
            // admin-only "Start now" (startScheduledVideoChat,
            // schema/td_api.tl:14277). Join appears once TDLib
            // activates the call.
            if group_call.scheduled_start_date > 0 {
                let tracked = self
                    .active_group_call
                    .get_or_insert_with(|| ActiveGroupCall::fresh(group_call.id));
                if tracked.id != group_call.id {
                    *tracked = ActiveGroupCall::fresh(group_call.id);
                }
                let tracked = self.active_group_call.as_mut().expect("just inserted");
                tracked.title = group_call.title.clone();
                tracked.can_be_managed = group_call.can_be_managed;
                tracked.is_owned = group_call.is_owned;
                tracked.is_video_chat = group_call.is_video_chat;
                tracked.scheduled_start_date = group_call.scheduled_start_date;
                tracked.enabled_start_notification = group_call.enabled_start_notification;
                return;
            }
            if self
                .active_group_call
                .as_ref()
                .is_some_and(|c| c.id == group_call.id)
            {
                self.active_group_call = None;
                self.diagnostics.record(Diagnostic {
                    category: "group_call",
                    type_name: Some("updateGroupCall".to_string()),
                    extra: None,
                    seq: Some(self.last_seq),
                    note: "call-ended-tracked-cleared",
                });
            }
            return;
        }
        let tracked = self
            .active_group_call
            .get_or_insert_with(|| ActiveGroupCall::fresh(group_call.id));
        if tracked.id != group_call.id {
            // A different call took over the slot — reset participant
            // state; fresh updates repopulate it.
            *tracked = ActiveGroupCall::fresh(group_call.id);
        }
        let tracked = self.active_group_call.as_mut().expect("just inserted");
        tracked.title = group_call.title.clone();
        tracked.is_video_chat = group_call.is_video_chat;
        tracked.scheduled_start_date = 0;
        tracked.is_joined = group_call.is_joined;
        tracked.need_rejoin = group_call.need_rejoin;
        tracked.can_be_managed = group_call.can_be_managed;
        tracked.is_owned = group_call.is_owned;
        tracked.participant_count = group_call.participant_count;
        tracked.loaded_all_participants = group_call.loaded_all_participants;
        tracked.is_my_video_enabled = group_call.is_my_video_enabled;
        tracked.is_my_video_paused = group_call.is_my_video_paused;
        tracked.can_enable_video = group_call.can_enable_video;
        tracked.mute_new_participants = group_call.mute_new_participants;
        tracked.can_toggle_mute_new_participants = group_call.can_toggle_mute_new_participants;
        // Phase C2h: in-call chat flags + recording state drive the
        // management UI.
        tracked.can_send_messages = group_call.can_send_messages;
        tracked.are_messages_allowed = group_call.are_messages_allowed;
        tracked.can_toggle_are_messages_allowed = group_call.can_toggle_are_messages_allowed;
        tracked.can_delete_messages = group_call.can_delete_messages;
        tracked.record_duration = group_call.record_duration;
        tracked.is_video_recorded = group_call.is_video_recorded;
        tracked.recent_speaker_order = group_call
            .recent_speakers
            .iter()
            .map(|(sender, _)| *sender)
            .collect();
        if group_call.need_rejoin {
            tracked.reconnecting = true;
        } else if group_call.is_joined {
            tracked.reconnecting = false;
            // Phase C2f: a clean joined update means the rejoin
            // succeeded — the attempt counter starts over.
            tracked.rejoin_attempts = 0;
            // Phase C2f: the failure is resolved — a stale
            // group-call error line would lie now.
            self.group_call_error = None;
        }
        tracked.sort_participants();
    }

    /// Phase C3a: `updateGroupCallParticipant`. Upserts the participant;
    /// an empty `order` removes them (schema note on
    /// `groupCallParticipant.order`). Updates for an untracked call id
    /// are ignored.
    fn accept_group_call_participant_update(
        &mut self,
        group_call_id: i32,
        participant: &ParsedGroupCallParticipant,
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        if participant.order.is_empty() {
            tracked
                .participants
                .retain(|p| p.participant_id != participant.participant_id);
        } else if let Some(existing) = tracked
            .participants
            .iter_mut()
            .find(|p| p.participant_id == participant.participant_id)
        {
            *existing = participant.clone();
        } else {
            tracked.participants.push(participant.clone());
        }
        tracked.sort_participants();
    }

    /// Phase C3a: `updateGroupCallParticipants`. Drops user participants
    /// not in the reported id list. Chat senders (`MessageSender::Chat`)
    /// are kept — the update only carries user ids, so chat senders
    /// can't be verified against it.
    fn accept_group_call_participants_update(
        &mut self,
        group_call_id: i32,
        participant_user_ids: &[i64],
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        tracked.participants.retain(|p| match p.participant_id {
            MessageSender::User { user_id } => participant_user_ids.contains(&user_id),
            MessageSender::Chat { .. } => true,
        });
        tracked.sort_participants();
    }

    /// Phase C2h: `updateNewGroupCallMessage`. Appends to the
    /// in-call chat feed (dedup by message_id; capped — TDLib offers
    /// no history getter for group-call messages).
    fn accept_new_group_call_message(
        &mut self,
        group_call_id: i32,
        message: &ParsedGroupCallMessage,
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        if !tracked
            .messages
            .iter()
            .any(|m| m.message_id == message.message_id)
        {
            tracked.messages.push(message.clone());
            // ponytail: hard cap — no history API exists to backfill.
            if tracked.messages.len() > 200 {
                tracked.messages.remove(0);
            }
        }
    }

    /// Phase C2h: `updateGroupCallMessagesDeleted`. Drops the
    /// deleted ids from the in-call chat feed.
    fn accept_group_call_messages_deleted(&mut self, group_call_id: i32, message_ids: &[i32]) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        tracked
            .messages
            .retain(|m| !message_ids.contains(&m.message_id));
    }

    /// Phase C3a: `updateGroupCallVerificationState`. Stores the E2E
    /// emoji check for the tracked call; ignored on id mismatch.
    fn accept_group_call_verification_state(
        &mut self,
        group_call_id: i32,
        generation: i32,
        emojis: &[String],
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        tracked.verification = Some(GroupCallVerificationState {
            generation,
            emojis: emojis.to_vec(),
        });
    }

    /// Phase C3a: `updateChatVideoChat`. Refreshes the chat's join
    /// affordance (`group_call_id` 0 → no active video chat).
    fn accept_chat_video_chat(&mut self, chat_id: ChatId, video_chat: &ParsedVideoChat) {
        let chat = self
            .chats
            .entry(chat_id.0)
            .or_insert_with(|| placeholder_chat(chat_id));
        chat.video_chat = if video_chat.group_call_id == 0 {
            None
        } else {
            Some(VideoChatInfo {
                group_call_id: video_chat.group_call_id,
                has_participants: video_chat.has_participants,
            })
        };
    }

    /// Phase C3a: drop the tracked group call after the local user
    /// leaves or ends it.
    pub fn leave_group_call_local(&mut self) {
        self.active_group_call = None;
    }

    /// Phase C3a: flip the local-only self-mute state. There is no
    /// TDLib "mute self" request for group calls outside the join
    /// parameters, and no audio path exists yet (C2) — the UI labels
    /// this honestly as local-only.
    pub fn set_group_call_self_muted(&mut self, muted: bool) {
        if let Some(tracked) = self.active_group_call.as_mut() {
            tracked.is_muted_self = muted;
        }
    }

    /// Phase C3a: clear the `reconnecting` flag once a rejoin has been
    /// issued by the driver.
    pub fn clear_group_call_reconnecting(&mut self) {
        if let Some(tracked) = self.active_group_call.as_mut() {
            tracked.reconnecting = false;
        }
    }

    /// Phase C3a: store the `joinVideoChat` `Text` response payload on
    /// the tracked call. Phase C2g consumes it in the driver pump to
    /// finish the native group handshake.
    pub fn set_group_call_join_payload(&mut self, group_call_id: i32, payload: String) {
        if let Some(tracked) = self.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.join_payload = payload;
        }
    }

    /// Phase C2g: store the `startGroupCallScreenSharing` `Text`
    /// response on the tracked call; the driver pump consumes it to
    /// finish the presentation handshake.
    pub fn set_group_call_screen_share_answer(&mut self, group_call_id: i32, payload: String) {
        if let Some(tracked) = self.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.screen_share_answer = payload;
        }
    }

    /// Phase C3a: store the `getVideoChatInviteLink` `HttpUrl` response
    /// on the tracked call.
    pub fn set_group_call_invite_link(&mut self, group_call_id: i32, link: String) {
        if let Some(tracked) = self.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.invite_link = Some(link);
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

    /// Phase 9.5: (re)start the viewers panel for a story — resets rows
    /// and pagination when the story changes, keeps them when the same
    /// story is re-opened.
    pub fn begin_story_viewers(&mut self, chat_id: i64, story_id: i32) {
        let same = self
            .story_viewers
            .as_ref()
            .is_some_and(|state| state.chat_id == chat_id && state.story_id == story_id);
        if !same {
            self.story_viewers = Some(StoryViewersState {
                chat_id,
                story_id,
                ..Default::default()
            });
        }
    }

    pub fn clear_story_viewers(&mut self) {
        self.story_viewers = None;
    }

    /// Phase 9.5: accumulate one `storyInteractions` page into the
    /// viewers panel. A page for a different story (stale response after
    /// the viewer moved on) is dropped.
    pub fn accept_story_interactions(
        &mut self,
        pending: &PendingRequest,
        view: StoryInteractionsView,
    ) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        let Some(state) = self.story_viewers.as_mut() else {
            return;
        };
        if state.chat_id != chat_id.0 || state.story_id != story_id {
            return;
        }
        state.loading = false;
        state.error = None;
        state.total_count = view.total_count;
        state.next_offset = view.next_offset;
        state.rows.extend(view.interactions);
    }

    /// Phase 9.5: mark the viewers fetch as failed; the panel shows the
    /// error with a retry instead of spinning forever.
    pub fn fail_story_viewers(&mut self, pending: &PendingRequest, message: String) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        if let Some(state) = self.story_viewers.as_mut()
            && state.chat_id == chat_id.0
            && state.story_id == story_id
        {
            state.loading = false;
            state.error = Some(message);
        }
    }

    /// Phase 9.5: start the `reportStory` flow for a story — the UI
    /// renders `Checking` until the first answer lands.
    pub fn begin_story_report(&mut self, chat_id: i64, story_id: i32) {
        self.story_report = Some(StoryReportFlow {
            chat_id,
            story_id,
            stage: StoryReportStage::Checking,
        });
    }

    pub fn clear_story_report(&mut self) {
        self.story_report = None;
    }

    /// Phase 9.5: apply a `ReportStoryResult` answer. A result for a
    /// different story (stale response) is dropped. Mirrors TDLib's
    /// `ReportStoryQuery` mapping (`td/telegram/StoryManager.cpp:1406-1430`
    /// @d1085f9): an empty option list is a success, not a picker.
    pub fn accept_story_report(&mut self, pending: &PendingRequest, result: ReportStoryResult) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        let Some(flow) = self.story_report.as_mut() else {
            return;
        };
        if flow.chat_id != chat_id.0 || flow.story_id != story_id {
            return;
        }
        flow.stage = match result {
            ReportStoryResult::Ok => StoryReportStage::Reported,
            ReportStoryResult::OptionRequired { title: _, options } if options.is_empty() => {
                StoryReportStage::Reported
            }
            ReportStoryResult::OptionRequired { title, options } => {
                StoryReportStage::PickOption { title, options }
            }
            ReportStoryResult::TextRequired {
                option_id,
                is_optional,
            } => StoryReportStage::TextRequired {
                option_id,
                is_optional,
            },
        };
    }

    /// Phase 9.5: the follow-up `reportStory` (reason picked / details
    /// submitted) is in flight.
    pub fn story_report_sending(&mut self, chat_id: i64, story_id: i32) {
        if let Some(flow) = self.story_report.as_mut()
            && flow.chat_id == chat_id
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Sending;
        }
    }

    /// Phase 9.5: a raw TDLib error on a `reportStory` request ends the
    /// flow with the sanitized message (there is no `Failed` result
    /// variant — errors arrive as `error` answers).
    pub fn fail_story_report(&mut self, pending: &PendingRequest, message: String) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        if let Some(flow) = self.story_report.as_mut()
            && flow.chat_id == chat_id.0
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Failed(message);
        }
    }

    /// Phase 9.5: `reportStory` failed to send at all — the driver took
    /// the pending request back, so no answer will ever arrive. End the
    /// flow with the error instead of spinning on `Checking`/`Sending`
    /// forever.
    pub fn fail_story_report_send(&mut self, chat_id: i64, story_id: i32, message: String) {
        if let Some(flow) = self.story_report.as_mut()
            && flow.chat_id == chat_id
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Failed(message);
        }
    }

    /// Phase 9.5: store the latest `updateStoryStealthMode` state.
    pub fn apply_update_story_stealth_mode(
        &mut self,
        active_until_date: i32,
        cooldown_until_date: i32,
    ) {
        self.story_stealth = StoryStealthMode {
            active_until_date,
            cooldown_until_date,
        };
    }

    pub fn request_download(&mut self, file_id: FileId) -> RequestId {
        let extra = self
            .requests
            .register_download(self.account_generation, file_id);
        self.download_extras.insert(extra.0, file_id.0);
        extra
    }

    /// Slice CL2: optimistic pinned-chat reorder for `setPinnedChats`.
    /// `new_ids` is the desired full pinned order (chat ids) of the
    /// list, highest first. The pinned chats swap `order` values among
    /// themselves so the new sequence survives `rebuild_main_order`
    /// until authoritative `updateChatPosition` orders arrive (TDLib
    /// `order` is opaque — new values can't be minted, only the
    /// existing ones permuted). Returns the previous `(chat_id,
    /// order)` pairs for rollback; returns an empty vec and changes
    /// nothing when the id set doesn't match the currently pinned set.
    /// Slice CL2: ordered pinned chat ids for a list (main or archive),
    /// highest first — the canonical order `setPinnedChats` expects.
    pub fn pinned_chat_ids(&self, archived: bool) -> Vec<i64> {
        let mut pinned: Vec<(i64, i64)> = self
            .chats
            .values()
            .filter(|c| {
                if archived {
                    c.in_archive && c.archive_is_pinned
                } else {
                    c.in_main_list && c.is_pinned
                }
            })
            .map(|c| {
                let order = if archived { c.archive_order } else { c.order };
                (c.id.0, order)
            })
            .collect();
        // Canonical current order: highest order first, matching
        // `rebuild_main_order`'s sort.
        pinned.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
        pinned.into_iter().map(|(id, _)| id).collect()
    }

    pub fn reorder_pinned_chats(&mut self, archived: bool, new_ids: &[i64]) -> Vec<(i64, i64)> {
        let current: Vec<(i64, i64)> = self
            .pinned_chat_ids(archived)
            .into_iter()
            .map(|id| {
                let order = self
                    .chats
                    .get(&id)
                    .map(|c| if archived { c.archive_order } else { c.order });
                (id, order.unwrap_or(0))
            })
            .collect();
        let mut current_ids: Vec<i64> = current.iter().map(|(id, _)| *id).collect();
        let mut new_sorted: Vec<i64> = new_ids.to_vec();
        current_ids.sort_unstable();
        new_sorted.sort_unstable();
        if current_ids != new_sorted || new_ids.is_empty() {
            return Vec::new();
        }
        let orders: Vec<i64> = current.iter().map(|(_, order)| *order).collect();
        for (id, order) in new_ids.iter().zip(orders.iter()) {
            if let Some(chat) = self.chats.get_mut(id) {
                if archived {
                    chat.archive_order = *order;
                } else {
                    chat.order = *order;
                }
            }
        }
        self.rebuild_main_order();
        current
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
        // Slice media-shared-gallery: switching chats closes the gallery so
        // its title and rows can't outlive the chat they belong to. `close`
        // also bumps the generation, dropping in-flight fetches for the old
        // chat. Same-chat re-select never reaches this method (see
        // `ConnectDriver::select_chat`), so the gallery survives it.
        if self.open_chat != Some(chat_id) {
            self.shared_media.close();
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

    /// Phase B1: like `request`, but stamps the secret chat id for
    /// `GetSecretChat` / `CloseSecretChat` correlation
    /// (`PendingRequest::secret_chat_id`) — the `secretChat` response and
    /// the `ok` carry no chat id of their own.
    pub fn request_for_secret_chat(
        &mut self,
        purpose: RequestPurpose,
        secret_chat_id: i32,
    ) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.secret_chat_id = Some(secret_chat_id);
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
                        preview: effective_preview(message),
                        is_outgoing: message.is_outgoing,
                        content: message.content.clone(),
                        author_signature: message.author_signature.clone(),
                        reply_to: message.reply_to.clone(),
                        forward_info: message.forward_info.clone(),
                        interaction_info: message.interaction_info.clone(),
                        is_pinned: message.is_pinned,
                        media_album_id: message.media_album_id,
                        reply_markup: message.reply_markup.clone(),
                        self_destruct: message.self_destruct,
                        auto_delete: message.auto_delete,
                        date: message.date,
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
        date: message.date,
        content: message.content,
        pending,
        reply_to: message.reply_to,
        forward_info: message.forward_info,
        interaction_info: message.interaction_info,
        is_pinned: message.is_pinned,
        media_album_id: message.media_album_id,
        reply_markup: message.reply_markup,
        self_destruct: message.self_destruct,
        auto_delete: message.auto_delete,
        author_signature: message.author_signature,
        failed: false,
        can_retry: message.can_retry,
        ephemeral: message.ephemeral,
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
        let mut touched = Vec::new();
        for (chat_id, history) in self.histories.iter_mut() {
            for message in history.messages.values_mut() {
                if let MessageContent::Poll(poll_content) = &mut message.content
                    && poll_content.poll.id == poll.id
                {
                    poll_content.poll = poll.clone();
                    touched.push((*chat_id, message.id.0));
                    updated += 1;
                }
            }
        }
        // N2: an open voter dialog goes stale when the poll updates
        // (counts/options change) — drop cached pages for touched
        // messages so the next open refetches.
        if !touched.is_empty() {
            self.poll_voters
                .retain(|(chat_id, message_id, _), _| !touched.contains(&(*chat_id, *message_id)));
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
            return effective_preview(original);
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
    use crate::telegram::envelope::ChatEventAction;
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
    fn effective_preview_prefers_ephemeral_content() {
        // M2 fix-up: preview surfaces (reply-to header, chat-list snippet,
        // search hits, pinned/scheduled labels, notifications) must show the
        // ephemeral content instead of the regular content (schema 1.8.67,
        // line 3161).
        let parsed = ParsedMessage {
            id: MessageId(602),
            chat_id: ChatId(14),
            date: 0,
            is_outgoing: false,
            is_pinned: false,
            topic_id: None,
            ephemeral: Some(EphemeralMessageContent {
                content: Box::new(MessageContent::Text("secret flow".into())),
                reply_markup: None,
            }),
            media_album_id: 0,
            author_signature: None,
            scheduling_state: None,
            can_retry: false,
            content: MessageContent::Text("public".into()),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        };
        assert_eq!(
            effective_preview(&history_message(parsed, false)),
            "secret flow"
        );
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

    /// M1 fix-up: editing a scheduled send returns the edited `message`
    /// with `scheduling_state` set — it must refresh the scheduled-list
    /// entry, not insert a phantom row into chat history.
    #[test]
    fn edit_scheduled_message_refreshes_scheduled_list_not_history() {
        use crate::telegram::envelope::MessageSchedulingState;

        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.scheduled_messages.push(ParsedMessage {
            id: MessageId(70),
            chat_id: ChatId(7),
            date: 0,
            is_outgoing: true,
            is_pinned: false,
            topic_id: None,
            ephemeral: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
            can_retry: false,
            content: MessageContent::Text("scheduled draft".into()),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        });
        let extra = session.request(RequestPurpose::EditMessage, Some(ChatId(7)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":70,"chat_id":7,"is_outgoing":true,"scheduling_state":{{"@type":"messageSchedulingStateSendAtDate","send_date":999}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"edited draft","entities":[]}}}}}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.scheduled_messages.len(), 1);
        assert_eq!(
            session.scheduled_messages[0].content.preview(),
            "edited draft"
        );
        assert!(
            session
                .histories
                .get(&7)
                .is_none_or(|h| !h.messages.contains_key(&70)),
            "edited scheduled send must not land in chat history"
        );
    }

    /// M1 fix-up: a regular (non-scheduled) edit still upserts history —
    /// the scheduled branch must not swallow it.
    #[test]
    fn edit_regular_message_still_upserts_history() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::EditMessage, Some(ChatId(7)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":71,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"edited","entities":[]}}}}}}"#,
                extra.0,
            ),
        );
        let history = session.histories.get(&7).expect("history row present");
        assert_eq!(
            history.messages.get(&71).unwrap().content.preview(),
            "edited"
        );
    }

    /// M1 fix-up: `updateMessageContent` for a scheduled send refreshes
    /// the scheduled-list entry, not just history.
    #[test]
    fn update_message_content_refreshes_scheduled_entry() {
        use crate::telegram::envelope::MessageSchedulingState;

        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.scheduled_messages.push(ParsedMessage {
            id: MessageId(70),
            chat_id: ChatId(7),
            date: 0,
            is_outgoing: true,
            is_pinned: false,
            topic_id: None,
            ephemeral: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
            can_retry: false,
            content: MessageContent::Text("old caption".into()),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        });
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageContent","chat_id":7,"message_id":70,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"new caption","entities":[]}}}"#,
        );
        assert_eq!(
            session.scheduled_messages[0].content.preview(),
            "new caption"
        );
    }

    /// M1 fix-up: `updateMessageSendFailed` parses
    /// `sending_state.can_retry` through to the history row — retry is
    /// gated on it, not offered unconditionally.
    #[test]
    fn send_failed_marks_can_retry_from_sending_state() {
        for (can_retry, label) in [(true, "retryable"), (false, "not retryable")] {
            let (mut session, sink) = session();
            let seq = AtomicU64::new(0);
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
                &format!(
                    r#"{{"@type":"updateMessageSendFailed","old_message_id":-1,"message":{{"id":88,"chat_id":1,"is_outgoing":true,"sending_state":{{"@type":"messageSendingStateFailed","can_retry":{}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}},"error":{{"code":400}}}}"#,
                    can_retry,
                ),
            );
            let row = session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&88)
                .unwrap();
            assert!(row.failed, "{label}: failed send marks the row failed");
            assert_eq!(
                row.can_retry, can_retry,
                "{label}: can_retry parsed through"
            );
        }
    }

    /// M1 fix-up: a failed `resendMessages` surfaces in
    /// `Session::resend_error` (drained into the status note) instead of
    /// vanishing into the `_ => {}` swallower.
    #[test]
    fn resend_error_surfaces_instead_of_silence() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::ResendMessages, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_SEND_FAILED"}}"#,
                extra.0,
            ),
        );
        let err = session.resend_error.expect("resend error surfaced");
        assert!(err.contains("Could not retry the send"), "{err}");
    }

    /// M1 fix-up: a failed `getMessageLink` surfaces in
    /// `Session::message_link_error` instead of silently doing nothing.
    #[test]
    fn message_link_error_surfaces_instead_of_silence() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetMessageLink, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"LINK_NOT_AVAILABLE"}}"#,
                extra.0,
            ),
        );
        let err = session
            .message_link_error
            .expect("message link error surfaced");
        assert!(err.contains("Could not get message link"), "{err}");
    }

    /// MED2 fix-up: a refused `recognizeSpeech` surfaces in
    /// `Session::recognize_speech_error` instead of silently doing nothing.
    #[test]
    fn recognize_speech_error_surfaces_instead_of_silence() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::RecognizeSpeech, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"SPEECH_NOT_RECOGNIZED"}}"#,
                extra.0,
            ),
        );
        let err = session
            .recognize_speech_error
            .expect("recognize speech error surfaced");
        assert!(err.contains("Could not transcribe this message"), "{err}");
    }

    /// Bots slice: a first `inlineQueryResults` page replaces the slot's
    /// `Loading` with `Loaded` (pending purpose + matching slot keys).
    #[test]
    fn inline_query_first_page_loads_slot() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.inline_query = Some(InlineQuerySlot {
            chat_id: ChatId(1),
            bot_user_id: 77,
            query: "@gif cats".to_string(),
            fetch: InlineQueryFetch::Loading,
        });
        let extra = session.request(
            RequestPurpose::GetInlineQueryResults {
                chat_id: ChatId(1),
                bot_user_id: 77,
                first_page: true,
            },
            Some(ChatId(1)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":9001,"button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"a1","title":"An article","description":"Desc"}}],"next_offset":"25"}}"#,
                extra.0,
            ),
        );
        match &session.inline_query {
            Some(slot) => match &slot.fetch {
                InlineQueryFetch::Loaded {
                    inline_query_id,
                    button,
                    results,
                    next_offset,
                } => {
                    assert_eq!(*inline_query_id, 9001);
                    assert_eq!(*button, None);
                    assert_eq!(next_offset, "25");
                    assert_eq!(results.len(), 1);
                    assert_eq!(results[0].id, "a1");
                    assert_eq!(results[0].kind, "article");
                    assert_eq!(results[0].title, "An article");
                    assert_eq!(results[0].description, "Desc");
                }
                other => panic!("unexpected {other:?}"),
            },
            None => panic!("inline query slot missing"),
        }
    }

    /// Bots slice: a later `inlineQueryResults` page appends to the
    /// loaded page (deduped by result id) and takes the new offset.
    #[test]
    fn inline_query_pagination_appends() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.inline_query = Some(InlineQuerySlot {
            chat_id: ChatId(1),
            bot_user_id: 77,
            query: "@gif cats".to_string(),
            fetch: InlineQueryFetch::Loaded {
                inline_query_id: 9001,
                button: None,
                results: vec![InlineQueryResultSummary {
                    id: "a1".to_string(),
                    kind: "article".to_string(),
                    title: "An article".to_string(),
                    description: String::new(),
                }],
                next_offset: "25".to_string(),
            },
        });
        let extra = session.request(
            RequestPurpose::GetInlineQueryResults {
                chat_id: ChatId(1),
                bot_user_id: 77,
                first_page: false,
            },
            Some(ChatId(1)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":9002,"button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"a1","title":"An article"}},{{"@type":"inlineQueryResultPhoto","id":"p1","title":"A photo"}}],"next_offset":"50"}}"#,
                extra.0,
            ),
        );
        match &session.inline_query {
            Some(slot) => match &slot.fetch {
                InlineQueryFetch::Loaded {
                    inline_query_id,
                    results,
                    next_offset,
                    ..
                } => {
                    assert_eq!(*inline_query_id, 9002);
                    assert_eq!(next_offset, "50");
                    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
                    assert_eq!(ids, vec!["a1", "p1"]);
                    assert_eq!(results[1].kind, "photo");
                }
                other => panic!("unexpected {other:?}"),
            },
            None => panic!("inline query slot missing"),
        }
    }

    /// Bots slice: a failed first page lands in `Failed` so the picker
    /// shows an honest error instead of spinning forever.
    #[test]
    fn inline_query_first_page_error_fails_slot() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.inline_query = Some(InlineQuerySlot {
            chat_id: ChatId(1),
            bot_user_id: 77,
            query: "@gif cats".to_string(),
            fetch: InlineQueryFetch::Loading,
        });
        let extra = session.request(
            RequestPurpose::GetInlineQueryResults {
                chat_id: ChatId(1),
                bot_user_id: 77,
                first_page: true,
            },
            Some(ChatId(1)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BOT_INLINE_DISABLED"}}"#,
                extra.0,
            ),
        );
        match &session.inline_query {
            Some(slot) => match &slot.fetch {
                InlineQueryFetch::Failed(line) => {
                    assert!(line.contains("Could not load inline results"), "{line}");
                }
                other => panic!("unexpected {other:?}"),
            },
            None => panic!("inline query slot missing"),
        }
    }

    /// Bots slice: a failed "load more" keeps the loaded page so the
    /// button stays retryable.
    #[test]
    fn inline_query_pagination_error_keeps_loaded_page() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.inline_query = Some(InlineQuerySlot {
            chat_id: ChatId(1),
            bot_user_id: 77,
            query: "@gif cats".to_string(),
            fetch: InlineQueryFetch::Loaded {
                inline_query_id: 9001,
                button: None,
                results: vec![InlineQueryResultSummary {
                    id: "a1".to_string(),
                    kind: "article".to_string(),
                    title: "An article".to_string(),
                    description: String::new(),
                }],
                next_offset: "25".to_string(),
            },
        });
        let extra = session.request(
            RequestPurpose::GetInlineQueryResults {
                chat_id: ChatId(1),
                bot_user_id: 77,
                first_page: false,
            },
            Some(ChatId(1)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
                extra.0,
            ),
        );
        match &session.inline_query {
            Some(slot) => match &slot.fetch {
                InlineQueryFetch::Loaded { results, .. } => {
                    assert_eq!(results.len(), 1);
                    assert_eq!(results[0].id, "a1");
                }
                other => panic!("unexpected {other:?}"),
            },
            None => panic!("inline query slot missing"),
        }
    }

    #[test]
    fn chat_statistics_fetch_flow_loads_and_caches() {
        // Phase D2 replay: `getChatStatistics` request →
        // `chatStatisticsChannel` response lands `Loaded` under the
        // requested chat id, correlated through the pending request (the
        // response itself carries no chat id).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
        let graph = r#"{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}"#;
        let value = r#"{"@type":"statisticalValue","value":1.0,"previous_value":1.0,"growth_rate_percentage":0.0}"#;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatStatisticsChannel","@extra":"{}","period":{{"@type":"dateRange","start_date":1788000000,"end_date":1788604800}},"member_count":{{"@type":"statisticalValue","value":12345.0,"previous_value":11700.0,"growth_rate_percentage":5.5}},"mean_message_view_count":{v},"mean_message_share_count":{v},"mean_message_reaction_count":{v},"mean_story_view_count":{v},"mean_story_share_count":{v},"mean_story_reaction_count":{v},"enabled_notifications_percentage":61.5,"member_count_graph":{g},"join_graph":{g},"mute_graph":{g},"view_count_by_hour_graph":{g},"view_count_by_source_graph":{g},"join_by_source_graph":{g},"language_graph":{g},"message_interaction_graph":{g},"message_reaction_graph":{g},"story_interaction_graph":{g},"story_reaction_graph":{g},"instant_view_interaction_graph":{g},"recent_interactions":[]}}"#,
                extra.0,
                g = graph,
                v = value,
            ),
        );
        let ChatStatisticsFetch::Loaded(boxed) =
            session.chat_statistics.get(&13).expect("statistics loaded")
        else {
            panic!("expected loaded channel statistics");
        };
        let ChatStatistics::Channel(stats) = boxed.as_ref() else {
            panic!("expected channel statistics");
        };
        assert_eq!(stats.member_count.value, 12345.0);
        assert_eq!(
            (stats.period_start, stats.period_end),
            (1788000000, 1788604800)
        );
    }

    #[test]
    fn chat_statistics_fetch_flow_error_marks_failed() {
        // Phase D2 replay: a TDLib `error` for `getChatStatistics` lands
        // `Failed` so the panel shows an honest error, not a spinner.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_STATISTICS_NOT_AVAILABLE"}}"#,
                extra.0
            ),
        );
        let ChatStatisticsFetch::Failed(message) =
            session.chat_statistics.get(&13).expect("statistics failed")
        else {
            panic!("expected failed statistics");
        };
        assert!(message.contains("Could not load statistics"));
    }

    #[test]
    fn custom_title_failure_surfaces_in_member_dialog() {
        // Slice G1 replay: a TDLib `error` for `setChatMemberTag` lands
        // in `member_action_error` (the member-management dialog reads
        // the member-list fetch states, not `admin_lists`) as well as
        // `admin_lists` for the info panel.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::SetChatMemberTag { user_id: 42 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        let message = session
            .member_action_error
            .get(&13)
            .expect("member action error recorded");
        assert!(message.contains("Could not set custom title"));
        assert!(matches!(
            session.admin_lists.get(&13),
            Some(AdminListFetch::Failed(_))
        ));
    }

    #[test]
    fn basic_group_add_member_failures_accumulate() {
        // Slice G1 fix-up replay: every per-user `addChatMember` answers
        // `failedToAddMembers` (schema 1.8.67, line 13578) — the real
        // shape, not `ok`/`error`. Two per-user responses must
        // accumulate (1 + 0), and a request-level error counts one more,
        // so the dialog's partial-add line is honest.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let failed_member = |user_id: i64| {
            format!(
                r#"{{"@type":"failedToAddMember","user_id":{user_id},"premium_would_allow_invite":false,"premium_required_to_send_messages":false}}"#
            )
        };
        for (user_id, extra_count) in [(7, 1), (8, 0)] {
            let extra = session.request(RequestPurpose::AddChatMember, Some(ChatId(13)));
            let members = (0..extra_count)
                .map(|_| failed_member(user_id))
                .collect::<Vec<_>>()
                .join(",");
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"failedToAddMembers","@extra":"{}","failed_to_add_members":[{members}]}}"#,
                    extra.0
                ),
            );
        }
        assert_eq!(session.add_members_failed.get(&13), Some(&1));
        let extra = session.request(RequestPurpose::AddChatMember, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"USER_PRIVACY_RESTRICTED"}}"#,
                extra.0
            ),
        );
        assert_eq!(session.add_members_failed.get(&13), Some(&2));
    }

    #[test]
    fn bulk_add_members_response_replaces_count() {
        // Slice G1: the single bulk `addChatMembers` response replaces
        // the failure count (no accumulation across attempts).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        for count in [3, 1] {
            let extra = session.request(RequestPurpose::AddChatMembers, Some(ChatId(13)));
            let members = (0..count)
                .map(|_| r#"{"@type":"failedToAddMember","user_id":9,"premium_would_allow_invite":false,"premium_required_to_send_messages":false}"#)
                .collect::<Vec<_>>()
                .join(",");
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"failedToAddMembers","@extra":"{}","failed_to_add_members":[{members}]}}"#,
                    extra.0
                ),
            );
        }
        assert_eq!(session.add_members_failed.get(&13), Some(&1));
    }

    #[test]
    fn optimistic_mutations_roll_back_on_error() {
        // Slice G1 replay: `setChatPermissions`,
        // `toggleSupergroupJoinByRequest`, and `setSupergroupUsername`
        // apply optimistically at send time. A TDLib error restores the
        // pre-request value carried on `PendingRequest::rollback` so the
        // UI never keeps showing a change the server rejected.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        // Permissions: optimistic write, then the error.
        let mut chat = placeholder_chat(ChatId(13));
        chat.permissions = Some(ChatPermissions::all());
        chat.can_send_basic_messages = true;
        session.chats.insert(13, chat);
        let extra = session.request(RequestPurpose::SetChatPermissions, Some(ChatId(13)));
        if let Some(pending) = session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ChatPermissions {
                previous: None,
                previous_can_send: false,
            });
        }
        // Simulate the optimistic write the driver performs at send.
        if let Some(chat) = session.chats.get_mut(&13) {
            chat.permissions = Some(ChatPermissions::all());
            chat.can_send_basic_messages = true;
        }
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        let chat = session.chats.get(&13).unwrap();
        assert_eq!(chat.permissions, None);
        assert!(!chat.can_send_basic_messages);

        // Join-by-request: previous flag restored.
        session.supergroup_join_by_request.insert(21, true);
        let extra = session.request(
            RequestPurpose::ToggleSupergroupJoinByRequest,
            Some(ChatId(21)),
        );
        if let Some(pending) = session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::JoinByRequest {
                supergroup_id: 21,
                previous: Some(true),
            });
        }
        session.supergroup_join_by_request.insert(21, false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        assert_eq!(session.supergroup_join_by_request.get(&21), Some(&true));

        // Username: absent stays absent, present is restored.
        session.supergroup_usernames.insert(22, "oldname".into());
        let extra = session.request(RequestPurpose::SetSupergroupUsername, Some(ChatId(22)));
        if let Some(pending) = session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::SupergroupUsername {
                supergroup_id: 22,
                previous: Some("oldname".into()),
            });
        }
        session
            .supergroup_usernames
            .insert(22, "newname".to_string());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"USERNAME_OCCUPIED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.supergroup_usernames.get(&22).map(String::as_str),
            Some("oldname")
        );
    }

    #[test]
    fn invite_link_replace_failure_surfaces_and_broadcast_rolls_back() {
        // Slice G1 fix-up replay: a failed `replacePrimaryChatInviteLink`
        // keeps the previously loaded list (no cache poisoning) and
        // surfaces the error via `invite_link_error` for the status
        // note; a failed `toggleSupergroupIsBroadcastGroup` removes the
        // optimistic broadcast flag so the panel doesn't lie.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.invite_links.insert(
            13,
            InviteLinkFetch::Loaded(InviteLinkList {
                total_count: 1,
                links: Vec::new(),
            }),
        );

        let extra = session.request(
            RequestPurpose::ReplacePrimaryChatInviteLink,
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"INVITE_LINK_INVALID"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.invite_links.get(&13),
            Some(InviteLinkFetch::Loaded(_))
        ));
        assert!(
            session
                .invite_link_error
                .as_ref()
                .is_some_and(|m| m.contains("Could not replace primary invite link"))
        );

        let mut chat = placeholder_chat(ChatId(14));
        chat.kind = ChatKind::Supergroup {
            supergroup_id: 14,
            is_channel: false,
        };
        session.chats.insert(14, chat);
        session.supergroup_is_broadcast.insert(14, true);
        let extra = session.request(RequestPurpose::ToggleBroadcastGroup, Some(ChatId(14)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        assert!(!session.supergroup_is_broadcast.contains_key(&14));
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

    /// Phase B3: a self-destructing `messagePhoto` (as it arrives in a
    /// 1:1 chat — `is_secret` content flag plus the message-level
    /// `self_destruct_type` / `self_destruct_in`) keeps the timer on the
    /// history row, and the `updateDeleteMessages` TDLib emits when the
    /// timer fires removes the row through the normal delete path.
    #[test]
    fn self_destructing_photo_disappears_via_delete_update() {
        use crate::telegram::envelope::SelfDestructKind;
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(41));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":901,"chat_id":41,"is_outgoing":false,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[]},"caption":{"@type":"formattedText","text":"","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":true},"self_destruct_type":{"@type":"messageSelfDestructTypeTimer","self_destruct_time":60},"self_destruct_in":42.5}}"#,
        );
        let history = session.histories.get(&41).unwrap();
        let row = history.messages.get(&901).unwrap();
        let sd = row.self_destruct.expect("timer parsed on history row");
        assert_eq!(sd.kind, SelfDestructKind::Timer { secs: 60 });
        assert!(row.has_live_self_destruct(unix_ms_now()));
        assert!(session.open_chat_has_live_self_destruct(unix_ms_now()));
        // The badge label comes from the latest `self_destruct_in`.
        let label = row
            .self_destruct_badge(sd.fetched_at_ms)
            .expect("badge for timer row");
        assert_eq!(label, "⏱ 43s left");
        // Timer fires server-side: the row leaves via `updateDeleteMessages`.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":41,"message_ids":[901],"is_permanent":true,"from_cache":false}"#,
        );
        assert!(
            !session
                .histories
                .get(&41)
                .unwrap()
                .messages
                .contains_key(&901)
        );
        assert!(!session.open_chat_has_live_self_destruct(unix_ms_now()));
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
        assert!(kind.is_supported_chat());
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
                    downloaded_size: 0,
                },
            },
            true,
        );
        assert!(!session.pending_sound_downloads.contains(&7));
        assert!(session.pending_sound_plays.is_empty());
        // The file→sound mapping itself stays (the list refetch prunes it).
        assert_eq!(session.sound_file_ids.get(&91), Some(&7));
    }

    /// Phase S2: `take_purpose` drops the in-flight storage-stats request
    /// (the Refresh path) and leaves other purposes alone.
    #[test]
    fn take_purpose_drops_only_matching_request() {
        let (mut session, _) = session();
        let stats_id = session.request(RequestPurpose::GetStorageStatistics, None);
        let sounds_id = session.request(RequestPurpose::GetSavedNotificationSounds, None);
        let dropped = session
            .requests
            .take_purpose(RequestPurpose::GetStorageStatistics);
        assert_eq!(dropped.map(|r| r.id), Some(stats_id));
        assert!(
            !session
                .requests
                .has_purpose(RequestPurpose::GetStorageStatistics)
        );
        assert!(
            session
                .requests
                .has_purpose(RequestPurpose::GetSavedNotificationSounds)
        );
        assert!(session.requests.take(sounds_id).is_some());
    }

    /// Phase S2: a `storageStatistics` answer lands in
    /// `Session::storage_stats` only when it answers our own in-flight
    /// `GetStorageStatistics` request (matched by `@extra`).
    #[test]
    fn storage_statistics_answer_cached_by_purpose() {
        let (mut with_purpose, sink) = session();
        let (mut without_purpose, sink2) = session();
        let seq = AtomicU64::new(0);
        let extra = with_purpose.request(RequestPurpose::GetStorageStatistics, None);
        with_purpose.storage_stats_loading = true;
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"storageStatistics","size":6000,"count":2,"by_chat":[{{"chat_id":0,"size":6000,"count":2,"by_file_type":[{{"file_type":{{"@type":"fileTypeSecret"}},"size":4000,"count":1}},{{"file_type":{{"@type":"fileTypePhoto"}},"size":2000,"count":1}}]}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        let stats = with_purpose.storage_stats.expect("stats cached");
        assert_eq!(stats.total_size, 6000);
        assert!(
            stats
                .by_file_type
                .iter()
                .any(|t| t.file_type == "fileTypeSecret" && t.size == 4000 && t.count == 1),
            "secret category present"
        );
        assert!(!with_purpose.storage_stats_loading);

        // A stray `storageStatistics` (no matching purpose) is ignored.
        let seq2 = AtomicU64::new(0);
        apply_json(
            &mut without_purpose,
            &seq2,
            &sink2,
            r#"{"@type":"storageStatistics","size":1,"count":1,"by_chat":[]}"#,
        );
        assert!(without_purpose.storage_stats.is_none());
    }

    /// Slice S8: sticker-backend answers are stored only under a matching
    /// request purpose (stray answers ignored), and mutation `ok`s
    /// invalidate the affected caches.
    #[test]
    fn s8_sticker_backend_purpose_gated_dispatch() {
        let (mut with_purpose, sink) = session();
        let seq = AtomicU64::new(0);

        // Trending sets land under GetTrendingStickerSets.
        let extra = with_purpose.request(RequestPurpose::GetTrendingStickerSets, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"trendingStickerSets","total_count":1,"is_premium":true,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":false,"size":2,"covers":[]}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.stickers.trending.len(), 1);
        assert_eq!(with_purpose.stickers.trending[0].id, 77);
        assert!(with_purpose.stickers.trending_is_premium);

        // A stray trendingStickerSets (no matching purpose) is ignored.
        let (mut without_purpose, sink2) = session();
        let seq2 = AtomicU64::new(0);
        apply_json(
            &mut without_purpose,
            &seq2,
            &sink2,
            r#"{"@type":"trendingStickerSets","total_count":1,"is_premium":false,"sets":[]}"#,
        );
        assert!(without_purpose.stickers.trending.is_empty());

        // Favorites arrive as the bare `stickers` type under
        // GetFavoriteStickers.
        let extra = with_purpose.request(RequestPurpose::GetFavoriteStickers, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.stickers.favorites.len(), 1);
        assert_eq!(with_purpose.stickers.favorites[0].emoji, "😀");

        // searchStickerSets answers with `stickerSets` under
        // SearchStickerSets (not the installed-sets slot).
        let extra = with_purpose.request(RequestPurpose::SearchStickerSets, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stickerSets","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"78","title":"Cats","name":"CatsStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":false,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":5,"covers":[]}}],"@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.stickers.found_sets.len(), 1);
        assert_eq!(with_purpose.stickers.found_sets[0].id, 78);
        assert!(with_purpose.stickers.sets.is_empty());

        // A removeFavoriteSticker `ok` clears the favorites cache.
        let extra = with_purpose.request(RequestPurpose::RemoveFavoriteSticker, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.stickers.favorites.is_empty());

        // A changeStickerSet `ok` drops the installed-sets cache so the
        // panel refetches the authoritative list.
        with_purpose.stickers.sets = vec![with_purpose.stickers.trending[0].clone()];
        let extra = with_purpose.request(RequestPurpose::ChangeStickerSet, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.stickers.sets.is_empty());
    }

    /// Slice S9: GIF-backend answers are stored only under a matching
    /// request purpose (stray answers ignored), search pages replace /
    /// append with `next_offset` paging, mutation `ok`s invalidate the
    /// saved list, and `updateAnimationSearchParameters` stores
    /// provider + emojis.
    #[test]
    fn s9_gif_backend_purpose_gated_dispatch() {
        let (mut with_purpose, sink) = session();
        let seq = AtomicU64::new(0);

        // First search page lands in `GifPanel` under GetGifSearchResults
        // (not the composer's inline_query slot).
        let extra = with_purpose.request(
            RequestPurpose::GetGifSearchResults { first_page: true },
            None,
        );
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"inlineQueryResults","inline_query_id":7,"button":null,"results":[{{"@type":"inlineQueryResultAnimation","id":"g1","title":"cat","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"c.gif","mime_type":"image/gif","animation":{{"@type":"file","id":101,"size":12,"expected_size":12,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}}},"thumbnail":null,"has_stickers":false}}}}],"next_offset":"50","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(with_purpose.gifs.search_results.len(), 1);
        assert_eq!(with_purpose.gifs.search_results[0].file_id.0, 101);
        assert_eq!(with_purpose.gifs.search_next_offset, "50");
        assert!(with_purpose.inline_query.is_none());

        // Second page appends new entries (deduped by file id) and
        // refreshes the offset.
        let extra = with_purpose.request(
            RequestPurpose::GetGifSearchResults { first_page: false },
            None,
        );
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"inlineQueryResults","inline_query_id":7,"button":null,"results":[{{"@type":"inlineQueryResultAnimation","id":"g1","title":"cat","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"c.gif","mime_type":"image/gif","animation":{{"@type":"file","id":101}},"thumbnail":null,"has_stickers":false}}}},{{"@type":"inlineQueryResultAnimation","id":"g2","title":"dog","animation":{{"@type":"animation","duration":3,"width":200,"height":200,"file_name":"d.gif","mime_type":"image/gif","animation":{{"@type":"file","id":102}},"thumbnail":null,"has_stickers":false}}}}],"next_offset":"","@extra":"{}"}}"#,
                extra.0
            ),
        );
        // g1 (file 101) was already on page one — deduped; g2 (file 102)
        // appends; the offset advances to "" (exhausted).
        assert_eq!(with_purpose.gifs.search_results.len(), 2);
        assert_eq!(with_purpose.gifs.search_results[1].file_id.0, 102);
        assert_eq!(with_purpose.gifs.search_next_offset, "");

        // A stray inlineQueryResults (no matching purpose) is ignored.
        let (mut without_purpose, sink2) = session();
        let seq2 = AtomicU64::new(0);
        apply_json(
            &mut without_purpose,
            &seq2,
            &sink2,
            r#"{"@type":"inlineQueryResults","inline_query_id":7,"button":null,"results":[],"next_offset":"9"}"#,
        );
        assert!(without_purpose.gifs.search_results.is_empty());

        // An addSavedAnimation `ok` clears the saved-GIF cache so it
        // refetches the server-confirmed list.
        with_purpose.gifs.animations = with_purpose.gifs.search_results.clone();
        let extra = with_purpose.request(RequestPurpose::AddSavedAnimation, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.gifs.animations.is_empty());

        // A removeSavedAnimation `ok` invalidates the same way.
        with_purpose.gifs.animations = with_purpose.gifs.search_results.clone();
        let extra = with_purpose.request(RequestPurpose::RemoveSavedAnimation, None);
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(with_purpose.gifs.animations.is_empty());

        // updateAnimationSearchParameters stores provider + emojis.
        apply_json(
            &mut with_purpose,
            &seq,
            &sink,
            r#"{"@type":"updateAnimationSearchParameters","provider":"Tenor","emojis":["😀","🎉"]}"#,
        );
        assert_eq!(with_purpose.gifs.search_provider, "Tenor");
        assert_eq!(with_purpose.gifs.provider_emojis, vec!["😀", "🎉"]);
    }

    /// Phase S2: a TDLib `error` answer to `getStorageStatistics`
    /// clears the in-flight flag so the overlay shows an honest empty
    /// state instead of a spinner.
    #[test]
    fn storage_statistics_error_clears_loading() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetStorageStatistics, None);
        session.storage_stats_loading = true;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORAGE_STATS_FAILED"}}"#,
                extra.0
            ),
        );
        assert!(session.storage_stats.is_none());
        assert!(!session.storage_stats_loading);
    }

    /// Slice A2: a `passwordState` answer to our own `PasswordStateOp`
    /// request replaces the cached state, clears loading, and clears a
    /// stale error. A stray `passwordState` (no matching purpose) is
    /// ignored — the overlay never shows unrequested state.
    #[test]
    fn password_state_response_replaces_cache_and_clears_error() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::PasswordStateOp {
                op: PasswordOp::SetPassword,
            },
            None,
        );
        session.password_state_loading = true;
        session.password_op_error = Some("stale".into());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"passwordState","has_password":true,"password_hint":"street","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                extra.0
            ),
        );
        let state = session.password_state.expect("password state cached");
        assert!(state.has_password);
        assert_eq!(state.password_hint, "street");
        assert!(state.has_recovery_email_address);
        assert_eq!(state.pending_email_pattern, None);
        assert!(!session.password_state_loading);
        assert!(session.password_op_error.is_none());
    }

    /// Slice A2: a `passwordState` that arrives with no matching pending
    /// `PasswordStateOp` is ignored — the overlay never shows
    /// unrequested state.
    #[test]
    fn stray_password_state_response_is_ignored() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"passwordState","has_password":true,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0}"#,
        );
        assert!(session.password_state.is_none());
        assert!(!session.password_state_loading);
    }

    /// Slice A2: TDLib errors surface as honest classified lines —
    /// wrong password (400), flood-wait (429) — and clear the
    /// in-flight flag. No optimistic state is ever written.
    #[test]
    fn password_state_error_surfaces_honest_classified_line() {
        for (op, code, expect) in [
            (PasswordOp::SetPassword, 400, "wrong password"),
            (PasswordOp::DisablePassword, 400, "wrong password"),
            (PasswordOp::SetRecoveryEmail, 400, "email was rejected"),
            (PasswordOp::ResendCode, 429, "too many attempts"),
            (PasswordOp::Fetch, 429, "too many attempts"),
        ] {
            let (mut session, sink) = session();
            let seq = AtomicU64::new(0);
            let extra = session.request(RequestPurpose::PasswordStateOp { op }, None);
            session.password_state_loading = true;
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":{code},"message":"SOME_TDLIB_ERROR"}}"#,
                    extra.0
                ),
            );
            let line = session.password_op_error.expect("error line set");
            assert!(line.contains(expect), "op {op:?} code {code}: {line}");
            assert!(!session.password_state_loading);
            assert!(session.password_state.is_none());
        }
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
    fn message_time_hhmm_formats_utc_and_rejects_missing() {
        // 2026-09-28 21:42:00 UTC.
        assert_eq!(message_time_hhmm(1790631720).as_deref(), Some("21:42"));
        assert_eq!(message_time_hhmm(0), None);
        assert_eq!(message_time_hhmm(-5), None);
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
            author_signature: None,
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            date: 0,
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
            author_signature: None,
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            date: 0,
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
                can_be_edited: false,
                can_set_privacy_settings: false,
                can_be_forwarded: false,
                is_edited: false,
                repost_info: None,
                privacy_settings: None,
                area_link_url: None,
                area_reaction_emojis: Vec::new(),
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

    /// Phase 9.5: `getStoryInteractions` pages accumulate into the
    /// viewers panel; a stale page (viewer moved to another story) is
    /// dropped; errors surface on the panel.
    #[test]
    fn story_viewers_accumulate_pages_and_drop_stale() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.begin_story_viewers(11, 5);
        let page = |extra: u64, offset: &str| {
            format!(
                r#"{{"@type":"storyInteractions","@extra":"{extra}","total_count":3,"interactions":[{{"actor_id":{{"@type":"messageSenderUser","user_id":777}},"interaction_date":1700000100,"block_list":null,"type":{{"@type":"storyInteractionTypeView","chosen_reaction_type":null}}}}],"next_offset":"{offset}"}}"#
            )
        };
        let extra1 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
        apply_json(&mut session, &seq, &sink, &page(extra1.0, "1"));
        let state = session.story_viewers.as_ref().unwrap();
        assert_eq!(state.rows.len(), 1);
        assert_eq!(state.next_offset, "1");
        assert!(!state.loading);
        // Second page appends.
        let extra2 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
        apply_json(&mut session, &seq, &sink, &page(extra2.0, ""));
        assert_eq!(session.story_viewers.as_ref().unwrap().rows.len(), 2);
        assert!(
            session
                .story_viewers
                .as_ref()
                .unwrap()
                .next_offset
                .is_empty()
        );
        // Stale page for another story is dropped.
        session.begin_story_viewers(11, 6);
        let extra3 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
        apply_json(&mut session, &seq, &sink, &page(extra3.0, "9"));
        assert!(session.story_viewers.as_ref().unwrap().rows.is_empty());
        // Error lands on the current panel.
        let extra4 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 6);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORY_NOT_FOUND"}}"#,
                extra4.0
            ),
        );
        let state = session.story_viewers.as_ref().unwrap();
        assert!(!state.loading);
        // The server message is classified by `error_reason` (native
        // text is never surfaced); the panel shows the failure.
        assert!(
            state
                .error
                .as_deref()
                .unwrap()
                .starts_with("Could not load viewers")
        );
    }

    /// Phase 9.5 review: reopening the viewers panel must not duplicate
    /// rows — `toggle_story_viewers` clears before the fresh page-1
    /// fetch (`begin_story_viewers` alone keeps rows for the same
    /// story, so the clear is what prevents [A,B] → [A,B,A,B]).
    #[test]
    fn story_viewers_reopen_resets_rows() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let page = |extra: u64| {
            format!(
                r#"{{"@type":"storyInteractions","@extra":"{extra}","total_count":1,"interactions":[{{"actor_id":{{"@type":"messageSenderUser","user_id":777}},"interaction_date":1700000100,"block_list":null,"type":{{"@type":"storyInteractionTypeView","chosen_reaction_type":null}}}}],"next_offset":""}}"#
            )
        };
        // Open the panel: begin + first page.
        session.begin_story_viewers(11, 5);
        let extra1 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
        apply_json(&mut session, &seq, &sink, &page(extra1.0));
        assert_eq!(session.story_viewers.as_ref().unwrap().rows.len(), 1);
        // `begin_story_viewers` alone keeps the same story's rows —
        // which is why the UI clears on re-open.
        session.begin_story_viewers(11, 5);
        assert_eq!(session.story_viewers.as_ref().unwrap().rows.len(), 1);
        // Re-open (clear, then begin + fresh page-1) → no duplication.
        session.clear_story_viewers();
        session.begin_story_viewers(11, 5);
        let extra2 = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
        apply_json(&mut session, &seq, &sink, &page(extra2.0));
        assert_eq!(session.story_viewers.as_ref().unwrap().rows.len(), 1);
    }

    /// Phase 9.5: the `reportStory` flow — Checking → OptionRequired
    /// arms the picker → TextRequired carries the option id → Ok
    /// reports; errors end the flow honestly.
    #[test]
    fn story_report_flow_through_option_and_text_steps() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.begin_story_report(12, 6);
        assert!(matches!(
            session.story_report.as_ref().unwrap().stage,
            StoryReportStage::Checking
        ));
        let extra = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"reportStoryResultOptionRequired","@extra":"{}","title":"Why?","options":[{{"@type":"reportOption","id":"aGk=","text":"Spam"}}]}}"#,
                extra.0
            ),
        );
        match &session.story_report.as_ref().unwrap().stage {
            StoryReportStage::PickOption { title, options } => {
                assert_eq!(title, "Why?");
                assert_eq!(options[0].id, "aGk=");
            }
            other => panic!("unexpected {other:?}"),
        }
        session.story_report_sending(12, 6);
        let extra2 = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"reportStoryResultTextRequired","@extra":"{}","option_id":"aGk=","is_optional":false}}"#,
                extra2.0
            ),
        );
        assert!(matches!(
            session.story_report.as_ref().unwrap().stage,
            StoryReportStage::TextRequired { ref option_id, .. } if option_id == "aGk="
        ));
        let extra3 = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"reportStoryResultOk","@extra":"{}"}}"#,
                extra3.0
            ),
        );
        assert!(matches!(
            session.story_report.as_ref().unwrap().stage,
            StoryReportStage::Reported
        ));
        // A late error after the flow closed does not resurrect it.
        session.clear_story_report();
        let extra4 = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"LATE"}}"#,
                extra4.0
            ),
        );
        assert!(session.story_report.is_none());
    }

    /// Phase 9.5: an empty `options` list in
    /// `reportStoryResultOptionRequired` is a success, not a picker —
    /// TDLib's `ReportStoryQuery` maps it to Ok (StoryManager.cpp:1414).
    #[test]
    fn story_report_empty_options_means_reported() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.begin_story_report(12, 6);
        let extra = session.request_for_story(RequestPurpose::ReportStory, ChatId(12), 6);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"reportStoryResultOptionRequired","@extra":"{}","title":"Why?","options":[]}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.story_report.as_ref().unwrap().stage,
            StoryReportStage::Reported
        ));
    }

    /// Phase 9.5 review: a `reportStory` send failure ends the flow
    /// with `Failed` — the driver took the pending request back, so no
    /// answer will ever arrive to move it off `Checking`/`Sending`.
    #[test]
    fn story_report_send_failure_ends_flow() {
        let (mut session, _sink) = session();
        session.begin_story_report(12, 6);
        session.fail_story_report_send(12, 6, "could not report story".into());
        assert!(matches!(
            session.story_report.as_ref().unwrap().stage,
            StoryReportStage::Failed(_)
        ));
        // A different story's flow is untouched.
        session.begin_story_report(12, 7);
        session.fail_story_report_send(12, 6, "could not report story".into());
        assert!(matches!(
            session.story_report.as_ref().unwrap().stage,
            StoryReportStage::Checking
        ));
    }

    /// Phase 9.5: `updateStoryStealthMode` stores the two timestamps;
    /// `StoryStealthMode` active/cooldown predicates read them.
    #[test]
    fn update_story_stealth_mode_stored() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateStoryStealthMode","active_until_date":1700003600,"cooldown_until_date":1700007200}"#,
        );
        assert_eq!(
            session.story_stealth,
            StoryStealthMode {
                active_until_date: 1700003600,
                cooldown_until_date: 1700007200,
            }
        );
        assert!(session.story_stealth.is_active(1700000000));
        assert!(!session.story_stealth.is_active(1700003600));
        assert!(session.story_stealth.is_cooling_down(1700003600));
        assert!(!session.story_stealth.is_cooling_down(1700007200));
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
    fn story_post_outcome_transitions() {
        // Phase 9.3: the composer's honest pending / succeeded / failed
        // states, driven by purpose-gated reducers.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        // `canPostStory` answer (purpose-gated into `story_post.eligibility`).
        let extra = session.request(RequestPurpose::CheckCanPostStory, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"canPostStoryResultWeeklyLimitExceeded","@extra":"{}","retry_after":9000}}"#,
                extra.0
            ),
        );
        let eligibility = session
            .story_post
            .eligibility
            .clone()
            .expect("eligibility stored");
        assert!(!eligibility.can_post());
        assert!(eligibility.user_message().contains("2h 30m"));

        // A stray result with no matching pending purpose is ignored.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"canPostStoryResultOk","story_count":1}"#,
        );
        assert!(!session.story_post.eligibility.clone().unwrap().can_post());

        // `postStory` answer → Posting with the temporary story id.
        let extra = session.request(RequestPurpose::PostStory, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"story","@extra":"{}","id":8,"poster_chat_id":777,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.story_post.outcome,
            StoryPostOutcome::Posting { story_id: 8 }
        );

        // `updateStoryPostSucceeded` with a matching old_story_id →
        // Succeeded (and the 9.2 upsert still runs).
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateStoryPostSucceeded","story":{"@type":"story","id":9,"poster_chat_id":777,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"old_story_id":8}"#,
        );
        assert_eq!(session.story_post.outcome, StoryPostOutcome::Succeeded);
        assert!(session.stories.contains_key(&(777, 9)));

        // Failed path: new pending post, then `updateStoryPostFailed`.
        let extra = session.request(RequestPurpose::PostStory, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"story","@extra":"{}","id":10,"poster_chat_id":777,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
                extra.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateStoryPostFailed","story":{"@type":"story","id":10,"poster_chat_id":777,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}},"error":{"@type":"error","code":400,"message":"x"},"error_type":{"@type":"canPostStoryResultOk"}}"#,
        );
        assert!(matches!(
            session.story_post.outcome,
            StoryPostOutcome::Failed(_)
        ));

        // A raw `error` answer on `postStory` → Failed; on `canPostStory`
        // → check_error (the composer stops spinning either way).
        let extra = session.request(RequestPurpose::PostStory, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"x"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.story_post.outcome,
            StoryPostOutcome::Failed(_)
        ));
        let extra = session.request(RequestPurpose::CheckCanPostStory, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":401,"message":"x"}}"#,
                extra.0
            ),
        );
        assert!(session.story_post.check_error.is_some());
    }

    #[test]
    fn story_manage_state_transitions() {
        // Phase 9.5: `editStory` / `editStoryCover` /
        // `setStoryPrivacySettings` pending is cleared by the `ok`
        // answer and the sanitized error lands on failure;
        // `getChatsToPostStories` stores the chat ids.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        let extra = session.request(RequestPurpose::EditStory, None);
        session.story_manage.pending = true;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(!session.story_manage.pending);
        assert_eq!(session.story_manage.error, None);

        let extra = session.request(RequestPurpose::EditStoryCover, None);
        session.story_manage.pending = true;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"STORY_NOT_EDITABLE"}}"#,
                extra.0
            ),
        );
        assert!(!session.story_manage.pending);
        let error = session.story_manage.error.clone().expect("manage error");
        assert!(error.contains("Story update failed"), "{error}");

        let extra = session.request(RequestPurpose::GetChatsToPostStories, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[111,222]}}"#,
                extra.0
            ),
        );
        assert_eq!(session.story_post_as_chats, vec![111, 222]);

        // Review fix-up: a failed `getChatsToPostStories` surfaces a
        // transient error instead of silently leaving only "Myself".
        let extra = session.request(RequestPurpose::GetChatsToPostStories, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"SOME_ERROR"}}"#,
                extra.0
            ),
        );
        let error = session
            .story_post
            .check_error
            .clone()
            .expect("post-as error");
        assert!(error.contains("Could not load"), "{error}");
    }

    #[test]
    fn story_post_second_answer_without_pending_is_absorbed() {
        // Phase 9.3 (review fix-up): the double-post window. The UI
        // `post_sent` guard blocks the second send path, and at the
        // reducer level a `postStory` answer with no matching pending
        // `PostStory` request is absorbed as a plain story upsert — it
        // neither re-enters `Posting` nor creates a second pending
        // request.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        // The (single) send: pending `PostStory`, answer → Posting.
        let extra = session.request(RequestPurpose::PostStory, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"story","@extra":"{}","id":8,"poster_chat_id":777,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.story_post.outcome,
            StoryPostOutcome::Posting { story_id: 8 }
        );
        assert!(!session.requests.has_purpose(RequestPurpose::PostStory));

        // A second `story` answer with no pending `PostStory` request
        // (the duplicate the guard prevents) is just a story upsert.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"story","id":8,"poster_chat_id":777,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}"#,
        );
        assert_eq!(
            session.story_post.outcome,
            StoryPostOutcome::Posting { story_id: 8 }
        );
        assert!(!session.requests.has_purpose(RequestPurpose::PostStory));
        assert!(session.stories.contains_key(&(777, 8)));
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

    /// Phase B2: `open_ready_secret_chat_for_user` returns the record
    /// (with its 36-byte hash) only when the open chat is a **Ready**
    /// secret chat with the matching partner — `None` for Pending chats,
    /// other users, and non-secret open chats. The header's info-panel
    /// target resolves to the secret chat partner.
    #[test]
    fn open_ready_secret_chat_for_user_gates_on_ready() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // Deterministic 36-byte key_hash (same fixture as the
        // ready-key-verification screenshot demo).
        let hash_b64 = "GUYMUT5VLuA6j7l7taiDAR9tM+Y30on50Cklur/t+/w57sWo";
        let secret_ready = format!(
            r#"{{"@type":"updateSecretChat","secret_chat":{{"@type":"secretChat","id":7,"user_id":41,"state":{{"@type":"secretChatStateReady"}},"is_outbound":true,"key_hash":"{hash_b64}","layer":144}}}}"#
        );
        apply_json(&mut session, &seq, &sink, &secret_ready);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Zed","type":{"@type":"chatTypeSecret","secret_chat_id":7,"user_id":41},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":8,"user_id":43,"state":{"@type":"secretChatStatePending"},"is_outbound":false,"key_hash":"","layer":144}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":42,"title":"Wendy","type":{"@type":"chatTypeSecret","secret_chat_id":8,"user_id":43},"unread_count":0}}"#,
        );

        session.open_chat(ChatId(41));
        let record = session
            .open_ready_secret_chat_for_user(41)
            .expect("Ready secret chat record");
        assert_eq!(record.id, 7);
        assert_eq!(record.key_hash.len(), 36);
        let pixels = crate::key_fingerprint::key_hash_pixels(&record.key_hash)
            .expect("36-byte hash is renderable");
        assert_eq!(pixels.len(), 144);
        // Wrong partner → None.
        assert!(session.open_ready_secret_chat_for_user(999).is_none());
        // The header opens the partner's panel for secret chats too.
        assert_eq!(
            session.info_panel_target_for_chat(ChatId(41)),
            Some(InfoPanelTarget::User(41))
        );

        // Pending chat → no record for the key UI.
        session.open_chat(ChatId(42));
        assert!(session.open_ready_secret_chat_for_user(43).is_none());
    }
    #[test]
    fn invite_link_fetch_flow_loads_and_caches() {
        // Phase D3a: Fetching invite links loads and caches the result.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

        let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
        let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
                extra.0, link1, link2
            ),
        );

        let InviteLinkFetch::Loaded(list) = session.invite_links.get(&13).unwrap() else {
            panic!("invite links were not loaded");
        };
        assert_eq!(list.total_count, 2);
        assert_eq!(list.links.len(), 2);
        assert_eq!(list.links[0].name, "Mods");

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLinks","@extra":"99999","total_count":1,"invite_links":[{}]}}"#,
                link1
            ),
        );

        let InviteLinkFetch::Loaded(list) = session.invite_links.get(&13).unwrap() else {
            panic!("invite links were not loaded");
        };
        assert_eq!(list.total_count, 2);
        assert_eq!(list.links.len(), 2);
    }

    #[test]
    fn invite_link_create_upsert_bumps_total() {
        // Phase D3a: A created invite link is appended and increments the total.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetch_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

        let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
        let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
                fetch_extra.0, link1, link2
            ),
        );

        let create_extra = session.request(RequestPurpose::CreateChatInviteLink, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLink","@extra":"{}","invite_link":"https://t.me/+new","name":"New","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":0,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#,
                create_extra.0
            ),
        );

        let InviteLinkFetch::Loaded(list) = session.invite_links.get(&13).unwrap() else {
            panic!("invite links were not loaded");
        };
        assert_eq!(list.total_count, 3);
        assert_eq!(list.links.len(), 3);
        assert_eq!(list.links.last().unwrap().name, "New");
    }

    #[test]
    fn invite_link_edit_replaces_in_place() {
        // Phase D3a: Editing an invite link replaces the matching cached entry.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetch_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

        let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
        let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
                fetch_extra.0, link1, link2
            ),
        );

        let edit_extra = session.request(RequestPurpose::EditChatInviteLink, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLink","@extra":"{}","invite_link":"https://t.me/+mods","name":"Mods!","creator_user_id":777,"date":1788000000,"edit_date":1788100000,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":9,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}}"#,
                edit_extra.0
            ),
        );

        let InviteLinkFetch::Loaded(list) = session.invite_links.get(&13).unwrap() else {
            panic!("invite links were not loaded");
        };
        assert_eq!(list.total_count, 2);
        assert_eq!(list.links.len(), 2);
        assert_eq!(list.links[0].name, "Mods!");
        assert_eq!(list.links[0].member_count, 9);
    }

    #[test]
    fn invite_link_revoke_replaces_list() {
        // Phase D3a: Revoking an invite link replaces the cached list response.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetch_extra = session.request(RequestPurpose::GetChatInviteLinks, Some(ChatId(13)));

        let link1 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods","name":"Mods","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
        let link2 = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+mods2","name":"Mods 2","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":4,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":2,"invite_links":[{},{}]}}"#,
                fetch_extra.0, link1, link2
            ),
        );

        let revoke_extra = session.request(RequestPurpose::RevokeChatInviteLink, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatInviteLinks","@extra":"{}","total_count":1,"invite_links":[{}]}}"#,
                revoke_extra.0, link2
            ),
        );

        let InviteLinkFetch::Loaded(list) = session.invite_links.get(&13).unwrap() else {
            panic!("invite links were not loaded");
        };
        assert_eq!(list.total_count, 1);
        assert_eq!(list.links.len(), 1);
        assert_eq!(list.links[0].invite_link, "https://t.me/+mods2");
    }

    #[test]
    fn join_request_fetch_flow_loads_and_caches() {
        // Phase D3a: Fetching join requests loads and caches the result.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi"}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Yo"}}]}}"#,
                extra.0
            ),
        );

        let JoinRequestFetch::Loaded(list) = session.join_requests.get(&13).unwrap() else {
            panic!("join requests were not loaded");
        };
        assert_eq!(list.total_count, 2);
        assert_eq!(list.requests.len(), 2);
        assert_eq!(list.requests[0].user_id, 7001);
        assert_eq!(list.requests[0].bio, "Hi");
    }

    #[test]
    fn join_request_update_prepends_and_dedupes() {
        // Phase D3a: New join-request updates prepend and deduplicate by user ID.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi"}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Yo"}}]}}"#,
                extra.0
            ),
        );

        let link = r#"{"@type":"chatInviteLink","invite_link":"https://t.me/+join","name":"Join","creator_user_id":777,"date":1788000000,"edit_date":0,"expiration_date":0,"subscription_pricing":null,"member_limit":25,"member_count":8,"expired_member_count":0,"pending_join_request_count":0,"creates_join_request":false,"is_primary":false,"is_revoked":false}"#;
        let update = format!(
            r#"{{"@type":"updateNewChatJoinRequest","chat_id":13,"request":{{"@type":"chatJoinRequest","user_id":7003,"date":1788600000,"bio":"New"}},"user_chat_id":0,"invite_link":{},"query_id":"42"}}"#,
            link
        );

        apply_json(&mut session, &seq, &sink, &update);

        let JoinRequestFetch::Loaded(list) = session.join_requests.get(&13).unwrap() else {
            panic!("join requests were not loaded");
        };
        assert_eq!(list.total_count, 3);
        assert_eq!(list.requests.len(), 3);
        assert_eq!(list.requests[0].user_id, 7003);

        apply_json(&mut session, &seq, &sink, &update);

        let JoinRequestFetch::Loaded(list) = session.join_requests.get(&13).unwrap() else {
            panic!("join requests were not loaded");
        };
        assert_eq!(list.total_count, 3);
        assert_eq!(list.requests.len(), 3);
        assert_eq!(list.requests[0].user_id, 7003);

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewChatJoinRequest","chat_id":14,"request":{{"@type":"chatJoinRequest","user_id":7004,"date":1788650000,"bio":"Unloaded"}},"user_chat_id":0,"invite_link":{},"query_id":"43"}}"#,
                link
            ),
        );

        assert!(!session.join_requests.contains_key(&14));
    }

    #[test]
    fn update_chat_pending_join_requests_sets_count() {
        // Phase D3a: Pending join-request updates cache the reported count.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPendingJoinRequests","chat_id":13,"pending_join_requests":{"@type":"chatJoinRequestsInfo","total_count":5,"user_ids":[7001]}}"#,
        );

        assert_eq!(session.pending_join_request_counts.get(&13), Some(&5));
    }

    #[test]
    fn process_join_request_ok_drops_from_list() {
        // Phase D3a: Successfully processing a join request removes it from the cache.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let fetch_extra = session.request(RequestPurpose::GetChatJoinRequests, Some(ChatId(13)));

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":2,"requests":[{{"@type":"chatJoinRequest","user_id":7001,"date":1788500000,"bio":"Hi"}},{{"@type":"chatJoinRequest","user_id":7002,"date":1788550000,"bio":"Yo"}}]}}"#,
                fetch_extra.0
            ),
        );

        let process_extra = session.request(
            RequestPurpose::ProcessChatJoinRequest { user_id: 7001 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, process_extra.0),
        );

        let JoinRequestFetch::Loaded(list) = session.join_requests.get(&13).unwrap() else {
            panic!("join requests were not loaded");
        };
        assert_eq!(list.total_count, 1);
        assert_eq!(list.requests.len(), 1);
        assert_eq!(list.requests[0].user_id, 7002);
    }

    #[test]
    fn chat_can_invite_users_gate() {
        // Phase D3a: Invite permissions follow channel and supergroup membership rights.
        let (mut session, _) = session();

        assert!(!session.chat_can_invite_users(ChatId(999)));

        let mut channel = placeholder_chat(ChatId(13));
        channel.kind = ChatKind::Supergroup {
            supergroup_id: 13,
            is_channel: true,
        };
        session.chats.insert(13, channel);
        assert!(!session.chat_can_invite_users(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_member_status(ChannelMemberStatus::Creator, None);
        assert!(session.chat_can_invite_users(ChatId(13)));

        {
            let channel = session.chats.get_mut(&13).unwrap();
            channel.set_member_status(ChannelMemberStatus::Administrator, None);
            channel.set_admin_can_invite_users(Some(true));
        }
        assert!(session.chat_can_invite_users(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_admin_can_invite_users(Some(false));
        assert!(!session.chat_can_invite_users(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_admin_can_invite_users(None);
        assert!(!session.chat_can_invite_users(ChatId(13)));

        let mut creator_group = placeholder_chat(ChatId(14));
        creator_group.kind = ChatKind::Supergroup {
            supergroup_id: 14,
            is_channel: false,
        };
        session.chats.insert(14, creator_group);
        session
            .supergroup_member_status
            .insert(14, ChannelMemberStatus::Creator);
        assert!(session.chat_can_invite_users(ChatId(14)));

        let mut admin_group = placeholder_chat(ChatId(15));
        admin_group.kind = ChatKind::Supergroup {
            supergroup_id: 15,
            is_channel: false,
        };
        session.chats.insert(15, admin_group);
        session
            .supergroup_member_status
            .insert(15, ChannelMemberStatus::Administrator);
        session.supergroup_invite_right.insert(15, true);
        assert!(session.chat_can_invite_users(ChatId(15)));

        session.supergroup_invite_right.insert(15, false);
        assert!(!session.chat_can_invite_users(ChatId(15)));

        let mut member_group = placeholder_chat(ChatId(16));
        member_group.kind = ChatKind::Supergroup {
            supergroup_id: 16,
            is_channel: false,
        };
        session.chats.insert(16, member_group);
        session
            .supergroup_member_status
            .insert(16, ChannelMemberStatus::Member);
        assert!(!session.chat_can_invite_users(ChatId(16)));
    }

    #[test]
    fn can_manage_admins_gate() {
        // Phase D3b: admin-management permissions follow channel and
        // supergroup membership rights; deny-by-default.
        let (mut session, _) = session();

        assert!(!session.chat_can_manage_admins(ChatId(999)));

        let mut channel = placeholder_chat(ChatId(13));
        channel.kind = ChatKind::Supergroup {
            supergroup_id: 13,
            is_channel: true,
        };
        session.chats.insert(13, channel);
        assert!(!session.chat_can_manage_admins(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_member_status(ChannelMemberStatus::Creator, None);
        assert!(session.chat_can_manage_admins(ChatId(13)));

        {
            let channel = session.chats.get_mut(&13).unwrap();
            channel.set_member_status(ChannelMemberStatus::Administrator, None);
            channel.set_admin_can_promote_members(Some(true));
        }
        assert!(session.chat_can_manage_admins(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_admin_can_promote_members(Some(false));
        assert!(!session.chat_can_manage_admins(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_admin_can_promote_members(None);
        assert!(!session.chat_can_manage_admins(ChatId(13)));

        // Non-channel supergroup path: creator always; admin needs the
        // explicit right from the supergroup status block.
        let mut creator_group = placeholder_chat(ChatId(14));
        creator_group.kind = ChatKind::Supergroup {
            supergroup_id: 14,
            is_channel: false,
        };
        session.chats.insert(14, creator_group);
        session
            .supergroup_member_status
            .insert(14, ChannelMemberStatus::Creator);
        assert!(session.chat_can_manage_admins(ChatId(14)));

        let mut admin_group = placeholder_chat(ChatId(15));
        admin_group.kind = ChatKind::Supergroup {
            supergroup_id: 15,
            is_channel: false,
        };
        session.chats.insert(15, admin_group);
        session
            .supergroup_member_status
            .insert(15, ChannelMemberStatus::Administrator);
        session.supergroup_promote_right.insert(15, true);
        assert!(session.chat_can_manage_admins(ChatId(15)));

        session.supergroup_promote_right.insert(15, false);
        assert!(!session.chat_can_manage_admins(ChatId(15)));
    }

    #[test]
    fn event_log_gate() {
        // Phase D3c: the event-log gate is deny-by-default. Any
        // administrator or the creator qualifies (no `can_promote_members`
        // right needed, unlike D3b): channels probe via the ChatSummary
        // path (`getChatMember`), supergroups via the status block.
        let (mut session, _) = session();

        assert!(!session.chat_can_view_event_log(ChatId(999)));

        let mut channel = placeholder_chat(ChatId(13));
        channel.kind = ChatKind::Supergroup {
            supergroup_id: 13,
            is_channel: true,
        };
        session.chats.insert(13, channel);
        assert!(!session.chat_can_view_event_log(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_member_status(ChannelMemberStatus::Administrator, None);
        assert!(session.chat_can_view_event_log(ChatId(13)));

        // An admin without the promote right still qualifies for the log.
        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_admin_can_promote_members(Some(false));
        assert!(session.chat_can_view_event_log(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_member_status(ChannelMemberStatus::Creator, None);
        assert!(session.chat_can_view_event_log(ChatId(13)));

        session
            .chats
            .get_mut(&13)
            .unwrap()
            .set_member_status(ChannelMemberStatus::Member, None);
        assert!(!session.chat_can_view_event_log(ChatId(13)));
        // Non-channel supergroup path: administrator without promote right.
        let mut group = placeholder_chat(ChatId(14));
        group.kind = ChatKind::Supergroup {
            supergroup_id: 14,
            is_channel: false,
        };
        session.chats.insert(14, group);
        assert!(!session.chat_can_view_event_log(ChatId(14)));
        session
            .supergroup_member_status
            .insert(14, ChannelMemberStatus::Administrator);
        assert!(session.chat_can_view_event_log(ChatId(14)));

        // Other chat kinds never qualify.
        let private = placeholder_chat(ChatId(15));
        session.chats.insert(15, private);
        assert!(!session.chat_can_view_event_log(ChatId(15)));
    }

    #[test]
    fn can_pin_messages_rights_gate() {
        // MED1: pin rights — private chats always; creator always; admins
        // need the explicit right; group members need the member right.
        let mut private = placeholder_chat(ChatId(20));
        private.kind = ChatKind::Private {
            user_id: crate::ids::UserId(9),
        };
        assert!(private.can_pin_messages());

        let mut creator = placeholder_chat(ChatId(21));
        creator.kind = ChatKind::Supergroup {
            supergroup_id: 21,
            is_channel: false,
        };
        creator.set_member_status(ChannelMemberStatus::Creator, None);
        assert!(creator.can_pin_messages());

        let mut admin = placeholder_chat(ChatId(22));
        admin.kind = ChatKind::BasicGroup { basic_group_id: 22 };
        admin.set_member_status(ChannelMemberStatus::Administrator, None);
        assert!(
            !admin.can_pin_messages(),
            "absent rights block keeps the gate closed"
        );
        admin.set_admin_can_pin_messages(Some(true));
        assert!(admin.can_pin_messages());
        admin.set_admin_can_pin_messages(Some(false));
        assert!(!admin.can_pin_messages());

        let mut member = placeholder_chat(ChatId(23));
        member.kind = ChatKind::BasicGroup { basic_group_id: 23 };
        member.set_member_status(ChannelMemberStatus::Member, None);
        assert!(!member.can_pin_messages(), "no permissions block → closed");
        member.permissions = Some(ChatPermissions::all());
        assert!(member.can_pin_messages());
        member.permissions.as_mut().unwrap().can_pin_messages = false;
        assert!(!member.can_pin_messages());
    }

    #[test]
    fn event_log_fetch_replaces_appends_and_dedups() {
        // Phase D3c: a first page replaces the cache; older pages append
        // in decreasing id order with duplicates dropped; a full page
        // sets `has_more`, a short one clears it.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let event = |id: i64| {
            format!(
                r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}"#
            )
        };
        let page = |extra: u64, ids: &[i64]| {
            format!(
                r#"{{"@type":"chatEvents","@extra":"{extra}","events":[{}]}}"#,
                ids.iter()
                    .map(|id| event(*id))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };

        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 0 },
            Some(ChatId(13)),
        );
        apply_json(&mut session, &seq, &sink, &page(extra.0, &[300, 299]));
        let ChatEventLogFetch::Loaded(loaded) = session.event_logs.get(&13).expect("log loaded")
        else {
            panic!("expected loaded event log");
        };
        assert_eq!(
            loaded.events.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![300, 299]
        );
        assert!(!loaded.has_more);

        // Older page appends; the overlapping id dedupes; a full page
        // (100 events) keeps `has_more`.
        let full: Vec<i64> = (200..300).rev().collect();
        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 299 },
            Some(ChatId(13)),
        );
        apply_json(&mut session, &seq, &sink, &page(extra.0, &full));
        let ChatEventLogFetch::Loaded(loaded) = session.event_logs.get(&13).expect("log loaded")
        else {
            panic!("expected loaded event log");
        };
        let ids: Vec<i64> = loaded.events.iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 101);
        assert_eq!(ids[0], 300);
        assert_eq!(ids[100], 200);
        assert!(loaded.has_more);

        // A short final page clears `has_more`.
        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 200 },
            Some(ChatId(13)),
        );
        apply_json(&mut session, &seq, &sink, &page(extra.0, &[199]));
        let ChatEventLogFetch::Loaded(loaded) = session.event_logs.get(&13).expect("log loaded")
        else {
            panic!("expected loaded event log");
        };
        assert_eq!(loaded.events.len(), 102);
        assert!(!loaded.has_more);

        // A first-page refetch replaces everything (refresh semantics).
        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 0 },
            Some(ChatId(13)),
        );
        apply_json(&mut session, &seq, &sink, &page(extra.0, &[500]));
        let ChatEventLogFetch::Loaded(loaded) = session.event_logs.get(&13).expect("log loaded")
        else {
            panic!("expected loaded event log");
        };
        assert_eq!(
            loaded.events.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![500]
        );
    }

    #[test]
    fn event_log_failure_states() {
        // Phase D3c: a failed first page becomes `Failed`; a failed
        // "load more" keeps the loaded page so the retry button stays.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 0 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        let ChatEventLogFetch::Failed(message) = session.event_logs.get(&13).expect("log failed")
        else {
            panic!("expected failed event log");
        };
        assert!(message.contains("Could not load recent actions"));

        // Load one page, then fail the older page: the loaded page stays.
        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 0 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatEvents","@extra":"{}","events":[{{"@type":"chatEvent","id":50,"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}]}}"#,
                extra.0
            ),
        );
        let extra = session.request(
            RequestPurpose::GetChatEventLog { from_event_id: 50 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
                extra.0
            ),
        );
        let ChatEventLogFetch::Loaded(loaded) = session.event_logs.get(&13).expect("log loaded")
        else {
            panic!("expected loaded event log");
        };
        assert_eq!(loaded.events.len(), 1);
    }

    #[test]
    fn event_log_relative_time_buckets() {
        // Phase D3c: the log only covers 48h, so relative buckets suffice.
        let now = 1_700_000_000i64;
        assert_eq!(event_log_relative_time_for(now as i32 - 5, now), "just now");
        assert_eq!(event_log_relative_time_for(now as i32 - 90, now), "1m ago");
        assert_eq!(
            event_log_relative_time_for(now as i32 - 3599, now),
            "59m ago"
        );
        assert_eq!(
            event_log_relative_time_for(now as i32 - 3600, now),
            "1h ago"
        );
        assert_eq!(
            event_log_relative_time_for(now as i32 - 86_399, now),
            "23h ago"
        );
        assert_eq!(
            event_log_relative_time_for(now as i32 - 86_400, now),
            "1d ago"
        );
        assert_eq!(
            event_log_relative_time_for(now as i32 - 172_800, now),
            "2d ago"
        );
    }

    #[test]
    fn admin_list_fetch_caches() {
        // Phase D3b: `getChatAdministrators` loads and caches the result;
        // a stale `@extra` is ignored.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatAdministrators, Some(ChatId(13)));

        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatAdministrators","@extra":"{}","administrators":[{{"@type":"chatAdministrator","user_id":777,"custom_title":"","is_owner":true,"can_be_edited":false}},{{"@type":"chatAdministrator","user_id":888,"custom_title":"News Desk","is_owner":false,"can_be_edited":true}}]}}"#,
                extra.0
            ),
        );

        let AdminListFetch::Loaded(admins) = session.admin_lists.get(&13).unwrap() else {
            panic!("admin list was not loaded");
        };
        assert_eq!(admins.len(), 2);
        assert!(admins[0].is_owner);
        assert_eq!(admins[1].custom_title, "News Desk");
        assert!(admins[1].can_be_edited);

        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"chatAdministrators","@extra":"99999","administrators":[]}"#,
        );
        let AdminListFetch::Loaded(admins) = session.admin_lists.get(&13).unwrap() else {
            panic!("admin list was not loaded");
        };
        assert_eq!(admins.len(), 2);
    }

    #[test]
    fn set_chat_member_status_ok_invalidates_admin_list() {
        // Phase D3b: a confirmed promote/demote/edit drops the cached admin
        // list so the panel refetches; the change itself arrives as
        // `updateChatMember`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.admin_lists.insert(
            13,
            AdminListFetch::Loaded(vec![ChatAdministratorEntry {
                user_id: 888,
                custom_title: String::new(),
                is_owner: false,
                can_be_edited: true,
            }]),
        );
        let extra = session.request(
            RequestPurpose::SetChatMemberStatus {
                user_id: 888,
                kind: MemberStatusChange::Demote,
            },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(!session.admin_lists.contains_key(&13));
    }

    #[test]
    fn update_chat_member_invalidates_admin_list_and_own_rights() {
        // Phase D3b: `updateChatMember` (schema 1.8.67, line 11202)
        // invalidates a cached admin list, and refreshes the viewer's own
        // `can_promote_members` when the member is the current user.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.my_user_id = Some(777);
        let mut channel = placeholder_chat(ChatId(13));
        channel.kind = ChatKind::Supergroup {
            supergroup_id: 13,
            is_channel: true,
        };
        session.chats.insert(13, channel);
        session.admin_lists.insert(
            13,
            AdminListFetch::Loaded(vec![ChatAdministratorEntry {
                user_id: 888,
                custom_title: String::new(),
                is_owner: false,
                can_be_edited: true,
            }]),
        );

        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":777,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":888},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":888},"status":{"@type":"chatMemberStatusMember","member_until_date":0}}}"#,
        );
        assert!(!session.admin_lists.contains_key(&13));

        // Own promotion to administrator with the promote right.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatMember","chat_id":13,"actor_user_id":777,"date":1,"invite_link":null,"via_join_request":false,"via_chat_folder_invite_link":false,"old_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusMember","member_until_date":0}},"new_chat_member":{"@type":"chatMember","member_id":{"@type":"messageSenderUser","user_id":777},"status":{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_promote_members":true}}}}"#,
        );
        let chat = session.chats.get(&13).unwrap();
        assert_eq!(
            chat.my_member_status,
            Some(ChannelMemberStatus::Administrator)
        );
        assert_eq!(chat.my_admin_can_promote_members, Some(true));
        assert!(chat.can_manage_admins());
    }

    #[test]
    fn get_admin_rights_response_caches_rights() {
        // Phase D3b: `getChatMember` tagged `GetAdminRights` stores the
        // administrator's rights for the edit dialog.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::GetAdminRights { user_id: 888 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatMember","@extra":"{}","member_id":{{"@type":"messageSenderUser","user_id":888}},"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{{"@type":"chatAdministratorRights","can_promote_members":true,"can_delete_messages":true}}}}}}"#,
                extra.0
            ),
        );
        let AdminRightsFetch::Loaded(rights) = session.admin_rights.get(&(13, 888)).unwrap() else {
            panic!("admin rights were not loaded");
        };
        assert!(rights.can_promote_members);
        assert!(rights.can_delete_messages);
        assert!(!rights.can_pin_messages);
    }

    #[test]
    fn supergroup_members_fetch_caches() {
        // Phase D3b: `getSupergroupMembers` loads the member-picker page.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::GetSupergroupMembers {
                filter: MemberListFilter::Recent,
            },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatMembers","@extra":"{}","total_count":2,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":111}},"tag":"","inviter_user_id":777,"joined_chat_date":1700000000,"status":{{"@type":"chatMemberStatusMember","member_until_date":0}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":222}},"tag":"","inviter_user_id":777,"joined_chat_date":1700000000,"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":true}}}}]}}"#,
                extra.0
            ),
        );
        let SupergroupMembersFetch::Loaded {
            members,
            total_count,
        } = session
            .supergroup_members
            .get(&(13, MemberListFilter::Recent))
            .unwrap()
        else {
            panic!("members were not loaded");
        };
        assert_eq!(*total_count, 2);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].status, ChannelMemberStatus::Member);
        assert_eq!(members[1].status, ChannelMemberStatus::Administrator);
    }

    #[test]
    fn basic_group_full_info_caches_members() {
        // Slice G1: `getBasicGroupFullInfo` loads the basic-group
        // member list into `basic_group_members`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetBasicGroupFullInfo, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"basicGroupFullInfo","@extra":"{}","creator_user_id":7,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":7}},"tag":"boss","status":{{"@type":"chatMemberStatusCreator"}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":8}},"tag":"","status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
                extra.0
            ),
        );
        let SupergroupMembersFetch::Loaded {
            members,
            total_count,
        } = session.basic_group_members.get(&13).unwrap()
        else {
            panic!("basic-group members were not loaded");
        };
        assert_eq!(*total_count, 2);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].status, ChannelMemberStatus::Creator);
        // Slice G1: `chatMember.tag` (schema 1.8.67, line 2526) is the
        // admin custom title.
        assert_eq!(members[0].tag, "boss");
        assert_eq!(members[1].status, ChannelMemberStatus::Member);
        assert_eq!(members[1].tag, "");
    }

    /// Phase C2h: group-call message updates route to the tracked call —
    /// new messages append (deduped), deletions remove them, and updates
    /// for other call ids are ignored.
    #[test]
    fn group_call_messages_route_to_tracked_call() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.active_group_call = Some(ActiveGroupCall::fresh(555));
        let new_message = |id: i32, call: i32| {
            format!(
                r#"{{"@type":"updateNewGroupCallMessage","group_call_id":{call},"message":{{"@type":"groupCallMessage","message_id":{id},"sender_id":{{"@type":"messageSenderUser","user_id":41}},"date":1788000000,"text":{{"@type":"formattedText","text":"hello {id}","entities":[]}},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":true}}}}"#
            )
        };
        apply_json(&mut session, &seq, &sink, &new_message(1, 555));
        apply_json(&mut session, &seq, &sink, &new_message(2, 999));
        let messages = &session.active_group_call.as_ref().unwrap().messages;
        assert_eq!(messages.len(), 1, "foreign call id must not append");
        assert_eq!(messages[0].message_id, 1);
        assert_eq!(messages[0].text, "hello 1");
        // Re-delivery of the same id dedupes rather than duplicating.
        apply_json(&mut session, &seq, &sink, &new_message(1, 555));
        assert_eq!(
            session.active_group_call.as_ref().unwrap().messages.len(),
            1
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateGroupCallMessagesDeleted","group_call_id":555,"message_ids":[1]}"#,
        );
        assert!(
            session
                .active_group_call
                .as_ref()
                .unwrap()
                .messages
                .is_empty()
        );
    }

    /// Phase C2h: a scheduled (not yet active) video chat tracks its
    /// `scheduled_start_date` so the UI can show the start time.
    #[test]
    fn scheduled_group_call_tracks_start_date() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateGroupCall","group_call":{"@type":"groupCall","id":555,"unique_id":"999","title":"Planned sync","invite_link":"","paid_message_star_count":0,"scheduled_start_date":1788003600,"enabled_start_notification":true,"is_active":false,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":true,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":true,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}"#,
        );
        let call = session.active_group_call.as_ref().expect("tracked");
        assert_eq!(call.scheduled_start_date, 1788003600);
        // `enabled_start_notification` (:7154) rides the same update.
        assert!(call.enabled_start_notification);
        assert!(!call.is_joined);
    }

    /// Phase C2h: the `rtmpUrl` answer caches on the tracked call whose
    /// chat the request targeted.
    #[test]
    fn rtmp_url_answer_caches_on_tracked_call() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let mut chat = placeholder_chat(ChatId(51));
        chat.video_chat = Some(VideoChatInfo {
            group_call_id: 555,
            has_participants: false,
        });
        session.chats.insert(51, chat);
        session.active_group_call = Some(ActiveGroupCall::fresh(555));
        let extra = session.request(RequestPurpose::GetVideoChatRtmpUrl { chat_id: 51 }, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"rtmpUrl","@extra":"{}","url":"rtmp://dc1-rtmp.telegram.org:443/live","stream_key":"secret-key"}}"#,
                extra.0
            ),
        );
        let call = session.active_group_call.as_ref().unwrap();
        assert_eq!(
            call.rtmp_url.as_deref(),
            Some("rtmp://dc1-rtmp.telegram.org:443/live")
        );
        assert_eq!(call.rtmp_stream_key.as_deref(), Some("secret-key"));
    }

    /// Phase C2i: `searchCallMessages` pages accumulate in
    /// `recent_calls` newest-first and the server `next_offset` is
    /// kept for "Load more".
    #[test]
    fn call_history_pages_accumulate_and_track_offset() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::SearchCallMessages, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":2,"next_offset":"page2","messages":[{{"@type":"message","id":901,"chat_id":71,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageCall","unique_id":901,"is_video":true,"discard_reason":{{"@type":"callDiscardReasonHungUp"}},"duration":372}}}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.recent_calls.len(), 1);
        assert_eq!(session.recent_calls_offset, "page2");
        assert!(!session.recent_calls_loading);
        assert!(!session.recent_calls_error);
        let extra = session.request(RequestPurpose::SearchCallMessages, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":2,"next_offset":"","messages":[{{"@type":"message","id":900,"chat_id":71,"is_outgoing":false,"date":1699999999,"content":{{"@type":"messageCall","unique_id":900,"is_video":false,"discard_reason":{{"@type":"callDiscardReasonMissed"}},"duration":0}}}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.recent_calls.len(), 2);
        assert_eq!(session.recent_calls_offset, "");
        assert_eq!(session.recent_calls[0].id.0, 901);
        assert_eq!(session.recent_calls[1].id.0, 900);
    }

    /// Phase C2i: `getUserPrivacySettingRules` maps the rule list to
    /// the simple choice; a failed set clears the optimistic value
    /// and flags the error so the UI shows it.
    #[test]
    fn call_privacy_get_maps_rules_and_set_failure_clears() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::GetCallPrivacyRules {
                setting: CallPrivacySetting::AllowCalls,
            },
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleAllowContacts"}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.call_privacy_allow_calls, Some(PrivacyWho::Contacts));
        assert!(!session.call_privacy_error);

        // Optimistic set, then a TDLib error: the optimistic value is
        // cleared (the next fetch restores the truth) and the error
        // flag is set.
        session.call_privacy_allow_calls = Some(PrivacyWho::Nobody);
        let extra = session.request(
            RequestPurpose::SetCallPrivacyRules {
                setting: CallPrivacySetting::AllowCalls,
            },
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"PRIVACY_TOO_LONG"}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.call_privacy_allow_calls, None);
        assert!(session.call_privacy_error);
    }

    #[test]
    fn call_privacy_loading_clears_only_after_both_gets_land() {
        // `fetch_call_privacy` fires two gets (AllowCalls + PeerToPeer):
        // clearing on the first would briefly render the radios with
        // nothing selected instead of "Loading…".
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.call_privacy_loading = true;
        session.call_privacy_pending = 2;
        for setting in [
            CallPrivacySetting::AllowCalls,
            CallPrivacySetting::PeerToPeer,
        ] {
            let extra = session.request(RequestPurpose::GetCallPrivacyRules { setting }, None);
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleAllowAll"}}]}}"#,
                    extra.0,
                ),
            );
            if setting == CallPrivacySetting::AllowCalls {
                assert!(session.call_privacy_loading);
                assert_eq!(session.call_privacy_p2p, None);
            }
        }
        assert!(!session.call_privacy_loading);
        assert_eq!(
            session.call_privacy_allow_calls,
            Some(PrivacyWho::Everybody)
        );
        assert_eq!(session.call_privacy_p2p, Some(PrivacyWho::Everybody));
    }

    #[test]
    fn g2_update_supergroup_caches_sign_flags_and_rights() {
        // Slice G2: `updateSupergroup` carries `sign_messages` /
        // `show_message_sender` plus the new admin rights; the session
        // maps feed the channel manage dialog and its gates.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":25,"is_channel":false},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"sign_messages":true,"show_message_sender":true,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_change_info":true,"can_manage_topics":true,"can_send_welcome_messages":true}}}}"#,
        );
        assert_eq!(session.supergroup_sign_messages.get(&25), Some(&true));
        assert_eq!(session.supergroup_show_message_sender.get(&25), Some(&true));
        assert_eq!(session.supergroup_manage_topics_right.get(&25), Some(&true));
        assert_eq!(session.supergroup_change_info_right.get(&25), Some(&true));
        assert_eq!(session.supergroup_send_welcome_right.get(&25), Some(&true));
        assert!(session.chat_can_manage_topics(ChatId(13)));
        assert!(session.chat_can_change_info(ChatId(13)));
        assert!(session.chat_can_send_welcome_messages(ChatId(13)));
        assert!(session.chat_sign_messages(ChatId(13)));
        assert!(session.chat_show_message_sender(ChatId(13)));
        // A member without the rights is gated out.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":25,"status":{"@type":"chatMemberStatusMember"}}}"#,
        );
        assert!(!session.chat_can_manage_topics(ChatId(13)));
        assert!(!session.chat_can_change_info(ChatId(13)));
    }

    #[test]
    fn g2_update_supergroup_full_info_caches_anti_spam() {
        // Slice G2: `updateSupergroupFullInfo` carries the anti-spam
        // state + capability; the toggle is gated on the capability.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":25,"is_channel":false},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":25,"supergroup_full_info":{"@type":"supergroupFullInfo","has_aggressive_anti_spam_enabled":true,"can_toggle_aggressive_anti_spam":true}}"#,
        );
        assert!(session.chat_anti_spam_enabled(ChatId(13)));
        assert!(session.chat_can_toggle_anti_spam(ChatId(13)));
    }

    #[test]
    fn g2_welcome_pack_and_flag_cached() {
        // Slice G2: the welcome pack and the `has_welcome_messages` flag
        // land in their caches (pack via the spontaneous update, flag
        // via both the update and `updateNewChat`).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatWelcomeMessages","chat_id":13,"messages":[{"id":7,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"welcome","entities":[]}}}]}"#,
        );
        let pack = session.welcome_messages.get(&13).expect("welcome pack");
        assert_eq!(pack.len(), 1);
        assert_eq!(pack[0].id, 7);
        assert_eq!(
            session.welcome_message_fetches.get(&13),
            Some(&WelcomeMessagesFetch::Loaded)
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatHasWelcomeMessages","chat_id":13,"has_welcome_messages":true}"#,
        );
        assert!(session.chat_has_welcome_messages_flag(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"g","type":{"@type":"chatTypeSupergroup","supergroup_id":26,"is_channel":false},"unread_count":0,"has_welcome_messages":true}}"#,
        );
        assert!(session.chat_has_welcome_messages_flag(ChatId(14)));
    }

    #[test]
    fn g2_chat_boost_status_cached() {
        // Slice G2: the `getChatBoostStatus` answer is cached per chat.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatBoostStatus","@extra":"{}","level":3,"boost_count":42}}"#,
                extra.0
            ),
        );
        assert_eq!(session.chat_boost_status.get(&13), Some(&(3, 42)));
    }

    #[test]
    fn g2_boost_slots_stashed_for_chain() {
        // Slice G2: the slots answer is stashed per chat so the driver
        // can chain `boostChat`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetBoostSlotsForBoost, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chatBoostSlots","@extra":"{}","slots":[{{"slot_id":3}},{{"slot_id":7}}]}}"#,
                extra.0
            ),
        );
        assert_eq!(session.boost_slots_by_chat.get(&13), Some(&vec![3, 7]));
    }

    #[test]
    fn g2_thread_history_cached_and_failed() {
        // Slice G2: `getMessageThreadHistory` success caches the thread;
        // failure marks it failed (the viewer shows an error, not a
        // spinner).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::GetMessageThreadHistory { message_id: 99 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[],"total_count":0}}"#,
                extra.0
            ),
        );
        let thread = session.comment_thread.as_ref().expect("comment thread");
        assert_eq!(thread.chat_id, ChatId(13));
        assert_eq!(thread.message_id, MessageId(99));
        assert_eq!(thread.failed, None);
        let extra = session.request(
            RequestPurpose::GetMessageThreadHistory { message_id: 100 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        let thread = session.comment_thread.as_ref().expect("comment thread");
        assert_eq!(thread.message_id, MessageId(100));
        assert!(
            thread
                .failed
                .as_ref()
                .unwrap()
                .contains("Could not load comments")
        );
    }

    #[test]
    fn cl_chat_preview_cached_for_unopened_chat() {
        // Slice CL: a `getChatHistory` answer for `GetChatPreview` is
        // retained under the requested chat even when that chat is not
        // open (the normal history branch drops non-open answers);
        // nothing leaks into the chat's history. A failure marks the
        // preview failed so the panel shows an error, not a spinner.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetChatPreview, Some(ChatId(12)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"@type":"message","id":7,"chat_id":12,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}],"total_count":1}}"#,
                extra.0
            ),
        );
        let fetch = session.chat_preview_fetch.as_ref().expect("preview fetch");
        assert_eq!(fetch.chat_id, ChatId(12));
        assert_eq!(fetch.messages.len(), 1);
        assert_eq!(fetch.failed, None);
        assert!(
            session
                .histories
                .get(&12)
                .is_none_or(|history| history.ordered().is_empty()),
            "preview must not merge into the chat's history"
        );
        let extra = session.request(RequestPurpose::GetChatPreview, Some(ChatId(12)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"SOMETHING"}}"#,
                extra.0
            ),
        );
        let fetch = session.chat_preview_fetch.as_ref().expect("preview fetch");
        assert!(
            fetch
                .failed
                .as_ref()
                .unwrap()
                .contains("Could not load preview")
        );
    }

    #[test]
    fn g2_sign_toggle_error_rolls_back() {
        // Slice G2: the optimistic sign-messages toggle restores the
        // previous flags when TDLib answers `error`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.supergroup_sign_messages.insert(25, false);
        session.supergroup_show_message_sender.insert(25, false);
        let extra = session.request(
            RequestPurpose::ToggleSupergroupSignMessages,
            Some(ChatId(13)),
        );
        session
            .requests
            .pending_mut(extra)
            .expect("pending")
            .rollback = Some(RequestRollback::SignMessages {
            supergroup_id: 25,
            previous_sign: Some(false),
            previous_show: Some(false),
        });
        session.supergroup_sign_messages.insert(25, true);
        session.supergroup_show_message_sender.insert(25, true);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        assert_eq!(session.supergroup_sign_messages.get(&25), Some(&false));
        assert_eq!(
            session.supergroup_show_message_sender.get(&25),
            Some(&false)
        );
    }

    #[test]
    fn g2_anti_spam_toggle_error_rolls_back() {
        // Slice G2: the optimistic anti-spam toggle restores the previous
        // flag when TDLib answers `error`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.supergroup_anti_spam_enabled.insert(25, false);
        let extra = session.request(
            RequestPurpose::ToggleSupergroupAggressiveAntiSpam,
            Some(ChatId(13)),
        );
        session
            .requests
            .pending_mut(extra)
            .expect("pending")
            .rollback = Some(RequestRollback::AntiSpam {
            supergroup_id: 25,
            previous: Some(false),
        });
        session.supergroup_anti_spam_enabled.insert(25, true);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        assert_eq!(session.supergroup_anti_spam_enabled.get(&25), Some(&false));
    }

    #[test]
    fn g2_welcome_fetch_error_marks_failed() {
        // Slice G2: a failed `loadChatWelcomeMessages` marks the fetch
        // failed so the dialog shows an error, not a spinner.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::LoadChatWelcomeMessages, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.welcome_message_fetches.get(&13),
            Some(WelcomeMessagesFetch::Failed(_))
        ));
    }

    #[test]
    fn cl1_pin_error_rolls_back_and_surfaces() {
        // Slice CL1: the optimistic pin restores the previous flag when
        // TDLib answers `error`, and the refusal surfaces in
        // `chat_action_error` (drained by the UI) — never silent.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.chats.insert(11, placeholder_chat(ChatId(11)));
        let extra = session.request(RequestPurpose::ToggleChatIsPinned, Some(ChatId(11)));
        session
            .requests
            .pending_mut(extra)
            .expect("pending")
            .rollback = Some(RequestRollback::ChatPin {
            previous: false,
            archived: false,
        });
        session.chats.get_mut(&11).expect("chat").is_pinned = true;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"PINNED_CHATS_LIMIT_EXCEEDED"}}"#,
                extra.0
            ),
        );
        assert!(!session.chats.get(&11).expect("chat").is_pinned);
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not pin the chat (error 400)")
        );
    }

    #[test]
    fn cl1_marked_as_unread_update_and_rollback() {
        // Slice CL1: `updateChatIsMarkedAsUnread` flips the row badge
        // flag, and a refused toggle restores it.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.chats.insert(12, placeholder_chat(ChatId(12)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatIsMarkedAsUnread","chat_id":12,"is_marked_as_unread":true}"#,
        );
        assert!(session.chats.get(&12).expect("chat").is_marked_as_unread);
        let extra = session.request(RequestPurpose::ToggleChatIsMarkedAsUnread, Some(ChatId(12)));
        session
            .requests
            .pending_mut(extra)
            .expect("pending")
            .rollback = Some(RequestRollback::ChatMarkedAsUnread { previous: true });
        session
            .chats
            .get_mut(&12)
            .expect("chat")
            .is_marked_as_unread = false;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        assert!(session.chats.get(&12).expect("chat").is_marked_as_unread);
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not change read state (error 400)")
        );
    }

    #[test]
    fn cl3_is_unread_guards_bulk_mark_read() {
        // CL3 review blocker: `mark_selected_read` skips chats with no
        // unread state (the single-chat path is a genuine toggle — on a
        // fully-read chat it would mark it *unread*).
        let read = placeholder_chat(ChatId(11));
        assert!(!read.is_unread());
        let mut with_unread = placeholder_chat(ChatId(12));
        with_unread.unread_count = 3;
        assert!(with_unread.is_unread());
        let mut marked = placeholder_chat(ChatId(13));
        marked.is_marked_as_unread = true;
        assert!(marked.is_unread());
    }

    #[test]
    fn cl1_clear_history_error_surfaces() {
        // Slice CL1: a refused `deleteChatHistory` surfaces in
        // `chat_action_error`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::DeleteChatHistory, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_HISTORY_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not clear history (error 400)")
        );
    }

    #[test]
    fn cl1_pin_limit_options_tracked() {
        // Slice CL1: `updateOption` for the pin limits (schema 1.8.67,
        // line 13674) feeds the client-side pin pre-check.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        assert_eq!(session.pinned_chat_count_max, 5);
        assert_eq!(session.pinned_archived_chat_count_max, 100);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateOption","name":"pinned_chat_count_max","value":{"@type":"optionValueInteger","value":10}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateOption","name":"pinned_archived_chat_count_max","value":{"@type":"optionValueInteger","value":200}}"#,
        );
        assert_eq!(session.pinned_chat_count_max, 10);
        assert_eq!(session.pinned_archived_chat_count_max, 200);
    }

    #[test]
    fn cl3_mention_reaction_counts_parse_and_update() {
        // Slice CL3: `chat.unread_mention_count` /
        // `chat.unread_reaction_count` (schema 1.8.67, lines 3611-3612)
        // and the updates (lines 10567/10570) feed the @ / ♥ badges.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Mentions","type":{"@type":"chatTypePrivate","user_id":14},"unread_count":3,"unread_mention_count":2,"unread_reaction_count":1,"can_be_reported":true}}"#,
        );
        let chat = session.chats.get(&14).expect("chat");
        assert_eq!(chat.unread_mention_count, 2);
        assert_eq!(chat.unread_reaction_count, 1);
        assert!(chat.can_be_reported);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatUnreadMentionCount","chat_id":14,"unread_mention_count":0}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatUnreadReactionCount","chat_id":14,"unread_reaction_count":0}"#,
        );
        let chat = session.chats.get(&14).expect("chat");
        assert_eq!(chat.unread_mention_count, 0);
        assert_eq!(chat.unread_reaction_count, 0);
    }

    #[test]
    fn cl3_report_chat_result_ok_and_more_info() {
        // Slice CL3: `reportChatResultOk` → "chat reported";
        // `reportChatResultOptionRequired` (and its siblings) → the
        // honest "more info required" note, never success.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::ReportChat, Some(ChatId(14)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"reportChatResultOk","@extra":"{}"}}"#, extra.0),
        );
        assert_eq!(
            session.report_chat_outcome.as_deref(),
            Some("chat reported")
        );

        let extra = session.request(RequestPurpose::ReportChat, Some(ChatId(14)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"reportChatResultOptionRequired","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.report_chat_outcome.as_deref(),
            Some(
                "report needs a reason or messages — the chat list only sends simple spam reports"
            )
        );
    }

    #[test]
    fn cl3_block_list_update_sets_blocked() {
        // Slice CL3: `updateChatBlockList` (schema 1.8.67, line 10594)
        // tracks the peer's block state.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.chats.insert(14, placeholder_chat(ChatId(14)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatBlockList","chat_id":14,"block_list":{"@type":"blockListMain"}}"#,
        );
        assert!(session.chats.get(&14).expect("chat").blocked);
    }

    #[test]
    fn cl3_report_and_block_errors_surface() {
        // Slice CL3: a refused `reportChat` /
        // `setMessageSenderBlockList` surfaces in `chat_action_error` —
        // never shown as success.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::ReportChat, Some(ChatId(14)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_REPORT_FAILED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not report the chat (error 400)")
        );
        let extra = session.request(
            RequestPurpose::SetMessageSenderBlockList { block: true },
            Some(ChatId(14)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":403,"message":"FORBIDDEN"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not change the block state (error 403)")
        );
    }

    #[test]
    fn a6_imported_contacts_response_invalidates_and_notices() {
        // Slice A6: `importContacts` answers `importedContacts`
        // (schema 1.8.67, line 14517), NOT `ok` — the reducer still
        // invalidates the list and records the notice.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.contacts = Some(vec![31]);
        let extra = session.request(RequestPurpose::ImportContacts, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"importedContacts","@extra":"{}","user_ids":[31,32],"importer_count":[2]}}"#,
                extra.0
            ),
        );
        assert!(session.contacts.is_none());
        assert_eq!(
            session.contacts_notice.as_deref(),
            Some("Contacts imported.")
        );
    }

    #[test]
    fn a6_remove_contact_ok_clears_cached_is_contact() {
        // Slice A6: a confirmed `removeContacts` drops the contact flag
        // on the cached user (in addition to invalidating the list) so
        // the info panel stops offering "Delete contact".
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"+15550101031","status":{"@type":"userStatusRecently"},"is_contact":true,"type":{"@type":"userTypeRegular"}}}"#,
        );
        assert!(session.users.get(&31).expect("user").is_contact);
        session.contacts = Some(vec![31]);
        let extra = session.request_for_user(RequestPurpose::RemoveContact, 31);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(session.contacts.is_none());
        assert!(!session.users.get(&31).expect("user").is_contact);
        assert_eq!(session.contacts_notice.as_deref(), Some("Contact deleted."));
    }

    #[test]
    fn a6_block_ok_updates_cached_blocked() {
        // Slice A6: a user-scoped `setMessageSenderBlockList` `ok`
        // carries no state, but the confirmed request does — the cached
        // `UserFullInfoData.blocked` flips authoritatively (never
        // optimistically).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"bio":null,"bot_info":null}}"#,
                extra.0
            ),
        );
        assert!(!session.user_full_infos.get(&31).expect("info").blocked);
        let extra = session.request_for_user(
            RequestPurpose::SetMessageSenderBlockList { block: true },
            31,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(session.user_full_infos.get(&31).expect("info").blocked);
    }

    #[test]
    fn b2_bot_start_link_parser_only_accepts_start_links() {
        // Slice B2: `t.me/<bot>?start=<param>` parses to
        // `internalLinkTypeBotStart`'s two pieces (schema 1.8.67, line
        // 9399); anything else is not a bot-start link.
        assert_eq!(
            parse_bot_start_link("https://t.me/demo_bot?start=demo_xyz"),
            Some(("demo_bot".into(), "demo_xyz".into()))
        );
        assert_eq!(
            parse_bot_start_link("t.me/demo_bot?start=demo_xyz&foo=bar"),
            Some(("demo_bot".into(), "demo_xyz".into()))
        );
        assert_eq!(parse_bot_start_link("https://t.me/demo_bot?start="), None);
        assert_eq!(parse_bot_start_link("https://t.me/demo_bot"), None);
        assert_eq!(
            parse_bot_start_link("https://t.me/demo_bot?startattach=x"),
            None
        );
        assert_eq!(
            parse_bot_start_link("https://t.me/s/demo_bot?start=x"),
            None
        );
        assert_eq!(
            parse_bot_start_link("https://example.com/demo_bot?start=x"),
            None
        );
    }

    #[test]
    fn b2_similar_bots_users_response_lands_by_pending_user() {
        // Slice B2: a `users` answer to our `getBotSimilarBots` fetch is
        // keyed by the pending request's `user_id`; strangers' `users`
        // payloads don't touch it.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_for_user(RequestPurpose::GetBotSimilarBots, 21);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,32]}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.similar_bots.get(&21),
            Some(SimilarBotsFetch::Loaded(ids)) if ids == &[31, 32]
        ));
    }

    #[test]
    fn b2_start_and_similar_bots_errors_surface() {
        // Slice B2: a refused `sendBotStartMessage` / `getBotSimilarBots`
        // surfaces in `chat_action_error` — never shown as success.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::SendBotStartMessage, Some(ChatId(21)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BOT_START_FAILED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not start the bot (error 400)")
        );
        let extra = session.request_for_user(RequestPurpose::GetBotSimilarBots, 21);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":403,"message":"FORBIDDEN"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not load similar bots (error 403)")
        );
    }

    #[test]
    fn cl2_reorder_pinned_chats_swaps_orders() {
        // Slice CL2: `reorder_pinned_chats` permutes the pinned chats'
        // `order` values into the new sequence so `rebuild_main_order`
        // keeps it; the returned pairs restore the old arrangement.
        let (mut session, _sink) = session();
        for (id, order) in [(11i64, 300i64), (12, 200), (13, 100), (14, 50)] {
            let mut chat = placeholder_chat(ChatId(id));
            chat.in_main_list = true;
            chat.order = order;
            chat.is_pinned = id != 14;
            session.chats.insert(id, chat);
        }
        session.rebuild_main_order();
        let previous = session.reorder_pinned_chats(false, &[13, 11, 12]);
        assert_eq!(previous, vec![(11, 300), (12, 200), (13, 100)]);
        let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![13, 11, 12, 14]);
        // Id-set mismatch changes nothing.
        let noop = session.reorder_pinned_chats(false, &[13, 11]);
        assert!(noop.is_empty());
        let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![13, 11, 12, 14]);
    }

    #[test]
    fn cl2_pin_order_error_rolls_back_and_surfaces() {
        // Slice CL2: a refused `setPinnedChats` restores the pre-reorder
        // order values and surfaces in `chat_action_error`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        for (id, order) in [(11i64, 300i64), (12, 200)] {
            let mut chat = placeholder_chat(ChatId(id));
            chat.in_main_list = true;
            chat.order = order;
            chat.is_pinned = true;
            session.chats.insert(id, chat);
        }
        let extra = session.request(RequestPurpose::SetPinnedChats, None);
        session
            .requests
            .pending_mut(extra)
            .expect("pending")
            .rollback = Some(RequestRollback::ChatPinOrder {
            previous: vec![(11, 300), (12, 200)],
            archived: false,
        });
        session.reorder_pinned_chats(false, &[12, 11]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        assert_eq!(session.chats.get(&11).expect("chat").order, 300);
        assert_eq!(session.chats.get(&12).expect("chat").order, 200);
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not reorder pinned chats (error 400)")
        );
    }

    #[test]
    fn cl2_archive_settings_fetch_stores_and_set_rolls_back() {
        // Slice CL2: the `getArchiveChatListSettings` answer lands in
        // the session (purpose-matched); a refused
        // `setArchiveChatListSettings` restores the previous settings
        // and surfaces.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetArchiveChatListSettings, None);
        session.archive_settings_loading = true;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"archiveChatListSettings","@extra":"{}","archive_and_mute_new_chats_from_unknown_users":true,"keep_unmuted_chats_archived":false,"keep_chats_from_folders_archived":true}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.archive_chat_list_settings,
            Some(ArchiveChatListSettings {
                archive_and_mute_new_chats_from_unknown_users: true,
                keep_unmuted_chats_archived: false,
                keep_chats_from_folders_archived: true,
            })
        );
        assert!(!session.archive_settings_loading);

        let old = session.archive_chat_list_settings;
        let extra = session.request(RequestPurpose::SetArchiveChatListSettings, None);
        session
            .requests
            .pending_mut(extra)
            .expect("pending")
            .rollback = Some(RequestRollback::ArchiveChatListSettings { previous: old });
        session.archive_chat_list_settings = Some(ArchiveChatListSettings {
            keep_unmuted_chats_archived: true,
            ..old.unwrap_or_default()
        });
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"ARCHIVE_SETTINGS_INVALID"}}"#,
                extra.0
            ),
        );
        assert_eq!(session.archive_chat_list_settings, old);
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not save archive settings (error 400)")
        );
    }

    #[test]
    fn cl2_mark_all_read_and_clear_recents_errors_surface() {
        // Slice CL2: refused `readChatList` / `clearRecentlyFoundChats`
        // surface in `chat_action_error` — never silent.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::ReadChatList, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"READ_FAILED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not mark all chats as read (error 500)")
        );
        let extra = session.request(RequestPurpose::ClearRecentlyFoundChats, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"CLEAR_FAILED"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.chat_action_error.as_deref(),
            Some("could not clear recent searches (error 500)")
        );
    }

    #[test]
    fn g2_create_forum_topic_answer_invalidates_topics() {
        // Slice G2: the `createForumTopic` answer (`forumTopicInfo`)
        // drops the cached topic list so the UI refetches it.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.forum_topics.insert(13, Vec::new());
        let extra = session.request(RequestPurpose::CreateForumTopic, Some(ChatId(13)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"forumTopicInfo","@extra":"{}","chat_id":13,"forum_topic_id":5,"name":"new"}}"#,
                extra.0
            ),
        );
        assert!(!session.forum_topics.contains_key(&13));
    }

    #[test]
    fn g2_forum_mutation_ok_drops_topic_cache() {
        // Slice G2: a confirmed forum-topic mutation (`ok`) drops the
        // cached topic list so the UI refetches it.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.forum_topics.insert(13, Vec::new());
        let extra = session.request(
            RequestPurpose::DeleteForumTopic { forum_topic_id: 5 },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(!session.forum_topics.contains_key(&13));
    }

    #[test]
    fn g2_welcome_delete_ok_drops_pack() {
        // Slice G2: a confirmed welcome-message deletion (`ok`) drops
        // the cached pack so the dialog refetches it.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.welcome_messages.insert(13, Vec::new());
        session
            .welcome_message_fetches
            .insert(13, WelcomeMessagesFetch::Loaded);
        let extra = session.request(
            RequestPurpose::DeleteChatWelcomeMessage {
                welcome_message_id: 7,
            },
            Some(ChatId(13)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(!session.welcome_messages.contains_key(&13));
        assert!(!session.welcome_message_fetches.contains_key(&13));
    }

    #[test]
    fn event_log_admin_ids_dedupes_and_skips_chat_senders() {
        let event = |id: i64, member_id: MessageSender| ParsedChatEvent {
            id,
            date: 1_700_000_000,
            member_id,
            action: ChatEventAction::MemberJoined,
        };
        let page = ChatEventLogPage {
            events: vec![
                event(1, MessageSender::User { user_id: 7 }),
                event(2, MessageSender::Chat { chat_id: 13 }),
                event(3, MessageSender::User { user_id: 9 }),
                event(4, MessageSender::User { user_id: 7 }),
            ],
            has_more: false,
        };
        assert_eq!(page.admin_user_ids(), vec![7, 9]);
    }

    #[test]
    fn auto_download_gate_respects_data_saver_and_chat_kind() {
        use crate::settings::{AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_PHOTO, AUTO_DOWNLOAD_VIDEO};
        let (mut session, _sink) = session();
        // Unknown kind → private bucket; defaults allow photos.
        let mut chat = placeholder_chat(ChatId(13));
        session.chats.insert(13, chat.clone());
        assert!(session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_PHOTO));
        assert!(!session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_VIDEO));
        // Channel bucket is independent.
        chat.kind = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        session.chats.insert(13, chat);
        session.media_prefs.auto_download_channels = AUTO_DOWNLOAD_VIDEO;
        assert!(!session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_PHOTO));
        assert!(session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_VIDEO));
        // Data saver pauses everything, regardless of bucket.
        session.media_prefs.data_saver = true;
        assert!(!session.auto_download_allowed(ChatId(13), AUTO_DOWNLOAD_VIDEO));
        session.media_prefs.data_saver = false;
        // Groups bucket: basic groups.
        let mut group = placeholder_chat(ChatId(14));
        group.kind = ChatKind::BasicGroup { basic_group_id: 2 };
        session.chats.insert(14, group);
        session.media_prefs.auto_download_groups = AUTO_DOWNLOAD_FILE;
        assert!(session.auto_download_allowed(ChatId(14), AUTO_DOWNLOAD_FILE));
        assert!(!session.auto_download_allowed(ChatId(14), AUTO_DOWNLOAD_PHOTO));
    }

    #[test]
    fn auto_download_media_ids_follow_per_type_flags() {
        // MED3: full-media auto-download honors the per-media-type flags —
        // voice on by default (TGX 0x63), video/file off; data saver and
        // spoiler/secret suppress everything.
        use crate::settings::{AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_VIDEO};
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        // Voice note (file 4), video (file 5), document (file 9).
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"mime_type":"audio/ogg","voice":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#,
                media_file_json(4, "", false),
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":21,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":10,"width":320,"height":240,"file_name":"v.mp4","mime_type":"video/mp4","has_stickers":false,"video":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#,
                media_file_json(5, "", false),
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":22,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"d.bin","mime_type":"application/octet-stream","document":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#,
                media_file_json(9, "", false),
            ),
        );
        // Defaults: voice auto-downloads; video and file do not.
        assert_eq!(session.auto_download_media_file_ids(), vec![FileId(4)]);
        // Enabling video+file for private chats picks them up.
        session.media_prefs.auto_download_private |= AUTO_DOWNLOAD_VIDEO | AUTO_DOWNLOAD_FILE;
        assert_eq!(
            session.auto_download_media_file_ids(),
            vec![FileId(4), FileId(5), FileId(9)]
        );
        // Data saver suppresses all automatic media.
        session.media_prefs.data_saver = true;
        assert!(session.auto_download_media_file_ids().is_empty());
        session.media_prefs.data_saver = false;
        // A spoiler video is never auto-downloaded.
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":23,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":10,"width":320,"height":240,"file_name":"s.mp4","mime_type":"video/mp4","has_stickers":false,"video":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":false}}}}}}"#,
                media_file_json(6, "", false),
            ),
        );
        assert_eq!(
            session.auto_download_media_file_ids(),
            vec![FileId(4), FileId(5), FileId(9)]
        );
    }

    #[test]
    fn auto_download_skips_files_over_size_cap() {
        // MED3 review: full-media auto-download skips files whose known
        // size exceeds `AUTO_DOWNLOAD_MAX_BYTES` (TGX
        // `canAutomaticallyDownload` download limit, WiFi default 50 MiB).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        // Voice note (file 4), auto-downloaded by default (TGX 0x63).
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":20,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"mime_type":"audio/ogg","voice":{}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#,
                media_file_json(4, "", false),
            ),
        );
        assert_eq!(session.auto_download_media_file_ids(), vec![FileId(4)]);
        // File 4 balloons past the cap: skipped from here on.
        session.files.get_mut(&4).unwrap().size = 100 * 1024 * 1024;
        assert!(session.auto_download_media_file_ids().is_empty());
    }

    #[test]
    fn completed_user_downloads_land_in_recent_list() {
        let (mut session, _sink) = session();
        let completed = |id: i32| ParsedFile {
            id: FileId(id),
            size: 100,
            expected_size: 100,
            local: LocalFileState {
                path: format!("/tmp/{id}.bin"),
                can_be_downloaded: true,
                is_downloading_active: false,
                is_downloading_completed: true,
                downloaded_size: 100,
            },
        };
        // User-initiated download completing → recorded.
        session.begin_download(FileId(7));
        session.user_downloads.insert(7);
        session.upsert_file(completed(7), true);
        // Automatic thumb completing → not recorded.
        session.begin_download(FileId(8));
        session.upsert_file(completed(8), true);
        assert_eq!(
            session
                .completed_downloads
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![7]
        );
        assert!(!session.downloading.contains(&7));
        assert!(!session.user_downloads.contains(&7));
        // begin_download clears a recorded failure (retry path).
        session.failed_downloads.insert(9);
        session.begin_download(FileId(9));
        assert!(!session.failed_downloads.contains(&9));
    }

    #[test]
    fn stalled_user_download_marks_failed_but_cancel_does_not() {
        // MED3: an `updateFile` that takes a user download active → idle
        // without completing is a stall — record the failure so the row
        // offers Retry. An explicit cancel (`abort_download` drops the id
        // from `user_downloads`) must not be mislabeled as a failure.
        fn update_file(id: i32, active: bool) -> String {
            format!(
                r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}}}"#,
            )
        }
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // Stalled: in-flight, then idle without completing.
        session.begin_download(FileId(12));
        session.user_downloads.insert(12);
        apply_json(&mut session, &seq, &sink, &update_file(12, true));
        assert!(!session.failed_downloads.contains(&12));
        apply_json(&mut session, &seq, &sink, &update_file(12, false));
        assert!(session.failed_downloads.contains(&12));
        assert!(!session.downloading.contains(&12));
        assert!(!session.user_downloads.contains(&12));
        // Cancelled: abort first, then the idle echo arrives.
        session.begin_download(FileId(13));
        session.user_downloads.insert(13);
        session.abort_download(FileId(13));
        apply_json(&mut session, &seq, &sink, &update_file(13, true));
        apply_json(&mut session, &seq, &sink, &update_file(13, false));
        assert!(!session.failed_downloads.contains(&13));
    }

    #[test]
    fn download_file_error_marks_failed_download() {
        // A `downloadFile` error response unsticks the download and records
        // the failure so the row can offer an honest retry — but only for
        // user-initiated downloads; an automatic (auto-download) error must
        // not surface in the Failed section.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_download(FileId(11));
        session.begin_download(FileId(11));
        session.user_downloads.insert(11);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"CANARY download failed"}}"#,
                extra.0
            ),
        );
        assert!(!session.downloading.contains(&11));
        assert!(session.failed_downloads.contains(&11));
        // Automatic download: unstuck, but not recorded as failed.
        let extra = session.request_download(FileId(12));
        session.begin_download(FileId(12));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"CANARY download failed"}}"#,
                extra.0
            ),
        );
        assert!(!session.downloading.contains(&12));
        assert!(!session.failed_downloads.contains(&12));
    }

    #[test]
    fn b1_force_reply_arms_pending_target() {
        // B1: an incoming message with `replyMarkupForceReply` arms the
        // composer's reply-to; outgoing or plain messages do not.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":306,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"x","entities":[]}}}}"#,
        );
        assert_eq!(
            session.pending_force_reply,
            Some(ForceReplyTarget {
                chat_id: ChatId(21),
                message_id: MessageId(306),
            })
        );
        // Outgoing force-reply does not arm (bots demand replies; we don't).
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":307,"chat_id":21,"is_outgoing":true,"reply_markup":{"@type":"replyMarkupForceReply","input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"y","entities":[]}}}}"#,
        );
        assert_eq!(
            session.pending_force_reply,
            Some(ForceReplyTarget {
                chat_id: ChatId(21),
                message_id: MessageId(306),
            })
        );
        // A plain incoming message leaves the armed target alone.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":308,"chat_id":21,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"z","entities":[]}}}}"#,
        );
        assert_eq!(
            session.pending_force_reply,
            Some(ForceReplyTarget {
                chat_id: ChatId(21),
                message_id: MessageId(306),
            })
        );
    }

    #[test]
    fn b1_password_callback_error_surfaces_wrong_password() {
        // B1: TDLib error 400 on a `GetCallbackQueryAnswerWithPassword`
        // request surfaces as "wrong 2-step verification password"; other
        // errors get the generic bot-timeout note.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let extra = session.request(
            RequestPurpose::GetCallbackQueryAnswerWithPassword,
            Some(ChatId(21)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"PASSWORD_HASH_INVALID"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session
                .last_callback_answer
                .as_ref()
                .map(|answer| answer.text.as_str()),
            Some("wrong 2-step verification password")
        );
        let extra = session.request(
            RequestPurpose::GetCallbackQueryAnswerWithPassword,
            Some(ChatId(21)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":502,"message":"Bad Gateway"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session
                .last_callback_answer
                .as_ref()
                .map(|answer| answer.text.as_str()),
            Some("bot did not answer")
        );
    }

    #[test]
    fn b1_login_url_info_error_degrades_to_url() {
        // B1: a refused `getLoginUrlInfo` degrades the login button to a
        // plain URL button press carrying the raw URL (schema 1.8.67 doc
        // on `getLoginUrl`). Same for a refused `getLoginUrl`.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let login_request = LoginUrlRequest {
            chat_id: ChatId(21),
            message_id: MessageId(301),
            button_id: 11,
            raw_url: "https://example.com/login".to_string(),
        };
        for purpose in [RequestPurpose::GetLoginUrlInfo, RequestPurpose::GetLoginUrl] {
            session.login_url_request = Some(login_request.clone());
            let extra = session.request(purpose, Some(ChatId(21)));
            apply_json(
                &mut session,
                &seq,
                &sink,
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":400,"message":"BUTTON_ID_INVALID"}}"#,
                    extra.0
                ),
            );
            assert_eq!(
                session.last_login_url_info,
                Some(LoginUrlInfo::Failed {
                    fallback_url: "https://example.com/login".to_string()
                })
            );
        }
    }

    #[test]
    fn b1_get_login_url_http_url_opens() {
        // B1: a `getLoginUrl` answer (`httpUrl`, schema:7458) with the
        // `GetLoginUrl` pending purpose lands as `LoginUrlInfo::Open` for
        // the UI drain.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let extra = session.request(RequestPurpose::GetLoginUrl, Some(ChatId(21)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"httpUrl","@extra":"{}","url":"https://example.com/authed"}}"#,
                extra.0
            ),
        );
        assert_eq!(
            session.last_login_url_info,
            Some(LoginUrlInfo::Open {
                url: "https://example.com/authed".to_string()
            })
        );
    }

    #[test]
    fn b1_active_custom_keyboard_rules() {
        // B1: newest `replyMarkupShowKeyboard` wins; a newer
        // `replyMarkupRemoveKeyboard` clears; a dismissed one-time
        // keyboard stays hidden.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let show = |id: i64| {
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":21,"is_outgoing":false,"reply_markup":{{"@type":"replyMarkupShowKeyboard","rows":[[{{"@type":"keyboardButton","text":"Yes","type":{{"@type":"keyboardButtonTypeText"}}}}]],"is_persistent":false,"resize_keyboard":false,"one_time":true,"is_personal":false,"force_reply":false,"input_field_placeholder":""}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
            )
        };
        let remove = |id: i64| {
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":21,"is_outgoing":false,"reply_markup":{{"@type":"replyMarkupRemoveKeyboard","is_personal":false}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
            )
        };
        apply_json(&mut session, &seq, &sink, &show(401));
        apply_json(&mut session, &seq, &sink, &show(402));
        let messages = &session.histories.get(&21).expect("history").messages;
        let none: HashSet<(i64, i64)> = HashSet::new();
        // Newest show-keyboard message wins.
        let active = active_custom_keyboard(messages, &none).expect("keyboard");
        assert_eq!(active.1, MessageId(402));
        // A dismissed one-time keyboard stays hidden.
        let dismissed: HashSet<(i64, i64)> = [(21, 402)].into_iter().collect();
        assert!(active_custom_keyboard(messages, &dismissed).is_none());
        // A newer remove-keyboard clears everything.
        apply_json(&mut session, &seq, &sink, &remove(403));
        let messages = &session.histories.get(&21).expect("history").messages;
        assert!(active_custom_keyboard(messages, &none).is_none());
    }

    /// Slice A3: the `getActiveSessions` answer replaces the cache and
    /// clears loading/error — but only for our own in-flight request
    /// (matched by `@extra`).
    #[test]
    fn sessions_answer_replaces_cache_for_matching_request() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetActiveSessions, None);
        session.sessions_loading = true;
        session.sessions_error = Some("stale".into());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"sessions","@extra":"{}","sessions":[{{"@type":"session","id":11,"is_current":true,"device_model":"Linux desktop","application_name":"Quill","application_version":"0.1","platform":"Linux","system_version":"6.8","last_active_date":1759000000,"ip_address":"1.2.3.4","location":"Austin, United States"}},{{"@type":"session","id":33,"is_current":false,"is_password_pending":true,"device_model":"Unknown","application_name":"Telegram Desktop","platform":"Windows","last_active_date":1758800000}}]}}"#,
                extra.0,
            ),
        );
        let sessions = session.sessions.as_ref().expect("cached");
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|s| s.is_current && s.id == 11));
        assert!(sessions.iter().any(|s| s.is_password_pending && s.id == 33));
        assert!(!session.sessions_loading);
        assert!(session.sessions_error.is_none());
        assert!(!session.sessions_stale);
    }

    /// Slice A3: a stray `sessions` answer (no matching pending purpose)
    /// must not clobber the cache.
    #[test]
    fn sessions_answer_ignored_without_matching_request() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sessions = Some(Vec::new());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"sessions","sessions":[{"@type":"session","id":11,"is_current":true,"device_model":"X"}]}"#,
        );
        assert_eq!(session.sessions.as_ref().unwrap().len(), 0);
    }

    /// Slice A3: a successful `terminateSession` keeps the old cache
    /// visible and marks it stale (the driver refetches the authoritative
    /// answer on the same ingest) — the row is NOT removed
    /// optimistically.
    #[test]
    fn terminate_session_ok_keeps_cache_and_marks_stale() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sessions = Some(vec![ParsedSession {
            id: 22,
            is_current: false,
            is_password_pending: false,
            can_accept_secret_chats: true,
            can_accept_calls: true,
            device_model: "iPhone".into(),
            application_name: "Telegram iOS".into(),
            application_version: "12.0".into(),
            platform: "iOS".into(),
            system_version: "18.0".into(),
            last_active_date: 1758900000,
            ip_address: "5.6.7.8".into(),
            location: "Tel Aviv, Israel".into(),
        }]);
        session.sessions_mutating = true;
        let extra = session.request(RequestPurpose::TerminateSession { session_id: 22 }, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        // The old cache stays visible until the authoritative refetch
        // replaces it — no optimistic deletion.
        assert_eq!(session.sessions.as_ref().unwrap().len(), 1);
        assert!(session.sessions_stale);
        assert!(!session.sessions_mutating);
        assert!(session.sessions_error.is_none());
    }

    /// Slice A3: a refused terminate surfaces an honest classified error
    /// and leaves the list untouched.
    #[test]
    fn terminate_session_error_surfaces_honestly() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sessions_mutating = true;
        let extra = session.request(RequestPurpose::TerminateSession { session_id: 22 }, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"SESSION_REVOKED"}}"#,
                extra.0
            ),
        );
        assert!(!session.sessions_mutating);
        assert_eq!(
            session.sessions_error.as_deref(),
            Some("Could not terminate the session: Telegram refused the request")
        );
    }

    /// Slice A3: a failed fetch clears the spinner and parks the error
    /// on the overlay.
    #[test]
    fn sessions_fetch_error_clears_loading() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sessions_loading = true;
        // A failed stale-refetch (e.g. after a terminate-ok marked the
        // cache stale) must NOT leave the cache stale — otherwise the
        // next ingest retries the fetch and flood state worsens.
        session.sessions_stale = true;
        let extra = session.request(RequestPurpose::GetActiveSessions, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":429,"message":"FLOOD_WAIT_3"}}"#,
                extra.0
            ),
        );
        assert!(!session.sessions_loading);
        // The failed stale-refetch clears staleness: no auto-retry on
        // the next ingest; the user retries via the Refresh button.
        assert!(!session.sessions_stale);
        assert_eq!(
            session.sessions_error.as_deref(),
            Some("Could not load the sessions list: too many requests — wait and try again")
        );
    }

    /// Slice A4: the `getConnectedWebsites` answer replaces the cache
    /// and clears loading/error — but only for our own in-flight request
    /// (matched by `@extra`).
    #[test]
    fn websites_answer_replaces_cache_for_matching_request() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetConnectedWebsites, None);
        session.connected_websites_loading = true;
        session.websites_error = Some("stale".into());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"connectedWebsites","@extra":"{}","websites":[{{"@type":"connectedWebsite","id":55,"domain_name":"example.com","bot_user_id":77,"browser":"Chrome","platform":"Web","log_in_date":1758900000,"last_active_date":1759000000,"ip_address":"9.9.9.9","location":"Boston, United States"}}]}}"#,
                extra.0,
            ),
        );
        let websites = session.connected_websites.as_ref().expect("cached");
        assert_eq!(websites.len(), 1);
        let site = &websites[0];
        assert_eq!(site.id, 55);
        assert_eq!(site.domain_name, "example.com");
        assert_eq!(site.bot_user_id, 77);
        assert_eq!(site.browser, "Chrome");
        assert_eq!(site.platform, "Web");
        assert_eq!(site.log_in_date, 1758900000);
        assert_eq!(site.ip_address, "9.9.9.9");
        assert_eq!(site.location, "Boston, United States");
        assert!(!session.connected_websites_loading);
        assert!(session.websites_error.is_none());
        assert!(!session.websites_stale);
    }

    /// Slice A4: a stray `connectedWebsites` answer (no matching pending
    /// purpose) must not clobber the cache.
    #[test]
    fn websites_answer_ignored_without_matching_request() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.connected_websites = Some(Vec::new());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"connectedWebsites","websites":[{"@type":"connectedWebsite","id":55,"domain_name":"example.com"}]}"#,
        );
        assert_eq!(session.connected_websites.as_ref().unwrap().len(), 0);
    }

    /// Slice A4: a successful `disconnectWebsite` keeps the old cache
    /// visible and marks it stale (the driver refetches the authoritative
    /// answer on the same ingest) — the row is NOT removed
    /// optimistically.
    #[test]
    fn disconnect_website_ok_keeps_cache_and_marks_stale() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.connected_websites = Some(vec![ParsedWebsite {
            id: 55,
            domain_name: "example.com".into(),
            bot_user_id: 77,
            browser: "Chrome".into(),
            platform: "Web".into(),
            log_in_date: 1758900000,
            last_active_date: 1759000000,
            ip_address: "9.9.9.9".into(),
            location: "Boston, United States".into(),
        }]);
        session.websites_mutating = true;
        let extra = session.request(RequestPurpose::DisconnectWebsite { website_id: 55 }, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        // The old cache stays visible until the authoritative refetch
        // replaces it — no optimistic deletion.
        assert_eq!(session.connected_websites.as_ref().unwrap().len(), 1);
        assert!(session.websites_stale);
        assert!(!session.websites_mutating);
        assert!(session.websites_error.is_none());
    }

    /// Slice A4: a refused `disconnectAllWebsites` surfaces an honest
    /// classified error and leaves the list untouched.
    #[test]
    fn disconnect_all_websites_error_surfaces_honestly() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.websites_mutating = true;
        let extra = session.request(RequestPurpose::DisconnectAllWebsites, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"WEBSITE_NOT_FOUND"}}"#,
                extra.0
            ),
        );
        assert!(!session.websites_mutating);
        assert_eq!(
            session.websites_error.as_deref(),
            Some("Could not disconnect the website: Telegram refused the request")
        );
    }

    /// Slice A4: a failed websites fetch clears the spinner and parks the
    /// error on the overlay — and clears `websites_stale` so the next
    /// ingest does not auto-retry (mirroring A3's
    /// `sessions_fetch_error_clears_loading`).
    #[test]
    fn websites_fetch_error_clears_loading() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.connected_websites_loading = true;
        // A failed stale-refetch (e.g. after a disconnect-ok marked the
        // cache stale) must NOT leave the cache stale — otherwise the
        // next ingest retries the fetch and flood state worsens.
        session.websites_stale = true;
        let extra = session.request(RequestPurpose::GetConnectedWebsites, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":429,"message":"FLOOD_WAIT_3"}}"#,
                extra.0
            ),
        );
        assert!(!session.connected_websites_loading);
        // The failed stale-refetch clears staleness: no auto-retry on
        // the next ingest; the user retries via the Refresh button.
        assert!(!session.websites_stale);
        assert_eq!(
            session.websites_error.as_deref(),
            Some("Could not load the websites list: too many requests — wait and try again")
        );
    }

    /// Slice A4: a successful `toggleSessionCanAcceptCalls` marks the
    /// sessions list stale (the toggled value arrives in the
    /// authoritative refetch) — the row is NOT flipped optimistically.
    #[test]
    fn toggle_session_calls_ok_marks_sessions_stale() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sessions = Some(vec![ParsedSession {
            id: 22,
            is_current: false,
            is_password_pending: false,
            can_accept_secret_chats: true,
            can_accept_calls: true,
            device_model: "iPhone".into(),
            application_name: "Telegram iOS".into(),
            application_version: "12.0".into(),
            platform: "iOS".into(),
            system_version: "18.0".into(),
            last_active_date: 1758900000,
            ip_address: "5.6.7.8".into(),
            location: "Tel Aviv, Israel".into(),
        }]);
        session.sessions_mutating = true;
        let extra = session.request(RequestPurpose::ToggleSessionCalls { session_id: 22 }, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        // No optimistic flip: the cached flag is untouched until the
        // authoritative refetch replaces the list.
        let sessions = session.sessions.as_ref().unwrap();
        assert!(sessions[0].can_accept_calls);
        assert!(session.sessions_stale);
        assert!(!session.sessions_mutating);
        assert!(session.sessions_error.is_none());
    }

    /// Slice A4: a refused secret-chats toggle surfaces an honest
    /// classified error and leaves the cached flags untouched.
    #[test]
    fn toggle_session_secret_chats_error_surfaces_honestly() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.sessions_mutating = true;
        let extra = session.request(
            RequestPurpose::ToggleSessionSecretChats { session_id: 22 },
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"SESSION_INVALID"}}"#,
                extra.0
            ),
        );
        assert!(!session.sessions_mutating);
        assert_eq!(
            session.sessions_error.as_deref(),
            Some("Could not change the session setting: Telegram refused the request")
        );
    }

    #[test]
    fn shared_media_empty_ready_failed_and_stale_drop() {
        // Slice media-shared-gallery: per-tab fetch state machine through
        // the real `foundChatMessages` / error reducer paths.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let chat = ChatId(11);
        let tab = session.shared_media.open_for(chat);
        assert_eq!(tab, SharedMediaTab::Media);
        // Empty answer → Empty, never Ready.
        let generation = session.shared_media.begin_fetch(SharedMediaTab::Media);
        let extra = session.request(
            RequestPurpose::GetSharedMedia {
                tab: SharedMediaTab::Media,
                generation,
            },
            Some(chat),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                extra.0
            ),
        );
        let media = &session.shared_media.tabs[SharedMediaTab::Media.index()];
        assert_eq!(media.status, SharedMediaTabStatus::Empty);
        assert!(media.items.is_empty());
        // Ready answer → Ready with items.
        let generation = session.shared_media.begin_fetch(SharedMediaTab::Files);
        let extra = session.request(
            RequestPurpose::GetSharedMedia {
                tab: SharedMediaTab::Files,
                generation,
            },
            Some(chat),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":201,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"report.pdf","mime_type":"application/pdf","document":{{"@type":"file","id":901,"size":1,"expected_size":1,"local":{{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false}},"remote":{{"@type":"remoteFile","id":"x"}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        let files = &session.shared_media.tabs[SharedMediaTab::Files.index()];
        assert_eq!(files.status, SharedMediaTabStatus::Ready);
        assert_eq!(files.items.len(), 1);
        assert_eq!(files.items[0].label, "report.pdf");
        // Stale generation → dropped, tab keeps its old state.
        let generation = session.shared_media.begin_fetch(SharedMediaTab::Music);
        let extra = session.request(
            RequestPurpose::GetSharedMedia {
                tab: SharedMediaTab::Music,
                generation,
            },
            Some(chat),
        );
        session.shared_media.close();
        session.shared_media.open_for(ChatId(12));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[]}}"#,
                extra.0
            ),
        );
        let music = &session.shared_media.tabs[SharedMediaTab::Music.index()];
        assert_eq!(music.status, SharedMediaTabStatus::Idle);
        // Error → Failed with the server text.
        let generation = session.shared_media.begin_fetch(SharedMediaTab::Links);
        let extra = session.request(
            RequestPurpose::GetSharedMedia {
                tab: SharedMediaTab::Links,
                generation,
            },
            Some(ChatId(12)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_INVALID"}}"#,
                extra.0
            ),
        );
        let links = &session.shared_media.tabs[SharedMediaTab::Links.index()];
        assert_eq!(links.status, SharedMediaTabStatus::Failed);
        assert!(!links.error.is_empty());
    }

    #[test]
    fn shared_media_tgx_copy_is_verbatim() {
        // Slice media-shared-gallery: the per-tab empty-state copy matches
        // TGX's strings.xml (chat + channel variants).
        assert_eq!(SharedMediaTab::Media.empty_title(), "No media to show");
        assert_eq!(
            SharedMediaTab::Media.empty_hint(false),
            "Share photos and videos in this chat and\naccess them on any of your devices."
        );
        assert_eq!(
            SharedMediaTab::Media.empty_hint(true),
            "Published photos and videos\nwill be shown here."
        );
        assert_eq!(
            SharedMediaTab::Gifs.empty_hint(false),
            "Share GIFs in this chat and\naccess them on any device you have."
        );
        assert_eq!(
            SharedMediaTab::Files.empty_hint(true),
            "Published documents and files\nwill be shown here."
        );
        for tab in SharedMediaTab::ALL {
            assert!(!tab.label().is_empty());
            assert!(!tab.glyph().is_empty());
            assert!(tab.filter_constructor().starts_with("searchMessagesFilter"));
        }
    }

    #[test]
    fn open_chat_closes_shared_media_gallery() {
        // Slice media-shared-gallery: gallery open for chat A, switching to
        // chat B closes it (B1 — otherwise the panel shows A's title beside
        // B's conversation and row jumps resolve the message id against B);
        // the same-chat path leaves it open.
        let (mut session, _sink) = session();
        session.shared_media.open_for(ChatId(11));
        assert!(session.shared_media.open);
        session.open_chat(ChatId(12));
        assert!(!session.shared_media.open);
        assert_eq!(session.shared_media.chat_id, None);
        session.shared_media.open_for(ChatId(12));
        session.open_chat(ChatId(12));
        assert!(session.shared_media.open);
        assert_eq!(session.shared_media.chat_id, Some(ChatId(12)));
    }

    /// Slice P1: a `paymentForm` answer applies only to our own Buy press
    /// (matched by `@extra`) — a stray form never opens the dialog.
    #[test]
    fn payment_form_applies_only_to_own_request() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let form_json = |extra: &str| {
            format!(
                r#"{{"@type":"paymentForm","@extra":"{extra}","id":7,"type":{{"@type":"paymentFormTypeRegular","invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":true,"need_name":true,"need_phone_number":false,"need_email_address":true,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"payment_provider_user_id":99,"payment_provider":{{"@type":"paymentProviderOther","url":"https://pay.example.com/x"}},"additional_payment_options":[],"saved_order_info":null,"saved_credentials":[],"can_save_credentials":true,"need_password":false}},"seller_bot_user_id":51,"product_info":{{"@type":"productInfo","title":"Time machine","description":{{"@type":"formattedText","text":"","entities":[]}},"photo":null}}}}"#
            )
        };
        apply_json(&mut session, &seq, &sink, &form_json("999"));
        assert!(session.payment_form.is_none());
        let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(51)));
        session.payment_form_loading = true;
        apply_json(&mut session, &seq, &sink, &form_json(&extra.0.to_string()));
        let form = session.payment_form.as_ref().expect("form applies");
        assert_eq!(form.id, 7);
        assert_eq!(form.product_title, "Time machine");
        assert!(!session.payment_form_loading);
    }

    /// Slice P1 fix-up: `payment_request` survives the form and validated
    /// answers — `validateOrderInfo` / `sendPaymentForm` need it after the
    /// form answer is applied; only closing the dialog clears it.
    #[test]
    fn payment_request_survives_form_and_validated_answers() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        // Buy press: `getPaymentForm` sent, request context set.
        let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(51)));
        session.payment_request = Some(PaymentRequest {
            chat_id: ChatId(51),
            message_id: MessageId(7),
        });
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"paymentForm","@extra":"{}","id":7,"type":{{"@type":"paymentFormTypeRegular","invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":true,"need_name":true,"need_phone_number":false,"need_email_address":true,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"payment_provider_user_id":99,"payment_provider":{{"@type":"paymentProviderOther","url":"https://pay.example.com/x"}},"additional_payment_options":[],"saved_order_info":null,"saved_credentials":[],"can_save_credentials":true,"need_password":false}},"seller_bot_user_id":51,"product_info":{{"@type":"productInfo","title":"Time machine","description":{{"@type":"formattedText","text":"","entities":[]}},"photo":null}}}}"#,
                extra.0
            ),
        );
        assert!(session.payment_form.is_some());
        assert!(
            session.payment_request.is_some(),
            "form answer must not clear the payment request"
        );
        // Continue: `validateOrderInfo` sent, request context refreshed.
        let extra = session.request(RequestPurpose::ValidateOrderInfo, Some(ChatId(51)));
        session.payment_request = Some(PaymentRequest {
            chat_id: ChatId(51),
            message_id: MessageId(7),
        });
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"validatedOrderInfo","@extra":"{}","order_info_id":"oid1","shipping_options":[{{"@type":"shippingOption","id":"ship1","title":"Standard","price_parts":[]}}]}}"#,
                extra.0
            ),
        );
        assert!(session.payment_validated.is_some());
        assert_eq!(session.payment_shipping_id.as_deref(), Some("ship1"));
        assert!(
            session.payment_request.is_some(),
            "validated answer must not clear the payment request"
        );
    }

    /// Slice P1: a failed `getPaymentForm` clears the spinner and surfaces
    /// the reason in the dialog note.
    #[test]
    fn payment_form_error_surfaces_note() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.payment_form_loading = true;
        let extra = session.request(RequestPurpose::GetPaymentForm, Some(ChatId(51)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_NOT_MODIFIED"}}"#,
                extra.0
            ),
        );
        assert!(!session.payment_form_loading);
        assert!(
            session
                .payment_note
                .as_deref()
                .unwrap_or("")
                .starts_with("Payment failed:")
        );
    }

    /// Slice P1: `validateOrderInfo` stores the validated info and
    /// preselects the first shipping option.
    #[test]
    fn validated_order_info_selects_first_shipping() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::ValidateOrderInfo, Some(ChatId(51)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"validatedOrderInfo","@extra":"{}","order_info_id":"oid1","shipping_options":[{{"@type":"shippingOption","id":"ship1","title":"Standard","price_parts":[{{"@type":"labeledPricePart","label":"Post","amount":100}}]}},{{"@type":"shippingOption","id":"ship2","title":"Express","price_parts":[]}}]}}"#,
                extra.0
            ),
        );
        let validated = session.payment_validated.as_ref().expect("validated");
        assert_eq!(validated.order_info_id, "oid1");
        assert_eq!(validated.shipping_options.len(), 2);
        assert_eq!(session.payment_shipping_id.as_deref(), Some("ship1"));
    }

    /// Slice P1: `sendPaymentForm` answers — success notes, a verification
    /// URL is stashed for the browser, a bare failure notes.
    #[test]
    fn payment_result_notes_and_verification_url() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let result_json = |extra: &str, success: bool, url: &str| {
            format!(
                r#"{{"@type":"paymentResult","@extra":"{extra}","success":{success},"verification_url":"{url}"}}"#
            )
        };
        let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(51)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &result_json(&extra.0.to_string(), true, ""),
        );
        assert_eq!(
            session.payment_note.as_deref(),
            Some("✅ Payment successful")
        );
        let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(51)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &result_json(&extra.0.to_string(), false, "https://pay.example.com/3ds"),
        );
        assert_eq!(
            session.payment_verification_url.as_deref(),
            Some("https://pay.example.com/3ds")
        );
        let extra = session.request(RequestPurpose::SendPaymentForm, Some(ChatId(51)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &result_json(&extra.0.to_string(), false, ""),
        );
        assert_eq!(session.payment_note.as_deref(), Some("Payment failed"));
    }

    /// Slice P1: `getPaymentReceipt` stores the receipt and opens the
    /// receipt dialog.
    #[test]
    fn payment_receipt_opens_dialog() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetPaymentReceipt, Some(ChatId(51)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"paymentReceipt","@extra":"{}","product_info":{{"@type":"productInfo","title":"Time machine","description":{{"@type":"formattedText","text":"","entities":[]}},"photo":null}},"date":1727400000,"seller_bot_user_id":51,"type":{{"@type":"paymentReceiptTypeRegular","payment_provider_user_id":99,"invoice":{{"@type":"invoice","currency":"USD","price_parts":[{{"@type":"labeledPricePart","label":"Machine","amount":1999}}],"subscription_period":0,"max_tip_amount":0,"suggested_tip_amounts":[],"recurring_payment_terms_of_service_url":"","terms_of_service_url":"","is_test":false,"need_name":false,"need_phone_number":false,"need_email_address":false,"need_shipping_address":false,"send_phone_number_to_provider":false,"send_email_address_to_provider":false,"is_flexible":false}},"order_info":{{"@type":"orderInfo","name":"","phone_number":"","email_address":"","shipping_address":{{"@type":"address","country_code":"","state":"","city":"","street_line1":"","street_line2":"","postal_code":""}}}},"shipping_option":{{"@type":"shippingOption","id":"","title":"","price_parts":[]}},"credentials_title":"Visa •• 4242","tip_amount":0}}}}"#,
                extra.0
            ),
        );
        let receipt = session.payment_receipt.as_ref().expect("receipt");
        assert_eq!(receipt.product_title, "Time machine");
        assert_eq!(receipt.total_amount, 1999);
        assert_eq!(receipt.credentials_title, "Visa •• 4242");
        assert!(session.payment_receipt_open);
    }
}
