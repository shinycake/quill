use super::*;
use crate::data_settings::{AutoDownloadNetSettings, StorageChatStats};
use crate::ids::{ChatId, MessageId, RequestId, UserId};
use crate::privacy::PrivacyRule;
use crate::telegram::envelope_emoji::{EmojiCategory, EmojiKeyword, EmojiStatusItem};
use crate::telegram::envelope_story::ParsedStoryAlbum;
use crate::telegram::profile_accent::ProfileAccentColor;
use crate::telegram::requests::ArchiveChatListSettings;
use crate::text::TextEntity;
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub type_name: String,
    pub extra: Option<RequestId>,
    pub client_id: Option<i32>,
    pub payload: EnvelopePayload,
}

/// MED4: `optionValue*` (TDLib 1.8.67, `schema/td_api.tl:8889`).
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Boolean(bool),
    Integer(i64),
    String(String),
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvelopePayload {
    AccountExport(Value),
    UpdateStickerSet {
        id: i64,
        is_custom_emoji: bool,
    },
    UpdateAuthorizationState(AuthorizationState),
    /// MED4: `updateOption` (TDLib 1.8.67, `schema/td_api.tl:10926`).
    /// Only the options Quill reads are kept; everything else is still a
    /// parsed-but-ignored update (never an error).
    UpdateOption {
        name: String,
        value: OptionValue,
    },
    UpdatePendingMessage {
        chat_id: ChatId,
        forum_topic_id: i32,
        draft_id: i64,
        can_stop: bool,
        keep_on_stop: bool,
        content: MessageContent,
        files: Vec<ParsedFile>,
    },
    UpdateStopMessageDraft {
        chat_id: ChatId,
        forum_topic_id: i32,
        draft_id: i64,
    },
    UpdateNewMessage(ParsedMessage),
    UpdateMessageSendSucceeded {
        message: ParsedMessage,
        old_message_id: MessageId,
    },
    UpdateMessageSendFailed {
        message: ParsedMessage,
        old_message_id: MessageId,
        error: TdError,
    },
    UpdateMessageSendAcknowledged {
        chat_id: ChatId,
        message_id: MessageId,
    },
    UpdateDeleteMessages {
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
        is_permanent: bool,
        from_cache: bool,
    },
    UpdateMessageContent {
        chat_id: ChatId,
        message_id: MessageId,
        content: MessageContent,
        files: Vec<ParsedFile>,
    },
    /// `updateMessageEphemeralContent` (TDLib 1.8.67,
    /// `schema/td_api.tl:10424`) — the secret-chat ephemeral content of a
    /// message refreshed over time; replaces `message.ephemeral_content`
    /// in place (secret-chat lane, `parity:msg-ephemeral-updates`).
    /// `None` = schema-legal explicit null ("no ephemeral content anymore"),
    /// which clears the stored content; only a missing/mistyped field is a
    /// parse error.
    UpdateMessageEphemeralContent {
        chat_id: ChatId,
        message_id: MessageId,
        ephemeral: Option<EphemeralMessageContent>,
    },
    /// `updateMessageContentOpened` — voice note listened (`is_listened`) or
    /// video note viewed (`is_viewed`).
    UpdateMessageContentOpened {
        chat_id: ChatId,
        message_id: MessageId,
    },
    /// `updateMessageEdited` (TDLib 1.8.67, `schema/td_api.tl:10431`): bots
    /// edit inline keyboards this way — the new `reply_markup` replaces the
    /// message's keyboard (Phase 3.2).
    UpdateMessageEdited {
        chat_id: ChatId,
        message_id: MessageId,
        edit_date: i32,
        reply_markup: Option<ReplyMarkup>,
    },
    /// `updatePoll` (TDLib 1.8.67, `schema/td_api.tl:11179`): vote counts /
    /// chosen marks changed. Carries only the new `poll` — no chat or
    /// message id — so the reducer matches it by `poll.id` (Phase 4.2).
    UpdatePoll {
        poll: Poll,
    },
    UpdateChatPosition(ChatPositionUpdate),
    UpdateChatTitle {
        chat_id: ChatId,
        title: String,
    },
    UpdateChatLastMessage {
        chat_id: ChatId,
        last_message: Option<ParsedMessage>,
        positions: Vec<ChatPositionUpdate>,
    },
    UpdateChatAddedToList {
        chat_id: ChatId,
        list: ChatList,
    },
    UpdateChatRemovedFromList {
        chat_id: ChatId,
        list: ChatList,
    },
    /// `updateUnreadMessageCount`: server-side unread-message totals for a
    /// whole chat list (schema 1.8.67, line 10877).
    UpdateUnreadMessageCount {
        list: ChatList,
        unread_count: i32,
        unread_unmuted_count: i32,
    },
    /// `updateUnreadChatCount`: server-side unread-chat totals for a whole
    /// chat list, marked-as-unread chats included (schema line 10886).
    UpdateUnreadChatCount {
        list: ChatList,
        total_count: i32,
        unread_count: i32,
        unread_unmuted_count: i32,
        marked_as_unread_count: i32,
        marked_as_unread_unmuted_count: i32,
    },
    UpdateChatReadInbox {
        chat_id: ChatId,
        last_read_inbox_message_id: MessageId,
        unread_count: i32,
    },
    UpdateChatReadOutbox {
        chat_id: ChatId,
        last_read_outbox_message_id: MessageId,
    },
    /// Slice CL3: `updateChatUnreadMentionCount` (schema 1.8.67, line
    /// 10567) — the row's @ mention badge.
    UpdateChatUnreadMentionCount {
        chat_id: ChatId,
        unread_mention_count: i32,
    },
    /// Slice CL3: `updateChatUnreadReactionCount` (schema 1.8.67, line
    /// 10570) — the row's ♥ reaction badge.
    UpdateChatUnreadReactionCount {
        chat_id: ChatId,
        unread_reaction_count: i32,
    },
    /// Slice CL3: `updateChatBlockList` (schema 1.8.67, line 10594) —
    /// `blocked` is true when the new `block_list` is `blockListMain`.
    UpdateChatBlockList {
        chat_id: ChatId,
        blocked: bool,
    },
    /// Batch 8: `updateChatActionBar` (schema 1.8.67, line 10549) — `None`
    /// when the bar was removed.
    UpdateChatActionBar {
        chat_id: ChatId,
        action_bar: Option<ChatActionBar>,
    },
    /// Slice CL3: `reportChat` result (schema 1.8.67, lines 9210–9219).
    /// The chat list only sends the simple spam report (empty
    /// option_id/message_ids/text, schema:3667), so every non-Ok
    /// variant collapses to "more info required" — surfaced honestly,
    /// never as success.
    ReportChatResult(ReportChatOutcome),
    UpdateConnectionState(ConnectionState),
    UpdateNewChat {
        chat_id: ChatId,
        title: String,
        kind: ChatKind,
        unread_count: i32,
        last_read_inbox_message_id: MessageId,
        last_read_outbox_message_id: MessageId,
        notification_settings: ChatNotificationSettings,
        /// `chat.draft_message`. Null when the chat has no draft.
        draft: Option<ChatDraft>,
        /// Parity slice: `chat.photo.small` (`chatPhotoInfo`, schema 1.8.67,
        /// line 762). `None` when the chat has no photo.
        photo: Option<ParsedFile>,
        /// Parity slice 4: `chat.permissions.can_send_basic_messages`
        /// (`chatPermissions`, schema 1.8.67, line 1070). Defaults to true
        /// when the block is absent (the real `chat` object always carries
        /// it); refreshed by `updateChatPermissions`.
        can_send_basic_messages: bool,
        /// Slice G1: the full `chat.permissions` block (`chatPermissions`,
        /// schema 1.8.67, line 1070) for the default chat permissions
        /// editor; `None` when the block is absent or malformed.
        permissions: Option<ChatPermissions>,
        /// Slice G1: `chat.can_be_deleted_for_all_users` (schema 1.8.67,
        /// line 3605). Gates `deleteChat` (schema line 11850: "Use the
        /// field chat.can_be_deleted_for_all_users to find whether the
        /// method can be applied to the chat").
        can_be_deleted_for_all_users: bool,
        /// Slice CL1: `chat.can_be_deleted_only_for_self` (schema 1.8.67,
        /// line 3604). Together with `can_be_deleted_for_all_users` it
        /// tells "whether and how" `deleteChatHistory` (schema line
        /// 11845) can be applied.
        can_be_deleted_only_for_self: bool,
        /// Phase B4: `chat.message_auto_delete_time` (schema 1.8.67,
        /// lines 3616 / 3627) — the chat-level auto-delete or
        /// self-destruct (secret chats) timer, in seconds; 0 when
        /// disabled. Refreshed by `updateChatMessageAutoDeleteTime`.
        message_auto_delete_time: i32,
        /// Phase C3a: `chat.video_chat` (`videoChat`, schema 1.8.67,
        /// lines 3576 / 3579 / 3627). `None` when `group_call_id` is 0
        /// (no active video chat).
        video_chat: Option<ParsedVideoChat>,
        /// Slice G2: `chat.has_welcome_messages` (schema 1.8.67, line 3627)
        /// — true when the chat has welcome messages; only sent for chat
        /// administrators with the `can_change_info` right.
        has_welcome_messages: bool,
        /// `chat.has_protected_content` (schema 1.8.67, lines 3598 / 3627)
        /// — the chat's content can't be saved, forwarded or copied.
        /// Refreshed by `updateChatHasProtectedContent` (line 10582).
        has_protected_content: bool,
        /// `chat.has_scheduled_messages` (schema 1.8.67, line 3627) — the
        /// chat has scheduled messages; refreshed by
        /// `updateChatHasScheduledMessages`.
        has_scheduled_messages: bool,
        /// `chat.is_translatable` (schema 1.8.67, lines 3599 / 3627) —
        /// translation of the chat's messages must be suggested.
        /// Refreshed by `updateChatIsTranslatable` (line 10585).
        is_translatable: bool,
        /// Slice CL1: `chat.is_marked_as_unread` (schema 1.8.67, lines
        /// 3600 / 3627). Refreshed by `updateChatIsMarkedAsUnread`
        /// (schema line 10588).
        is_marked_as_unread: bool,
        /// Slice CL3: `chat.unread_mention_count` (schema 1.8.67, lines
        /// 3611 / 3627). Refreshed by `updateChatUnreadMentionCount`
        /// (schema line 10567).
        unread_mention_count: i32,
        /// Slice CL3: `chat.unread_reaction_count` (schema 1.8.67, lines
        /// 3612 / 3627). Refreshed by `updateChatUnreadReactionCount`
        /// (schema line 10570).
        unread_reaction_count: i32,
        /// Slice CL3: `chat.can_be_reported` (schema 1.8.67, lines 3606 /
        /// 3627). Gates the row-menu Report item (`reportChat`, schema
        /// line 15693).
        can_be_reported: bool,
        /// Batch 8: `chat.action_bar` (schema 1.8.67, line 3627). Refreshed
        /// by `updateChatActionBar`.
        action_bar: Option<ChatActionBar>,
        /// Slice CL3: `chat.block_list` is `blockListMain` (schema 1.8.67,
        /// lines 3627 / 9692). Refreshed by `updateChatBlockList`
        /// (schema line 10594); drives the row-menu Block/Unblock label.
        blocked: bool,
        /// `chat.positions` (schema 1.8.67, line 3594) — the chat's
        /// positions in the chat lists it is already placed in.
        positions: Vec<ChatPositionUpdate>,
        /// `chat.last_message` (schema 1.8.67, line 3593): the preview a
        /// newly loaded chat starts with; `updateChatLastMessage` only
        /// reports later changes. Boxed: messages are large.
        last_message: Option<Box<ParsedMessage>>,
    },
    /// `updateChatDraftMessage`. Positions are the new chat-list orders.
    UpdateChatDraftMessage {
        chat_id: ChatId,
        draft: Option<ChatDraft>,
        positions: Vec<ChatPositionUpdate>,
    },
    /// Parity slice 4 / slice G1: `updateChatPermissions` (schema 1.8.67,
    /// line 10500). `can_send_basic_messages` is kept for the
    /// topic-composer gate; the full block drives the default chat
    /// permissions editor.
    UpdateChatPermissions {
        chat_id: ChatId,
        can_send_basic_messages: bool,
        permissions: Option<ChatPermissions>,
    },
    /// Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
    /// line 10549) — the chat-level auto-delete or self-destruct
    /// (secret chats) timer changed.
    UpdateChatMessageAutoDeleteTime {
        chat_id: ChatId,
        message_auto_delete_time: i32,
    },
    /// `updateUser` — Phase 6 keeps the full parsed user (contacts list,
    /// user info panel) in `Session::users`; the bot bit still drives the
    /// private-chat draft gate.
    UpdateUser {
        user_id: UserId,
        user: ParsedUser,
    },
    /// Slice A12: `updateProfileAccentColors` (TDLib 1.8.67,
    /// `schema/td_api.tl:10964`) — the accent palette plus the ids
    /// `setProfileAccentColor` accepts. Stored in `Session`; drives the
    /// edit-profile accent picker.
    UpdateProfileAccentColors {
        colors: Vec<ProfileAccentColor>,
        available_ids: Vec<i32>,
    },
    /// `updateUserStatus` (schema 1.8.67, line 10729) — online / last-seen
    /// for a known user; refreshes the contacts list row.
    UpdateUserStatus {
        user_id: UserId,
        status: UserStatusKind,
    },
    /// `users` — `getContacts` response (schema 1.8.67, line 2471). Only
    /// the ids are authoritative here; the user objects themselves arrive
    /// via `updateUser`. `total_count` is dropped.
    Users {
        user_ids: Vec<i64>,
    },
    /// `updateChatNotificationSettings` — chat mute / sound exception changed.
    UpdateChatNotificationSettings {
        chat_id: ChatId,
        notification_settings: ChatNotificationSettings,
    },
    /// Slice CL1: `updateChatIsMarkedAsUnread` (schema 1.8.67, line
    /// 10588) — the chat was marked as unread or was read.
    UpdateChatIsMarkedAsUnread {
        chat_id: ChatId,
        is_marked_as_unread: bool,
    },
    /// Parity slice: `updateChatPhoto` (schema 1.8.67, line 10488) — the
    /// chat's photo changed. `photo` is the new `chatPhotoInfo.small`
    /// file (`None` when the photo was removed).
    UpdateChatPhoto {
        chat_id: ChatId,
        photo: Option<ParsedFile>,
    },
    /// `updateChatAction` — peer activity (`chatActionTyping` /
    /// `chatActionChoosingSticker` / `chatActionCancel`).
    UpdateChatAction {
        chat_id: ChatId,
        sender: MessageSender,
        action: ChatAction,
    },
    /// Phase B1: `updateSecretChat` (schema 1.8.67, line 10741) — the
    /// secret chat's state changed. Guaranteed to arrive before the
    /// chat identifier is returned (i.e. before `updateNewChat`).
    UpdateSecretChat {
        secret_chat: ParsedSecretChat,
    },
    /// Phase B1: `secretChat` (schema 1.8.67, line 2816) — the
    /// `getSecretChat` answer.
    SecretChat {
        secret_chat: ParsedSecretChat,
    },
    /// Phase C1: `updateCall` (schema 1.8.67, line 10816) — a new call
    /// was created or information about a call was updated.
    UpdateCall {
        call: ParsedCall,
    },
    /// Phase C1: `updateNewCallSignalingData` (schema 1.8.67, line
    /// 10862) — new call signaling data arrived. Quill has no media
    /// transport yet (C2), so the session queues it honestly; nothing
    /// consumes it.
    UpdateNewCallSignalingData {
        call_id: i32,
        data: Vec<u8>,
    },
    /// Phase C1: `callId` (schema 1.8.67, line 7034) — the `createCall`
    /// answer. Correlated to the outgoing request via `@extra` /
    /// `RequestPurpose::CreateCall`.
    CallId {
        id: i32,
    },
    /// Phase C3a: `groupCallId` (schema 1.8.67, line 7037) — the
    /// `createVideoChat` answer. Correlated via `@extra` /
    /// `RequestPurpose::CreateVideoChat`.
    GroupCallId {
        id: i32,
    },
    /// Phase C2f: `groupCallInfo` (schema 1.8.67, line 7190) — the
    /// `joinGroupCall` answer (invitation acceptance). Correlated via
    /// `@extra` / `RequestPurpose::JoinGroupCallInvitation`. The
    /// `join_payload` is the tgcalls payload (stored, never consumed —
    /// no media transport until Phase C2); `updateGroupCall` remains
    /// the source of truth for join state.
    GroupCallInfo {
        group_call_id: i32,
        join_payload: String,
    },
    /// Phase C3a: `updateGroupCall` (schema 1.8.67, line 10819) — a
    /// group call was created or its information was updated.
    UpdateGroupCall {
        group_call: ParsedGroupCall,
    },
    /// Phase C3a: `updateGroupCallParticipant` (schema 1.8.67, line
    /// 10824) — information about a group call participant changed.
    UpdateGroupCallParticipant {
        group_call_id: i32,
        participant: ParsedGroupCallParticipant,
    },
    /// Phase C3a: `updateGroupCallParticipants` (schema 1.8.67, line
    /// 10830) — the participant list changed; carries only user ids.
    UpdateGroupCallParticipants {
        group_call_id: i32,
        participant_user_ids: Vec<i64>,
    },
    /// Phase C3a: `updateGroupCallVerificationState` (schema 1.8.67,
    /// line 10836) — E2E verification emojis for the group call.
    UpdateGroupCallVerificationState {
        group_call_id: i32,
        generation: i32,
        emojis: Vec<String>,
    },
    /// Phase C3a: `updateChatVideoChat` (schema 1.8.67, line 10576) —
    /// a chat's video chat changed.
    UpdateChatVideoChat {
        chat_id: i64,
        video_chat: ParsedVideoChat,
    },
    /// Phase C2h: `updateNewGroupCallMessage` (schema 1.8.67, line
    /// 10839) — a message was sent in a group call (including by the
    /// current user; the echo is the confirmation).
    UpdateNewGroupCallMessage {
        group_call_id: i32,
        message: ParsedGroupCallMessage,
    },
    /// Phase C2h: `updateGroupCallMessageSendFailed` (schema 1.8.67,
    /// line 10851) — a sent group-call message failed.
    UpdateGroupCallMessageSendFailed {
        group_call_id: i32,
        message_id: i32,
        error: TdError,
    },
    /// Phase C2h: `updateGroupCallMessagesDeleted` (schema 1.8.67,
    /// line 10856) — group-call messages were deleted.
    UpdateGroupCallMessagesDeleted {
        group_call_id: i32,
        message_ids: Vec<i32>,
    },
    Ok,
    /// Slice A6: `importedContacts` (schema 1.8.67, line 14517) — the
    /// `importContacts` answer. Only the user ids are kept; the
    /// reducer treats it like the `ok` of the other contact
    /// mutations (invalidate + notice).
    ImportedContacts {
        user_ids: Vec<i64>,
    },
    /// Phase C3a: `text` (schema 1.8.67, line 10071) — the
    /// `joinVideoChat` answer (join payload for tgcalls).
    Text {
        text: String,
    },
    /// Phase C3a: `httpUrl` (schema 1.8.67, line 7458) — the
    /// `getVideoChatInviteLink` answer.
    HttpUrl {
        url: String,
    },
    /// Phase C2h: `rtmpUrl` (schema 1.8.67, line 7113) — the
    /// `getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl` answer.
    RtmpUrl {
        url: String,
        stream_key: String,
    },
    /// Phase C2f: `inviteGroupCallParticipantResult*` (schema 1.8.67,
    /// lines 7216-7227) — the `inviteGroupCallParticipant` answer.
    InviteGroupCallParticipantResult(InviteGroupCallParticipantResult),
    /// A5: `checkChatUsername` answer (schema 1.8.67, lines 8583–8598).
    /// Correlated by `@extra` in `ConnectDriver` before `apply` takes the
    /// pending request (the response carries no chat id).
    CheckChatUsernameResult(UsernameCheckResult),
    Error(TdError),
    Messages(Vec<ParsedMessage>),
    /// `messageThreadInfo` — the answer to `getMessageThread`.
    MessageThreadInfo(Box<ParsedMessageThreadInfo>),
    Message(ParsedMessage),
    /// M1: `messageLink` (TDLib 1.8.67, `schema/td_api.tl:9666` —
    /// `messageLink link is_public`) — the `getMessageLink` answer. The
    /// driver stashes `link` in `Session::message_link_result`; the UI
    /// copies it to the clipboard.
    MessageLink {
        link: String,
        is_public: bool,
    },
    /// M2: `richMessage` (TDLib 1.8.67, `schema/td_api.tl:5143`) — the
    /// `getFullRichMessage` answer. The driver replaces the blocks of the
    /// partially-received message in history with the full blocks.
    RichMessage {
        rich: RichMessageContent,
    },
    /// Slice msg-richtext-ai-tools: `fixedText` (TDLib 1.8.67,
    /// `schema/td_api.tl:157`) — the `fixTextWithAi` answer. `text` is
    /// the fixed text the composer applies (`diffText` is not parsed —
    /// nothing renders it; parsing what you never use is slop).
    FixedText {
        text: String,
    },
    /// Slice msg-richtext-ai-tools: bare `formattedText` (TDLib 1.8.67,
    /// `schema/td_api.tl:3046`) — the `composeTextWithAi`,
    /// `translateText` and `translateMessageText` answer. The composer
    /// applies `text` (entities are dropped: the draft is plain text,
    /// documented in the driver); translations keep `entities`.
    FormattedText {
        text: String,
        entities: Vec<TextEntity>,
    },
    /// MED4: `webPageInstantView` (TDLib 1.8.67, `schema/td_api.tl:4377`)
    /// — the `getWebPageInstantView` answer. `blocks` are the same
    /// `pageBlock*` list as `richMessage`, so the IV reader reuses the M2
    /// block parser/renderer verbatim.
    WebPageInstantView {
        rich: RichMessageContent,
    },
    /// MED4b: `linkPreview` (TDLib 1.8.67, `schema/td_api.tl:4570`) — the
    /// `getLinkPreview` answer for the composer prefetch. `None` when the
    /// payload isn't a well-formed `linkPreview` (defensive; a success
    /// always carries the object).
    LinkPreview {
        preview: Option<LinkPreview>,
    },
    /// M1 fix-up: `messageProperties` (TDLib 1.8.67,
    /// `schema/td_api.tl:11557`) — the `getMessageProperties` answer.
    /// Only `can_get_link` is kept: `getMessageLink` is "available only
    /// if messageProperties.can_get_link" (schema line 12056), so the
    /// driver gates the link request on it instead of letting "Share
    /// link" silently 400.
    MessageProperties(MessageActions),
    /// `messageViewers` (`getMessageViewers` answer).
    MessageViewers(Vec<MessageViewer>),
    /// `MessageReadDate` (`getMessageReadDate` answer).
    MessageReadDate(MessageReadDate),
    /// `addedReactions` (`getMessageAddedReactions` answer).
    AddedReactions(AddedReactionsPage),
    /// B4: `pollVoters` (TDLib 1.8.67, `schema/td_api.tl:2854`) — the
    /// `getPollVoters` answer (schema line 12941). `total_count` is the
    /// approximate total; `voters` is one page of senders, in server
    /// order. Only the senders are kept (the `date` is unused).
    PollVoters {
        total_count: i32,
        voters: Vec<MessageSender>,
    },
    /// Bots slice: `inlineQueryResults` (TDLib 1.8.67,
    /// `schema/td_api.tl:7716`) — the `getInlineQueryResults` answer
    /// (schema line 13019). One page of result summaries; the reducer
    /// appends pages into `Session::inline_query`.
    InlineQueryResults(InlineQueryResultsPage),
    /// `chats` — `searchChats` / `searchRecentlyFoundChats` / similar.
    Chats {
        total_count: i32,
        chat_ids: Vec<ChatId>,
    },
    /// `foundMessages` — `searchMessages` (and secret-chat search).
    FoundMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_offset: String,
    },
    /// Phase C2i: `userPrivacySettingRules` — `getUserPrivacySettingRules`.
    /// Slice S3: now carries the parsed rule details (exception user ids),
    /// not just constructor names.
    UserPrivacySettingRules {
        rules: Vec<PrivacyRule>,
    },
    /// Slice S3: `updateUserPrivacySettingRules` (schema 1.8.67, :10871) —
    /// rules changed on another device; `setting` is the
    /// `UserPrivacySetting` constructor name.
    UpdateUserPrivacySettingRules {
        setting: String,
        rules: Vec<PrivacyRule>,
    },
    /// Slice S3: `readDatePrivacySettings` (schema 1.8.67, :9026) —
    /// the `getReadDatePrivacySettings` answer.
    ReadDatePrivacySettings {
        show_read_date: bool,
    },
    /// Slice S3: `messageSenders` (schema 1.8.67, :14505) — the
    /// `getBlockedMessageSenders` answer; only user senders are kept.
    BlockedMessageSenders {
        total_count: i32,
        sender_ids: Vec<i64>,
    },
    /// `count` — the answer to `getChatMessageCount` (schema 1.8.67,
    /// line 10068).
    Count {
        count: i32,
    },
    /// `foundChatMessages` — `searchChatMessages`.
    FoundChatMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_from_message_id: MessageId,
    },
    /// `messageCalendar` — `getChatMessageCalendar` (schema line 3194):
    /// per-day counts, newest day first.
    MessageCalendar {
        total_count: i32,
        days: Vec<CalendarDay>,
    },
    /// `updateSupergroup` — `supergroup.is_forum` is how Quill learns a
    /// supergroup is a forum (`chatTypeSupergroup` has no forum flag).
    /// Parity slice: the first active username (`supergroup.usernames`,
    /// schema 1.8.67 lines 2746/2372) feeds the channel/supergroup header.
    /// `updateBasicGroup` — only `basicGroup.member_count` is kept (the
    /// header's "N members").
    UpdateBasicGroup {
        basic_group_id: i64,
        member_count: i32,
    },
    /// `updateChatOnlineMemberCount` — sent for opened groups.
    UpdateChatOnlineMemberCount {
        chat_id: i64,
        online_member_count: i32,
    },
    UpdateSupergroup {
        supergroup_id: i64,
        /// `supergroup.verification_status` (schema line 2746): the chat
        /// row's verified check / SCAM / FAKE label.
        verification: crate::peer_badge::VerificationStatus,
        is_forum: bool,
        /// Subsection tabs: `supergroup.has_forum_tabs` (schema 1.8.67,
        /// line 2746) — a forum whose topics show as tabs, the way
        /// Telegram Desktop shows them (`ChannelData::useSubsectionTabs`).
        has_forum_tabs: bool,
        username: String,
        /// `supergroup.member_count` — may be 0 until full info is known.
        member_count: i32,
        /// Phase A1: own `chatMemberStatus*` (`supergroup.status`, schema
        /// 1.8.67 line 2746 — "Current user status in the supergroup or
        /// channel"). Drives slow-mode bypass (admins/creators are exempt).
        status: ChannelMemberStatus,
        /// Phase A1: `rights.can_restrict_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, lines 2500/1092);
        /// `None` for any other status or a missing rights block.
        /// `setChatSlowModeDelay` requires this right (line 13551).
        can_restrict_members: Option<bool>,
        /// Phase D3a: `rights.can_invite_users` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Invite-link management requires this right (or creator status).
        can_invite_users: Option<bool>,
        /// Phase D3b: `rights.can_promote_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Admin management requires this right (or creator status).
        can_promote_members: Option<bool>,
        /// Slice G1: `rights.can_manage_tags` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Changing another member's custom title requires this right
        /// (or creator status); changing your own tag is always allowed.
        can_manage_tags: Option<bool>,
        /// Slice G2: `rights.can_manage_topics` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Forum topic management requires this right (or creator
        /// status).
        can_manage_topics: Option<bool>,
        /// Slice G2: `rights.can_change_info` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// `toggleSupergroupSignMessages` requires this right.
        can_change_info: Option<bool>,
        /// Slice G2: `rights.can_send_welcome_messages` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1090);
        /// `None` for any other status or a missing rights block.
        /// Welcome-message management requires this right (or creator
        /// status).
        can_send_welcome_messages: Option<bool>,
        /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
        /// 2733/2746) — drives the "Approve new members" toggle.
        join_by_request: bool,
        /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67,
        /// lines 2736/2746) — drives the broadcast-group toggle.
        is_broadcast_group: bool,
        /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746)
        /// — channel author signatures.
        sign_messages: bool,
        /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
        /// 2746) — sender shown alongside the signature; only meaningful
        /// when `sign_messages` is true.
        show_message_sender: bool,
    },
    /// `supergroup` — `getSupergroup` response. Phase A1: also keeps own
    /// `status` (`supergroup.status`, schema 1.8.67 line 2746) for the
    /// slow-mode bypass check.
    Supergroup {
        supergroup_id: i64,
        is_forum: bool,
        /// Subsection tabs: `supergroup.has_forum_tabs` (schema 1.8.67,
        /// line 2746).
        has_forum_tabs: bool,
        username: String,
        status: ChannelMemberStatus,
        /// Phase A1: `rights.can_restrict_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, lines 2500/1092);
        /// `None` for any other status or a missing rights block.
        can_restrict_members: Option<bool>,
        /// Phase D3a: `rights.can_invite_users` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Invite-link management requires this right (or creator status).
        can_invite_users: Option<bool>,
        /// Phase D3b: `rights.can_promote_members` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Admin management requires this right (or creator status).
        can_promote_members: Option<bool>,
        /// Slice G1: `rights.can_manage_tags` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        /// Changing another member's custom title requires this right
        /// (or creator status); changing your own tag is always allowed.
        can_manage_tags: Option<bool>,
        /// Slice G2: `rights.can_manage_topics` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        can_manage_topics: Option<bool>,
        /// Slice G2: `rights.can_change_info` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1092);
        /// `None` for any other status or a missing rights block.
        can_change_info: Option<bool>,
        /// Slice G2: `rights.can_send_welcome_messages` from own
        /// `chatMemberStatusAdministrator` (schema 1.8.67, line 1090);
        /// `None` for any other status or a missing rights block.
        can_send_welcome_messages: Option<bool>,
        /// Slice G1: `supergroup.join_by_request` (schema 1.8.67, lines
        /// 2733/2746) — drives the "Approve new members" toggle.
        join_by_request: bool,
        /// Slice G1: `supergroup.is_broadcast_group` (schema 1.8.67,
        /// lines 2736/2746) — drives the broadcast-group toggle.
        is_broadcast_group: bool,
        /// Slice G2: `supergroup.sign_messages` (schema 1.8.67, line 2746).
        sign_messages: bool,
        /// Slice G2: `supergroup.show_message_sender` (schema 1.8.67, line
        /// 2746).
        show_message_sender: bool,
    },
    /// `forumTopics` — `getForumTopics` response. Only the first page is
    /// fetched; `next_offset_*` are dropped (see Phase 5.1 DECISIONS).
    ForumTopics {
        total_count: i32,
        topics: Vec<ForumTopic>,
    },
    /// `forumTopicInfo` — `createForumTopic` answer. Only the chat id is
    /// kept; the topic list is refetched on success.
    ForumTopic {
        chat_id: i64,
    },
    /// Subsection tabs: `updateForumTopicInfo` (schema 1.8.67, line 10652).
    UpdateForumTopicInfo(ForumTopicInfoUpdate),
    /// Subsection tabs: `updateForumTopic` (schema 1.8.67, line 10665).
    UpdateForumTopic(ForumTopicUpdate),
    /// Subsection tabs: `forumTopic` — the `getForumTopic` answer.
    ForumTopicAnswer(ForumTopic),
    UpdateFile(ParsedFile),
    File(ParsedFile),
    /// Slice media-downloads-pause: `updateFileDownload` — pause state and
    /// completion for a file in the persistent download list (schema
    /// 1.8.67, line 10795). `counts` is not kept (no list-wide UI).
    UpdateFileDownload {
        file_id: i32,
        is_paused: bool,
        complete_date: i32,
    },
    /// `stickerSets` — `getInstalledStickerSets`.
    StickerSets {
        total_count: i32,
        sets: Vec<StickerSetInfo>,
    },
    /// `stickerSet` — `getStickerSet`. Files on each sticker are in `files`.
    StickerSet {
        id: i64,
        title: String,
        name: String,
        /// `stickerSet.is_installed`: the set is in the user's collection.
        is_installed: bool,
        stickers: Vec<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// Slice S8: `trendingStickerSets` — `getTrendingStickerSets`.
    /// `is_premium` flags the premium-only row (tdesktop renders it
    /// separately).
    TrendingStickerSets {
        total_count: i32,
        sets: Vec<StickerSetInfo>,
        is_premium: bool,
    },
    /// Slice S8: `stickers` — `searchStickers` / `getFavoriteStickers` /
    /// `getRecentStickers`. Files on each sticker are in `files`.
    Stickers {
        stickers: Vec<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// Slice S10: `emojiStatuses` — `getRecentEmojiStatuses` / `getUpgradedGiftEmojiStatuses`.
    EmojiStatuses {
        statuses: Vec<EmojiStatusItem>,
    },
    /// Slice S10: `emojiStatusCustomEmojis` — `getThemedEmojiStatuses` / `getDefaultEmojiStatuses`.
    EmojiStatusCustomEmojis {
        custom_emoji_ids: Vec<i64>,
    },
    /// Slice S10: `animatedEmoji` — `getAnimatedEmoji` (sticker + `sound` file).
    AnimatedEmoji {
        sticker: Option<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// Slice S10: `emojiKeywords` — `searchEmojis` answers for the picker.
    EmojiKeywords {
        keywords: Vec<EmojiKeyword>,
    },
    /// Slice S10: `emojiCategories` — `getEmojiCategories` answers for the picker.
    EmojiCategories {
        categories: Vec<EmojiCategory>,
        files: Vec<ParsedFile>,
    },
    /// `animations` — `getSavedAnimations`.
    Animations {
        animations: Vec<AnimationItem>,
        files: Vec<ParsedFile>,
    },
    /// `sponsoredMessages` — `getChatSponsoredMessages`. `messages_between` is
    /// the minimum number of ordinary messages between shown sponsored rows
    /// (0 = show after all ordinary messages).
    SponsoredMessages {
        messages: Vec<SponsoredMessage>,
        files: Vec<ParsedFile>,
        messages_between: i32,
    },
    /// `ReportSponsoredResult` — `reportChatSponsoredMessage` /
    /// `reportSponsoredChat` response.
    ReportSponsoredResult(ReportSponsoredResult),
    /// Phase 9.5: `ReportStoryResult` — `reportStory` response.
    ReportStoryResult(ReportStoryResult),
    /// Phase 9.5: `storyInteractions` — one page of `getStoryInteractions`
    /// results (an own story's viewers).
    StoryInteractions {
        interactions: StoryInteractionsView,
    },
    /// Phase 9.5: `updateStoryStealthMode` — stealth-mode state changed.
    UpdateStoryStealthMode {
        active_until_date: i32,
        cooldown_until_date: i32,
    },
    /// `updateSavedAnimations` — file ids of saved GIFs, newest first.
    UpdateSavedAnimations {
        animation_ids: Vec<i32>,
    },
    /// Slice S9: `updateAnimationSearchParameters` (schema 1.8.67, line
    /// 11064) — the server-pushed animation-search provider parameters:
    /// the upstream provider name (e.g. GIPHY/Tenor) and the suggested
    /// search emojis.
    UpdateAnimationSearchParameters {
        provider: String,
        emojis: Vec<String>,
    },
    /// Slice S15: `updateInstalledStickerSets` (schema 1.8.67, line 10932)
    /// — TDLib's authoritative new order of installed set ids after a
    /// reorder (manual, or usage-driven via `update_order_of_installed_
    /// sticker_sets` on a sticker send). `is_regular` selects the sticker
    /// panel's sets; other types route to the emoji panel's installed sets.
    UpdateInstalledStickerSets {
        sticker_set_ids: Vec<i64>,
        is_regular: bool,
    },
    /// `notificationSounds` — `getSavedNotificationSounds` response.
    NotificationSounds {
        sounds: Vec<NotificationSound>,
    },
    /// `storageStatistics` — `getStorageStatistics` response (Phase S2).
    /// Aggregated by file type across chats; stored in
    /// `Session::storage_stats` when the pending purpose is
    /// `GetStorageStatistics`.
    StorageStatistics {
        total_size: i64,
        by_file_type: Vec<StorageFileTypeStats>,
        /// Slice S4: per-chat rows (`chat_limit` > 0).
        by_chat: Vec<StorageChatStats>,
    },
    /// Slice S4: `autoDownloadSettingsPresets` — the
    /// `getAutoDownloadSettingsPresets` response (schema 1.8.67, line
    /// 9862: `autoDownloadSettingsPresets low:autoDownloadSettings
    /// medium:autoDownloadSettings high:autoDownloadSettings =
    /// AutoDownloadSettingsPresets;`). Seeds the local per-network
    /// settings once (the reducer marks them dirty for the driver to
    /// persist).
    AutoDownloadSettingsPresets {
        low: AutoDownloadNetSettings,
        medium: AutoDownloadNetSettings,
        high: AutoDownloadNetSettings,
    },
    /// Batch 4: `updateUnconfirmedSession` — the first unconfirmed login
    /// (`None` when none is left) and how many there are.
    UpdateUnconfirmedSession {
        session: Option<UnconfirmedLogin>,
        count: i32,
    },
    /// Batch 4: `updateServiceNotification` — a server popup.
    UpdateServiceNotification {
        kind: String,
        text: String,
    },
    /// Batch 4: `updateTermsOfService` — terms that must be accepted.
    UpdateTermsOfService {
        terms: TermsOfService,
    },
    /// Batch 6: `emailAddressAuthenticationCodeInfo` — the answer of
    /// `requestPasswordRecovery` / `setLoginEmailAddress` /
    /// `resendLoginEmailAddressCode`.
    EmailCodeInfo {
        pattern: String,
        length: i32,
    },
    /// Batch 6: `resetPasswordResult*` — the `resetPassword` answer.
    ResetPasswordResult(ResetPasswordOutcome),
    /// Slice A2: `passwordState` — the `getPasswordState` /
    /// `setPassword` / `setRecoveryEmailAddress` /
    /// `resendRecoveryEmailAddressCode` /
    /// `cancelRecoveryEmailAddressVerification` response (schema
    /// 1.8.67, line 273). Stored in `Session::password_state` when the
    /// pending purpose is `PasswordStateOp`.
    PasswordState {
        state: PasswordState,
    },
    /// Slice A3: `sessions` — `getActiveSessions` response (schema
    /// 1.8.67, lines 9147/15102). Stored in `Session::sessions` when the
    /// pending purpose is `GetActiveSessions`.
    DeviceLoginResult {
        result: crate::auth::DeviceLoginResult,
    },
    Sessions {
        sessions: Vec<ParsedSession>,
    },
    /// `parity:proxy-settings`: `addedProxies` — `getProxies` answer.
    AddedProxies {
        proxies: Vec<crate::proxy::ProxyEntry>,
    },
    /// `parity:proxy-settings`: `addedProxy` — `addProxy` / `editProxy`
    /// answer (`None`: an unknown proxy type).
    AddedProxy {
        proxy: Option<crate::proxy::ProxyEntry>,
    },
    /// `parity:proxy-settings`: `seconds` — `pingProxy` answer.
    Seconds {
        seconds: f64,
    },
    /// Slice A7: `accountTtl` — `getAccountTtl` response (schema 1.8.67,
    /// line 9053). Stored in `Session::account_ttl_days` when the
    /// pending purpose is `GetAccountTtl`.
    AccountTtl {
        days: i32,
    },
    /// Slice A8: `authenticationCodeInfo` — the `sendPhoneNumberCode` /
    /// `resendPhoneNumberCode` answer (schema 1.8.67, line 78). Stored in
    /// `Session::change_number_phone` / `change_number_timeout` when the
    /// pending purpose is `SendPhoneNumberCode` / `ResendPhoneNumberCode`.
    /// The `type` / `next_type` variants are not kept: this slice is
    /// backend-only and the UI half will parse them when it ships.
    AuthenticationCodeInfo {
        phone_number: String,
        timeout: i32,
    },
    /// Slice A4: `connectedWebsites` — `getConnectedWebsites` response
    /// (schema 1.8.67, lines 9171/15124). Stored in
    /// `Session::connected_websites` when the pending purpose is
    /// `GetConnectedWebsites`.
    ConnectedWebsites {
        websites: Vec<ParsedWebsite>,
    },
    /// Slice CL2: `archiveChatListSettings` — `getArchiveChatListSettings`
    /// response (schema 1.8.67, line 3512); stored in
    /// `Session::archive_chat_list_settings` when the pending purpose is
    /// `GetArchiveChatListSettings`.
    ArchiveChatListSettings {
        settings: ArchiveChatListSettings,
    },
    /// `updateSavedNotificationSounds` — the saved-sound list changed;
    /// the reducer marks the cached list stale (schema line 10947).
    UpdateSavedNotificationSounds {
        sound_ids: Vec<i64>,
    },
    /// `scopeNotificationSettings` — `getScopeNotificationSettings` response.
    ScopeNotificationSettings {
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    },
    /// `updateScopeNotificationSettings` — a scope's defaults changed
    /// (schema line 10668).
    UpdateScopeNotificationSettings {
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    },
    /// `updateReactionNotificationSettings` — reaction and poll-vote
    /// notification settings changed (schema line 10671). No getter
    /// exists; the update stream is the source of truth.
    UpdateReactionNotificationSettings {
        settings: ReactionNotificationSettings,
    },
    /// `updateMessageInteractionInfo` — views / forwards / `messageReactions`.
    UpdateMessageInteractionInfo {
        chat_id: ChatId,
        message_id: MessageId,
        interaction_info: Option<MessageInteractionInfo>,
    },
    /// `updateMessageIsPinned` — message pin state changed (TDLib 1.8.67).
    UpdateMessageIsPinned {
        chat_id: ChatId,
        message_id: MessageId,
        is_pinned: bool,
    },
    /// `chatMember` — `getChatMember` response for a channel.
    ChatMember {
        member: ParsedChatMember,
    },
    /// `updateChatMember` — own or peer membership changed; only the
    /// `new_chat_member` is kept.
    UpdateChatMember {
        chat_id: ChatId,
        member: ParsedChatMember,
    },
    /// `user` — `getMe` response; only the id is kept.
    Me {
        user_id: i64,
    },
    /// `userFullInfo` — `getUserFullInfo` response (schema 1.8.67,
    /// `getUserFullInfo user_id:int53 = UserFullInfo`, line 11501). The
    /// response carries no user id; it is resolved from the pending
    /// request in `Session::apply`, so only `bot_info` (from
    /// `userFullInfo.bot_info:botInfo`, line 2468), `bio` (from
    /// `userFullInfo.bio:formattedText`) and the preferred profile-photo
    /// file (from `userFullInfo.photo:chatPhoto` sizes) are kept.
    UserFullInfo {
        /// `userFullInfo.birthdate` and `group_in_common_count` (schema
        /// 1.8.67, line 2468) for the profile panel.
        extras: UserProfileExtras,
        bot_info: Option<BotInfo>,
        bio: String,
        /// Preferred size's `photo:file` from `chatPhoto.sizes`
        /// (schema 1.8.67, line 1030); `None` when the user has no photo.
        photo: Option<ParsedFile>,
        /// A5: `chatPhoto.id` (schema 1.8.67, line 1030) — the
        /// `profile_photo_id` for `deleteProfilePhoto`.
        photo_id: Option<i64>,
        /// Slice A6: `userFullInfo.block_list:BlockList` (schema 1.8.67,
        /// line 2468) — true when it is `blockListMain` (line 9692),
        /// parsed with the same `is_block_list_main` as `chat.block_list`.
        blocked: bool,
    },
    /// `updateUserFullInfo` — full info changed (schema 1.8.67, line 10744);
    /// the user id is explicit here.
    UpdateUserFullInfo {
        user_id: UserId,
        extras: UserProfileExtras,
        bot_info: Option<BotInfo>,
        bio: String,
        photo: Option<ParsedFile>,
        /// A5: `chatPhoto.id`, as above.
        photo_id: Option<i64>,
        /// Slice A6: `block_list` (schema 1.8.67, line 2468), same as
        /// `UserFullInfo::blocked`.
        blocked: bool,
    },
    /// `supergroupFullInfo` — `getSupergroupFullInfo` response (schema
    /// 1.8.67, line 11513). The response carries no supergroup id; it is
    /// resolved from the pending request in `Session::apply`. Kept:
    /// `description`, `member_count`, `linked_chat_id` (schema 1.8.67,
    /// line 2792; the discussion-group chat id for the channel header's
    /// "Discuss" affordance), plus the slow-mode fields and boost counts
    /// (Phase A1: `slow_mode_delay` / `slow_mode_delay_expires_in`, schema
    /// 1.8.67 lines 2758–2759; `my_boost_count` / `unrestrict_boost_count`,
    /// lines 2779–2780) that drive composer slow-mode enforcement, plus
    /// Phase D2's `can_get_statistics` (line 2792) gating the statistics
    /// entry point. Slice S11: `can_set_sticker_set` (line 2765),
    /// `sticker_set_id` / `custom_emoji_sticker_set_id` (line 2792) for
    /// group sticker-set management. Dropped: admin/restricted/banned
    /// counts, invite link, gift fields, paid-message and other statistics
    /// flags, location.
    SupergroupFullInfo {
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
        /// Phase D2: `supergroupFullInfo.can_get_statistics` (schema 1.8.67,
        /// line 2792) — true when chat statistics are available via
        /// `getChatStatistics`. Gates the statistics entry point.
        can_get_statistics: bool,
        /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
        /// (schema 1.8.67, line 2792).
        has_aggressive_anti_spam_enabled: bool,
        /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
        /// (schema 1.8.67, line 2792) — gates the anti-spam toggle.
        can_toggle_aggressive_anti_spam: bool,
        /// Slice S11: `supergroupFullInfo.can_set_sticker_set` (schema
        /// 1.8.67, line 2765) — true when the supergroup sticker set can
        /// be changed; gates the group sticker-set affordance.
        can_set_sticker_set: bool,
        /// Slice S11: `supergroupFullInfo.sticker_set_id` (schema 1.8.67,
        /// line 2792) — the installed group sticker set; 0 when none.
        sticker_set_id: i64,
        /// Slice S11: `supergroupFullInfo.custom_emoji_sticker_set_id`
        /// (schema 1.8.67, line 2792) — the group's custom-emoji set; 0
        /// when none.
        custom_emoji_sticker_set_id: i64,
    },
    /// Slice (communities backend core): `updateCommunity` (schema 1.8.67,
    /// line 10726) — the update carries the full `community` object and is
    /// guaranteed to arrive before the `communityId` of a just-created
    /// community, so the reducer inserts it create-on-first-sight.
    UpdateCommunity {
        community: ParsedCommunity,
    },
    /// Slice (communities backend core): `updateCommunityFullInfo`
    /// (schema 1.8.67, line 10753) — carries its own `community_id`, so
    /// it applies whenever it arrives (no pending-request correlation).
    /// This is the arrival path for `loadCommunityFullInfo` (schema line
    /// 11799, which answers `ok` and delivers the data through update).
    UpdateCommunityFullInfo {
        community_id: i64,
        full_info: ParsedCommunityFullInfo,
    },
    /// Slice (communities backend core): `communityId` (schema 1.8.67,
    /// line 2264) — the response of `createCommunity` (line 11806). The
    /// driver chains it into `loadCommunityFullInfo`.
    CommunityId {
        id: i64,
    },
    /// Slice G2: `updateChatWelcomeMessages` (schema 1.8.67, line 10649)
    /// — the chat's welcome-message pack, sent after
    /// `loadChatWelcomeMessages` and whenever the pack changes.
    UpdateChatWelcomeMessages {
        chat_id: i64,
        messages: Vec<ParsedWelcomeMessage>,
    },
    /// Slice G2: `updateChatHasWelcomeMessages` (schema 1.8.67, line
    /// 10600) — the chat's `has_welcome_messages` field changed.
    UpdateChatHasWelcomeMessages {
        chat_id: i64,
        has_welcome_messages: bool,
    },
    /// `updateChatHasProtectedContent` (schema 1.8.67, line 10582) — the
    /// chat's `has_protected_content` changed.
    UpdateChatHasProtectedContent {
        chat_id: i64,
        has_protected_content: bool,
    },
    /// `updateChatHasScheduledMessages` — the chat gained its first or lost
    /// its last scheduled message.
    UpdateChatHasScheduledMessages {
        chat_id: i64,
        has_scheduled_messages: bool,
    },
    /// `updateChatIsTranslatable` (schema 1.8.67, line 10585) — translation
    /// of the chat's messages was enabled or disabled.
    UpdateChatIsTranslatable {
        chat_id: i64,
        is_translatable: bool,
    },
    /// Slice G2: `chatBoostStatus` (schema 1.8.67, line 6943) — the
    /// `getChatBoostStatus` response. Only `level` and `boost_count`
    /// drive the channel profile row.
    ChatBoostStatus {
        level: i32,
        boost_count: i32,
    },
    /// Slice G2: `chatBoostSlots` (schema 1.8.67, line 6968) — the
    /// `getAvailableChatBoostSlots` / `boostChat` response; only the slot
    /// ids are kept.
    ChatBoostSlots {
        slots: Vec<i32>,
    },
    /// Phase D2: `chatStatisticsChannel` / `chatStatisticsSupergroup` —
    /// `getChatStatistics` response (schema 1.8.67, line 15760). The
    /// response carries no chat id; it is resolved from the pending
    /// request in `Session::apply`.
    ChatStatistics {
        statistics: ChatStatistics,
    },
    /// Phase D3a: `chatInviteLink` response (TDLib 1.8.67, line 2627) —
    /// the created/edited link from `createChatInviteLink` /
    /// `editChatInviteLink`. Correlated to the chat by the request's
    /// `PendingRequest::chat_id`.
    ChatInviteLink {
        link: ParsedChatInviteLink,
    },
    /// `checkChatInviteLink` answer (schema 1.8.67, line 2684).
    ChatInviteLinkInfo {
        title: String,
        member_count: i32,
        creates_join_request: bool,
        is_channel: bool,
    },
    /// `parity:platform-deep-links`: `deepLinkInfo` (schema 1.8.67, line
    /// 10087) — the `getDeepLinkInfo` answer. The actionable data is in
    /// `entities`: TDLib marks the resolved action with
    /// `textEntityTypeTextUrl` entities whose `url` is a `tg://` URL.
    DeepLinkInfo {
        text: String,
        need_update: bool,
        entities: Vec<TextEntity>,
    },
    /// Phase D3a: `chatInviteLinks` (TDLib 1.8.67, line 2630) — the
    /// response of `getChatInviteLinks` / `revokeChatInviteLink`.
    /// Correlated to the chat by the request's `PendingRequest::chat_id`.
    ChatInviteLinks {
        total_count: i32,
        links: Vec<ParsedChatInviteLink>,
    },
    /// Phase D3a: `chatJoinRequests` (TDLib 1.8.67, line 2691) — the
    /// response of `getChatJoinRequests`. Correlated to the chat by the
    /// request's `PendingRequest::chat_id`.
    ChatJoinRequests {
        total_count: i32,
        requests: Vec<ParsedChatJoinRequest>,
    },
    /// Phase D3b: `chatAdministrators` (TDLib 1.8.67, line 2485) — the
    /// response of `getChatAdministrators` (line 13632). Carries no chat
    /// id; correlated to the chat by the request's
    /// `PendingRequest::chat_id`.
    ChatAdministrators {
        administrators: Vec<ChatAdministratorEntry>,
    },
    /// Slice G1: `createdBasicGroupChat` (TDLib 1.8.67, line 3644) — the
    /// response of `createNewBasicGroupChat` (line 13327):
    /// `createdBasicGroupChat chat_id:int53
    /// failed_to_add_members:failedToAddMembers = CreatedBasicGroupChat;`
    /// The new chat itself arrives as `updateNewChat`.
    CreatedBasicGroupChat {
        chat_id: i64,
    },
    /// Slice G1: `failedToAddMembers` (TDLib 1.8.67, line 3640) — the
    /// response of `addChatMembers` (line 13584):
    /// `failedToAddMembers
    /// failed_to_add_members:vector<failedToAddMember> =
    /// FailedToAddMembers;`
    /// Only the failure count is kept; per-user errors are dropped.
    FailedToAddMembers {
        failed_count: i32,
    },
    /// Slice G1: `basicGroupFullInfo` (TDLib 1.8.67, line 2714) — the
    /// response of `getBasicGroupFullInfo` (line 11507):
    /// `basicGroupFullInfo photo:chatPhoto description:string
    /// creator_user_id:int53 members:vector<chatMember> ... =
    /// BasicGroupFullInfo;`
    /// Only the member list is kept (the basic-group member dialog);
    /// correlated to the chat by the request's `PendingRequest::chat_id`.
    BasicGroupFullInfo {
        members: Vec<ParsedChatMember>,
    },
    /// Phase D3c: `chatEvents` (TDLib 1.8.67, line 7938) — the response
    /// of `getChatEventLog` (line 15252). Carries no chat id; correlated
    /// to the chat by the request's `PendingRequest::chat_id`. Events
    /// arrive in reverse chronological order (decreasing event `id`).
    ChatEvents {
        events: Vec<ParsedChatEvent>,
    },
    /// Phase D3b: `chatMembers` (TDLib 1.8.67, line 2529) — the response
    /// of `getSupergroupMembers` (line 15238). Drives the promote flow's
    /// member picker. Carries no supergroup id; correlated by the
    /// request's `PendingRequest::chat_id`.
    SupergroupMembers {
        members: Vec<ParsedChatMember>,
        total_count: i32,
    },
    /// Phase D3a: `updateNewChatJoinRequest` (TDLib 1.8.67, line 11210) —
    /// a user requested to join the chat. Carries its own `chat_id`.
    UpdateNewChatJoinRequest {
        chat_id: i64,
        request: ParsedChatJoinRequest,
        user_chat_id: i64,
        invite_link: ParsedChatInviteLink,
        query_id: i64,
    },
    /// Phase D3a: `updateChatPendingJoinRequests` (TDLib 1.8.67, line
    /// 10555) — the pending-join-request summary changed. Carries its own
    /// `chat_id`; the full request list still needs `getChatJoinRequests`.
    UpdateChatPendingJoinRequests {
        chat_id: i64,
        total_count: i32,
        user_ids: Vec<i64>,
    },
    /// Parity slice: `updateSupergroupFullInfo` (schema 1.8.67, line 10750)
    /// — the update carries its own `supergroup_id`, so it applies
    /// whenever it arrives (no pending-request correlation). Phase A1:
    /// also carries the slow-mode fields (schema lines 2758–2759) and
    /// boost counts (lines 2779–2780); note the schema warns no update
    /// fires when only `slow_mode_delay_expires_in` changes while both
    /// old and new values are non-zero, so the reducer timestamps every
    /// arrival and the gate decays locally.
    UpdateSupergroupFullInfo {
        supergroup_id: i64,
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
        /// Phase D2: `supergroupFullInfo.can_get_statistics` (schema 1.8.67,
        /// line 2792).
        can_get_statistics: bool,
        /// Slice G2: `supergroupFullInfo.has_aggressive_anti_spam_enabled`
        /// (schema 1.8.67, line 2792).
        has_aggressive_anti_spam_enabled: bool,
        /// Slice G2: `supergroupFullInfo.can_toggle_aggressive_anti_spam`
        /// (schema 1.8.67, line 2792) — gates the anti-spam toggle.
        can_toggle_aggressive_anti_spam: bool,
        /// Slice S11: sticker-set fields (schema 1.8.67, lines 2765 and
        /// 2792), nested like the other fields.
        can_set_sticker_set: bool,
        sticker_set_id: i64,
        custom_emoji_sticker_set_id: i64,
    },
    /// `botCommands` — `getCommands` response (TDLib 1.8.67,
    /// `schema/td_api.tl:829`): the bot's commands for the requested scope
    /// as a bare `vector<botCommand>`. The schema annotates `getCommands`
    /// "for bots only" (line 14953); on a user session the response is an
    /// `error` instead, which `Session::apply` absorbs silently. The
    /// response's `bot_user_id` is cached as the global-scope command set
    /// shown below the bot's `botInfo` commands in the 3.3 `/` menu.
    BotCommands {
        bot_user_id: UserId,
        commands: Vec<BotCommand>,
    },
    /// `ChatJoinResult` — `joinChat` response.
    JoinChatResult(ChatJoinResult),
    /// `callbackQueryAnswer` — response to `getCallbackQueryAnswer` after an
    /// inline keyboard callback-button press (Phase 3.2).
    CallbackQueryAnswer(CallbackQueryAnswer),
    /// Slice bots-games: `gameHighScores` — response to `getGameHighScores`
    /// (TDLib 1.8.67, `schema/td_api.tl:13174`).
    GameHighScores(Vec<GameHighScore>),
    /// B1: `loginUrlInfo*` — response to `getLoginUrlInfo` after a
    /// login-URL button press.
    LoginUrlInfo(LoginUrlInfo),
    /// Slice P1: `paymentForm` — the `getPaymentForm` answer after a Buy
    /// button press (schema/td_api.tl:4734).
    PaymentForm(PaymentFormData),
    MarketplaceGift(Option<crate::marketplace::GiftQuote>),
    GiftTextLimit(i64),
    GiftPurchaseResult(crate::marketplace::GiftPurchaseResult),
    /// Slice P1: `validatedOrderInfo` — the `validateOrderInfo` answer
    /// (schema/td_api.tl:4737).
    ValidatedOrderInfo(ValidatedOrderInfoData),
    /// Slice P1: `paymentResult` — the `sendPaymentForm` answer
    /// (schema/td_api.tl:4740).
    PaymentResult(PaymentResultData),
    /// Slice P1: `paymentReceipt` — the `getPaymentReceipt` answer
    /// (schema/td_api.tl:4765).
    PaymentReceipt(PaymentReceiptData),
    /// Slice `parity:bots-payment-recurring`: `starSubscriptions` — the
    /// `getStarSubscriptions` answer (schema/td_api.tl:1269).
    StarSubscriptions(StarSubscriptionsData),
    /// `updateChatFolders` (TDLib 1.8.67, `schema/td_api.tl:10606`) — the
    /// full ordered folder list. There is no `getChatFolders` function in
    /// 1.8.67; TDLib pushes this update after authorization and whenever
    /// folders change. `main_chat_list_position` is dropped (folder reorder
    /// always sends position 0); `are_tags_enabled` is kept (parity slice:
    /// folder tags UI).
    UpdateChatFolders {
        folders: Vec<ChatFolderInfo>,
        are_tags_enabled: bool,
    },
    /// Parity slice: `chatFolderInfo` as the response of `createChatFolder`
    /// / `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358` /
    /// `:13361`). The reducer upserts it into `Session::chat_folders`;
    /// `updateChatFolders` stays the source of truth.
    ChatFolderInfo(ChatFolderInfo),
    /// Parity slice: `chatFolder` as the response of `getChatFolder`
    /// (TDLib 1.8.67, `schema/td_api.tl:13355`) — the full editable spec
    /// for the edit dialog prefill / remove-from-folder chain.
    ChatFolder {
        spec: ChatFolderSpec,
    },
    /// Parity slice: `chatLists` as the response of `getChatListsToAddChat`
    /// (TDLib 1.8.67, `schema/td_api.tl:13347`) — the chat lists a chat may
    /// be added to via `addChatToList`. Correlated to the chat by the
    /// request's `PendingRequest::chat_id`.
    ChatLists {
        lists: Vec<ChatList>,
    },
    /// Phase 9.1: `updateChatActiveStories` (TDLib 1.8.67,
    /// `schema/td_api.tl:10911`) — the active stories of a chat changed.
    /// The reducer keeps it in `Session::story_tray`; only entries with
    /// `list == Main` are shown in the story tray above the chat list.
    UpdateChatActiveStories {
        active_stories: ChatActiveStoriesView,
    },
    /// Phase 9.1: `chatActiveStories` — the `getChatActiveStories` response
    /// (TDLib 1.8.67, `schema/td_api.tl:13768`); handled like the update.
    ChatActiveStories {
        active_stories: ChatActiveStoriesView,
    },
    /// Phase 9.1: `story` — the `getStory` response (TDLib 1.8.67,
    /// `schema/td_api.tl:13695`); also the `updateStory` update (line
    /// 10895). The reducer keeps it in `Session::stories` keyed by
    /// `(poster_chat_id, id)` for the story viewer.
    Story {
        story: ParsedStory,
        files: Vec<ParsedFile>,
    },
    /// Phase 9.7: `storyAlbums` — the `getChatStoryAlbums` response
    /// (TDLib 1.8.67, `schema/td_api.tl:13850`). The reducer replaces the
    /// chat's album list (`Session::story_albums`), correlated via
    /// `PendingRequest::chat_id`.
    StoryAlbums {
        albums: Vec<ParsedStoryAlbum>,
    },
    /// Phase 9.7: `storyAlbum` — the `createStoryAlbum` /
    /// `setStoryAlbumName` / `addStoryAlbumStories` /
    /// `removeStoryAlbumStories` / `reorderStoryAlbumStories` response
    /// (schema `td_api.tl:13863` / `13879` / `13887` / `13894` / `13901`;
    /// each returns "the changed album"). The reducer upserts it into
    /// the chat's album list.
    StoryAlbum {
        album: ParsedStoryAlbum,
    },
    /// Phase 9.7: `stories` — the `getStoryAlbumStories` /
    /// `getChatArchivedStories` / `getChatPostedToChatPageStories`
    /// response (schema `td_api.tl:13857` / `13784` / `13776`).
    /// `pinned_story_ids` is populated only by
    /// `getChatPostedToChatPageStories` with `from_story_id == 0`
    /// (schema comment at `td_api.tl:6747`). Stories are cached in
    /// `Session::stories`; the reducer accumulates the ids per purpose.
    Stories {
        total_count: i32,
        stories: Vec<(ParsedStory, Vec<ParsedFile>)>,
        pinned_story_ids: Vec<i32>,
    },
    /// Phase 9.2: `updateStoryDeleted` (TDLib 1.8.67, `schema/td_api.tl:10898`)
    /// — a story was deleted. The reducer drops it from `Session::stories`
    /// and from the poster's tray entry.
    UpdateStoryDeleted {
        poster_chat_id: i64,
        story_id: i32,
    },
    /// Phase 9.2: `updateStoryPostSucceeded` (TDLib 1.8.67,
    /// `schema/td_api.tl:10901`) — a story posted from another client is
    /// live. The reducer upserts it into `Session::stories` and asks the
    /// driver to refresh the poster's tray (`getChatActiveStories`), so an
    /// own story appears in the tray.
    UpdateStoryPostSucceeded {
        story: ParsedStory,
        files: Vec<ParsedFile>,
        old_story_id: i32,
    },
    /// Phase 9.2: `updateStoryPostFailed` (TDLib 1.8.67,
    /// `schema/td_api.tl:10907`) — a story failed to post. The reducer drops
    /// the failed story from `Session::stories` and the poster's tray entry
    /// (it never went live). Phase 9.3: also feeds the composer's
    /// `StoryPostOutcome::Failed` for our own pending post.
    UpdateStoryPostFailed {
        story: ParsedStory,
        error: TdError,
    },
    /// Phase 9.2: `availableReactions` — the `getStoryAvailableReactions`
    /// response (TDLib 1.8.67, `schema/td_api.tl:13802`). The reducer keeps
    /// it in `Session::story_available_reactions` for the viewer picker.
    StoryAvailableReactions {
        reactions: Vec<StoryAvailableReactionView>,
        /// `recent_reactions` / `popular_reactions` and
        /// `allow_custom_emoji` — used by the message reaction picker
        /// (`getMessageAvailableReactions` answers the same type).
        recent: Vec<StoryAvailableReactionView>,
        popular: Vec<StoryAvailableReactionView>,
        allow_custom_emoji: bool,
    },
    /// Phase 9.3: `canPostStory` answer (TDLib 1.8.67,
    /// `schema/td_api.tl:8535` – `td_api.tl:8553`). The reducer honors it
    /// only when the pending purpose is `CheckCanPostStory`.
    CanPostStoryResult {
        result: CanPostStoryResult,
    },
    Unknown(UnknownKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    pub type_name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEnvelope {
    #[serde(rename = "@type")]
    type_name: String,
    #[serde(rename = "@extra")]
    extra: Option<Value>,
    #[serde(rename = "@client_id")]
    client_id: Option<i32>,
}

pub fn parse_envelope(json: &str) -> Result<Envelope, ParseError> {
    let raw: RawEnvelope = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    let export_extra = raw
        .extra
        .as_ref()
        .and_then(|v| v.get("quill_account_export"));
    let extra = parse_extra(export_extra.or(raw.extra.as_ref()));
    let payload = if export_extra.is_some() {
        EnvelopePayload::AccountExport(
            serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?,
        )
    } else {
        parse_payload(&raw.type_name, json)?
    };
    Ok(Envelope {
        type_name: raw.type_name,
        extra,
        client_id: raw.client_id,
        payload,
    })
}

pub(crate) fn parse_extra(value: Option<&Value>) -> Option<RequestId> {
    match value {
        Some(Value::String(s)) => RequestId::from_str(s).ok(),
        Some(Value::Number(n)) => n.as_u64().map(RequestId),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidJson,
    MissingField,
    BadInt,
}

/// The `messageProperties` flags the message context menu needs (TDLib
/// 1.8.67, `schema/td_api.tl:6262`): Telegram Desktop shows an action only
/// when the message allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessageActions {
    pub can_be_copied: bool,
    pub can_be_deleted_only_for_self: bool,
    pub can_be_deleted_for_all_users: bool,
    pub can_be_edited: bool,
    pub can_be_forwarded: bool,
    pub can_be_pinned: bool,
    pub can_be_replied: bool,
    pub can_get_link: bool,
    pub can_get_message_thread: bool,
    /// Content may be saved locally (Save As…, Copy Image, Show in Folder).
    pub can_be_saved: bool,
    /// The message can be reported with `reportChat`.
    pub can_report_chat: bool,
    /// `getMessageViewers` works ("N Seen").
    pub can_get_viewers: bool,
    /// `getMessageReadDate` works ("Seen 12:34" in private chats).
    pub can_get_read_date: bool,
    /// An admin may report it with `reportSupergroupSpam`.
    pub can_report_supergroup_spam: bool,
    /// An admin may delete other members' reactions on it.
    pub can_delete_reactions: bool,
    /// A scheduled message may be rescheduled or sent now.
    pub can_edit_scheduling_state: bool,
}

impl MessageActions {
    pub fn can_be_deleted(&self) -> bool {
        self.can_be_deleted_only_for_self || self.can_be_deleted_for_all_users
    }
}

/// Profile details from `userFullInfo` beyond the bio: the birthday and
/// how many groups you share with the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserProfileExtras {
    pub birthdate: Option<Birthdate>,
    pub groups_in_common: i32,
}

/// `birthdate` (schema 1.8.67, line 868); the year is optional (0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Birthdate {
    pub day: u8,
    pub month: u8,
    pub year: Option<i32>,
}

pub(crate) fn parse_user_profile_extras(info: Option<&serde_json::Value>) -> UserProfileExtras {
    let Some(info) = info else {
        return UserProfileExtras::default();
    };
    let field = |value: &serde_json::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    let birthdate = info
        .get("birthdate")
        .filter(|value| !value.is_null())
        .and_then(|value| {
            let (day, month, year) = (
                field(value, "day"),
                field(value, "month"),
                field(value, "year"),
            );
            ((1..=31).contains(&day) && (1..=12).contains(&month)).then(|| Birthdate {
                day: day as u8,
                month: month as u8,
                year: (year > 0).then_some(year as i32),
            })
        });
    UserProfileExtras {
        birthdate,
        groups_in_common: field(info, "group_in_common_count").max(0) as i32,
    }
}

/// One `messageCalendarDay` (schema line 3191): the first message sent on
/// the day and how many matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDay {
    pub total_count: i32,
    pub message_id: MessageId,
    pub date: i32,
}
