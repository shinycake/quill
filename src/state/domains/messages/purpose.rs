//! Request purposes for history, sending, editing, reactions, polls, translation and message menus.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for history, sending, editing, reactions, polls, translation and message menus; wrapped as
/// [`RequestPurpose::Messages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagesPurpose {
    GetHistory,
    /// `parity:platform-chat-export` — `getChatHistory` pages for a chat
    /// history export. The `messages` answer appends to
    /// `Session::chat_export` instead of merging into view history.
    ExportChatHistory,
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
    /// `getChatHistory` around a jump target (Unigram `LoadMessageSliceImpl`).
    GetHistoryAround,
    /// `getChatHistory` with a negative offset: the page newer than the
    /// loaded window's newest message, while the window does not reach the
    /// chat's latest message (`HistoryState::has_newer`).
    GetHistoryNewer,
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
    /// The message menu's "Add Fact Check" / "Edit Fact Check"
    /// (`setMessageFactCheck`). Response is `ok`; `updateMessageFactCheck`
    /// carries the new text.
    SetMessageFactCheck,
    /// `setChatDraftMessage`. Response is `ok`; the draft also arrives as
    /// `updateChatDraftMessage`.
    SetChatDraftMessage,
    /// `getChatSponsoredMessages` (channel / bot chats). Response is
    /// `sponsoredMessages`; rows render Sponsored / Recommended.
    GetChatSponsoredMessages,
    /// `reportChatSponsoredMessage`. Response is `ReportSponsoredResult`;
    /// `OptionRequired` opens the report-option picker.
    ReportChatSponsoredMessage,
    /// `viewMessages` carrying a sponsored message id (TDLib 1.8.67 has no
    /// `viewSponsoredMessage`; the schema says sponsored messages are marked
    /// viewed through `viewMessages`). Response is `ok`; fire-and-forget.
    ViewSponsoredMessages,
    /// `toggleHasSponsoredMessagesEnabled(false)`: the Premium "hide ads"
    /// action. Response is `ok`; the reducer then hides all ads.
    ToggleHasSponsoredMessagesEnabled,
    /// `clickChatSponsoredMessage`. Response is `ok`; fire-and-forget.
    ClickChatSponsoredMessage,
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
}

flat_purposes!(Messages(MessagesPurpose) {
    GetHistory,
    ExportChatHistory,
    SendMessage,
    SendMessageAlbum,
    OpenChat,
    CloseChat,
    ViewMessages,
    GetPinnedMessages,
    GetHistoryAround,
    GetHistoryNewer,
    SearchMentionMembers,
    EditMessage,
    DeleteMessages,
    ForwardMessages,
    AddMessageReaction,
    RemoveMessageReaction,
    PinChatMessage,
    SetPollAnswer,
    AddPollOption,
    MarkChecklistTasks,
    AddChecklistTasks,
    StopPoll,
    StopLiveLocation,
    UnpinChatMessage,
    UnpinAllChatMessages,
    GetMessageLink,
    ReportMessages,
    DeleteChatMessagesBySender,
    ReportSupergroupSpam,
    GetWebPageInstantView,
    GetLinkPreview,
    ResendMessages,
    GetChatScheduledMessages,
    SendChatAction,
    OpenMessageContent,
    RecognizeSpeech,
    SetMessageFactCheck,
    SetChatDraftMessage,
    GetChatSponsoredMessages,
    ReportChatSponsoredMessage,
    ViewSponsoredMessages,
    ToggleHasSponsoredMessagesEnabled,
    ClickChatSponsoredMessage,
    SearchShareChats,
    SearchShareChatsOnServer,
    GetChatAvailableMessageSenders,
    SetChatMessageSender,
    ToggleChatIsTranslatable,
    ToggleSupergroupAutoTranslate,
    FixTextWithAi,
    ComposeTextWithAi,
    ComposeRichMessageWithAi,
    CreateRichMessageWithAi,
    FixRichMessageWithAi,
});
