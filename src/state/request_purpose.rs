//! Request purposes: every in-flight TDLib request, typed.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPurpose {
    GetEmojiSet,
    ViewTrendingEmojiSets,
    StopPendingMessage {
        topic_id: i32,
        draft_id: i64,
    },
    GetAuthorizationState,
    SetParameters,
    SetPhoneNumber,
    CheckAuthenticationCode,
    SetAuthenticationEmail,
    RegisterUser,
    CheckAuthenticationEmailCode,
    CheckAuthenticationPassword,
    /// Slice A1: `resendAuthenticationCode` from the code-entry screen.
    ResendAuthenticationCode,
    /// Slice A10: `requestAuthenticationPasswordRecovery` from the
    /// password screen ("Forgot password?"). Resend re-issues this call —
    /// TDLib enforces the server-side cooldown, no local countdown.
    RequestAuthenticationPasswordRecovery,
    /// Slice A10: `recoverAuthenticationPassword` with the emailed code.
    RecoverAuthenticationPassword,
    /// Slice A1: `requestQrCodeAuthentication` from the phone screen.
    RequestQrCodeAuthentication,
    /// `resetAuthenticationEmailAddress` on the email-code step.
    ResetAuthenticationEmail,
    /// `getCountries` for the sign-in country picker.
    GetCountries,
    /// `getCountryCode`: the default country guess for the phone screen.
    GetCountryCode,
    LoadChats,
    /// Phase 7.1: single-shot `loadChats(chatListFolder(id))` when a folder
    /// tab is selected. Separate from `LoadChats` so the ok-response does
    /// not re-trigger main-list paging.
    LoadFolderChats,
    /// `loadChats(chatListArchive)` pages, sent once the main list is
    /// exhausted. Separate from `LoadChats` so its ok pages the archive,
    /// not the main list.
    LoadArchiveChats,
    GetHistory,
    /// Slice CL: one-shot `getChatHistory` for the chat-list peek preview
    /// (`parity:chatlist-chat-preview`). The `messages` answer lands in
    /// `Session::chat_preview_fetch` — it must NOT merge into the open
    /// chat's history (the `GetHistory` branch drops answers for non-open
    /// chats, and the preview never calls `openChat`).
    GetChatPreview,
    /// `parity:platform-chat-export` — `getChatHistory` pages for a chat
    /// history export. The `messages` answer appends to
    /// `Session::chat_export` instead of merging into view history.
    ExportChatHistory,
    ExportAccount,
    /// Any `sendMessage` (text / photo / document). Response `message` is pending.
    SendMessage,
    /// M2: `getFullRichMessage`. Response `richMessage` replaces the
    /// partial blocks of the history message.
    GetFullRichMessage {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `getRepliedMessage` for a bubble's reply strip; the `message`
    /// answer lands in `Session::reply_targets`, not in the history.
    GetRepliedMessage {
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
    /// Slice media-downloads-pause: `toggleDownloadIsPaused`. Response is
    /// `Ok`; the pause state itself arrives on `updateFileDownload`.
    ToggleDownloadIsPaused,
    SearchChats,
    SearchMessages,
    SearchRecentlyFoundChats,
    /// Phase 7.2: `searchPublicChats` — public username/title lookup across
    /// all public chats (not just known ones). Sent alongside `searchChats`.
    SearchPublicChats,
    AddRecentlyFoundChat,
    /// `searchChatsOnServer`: the server's title/username hits, merged
    /// behind `SearchChats` (never gates the search status).
    SearchChatsOnServer,
    /// `searchPublicPosts` (the "Public posts" scope of a plain query).
    SearchPublicPosts,
    /// `searchPublicMessagesByTag` (the "Public posts" scope of a hashtag).
    SearchPublicMessagesByTag,
    /// `getTopChats(topChatCategoryUsers)`: the "Frequent contacts" strip.
    GetTopChats,
    /// `removeTopChat`; response is `ok`, the entry left the strip
    /// optimistically.
    RemoveTopChat,
    /// `removeRecentlyFoundChat`; response is `ok`, the row left the Recent
    /// list optimistically.
    RemoveRecentlyFoundChat,
    /// `setOption(disable_top_chats)`; the truth arrives as `updateOption`.
    SetTopChatsDisabled,
    SearchChatMessages,
    /// The next older page of the open in-chat search (appended to the
    /// hits; carries the search generation).
    SearchChatMessagesMore,
    /// `searchChatMembers` behind the in-chat "From:" picker.
    SearchFromMembers,
    /// `getChatMessageByDate` of a jump to date.
    GetChatMessageByDate,
    /// `getChatMessageCalendar` page of the calendar box (the box's
    /// generation drops late answers).
    GetChatMessageCalendar {
        generation: u64,
    },
    /// The open chat's pinned messages: `searchChatMessages` with
    /// `searchMessagesFilterPinned` (schema 1.8.67, line 6316), newest
    /// first. Feeds the pinned bar (Telegram Desktop's pinned tracker).
    GetPinnedMessages,
    /// The corner "@" / heart button: `searchChatMessages` with
    /// `searchMessagesFilterUnreadMention` / `…UnreadReaction`; the oldest
    /// hit becomes the jump target (`Session::unread_jump`).
    JumpToUnread {
        kind: UnreadJumpKind,
    },
    /// `readAllChatMentions` / `readAllChatReactions` (corner button
    /// "Mark all as read"). Response is `ok`; counters follow via
    /// `updateChatUnread*Count`.
    ReadAllUnreadMarkers {
        kind: UnreadJumpKind,
    },
    /// The info panel's media counts: `getChatMessageCount` with the
    /// filter at index `filter` of `MEDIA_COUNT_FILTERS`.
    GetChatMessageCount {
        filter: u8,
    },
    /// Slice media-shared-gallery: one `searchChatMessages` page for a
    /// gallery tab. `generation` is the `SharedMediaState` generation at
    /// send time — late answers drop on mismatch.
    GetSharedMedia {
        tab: SharedMediaTab,
        generation: u64,
    },
    /// The next older `searchChatMessages` page for a gallery tab, asked
    /// for while the media viewer pages toward the end of the list.
    GetSharedMediaMore {
        tab: SharedMediaTab,
        generation: u64,
    },
    /// `getChatHistory` around a jump target (Unigram `LoadMessageSliceImpl`).
    GetHistoryAround,
    /// `getChatHistory` with a negative offset: the page newer than the
    /// loaded window's newest message, while the window does not reach the
    /// chat's latest message (`HistoryState::has_newer`).
    GetHistoryNewer,
    /// `getStickerSet` for the composer's emoji/sticker panel library
    /// (`Session::media_library`), one per installed set as its section
    /// comes into view.
    LoadLibrarySet {
        set_id: i64,
    },
    /// `getMessageAvailableReactions` for the message reaction picker.
    GetMessageAvailableReactions {
        message_id: i64,
    },
    /// `searchChatMembers` for the composer's `@` suggestions.
    SearchMentionMembers,
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
    /// Bots slice: `searchPublicChat` for `@botname` → bot user id
    /// resolution (schema 1.8.67, line 11603). Response is `chat`; the
    /// outcome lands in `Session::inline_bot_resolve`. `generation`
    /// matches `InlineBotResolve::Resolving::generation` so a stale
    /// answer (a newer username is already being resolved) is ignored —
    /// a `u64` keeps the `Copy` purpose enum intact.
    ResolveInlineBot {
        generation: u64,
    },
    /// `parity:platform-deep-links`: `getDeepLinkInfo` for a launch-time
    /// `t.me` / `tg:` link (schema 1.8.67, line 16189). The answer lands
    /// in `Session::deep_link`; `generation` drops stale answers.
    DeepLinkInfo {
        generation: u64,
    },
    /// `getInternalLinkType` for a link the local parsers do not cover
    /// (`parity:deeplink-internal-link-type`). Same slot as
    /// [`Self::DeepLinkInfo`] (`ResolvingInfo`).
    DeepLinkInternalType {
        generation: u64,
    },
    /// `parity:platform-deep-links`: deep-link follow-up resolving to a
    /// chat (`searchPublicChat` / `createPrivateChat` / `getChat`). The
    /// `chat` answer is picked up in `apply_update_new_chat` and opens
    /// via `ChatReady`.
    DeepLinkResolve {
        generation: u64,
    },
    /// Check an invite without joining; answer becomes a guarded preview.
    DeepLinkCheckInvite {
        generation: u64,
    },
    /// `parity:platform-deep-links`: explicitly confirmed `joinChatByInviteLink`
    /// (schema 1.8.67, line 14166); answer is a `chatJoinResult`.
    DeepLinkJoin {
        generation: u64,
    },
    /// Bots slice: `sendInlineQueryResultMessage` (schema 1.8.67, line
    /// 12226). Response is the sent `message`; failures surface through
    /// the normal message-send failure path.
    SendInlineQueryResult,
    /// B15: `addPollOption` (schema 1.8.67 line 12920). Response is `ok`;
    /// the option arrives through `updatePoll`.
    AddPollOption,
    /// B15: `getPollVoteStatistics` (schema 1.8.67 line 12947). Response
    /// is `pollVoteStatistics`, cached in `Session::poll_stats`.
    GetPollVoteStatistics {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// B15: `markChecklistTasksAsDone` (schema 1.8.67 line 12967).
    /// Response is `ok`; the list refreshes via `updateMessageContent`.
    MarkChecklistTasks,
    /// B15: `addChecklistTasks` (schema 1.8.67 line 12960).
    AddChecklistTasks,
    /// B4: `stopPoll` (schema 1.8.67 line 12953). Response is `ok`; the
    /// poll closes via `updatePoll`.
    StopPoll,
    /// `editMessageLiveLocation` with a null location (stop sharing). The
    /// `message` answer is ignored; `updateMessageContent` carries the result.
    StopLiveLocation,
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
    /// `getMessageProperties` for the open message context menu; the
    /// answer lands in `Session::message_menu_actions`.
    GetMessageMenuActions {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `reportChat` with message ids from the message menu; each answer
    /// drives `Session::message_report` (reason list, details, done).
    ReportMessages,
    /// `getMessageViewers` for the menu's "N Seen" row.
    GetMessageViewers {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `getMessageReadDate` for the private-chat "Seen at" row.
    GetMessageReadDate {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `getMessageAddedReactions` for the menu's "N Reacted" row.
    GetMessageAddedReactions {
        chat_id: ChatId,
        message_id: MessageId,
        /// `reaction_filter_key` of the tab (0 = every reaction).
        filter: u64,
        /// A later page: appended to the tab.
        append: bool,
    },
    /// "View Sticker Set" / "Add Stickers" on a sticker message:
    /// `getStickerSet`, answered into `Session::sticker_set_view`.
    ViewStickerSet {
        set_id: i64,
    },
    /// "Save to... Profile" on a song: `addProfileAudio`.
    AddProfileAudio,
    /// "Cancel Upload": `deleteMessages` on a message still being sent.
    CancelUpload,
    /// Admin moderation from the delete box: `deleteChatMessagesBySender`.
    DeleteChatMessagesBySender,
    /// Admin moderation from the delete box: `reportSupergroupSpam`.
    ReportSupergroupSpam,
    /// Admin moderation: `deleteMessageReactionsFromSender` (the who-reacted
    /// list's "Delete reaction" and the delete box's reactions checkbox).
    DeleteMessageReactionsFromSender {
        message_id: i64,
        /// The member whose reactions go; 0 for a channel sender.
        user_id: i64,
    },
    /// `canTransferOwnership`: the 2-step-verification / session-age gate
    /// before a transfer. Answered into `Session::ownership`.
    CanTransferOwnership,
    /// `transferChatOwnership` to `user_id`. Response is `ok`; the
    /// password never rides the purpose.
    TransferChatOwnership {
        user_id: i64,
    },
    /// `getChatOwnerAfterLeaving`: who inherits the chat when the owner
    /// leaves. Response is a `user`; correlated via the chat id.
    GetChatOwnerAfterLeaving,
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
    /// `editMessageSchedulingState`. Response is `ok`; `scheduling` is the
    /// new state (`None` = send now). The scheduled list entry is updated
    /// or dropped on success.
    EditMessageSchedulingState {
        message_id: MessageId,
        scheduling: ComposerScheduling,
    },
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
    /// Parity slice: `setReactionNotificationSettings`. Response is `ok`;
    /// the new values arrive as `updateReactionNotificationSettings`.
    SetReactionNotificationSettings,
    /// Parity slice: `getChatNotificationSettingsExceptions`. Response is
    /// `chats` (the exception chat ids); the scope is correlated via
    /// `PendingRequest::scope`.
    GetChatNotificationSettingsExceptions,
    /// Parity slice: `resetAllNotificationSettings`. Response is `ok`;
    /// the new values arrive as `updateScopeNotificationSettings` /
    /// `updateChatNotificationSettings` (the ok arm drops the cached
    /// scope settings so they refetch fresh).
    ResetAllNotificationSettings,

    /// Slice A3: `getActiveSessions`. Response is `sessions`; the list is
    /// replaced from the authoritative answer (never optimistic).
    GetActiveSessions,
    ConfirmDeviceLogin,
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
    /// `parity:proxy-settings`: `getProxies`. Response is `addedProxies`.
    GetProxies,
    /// `parity:proxy-settings`: `addProxy` / `editProxy` / `enableProxy`
    /// / `disableProxy` / `removeProxy`. The answer (`addedProxy` /
    /// `ok`) only marks the list stale; the authoritative `getProxies`
    /// refetch replaces it (never optimistic).
    MutateProxy,
    /// `parity:proxy-settings`: `pingProxy`. Response is `seconds`; an
    /// error means the proxy is not available.
    PingProxy {
        proxy_id: i32,
    },
    /// `parity:proxy-settings`: `setOption("prefer_ipv6")`. Response is
    /// `ok`; `on` is the value that was requested.
    SetPreferIpv6 {
        on: bool,
    },
    /// Slice A4: `disconnectWebsite`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    DisconnectWebsite {
        website_id: i64,
    },
    /// Slice A4: `disconnectAllWebsites`. Response is `ok`; the list is
    /// refetched from the authoritative answer (never optimistic).
    DisconnectAllWebsites,
    /// `addChatToList` (`chatListArchive` or `chatListMain`). Response is `ok`;
    /// the row moves via position updates.
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
    GetArchivedStickerSets,
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
    /// Slice S12: `searchStickers` for the composer trailing-emoji
    /// suggestions. Response is `stickers`, stored in
    /// `StickerPanel::suggestions` — never the search UI's
    /// `found_stickers` slot.
    SuggestStickers,
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
    ManageStickerSet {
        set_id: i64,
        installed: bool,
        archived: bool,
    },
    /// Slice S8: `reorderInstalledStickerSets`. Response is `ok`; same
    /// installed-sets invalidation as change.
    ReorderInstalledStickerSets,
    /// Slice S10: `setEmojiStatus` (td_api.tl:14850). Response is `ok`.
    SetEmojiStatus,
    /// Slice S10: `getRecentEmojiStatuses` (td_api.tl:13957). Response is `emojiStatuses`.
    GetRecentEmojiStatuses,
    /// Slice S10: `getThemedEmojiStatuses` (td_api.tl:13954). Response is `emojiStatusCustomEmojis`.
    GetThemedEmojiStatuses,
    /// Slice S10: `getDefaultEmojiStatuses` (td_api.tl:13963). Response is `emojiStatusCustomEmojis`.
    GetDefaultEmojiStatuses,
    /// Slice S10: `getUpgradedGiftEmojiStatuses` (td_api.tl:13960). Response is `emojiStatuses`.
    GetUpgradedGiftEmojiStatuses,
    /// Slice S10: `clearRecentEmojiStatuses` (td_api.tl:13966). Response is `ok`.
    ClearRecentEmojiStatuses,
    /// Slice S10: `getAnimatedEmoji` (td_api.tl:14743). Response is `animatedEmoji`.
    GetAnimatedEmoji,
    /// Slice S10: `getCustomEmojiStickers` (td_api.tl:14751). Response is `stickers`.
    GetCustomEmojiStickers,
    /// Slice S10: `searchEmojis` (td_api.tl:14732). Response is `emojiKeywords`.
    SearchEmojis,
    /// B11: `setDefaultReactionType` (td_api.tl:12852). Response is `ok`.
    SetDefaultReactionType,
    /// B11: `removeRecentSticker` (td_api.tl:14710). Response is `ok`.
    RemoveRecentSticker,
    /// B11: `getKeywordEmojis` (td_api.tl:14737). Response is `emojis`.
    GetKeywordEmojis,
    /// B11: `getAttachedStickerSets` (td_api.tl:14672) for a photo.
    GetAttachedStickerSets {
        file_id: i32,
    },
    /// B11: `getGreetingStickers` (td_api.tl:14651). Response is `stickers`.
    GetGreetingStickers,
    /// Slice S10: `getEmojiCategories` (td_api.tl:14738). Response is `emojiCategories`.
    GetEmojiCategories,
    /// Slice S10: `getInstalledStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14657). Response is `stickerSets`.
    GetInstalledEmojiSets,
    /// Slice S10: `getArchivedStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14663). Response is `stickerSets`.
    GetArchivedEmojiSets {
        first_page: bool,
    },
    /// Slice S10: `getTrendingStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14669). Response is `trendingStickerSets`.
    GetTrendingEmojiSets,
    /// Slice S10: `searchStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14689). Response is `stickerSets`.
    SearchEmojiSets,
    /// Slice S10: `changeStickerSet` on an emoji set (td_api.tl:14692). Response is `ok`.
    ChangeEmojiSet,
    /// Slice S10: `reorderInstalledStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14698). Response is `ok`.
    ReorderInstalledEmojiSets,
    /// `getSavedAnimations`. Response is `animations`.
    GetSavedAnimations,
    /// Slice S9: `getInlineQueryResults` against the animation search
    /// bot for the GIF panel search. The bot is resolved via
    /// `getOption("animation_search_bot_username")` + `searchPublicChat`
    /// (schema 1.8.67, lines 6483, 11063); the driver slice will carry the
    /// resolved id when it issues searches.
    /// New queries discard prior tracked pages; late replies are ignored.
    ResolveGifSearchBot,
    GetGifSearchResults {
        first_page: bool,
    },
    /// The message menu's "Add Fact Check" / "Edit Fact Check"
    /// (`setMessageFactCheck`). Response is `ok`; `updateMessageFactCheck`
    /// carries the new text.
    SetMessageFactCheck,
    /// The message menu's "Save for Notifications"
    /// (`addSavedNotificationSound`). Response is `notificationSound`; the
    /// saved list refetches.
    AddSavedNotificationSound,
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
    /// `viewMessages` carrying a sponsored message id (TDLib 1.8.67 has no
    /// `viewSponsoredMessage`; the schema says sponsored messages are marked
    /// viewed through `viewMessages`). Response is `ok`; fire-and-forget.
    ViewSponsoredMessages,
    /// `toggleHasSponsoredMessagesEnabled(false)`: the Premium "hide ads"
    /// action. Response is `ok`; the reducer then hides all ads.
    ToggleHasSponsoredMessagesEnabled,
    /// `clickChatSponsoredMessage`. Response is `ok`; fire-and-forget.
    ClickChatSponsoredMessage,
    /// `getMapThumbnailFile` for a location or venue message. Response is
    /// `file`; the tile's file id is kept per place (`Session::map_thumbs`).
    GetMapThumbnailFile,
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
    /// Slice bots-games: `getGameHighScores` after a Scores press.
    /// Response is `gameHighScores`; the panel shows a loading row until
    /// it lands, and an error closes the panel with a status note.
    GetGameHighScores,
    /// B1: `getLoginUrlInfo` for a login-URL button press. Response is
    /// `loginUrlInfo*`; on error the button degrades to a plain URL button
    /// (schema 1.8.67 doc on `getLoginUrl`).
    GetLoginUrlInfo,
    /// Slice P1: `getPaymentForm` after a Buy button press. Response is
    /// `paymentForm` (schema 1.8.67, line 15262).
    GetPaymentForm,
    GetMarketplaceGift,
    GetGiftTextLimit,
    SendMarketplaceGift,
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
    /// Slice `parity:bots-payment-recurring`: `getStarSubscriptions`.
    /// Response is `starSubscriptions` (schema 1.8.67, line 16075).
    /// `append` = this is a follow-up page (offset was non-empty).
    GetStarSubscriptions {
        append: bool,
    },
    /// Slice `parity:bots-payment-recurring`: `editStarSubscription`
    /// (cancel / re-enable). Response is `ok` (schema 1.8.67, line 16086).
    EditStarSubscription,
    /// Slice `parity:bots-payment-recurring`: `reuseStarSubscription`
    /// (rejoin an expired channel subscription). Response is `ok`
    /// (schema 1.8.67, line 16095).
    ReuseStarSubscription,
    /// `parity:premium-stars-balance`: `getStarTransactions`; `append` =
    /// follow-up page of the same filter.
    GetStarTransactions {
        append: bool,
    },
    /// `parity:premium-received-gifts`: `getReceivedGifts`.
    GetReceivedGifts {
        append: bool,
    },
    /// `toggleGiftIsSaved` for the gift in `PremiumHub::gift_selected`.
    ToggleGiftSaved {
        saved: bool,
    },
    /// `sellGift` (convert to Stars) for the confirmed gift.
    SellGift,
    /// `parity:premium-promo-page`: `getPremiumFeatures`.
    GetPremiumFeatures,
    /// `getPremiumState`.
    GetPremiumState,
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
    /// Slice A12: `setProfileAccentColor` (schema 1.8.67, line 14820).
    /// Response is `ok`; the new color arrives via `updateUser` on our
    /// own user (`profile_accent_color_id`).
    SetProfileAccentColor,
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
    /// B8: next page of `getChatJoinRequests`; the answer is appended.
    GetMoreChatJoinRequests,
    /// B8: `processChatJoinRequests` for every pending request.
    ProcessAllChatJoinRequests {
        approve: bool,
    },
    /// B8: `getChatInviteLinks` with `is_revoked = true`.
    GetRevokedChatInviteLinks,
    /// B8: `getChatInviteLinkCounts`. Response is `chatInviteLinkCounts`.
    GetChatInviteLinkCounts,
    /// B8: `getChatInviteLinkMembers`; `append` marks a later page.
    GetChatInviteLinkMembers {
        append: bool,
    },
    /// B8: `deleteRevokedChatInviteLink`; the link is looked up in
    /// `Session::revoked_link_deletions` by request id.
    DeleteRevokedChatInviteLink,
    /// B8: `deleteAllRevokedChatInviteLinks`.
    DeleteAllRevokedChatInviteLinks,
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
    /// Slice: group/channel title edit — `setChatTitle` (schema 1.8.67,
    /// line 13430). Response is `ok`; `updateChatTitle` carries the new
    /// title. No optimistic state: the update arrives from the server.
    SetChatTitle,
    /// Slice: group/channel description edit — `setChatDescription`
    /// (schema 1.8.67, line 13533). Response is `ok`; TDLib has no
    /// `updateChatDescription` broadcast, so the new description arrives
    /// on the next full-info pull. No optimistic state.
    SetChatDescription,
    /// Slice: group/channel photo edit — `setChatPhoto` (schema 1.8.67,
    /// line 13435). Response is `ok`; `updateChatPhoto` carries the new
    /// photo. No optimistic state: the update arrives from the server.
    SetChatPhoto,
    /// Slice S11: `setSupergroupStickerSet` (schema 1.8.67, line 15154).
    /// Response is `ok`; `updateSupergroupFullInfo` carries the new
    /// `sticker_set_id`. No optimistic state: the update arrives from
    /// the server. Driver validates the id client-side (negative is
    /// refused); 0 removes the group sticker set per the schema.
    SetSupergroupStickerSet,
    /// Slice S11: `setSupergroupCustomEmojiStickerSet` (schema 1.8.67,
    /// line 15159). Response is `ok`; `updateSupergroupFullInfo` carries
    /// the new `custom_emoji_sticker_set_id`. No optimistic state.
    SetSupergroupCustomEmojiStickerSet,
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
    /// Subsection tabs: the tab menu's "Mark as read" — `viewMessages`
    /// on the topic's last message (`messageSourceForumTopicHistory`,
    /// schema 1.8.67 lines 13230 / 3213). Answers `ok`.
    ReadForumTopic {
        forum_topic_id: i32,
    },
    /// Subsection tabs: the tab menu's Mute / Unmute —
    /// `setForumTopicNotificationSettings` (schema 1.8.67, line 12707).
    SetForumTopicNotificationSettings {
        forum_topic_id: i32,
    },
    /// Subsection tabs: `getForumTopic` (schema 1.8.67, line 12679) — one
    /// topic's authoritative state (unread count, read position), answered
    /// with `forumTopic`.
    GetForumTopic {
        forum_topic_id: i32,
    },
    /// `toggleChatViewAsTopics` (schema 1.8.67, line 13513). Answers `ok`;
    /// `updateChatViewAsTopics` carries the new value.
    ToggleChatViewAsTopics,
    /// `getForumTopicDefaultIcons` (schema 1.8.67, line 12658): `stickers`.
    GetForumTopicDefaultIcons,
    /// `getForumTopicLink` (schema 1.8.67, line 12692): `messageLink`,
    /// copied to the clipboard like a message link.
    GetForumTopicLink,
    /// `setPinnedForumTopics` (schema 1.8.67, line 12730): `ok`.
    SetPinnedForumTopics,
    /// `readAllForumTopicMentions` (schema 1.8.67, line 12741): `ok`.
    ReadAllForumTopicMentions {
        forum_topic_id: i32,
    },
    /// `readAllForumTopicReactions` (schema 1.8.67, line 12746): `ok`.
    ReadAllForumTopicReactions {
        forum_topic_id: i32,
    },
    /// `unpinAllForumTopicMessages` (schema 1.8.67, line 12756): `ok`.
    UnpinAllForumTopicMessages {
        forum_topic_id: i32,
    },
    /// `loadSavedMessagesTopics` (schema 1.8.67, line 11765): `ok`; the
    /// sublists arrive as `updateSavedMessagesTopic`, a 404 means all
    /// of them were loaded.
    LoadSavedMessagesTopics,
    /// `getSavedMessagesTopicHistory` (schema 1.8.67, line 11773): `messages`.
    GetSavedMessagesTopicHistory {
        topic_id: i64,
    },
    /// `deleteSavedMessagesTopicHistory` (schema 1.8.67, line 11781): `ok`.
    DeleteSavedMessagesTopicHistory {
        topic_id: i64,
    },
    /// `toggleSavedMessagesTopicIsPinned` (schema 1.8.67, line 11792): `ok`.
    ToggleSavedMessagesTopicPinned {
        topic_id: i64,
    },
    /// `getSavedMessagesTags` (schema 1.8.67, line 12856): `savedMessagesTags`.
    GetSavedMessagesTags {
        topic_id: i64,
    },
    /// `setSavedMessagesTagLabel` (schema 1.8.67, line 12859): `ok`.
    SetSavedMessagesTagLabel,
    /// `searchSavedMessages` (schema 1.8.67, line 11897): `foundChatMessages`.
    SearchSavedMessages {
        topic_id: i64,
    },
    /// `getMessageThread` (schema 1.8.67, line 11566) — resolves the
    /// comment / reply thread of `message_id`. Response is
    /// `messageThreadInfo`; correlated to the origin chat via
    /// `PendingRequest::chat_id`.
    GetMessageThread {
        message_id: i64,
    },
    /// `getMessageThreadHistory` (schema 1.8.67, line 11839) — one page of
    /// the open thread. Response is `messages`; `message_id` identifies the
    /// thread's origin message, correlated to the chat via
    /// `PendingRequest::chat_id`.
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
    /// Slice CL3: `reportChat` (schema 1.8.67, line 15693). Response is
    /// `ReportChatResult`; `Ok` reports the chat, any other variant
    /// surfaces as "more info required" (never success).
    ReportChat,
    /// Batch 8: `removeChatActionBar` (the bar's close button). Response
    /// is `ok`; the bar is dropped optimistically.
    RemoveChatActionBar,
    /// Batch 8: `sharePhoneNumber` (the bar's "Share my phone number").
    /// Response is `ok`; TDLib then clears the bar.
    SharePhoneNumber,
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
    /// B10: a chat-id list for a profile panel — groups in common
    /// (`getGroupsInCommon`, pending `user_id`), similar channels
    /// (`getChatSimilarChats`, pending `chat_id`) or the channels that
    /// can be a personal channel (`getSuitablePersonalChats`). Response
    /// is `chats`; ids land in `Session::profile_chat_lists`.
    GetProfileChats(ProfileChatsKind),
    /// B7: `toggleSupergroupIsForum` (line 15218). Response `ok`; the new
    /// `is_forum` arrives via `updateSupergroup`.
    ToggleSupergroupIsForum,
    /// B7: `toggleSupergroupIsAllHistoryAvailable` (line 15191). Applied
    /// optimistically; rolled back on error.
    ToggleSupergroupIsAllHistoryAvailable,
    /// B7: `toggleSupergroupJoinToSendMessages` (line 15180). Optimistic.
    ToggleSupergroupJoinToSendMessages,
    /// B7: `toggleSupergroupHasHiddenMembers` (line 15207). Optimistic.
    ToggleSupergroupHasHiddenMembers,
    /// B7: `toggleChatHasProtectedContent` (line 13504). Optimistic.
    ToggleChatHasProtectedContent,
    /// B7: `setChatAvailableReactions` (line 13527). Optimistic.
    SetChatAvailableReactions,
    /// B7: `setChatDiscussionGroup` (line 13539). Response `ok`; the new
    /// link arrives via `updateSupergroupFullInfo`.
    SetChatDiscussionGroup,
    /// B7: `upgradeBasicGroupChatToSupergroupChat` (line 13343). The
    /// answer is the new supergroup `chat`; pending `chat_id` is the old
    /// basic group chat.
    UpgradeBasicGroup,
    /// B10: `setBirthdate` (schema 1.8.67, line 14841). Response is
    /// `ok`; the new value arrives via `updateUserFullInfo`.
    SetBirthdate,
    /// B10: `setPersonalChat` (line 14847). Response is `ok`.
    SetPersonalChat,
    /// B10: `setUserNote` (line 14553). Response is `ok`.
    SetUserNote,
    /// `setUserPersonalProfilePhoto` / `suggestUserProfilePhoto`
    /// (lines 14942, 14952). Response is `ok`; pending `user_id`.
    SetUserPersonalPhoto,
    /// `reportChatPhoto` (line 16107). Response is `ok`.
    ReportChatPhoto,
    /// B10: `getUserProfilePhotos` (line 14591) for the profile photo
    /// gallery; pending `user_id`. Response is `chatPhotos`.
    GetUserProfilePhotos,
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
    /// Phase 9.2+: `getCustomEmojiStickers` for the story reaction picker.
    /// Response is `stickers`; cached in
    /// `Session::story_custom_emoji_stickers` keyed by sticker id (=
    /// custom emoji id).
    GetStoryCustomEmojiStickers,
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
    /// Share box: `searchChats` for the typed query. Response is `chats`;
    /// stored in `Session::share_search`.
    SearchShareChats,
    /// Share box: `searchChatsOnServer` for the typed query.
    SearchShareChatsOnServer,
    /// `getChatAvailableMessageSenders`. Response is `chatMessageSenders`;
    /// stored in `Session::send_as_options[chat_id]`.
    GetChatAvailableMessageSenders,
    /// `setChatMessageSender`. Response is `ok`; the choice arrives via
    /// `updateChatMessageSender`.
    SetChatMessageSender,
    /// Phase 9.7: `getChatStoryAlbums`. Response is `storyAlbums`;
    /// replaces `Session::story_albums[chat_id]`.
    GetChatStoryAlbums,
    /// Phase 9.7: `getStoryAlbumStories`. Response is `stories`;
    /// accumulated into `Session::story_album_stories[(chat_id, album_id)]`.
    GetStoryAlbumStories,
    /// Phase 9.7: `createStoryAlbum`. Response is the new `storyAlbum`;
    /// upserted into `Session::story_albums[chat_id]`.
    CreateStoryAlbum,
    /// Phase 9.7: `reorderStoryAlbums`. Response is `ok`; the sent order
    /// is applied to `Session::story_albums[chat_id]` (correlated via
    /// `PendingRequest::story_ids`, which carries album ids here).
    ReorderStoryAlbums,
    /// Phase 9.7: `deleteStoryAlbum`. Response is `ok`; the album is
    /// dropped from `Session::story_albums[chat_id]` (album id rides on
    /// `PendingRequest::story_album_id`).
    DeleteStoryAlbum,
    /// Phase 9.7: `setStoryAlbumName`. Response is the changed
    /// `storyAlbum`; upserted into `Session::story_albums[chat_id]`.
    SetStoryAlbumName,
    /// Phase 9.7: `addStoryAlbumStories` / `removeStoryAlbumStories` /
    /// `reorderStoryAlbumStories`. Response is the changed `storyAlbum`;
    /// upserted; the album's story id list is refreshed from the next
    /// `getStoryAlbumStories` page.
    AddStoryAlbumStories,
    RemoveStoryAlbumStories,
    ReorderStoryAlbumStories,
    /// Phase 9.7: `getChatArchivedStories`. Response is `stories`;
    /// accumulated into `Session::archived_stories[chat_id]`.
    GetChatArchivedStories,
    /// Phase 9.7: `getChatPostedToChatPageStories`. Response is `stories`
    /// (with `pinned_story_ids` on the first page); replaces
    /// `Session::chat_page_stories[chat_id]`.
    GetChatPostedToChatPageStories,
    /// Phase 9.7: `setChatPinnedStories`. Response is `ok`; the sent
    /// story ids (correlated via `PendingRequest::story_ids`) replace
    /// the chat's `pinned_story_ids`.
    SetChatPinnedStories,
    /// B14: `getCloseFriends`. Response is `users`; the ids land in
    /// `Session::close_friends` (user objects arrive via `updateUser`).
    GetCloseFriends,
    /// B14: `setCloseFriends`. Response is `ok`; the sent ids (staged in
    /// `Session::close_friends_pending`) become `Session::close_friends`.
    SetCloseFriends,
    /// B14: `setChatActiveStoriesList` (hide / unhide a peer's stories).
    /// Response is `ok`; the tray moves via `updateChatActiveStories`.
    SetChatActiveStoriesList,
    /// B14: `toggleStoryIsPostedToChatPage` (post to / remove from
    /// profile). Response is `ok`; the flag arrives via `updateStory`.
    ToggleStoryIsPostedToChatPage,
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
    /// `getInstalledBackgrounds`. Response is `backgrounds`
    /// (`Session::installed_backgrounds`).
    GetInstalledBackgrounds,
    /// `setDefaultBackground`. Response is `background`
    /// (`Session::default_backgrounds`).
    SetDefaultBackground,
    /// `deleteDefaultBackground`. Response is `ok`.
    DeleteDefaultBackground,
    /// `removeInstalledBackground`. Response is `ok`; the entry leaves the
    /// list optimistically at send time.
    RemoveInstalledBackground,
    /// `setDefaultBackground` with a local image (`inputBackgroundLocal`).
    /// Response is `background`, stored like `SetDefaultBackground`.
    SetDefaultBackgroundLocal,
    /// `searchBackground` for a `bg/` link. Response is `background`
    /// (`Session::searched_background`).
    SearchBackground,
    /// `setChatBackground`. Response is `ok`; `updateChatBackground`
    /// carries the new wallpaper.
    SetChatBackground,
    /// `deleteChatBackground`. Response is `ok`.
    DeleteChatBackground,
    /// `setChatTheme`. Response is `ok`; `updateChatTheme` carries it.
    SetChatTheme,
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
    /// Slice S3: `getUserPrivacySettingRules` for a Privacy-screen key.
    /// Response is `userPrivacySettingRules`.
    GetPrivacyRules {
        key: PrivacySettingKey,
    },
    /// Slice S3: `setUserPrivacySettingRules` for a Privacy-screen key.
    /// Response is `ok`; the new detail is applied optimistically at
    /// send time.
    SetPrivacyRules {
        key: PrivacySettingKey,
    },
    /// B13: `getNewChatPrivacySettings`. Response is
    /// `newChatPrivacySettings`.
    GetNewChatPrivacy,
    /// B13: `setNewChatPrivacySettings`. Response is `ok`; applied
    /// optimistically at send time.
    SetNewChatPrivacy {
        previous_allow: bool,
    },
    /// B13: `setGiftSettings`. Response is `ok`; applied optimistically.
    SetGiftSettings,
    /// B13: `setInactiveSessionTtl`. Response is `ok`; applied
    /// optimistically.
    SetInactiveSessionTtl,
    /// B13: `setOption(ignore_sensitive_content_restrictions)`. The truth
    /// arrives as `updateOption`.
    SetSensitiveContent,
    /// `setOption(disable_contact_registered_notifications)`. The truth
    /// arrives as `updateOption`.
    SetContactJoinedNotifications,
    /// B13: `getNetworkStatistics`. Response is `networkStatistics`.
    GetNetworkStatistics,
    /// B13: `resetNetworkStatistics`. Response is `ok`.
    ResetNetworkStatistics,
    /// B13: `getRecoveryEmailAddress` used to verify a typed password.
    CheckRememberedPassword,
    /// B13: `hideSuggestedAction(suggestedActionCheckPassword)`.
    HideCheckPasswordSuggestion,
    /// Slice S3: `getReadDatePrivacySettings`. Response is
    /// `readDatePrivacySettings`.
    GetReadDatePrivacy,
    /// Slice S3: `setReadDatePrivacySettings`. Response is `ok`;
    /// applied optimistically at send time.
    SetReadDatePrivacy,
    /// Slice S3: `getBlockedMessageSenders` page. Response is
    /// `messageSenders`.
    GetBlockedSenders {
        offset: i32,
    },
    /// Slice S3: `setMessageSenderBlockList`. Response is `ok`; an
    /// unblock is applied optimistically at send time.
    SetSenderBlockList {
        user_id: i64,
        block: bool,
    },
    /// Phase C2i: `sendCallLog` for the ended call. Response is `ok`.
    SendCallLog,
    /// Phase C3a: `createVideoChat`. Response is `groupCallId`; the
    /// chat-bound voice chat's states arrive as `updateGroupCall`.
    CreateVideoChat {
        chat_id: i64,
    },
    /// Phase C3a: `joinVideoChat` or `joinLiveStory`. Response is `text` (join payload
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
    /// `getVideoChatAvailableParticipants`. Response is `messageSenders`;
    /// the "join as" choices of the tracked call.
    GetVideoChatAvailableParticipants {
        group_call_id: i32,
    },
    /// `setVideoChatDefaultParticipant`. Response is `ok`.
    SetVideoChatDefaultParticipant {
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
    /// `getDefaultMessageAutoDeleteTime`. Response is `messageAutoDeleteTime`,
    /// stored in `Session::default_auto_delete_secs`.
    GetDefaultAutoDelete,
    /// `setDefaultMessageAutoDeleteTime`. Response is `ok`; the confirmed
    /// `seconds` are stored (the server accepted exactly this value).
    SetDefaultAutoDelete {
        seconds: i32,
    },
    /// Phase S2: `getStorageStatistics`. Response is `storageStatistics`;
    /// aggregated by file type into `Session::storage_stats` (TGX
    /// `SettingsCacheController` / `TGStorageStats` style, including the
    /// "Secret media and files" category for `fileTypeSecret`). Slice S4:
    /// requested with `chat_limit` 50 — per-chat rows feed the usage
    /// screen's per-chat breakdown.
    GetStorageStatistics,
    /// Slice S4: `setAutoDownloadSettings` for one network type.
    /// Response is `ok`; the confirmed sent settings are applied then
    /// (the setAccountTtl precedent — the server confirmed the write of
    /// exactly this value, not an optimistic guess). Carries the sent
    /// settings because the `ok` carries none.
    SetAutoDownloadSettings {
        network: NetworkKind,
        settings: AutoDownloadNetSettings,
    },
    /// Batch 6: `optimizeStorage` ("Clear cache", per-type and per-chat
    /// clears). Response is `storageStatistics` of the deleted files;
    /// the cached usage stats are dropped and refetched so the screen
    /// shows the post-clear numbers.
    OptimizeStorage,
    /// Batch 6: a `setOption` for a storage limit (`storage_max_*`,
    /// `use_storage_optimizer`). Response is `ok`; TDLib echoes the new
    /// value as `updateOption`.
    SetStorageOption,
    /// Batch 4: `setOption("online")`. Response is `ok`; nothing to apply.
    SetOnline,
    /// Batch 4: `confirmSession` / `terminateSession` for the
    /// new-login alert ("Yes, it's me" / "No, it's not me!").
    ReviewUnconfirmedSession {
        confirmed: bool,
    },
    /// Batch 4: `acceptTermsOfService`.
    AcceptTermsOfService,
    /// Slice S4: `getAutoDownloadSettingsPresets`. Response is
    /// `autoDownloadSettingsPresets`; seeds the local per-network
    /// settings once (TDLib has no getter for the current values).
    GetAutoDownloadSettingsPresets,
    /// Slice A2: a 2FA management request (`getPasswordState` /
    /// `setPassword` / `setRecoveryEmailAddress` /
    /// `resendRecoveryEmailAddressCode` /
    /// `cancelRecoveryEmailAddressVerification`). All answer
    /// `passwordState`, stored in `Session::password_state`; `op`
    /// classifies the honest error line.
    PasswordStateOp {
        op: PasswordOp,
    },
    /// Slice (communities backend core): `createCommunity` (schema 1.8.67,
    /// line 11806). Response is `communityId`; the driver chains it into
    /// `getCommunityFullInfo`.
    CreateCommunity,
    /// `getCommunityFullInfo` (TDLib 1.8.68; replaced
    /// `loadCommunityFullInfo`). Response is `communityFullInfo`,
    /// correlated through `PendingRequest::community_id`.
    GetCommunityFullInfo,
    /// Slice (communities backend core): `setCommunityName` (schema 1.8.67,
    /// line 11811). Response is `ok`; the pack is reloaded on success and
    /// the new name arrives via `updateCommunity`.
    SetCommunityName,
    /// `setCommunityPhoto` (TDLib 1.8.68). Response is `ok`; the new
    /// photo arrives via `updateCommunity` / `updateCommunityFullInfo`.
    SetCommunityPhoto,
    /// `setCommunityPermissions` (TDLib 1.8.68). Response is `ok`; the
    /// new permissions arrive via `updateCommunity`.
    SetCommunityPermissions,
    /// `deleteCommunity` (TDLib 1.8.68). Response is `ok`; the state
    /// drops the community and its full-info pack.
    DeleteCommunity,
    /// Slice A7: `getAccountTtl`. Response is `accountTtl`, stored in
    /// `Session::account_ttl_days`.
    GetAccountTtl,
    /// Slice A7: `setAccountTtl`. Response is `ok`; the confirmed `days`
    /// are stored (the server confirmed the write of exactly this value).
    SetAccountTtl {
        days: i32,
    },
    /// Slice A7: `deleteAccount`. Response is `ok`; the authoritative
    /// account teardown arrives via TDLib's own
    /// `updateAuthorizationState` → `Closed` (already handled by
    /// `set_auth`) — never faked client-side.
    DeleteAccount,
    /// Slice A8: `sendPhoneNumberCode` with `phoneNumberCodeTypeChange`.
    /// Response is `authenticationCodeInfo`, stored in
    /// `Session::change_number_phone` / `change_number_timeout`.
    SendPhoneNumberCode,
    /// Slice A8: `resendPhoneNumberCode`. Response is
    /// `authenticationCodeInfo`; same storage as `SendPhoneNumberCode`.
    ResendPhoneNumberCode,
    /// Slice A8: `checkPhoneNumberCode`. Response is `ok`; the server
    /// completed the number change, so the own user's `phone_number` is
    /// updated to the confirmed `Session::change_number_phone` target
    /// (not an optimistic guess — the `ok` confirms the change of
    /// exactly the number the code was sent to).
    CheckPhoneNumberCode,
    /// Slice payments: `deleteSavedOrderInfo` (schema 1.8.67, line
    /// 15286). Response is `ok`; the saved info lives server-side, so
    /// there is no local state to invalidate — the `ok` just retires
    /// the pending request.
    DeleteSavedOrderInfo,
    /// Slice payments: `deleteSavedCredentials` (schema 1.8.67, line
    /// 15289). Response is `ok`; same no-local-state treatment as
    /// `DeleteSavedOrderInfo`.
    DeleteSavedCredentials,
    /// `getMessage` for `chat.reply_markup_message_id` when that message is
    /// not in the loaded history; the `message` answer feeds the chat's
    /// reply keyboard.
    GetChatReplyMarkupMessage,
    /// `shareUsersWithBot` / `shareChatWithBot` / `sharePhoneNumber`
    /// (schema 1.8.67, lines 13001 / 13010 / 14584). Response is `ok`.
    ShareWithBot,
    /// `getRecentInlineBots` (schema 1.8.67, line 14776). Response is
    /// `users`.
    GetRecentInlineBots,
    /// `toggleChatIsTranslatable` (schema 1.8.67, line 13516). Response is
    /// `ok`; `updateChatIsTranslatable` carries the new flag.
    ToggleChatIsTranslatable,
    /// `toggleSupergroupHasAutomaticTranslation` (schema 1.8.67, line
    /// 15202). Response is `ok`; `updateSupergroup` carries the flag.
    ToggleSupergroupAutoTranslate,
    /// `translateText` / `translateMessageText` (schema 1.8.67). Response
    /// is `formattedText`; `job` indexes `Session::translate.jobs`.
    TranslateJob {
        job: u64,
    },
    /// Slice msg-richtext-ai-tools: `fixTextWithAi` (schema 1.8.67,
    /// :12172). Response is `fixedText`; the fixed text replaces the
    /// open chat's composer draft.
    FixTextWithAi,
    /// Slice msg-richtext-ai-tools: `composeTextWithAi` (schema 1.8.67,
    /// :12154). Response is `formattedText`; the composed text replaces
    /// the open chat's composer draft.
    ComposeTextWithAi,
    /// Slice msg-richtext-ai-tools: `composeRichMessageWithAi` (schema
    /// 1.8.67, :12162). Response is `richMessage`; the parsed blocks
    /// replace the open chat's composer draft as editor markup.
    ComposeRichMessageWithAi,
    /// Slice msg-richtext-ai-tools: `createRichMessageWithAi` (schema
    /// 1.8.67, :12168). Response is `richMessage`; the parsed blocks
    /// replace the open chat's composer draft as editor markup.
    CreateRichMessageWithAi,
    /// Slice msg-richtext-ai-tools: `fixRichMessageWithAi` (schema
    /// 1.8.67, :12176). Response is `richMessage`; the parsed blocks
    /// replace the open chat's composer draft as editor markup.
    FixRichMessageWithAi,
    Close,
    LogOut,
    Other,
}
